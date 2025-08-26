mod verification;

use std::{
    io::{self, BufRead, Read, Write},
    net::TcpStream,
    num,
    string::{self},
    sync::Arc,
};

use rustls::{
    ClientConfig, ClientConnection, StreamOwned,
    pki_types::{InvalidDnsNameError, ServerName},
};
use thiserror::Error;
use url::Url;

const PROTOCOL: &str = "gemini://";
const DEFAULT_PORT: usize = 1965;

#[derive(Error, Debug)]
pub enum GeminiClientError {
    #[error("Something unexpected happened")]
    UnexpectedError,
    #[error("URL does not contain a host")]
    NoHostError,
    #[error("Request status is not supported")]
    UnsupportedStatusError,
    #[error("MIME type {0} is not supported")]
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

pub struct GeminiClientConnection {
    url: Url,
    stream: StreamOwned<ClientConnection, TcpStream>,
}

pub struct GeminiClient {
    config: Arc<ClientConfig>,
    connection: Option<GeminiClientConnection>,
}

type Result<T, E = GeminiClientError> = core::result::Result<T, E>;

impl GeminiClient {
    pub fn new() -> Self {
        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verification::AllowUnknownIssuerVerification::new())
            .with_no_client_auth();

        Self {
            config: Arc::new(config),
            connection: None,
        }
    }

    pub fn request(&mut self, url: &str) -> Result<String> {
        self.update_connection(url)?;

        if let Some(connection) = &mut self.connection {
            let host = connection
                .url
                .host_str()
                .ok_or(GeminiClientError::UnexpectedError)?;
            let path = connection.url.path();

            // https://geminiprotocol.net/docs/protocol-specification.gmi#requests
            // - Needs trailing `/` otherwise it redirects (status 3X)
            // - Must end with CRLF
            let request = format!("{}{}{}\r\n", PROTOCOL, host, path);
            connection.stream.write_all(request.as_bytes())?;

            let mut header = Vec::new();
            connection.stream.read_until(b'\n', &mut header)?;

            let space_pos = header.iter().position(|&c| c == b' ').unwrap();
            let header = String::from_utf8(header)?;

            // https://geminiprotocol.net/docs/protocol-specification.gmi#responses
            // - {status}{SP}{mimetype|URI-reference|errormsg}{CRLF}{body}
            let (status_str, meta) = header.split_at(space_pos);
            let status = status_str[..1].parse::<u8>()?;

            match status {
                1 | 3 | 6 => Err(GeminiClientError::UnsupportedStatusError),
                2 => {
                    let mime = meta.trim();
                    if !mime.starts_with("text/") {
                        return Err(GeminiClientError::UnsupportedMimeError(mime.to_owned()));
                    }

                    let mut body = String::new();
                    connection.stream.read_to_string(&mut body)?;

                    Ok(body)
                }
                _ => Err(GeminiClientError::RequestError(meta.to_owned())),
            }
        } else {
            Err(GeminiClientError::UnexpectedError)
        }
    }

    fn update_connection(&mut self, url: &str) -> Result<()> {
        let url = Url::parse(url)?;
        let host = url.host_str().ok_or(GeminiClientError::NoHostError)?;
        let stream = self.open_tls_socket(host.to_owned())?;

        self.connection = Some(GeminiClientConnection { url, stream });

        Ok(())
    }

    fn open_tls_socket(&self, host: String) -> Result<StreamOwned<ClientConnection, TcpStream>> {
        let address = format!("{}:{}", host, DEFAULT_PORT);
        let connection = ClientConnection::new(self.config.clone(), ServerName::try_from(host)?)?;
        let socket = TcpStream::connect(address)?;

        Ok(StreamOwned::new(connection, socket))
    }
}

impl Default for GeminiClient {
    fn default() -> Self {
        Self::new()
    }
}
