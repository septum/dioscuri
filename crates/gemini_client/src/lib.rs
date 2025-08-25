mod verification;

use std::{
    io::{self, BufRead, Read, Write},
    net::TcpStream,
    num,
    string::{self},
    sync::Arc,
};

use rustls::{ClientConfig, ClientConnection, StreamOwned, pki_types::InvalidDnsNameError};
use thiserror::Error;
use url::Url;

const PROTOCOL: &str = "gemini://";
const DEFAULT_PORT: usize = 1965;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Something unexpected happened while making the request")]
    UnexpectedError,
    #[error("The provided URL does not contain a host")]
    NoHostError,
    #[error("The request type is not supported")]
    UnsupportedError,
    #[error("Mime type {0} is not supported")]
    UnsupportedMimeError(String),
    #[error("An error happened while performing the request: {0}")]
    RequestError(String),
    #[error("The host provided is invalid: {0}")]
    ConvertError(#[from] InvalidDnsNameError),
    #[error("Could not open the TCP connection: {0}")]
    IoError(#[from] io::Error),
    #[error("Could not create the client configuration: {0}")]
    RustlsError(#[from] rustls::Error),
    #[error("Url could not be parsed: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("UTF-8 string could not be parsed: {0}")]
    Utf8Error(#[from] string::FromUtf8Error),
    #[error("Integer could not be parsed: {0}")]
    IntegerParseError(#[from] num::ParseIntError),
}

fn create_client_config() -> Arc<ClientConfig> {
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verification::AllowUnknownIssuerVerification::new())
        .with_no_client_auth();

    Arc::new(config)
}

fn open_tls_socket(
    host: &str,
    config: Arc<ClientConfig>,
) -> Result<StreamOwned<ClientConnection, TcpStream>, ClientError> {
    let address = format!("{}:{}", host, DEFAULT_PORT);

    let connection = ClientConnection::new(config, host.to_owned().try_into()?)?;
    let socket = TcpStream::connect(address)?;

    Ok(StreamOwned::new(connection, socket))
}

pub struct Connection {
    url: Url,
    socket: StreamOwned<ClientConnection, TcpStream>,
}

pub struct GeminiClient {
    config: Arc<ClientConfig>,
    connection: Option<Connection>,
}

impl GeminiClient {
    pub fn new() -> Self {
        Self {
            config: create_client_config(),
            connection: None,
        }
    }

    pub fn request(&mut self, url: &str) -> Result<String, ClientError> {
        self.update_connection(url)?;

        if let Some(connection) = &mut self.connection {
            let host = connection
                .url
                .host_str()
                .ok_or(ClientError::UnexpectedError)?;
            let path = connection.url.path();

            // https://geminiprotocol.net/docs/protocol-specification.gmi#requests
            // - Needs trailing `/` otherwise it redirects (status 3X)
            // - Must end with CRLF
            let request = format!("{}{}{}\r\n", PROTOCOL, host, path);
            connection.socket.write_all(request.as_bytes())?;

            let mut header = Vec::new();
            connection.socket.read_until(b'\n', &mut header)?;

            let space_pos = header.iter().position(|&c| c == b' ').unwrap();
            let header = String::from_utf8(header)?;

            // https://geminiprotocol.net/docs/protocol-specification.gmi#responses
            // - {status}{SP}{mimetype|URI-reference|errormsg}{CRLF}{body?}
            let (status_str, meta) = header.split_at(space_pos);
            let status = status_str[..1].parse::<u8>()?;

            match status {
                1 | 3 | 6 => Err(ClientError::UnsupportedError),
                2 => {
                    let mime = meta.trim();
                    if !mime.starts_with("text/") {
                        return Err(ClientError::UnsupportedMimeError(mime.to_owned()));
                    }

                    let mut body = String::new();
                    connection.socket.read_to_string(&mut body)?;

                    Ok(body)
                }
                _ => Err(ClientError::RequestError(meta.to_owned())),
            }
        } else {
            Err(ClientError::UnexpectedError)
        }
    }

    fn update_connection(&mut self, url: &str) -> Result<(), ClientError> {
        let url = Url::parse(url)?;

        let host = url.host_str().ok_or(ClientError::NoHostError)?;
        let socket = open_tls_socket(host, self.config.clone())?;
        self.connection = Some(Connection { url, socket });

        Ok(())
    }
}
