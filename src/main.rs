use std::{
    error::Error,
    io::{BufRead, Read, Write},
    net::TcpStream,
    sync::Arc,
};

use dioscuri::{SkipServerVerification, browser};

const PROTOCOL: &str = "gemini://";
const HOST: &str = "geminiprotocol.net";
const PORT: usize = 1965;
const PATH: &str = "/docs/faq.gmi";

fn main() -> Result<(), Box<dyn Error>> {
    let client_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();
    let mut connection = rustls::ClientConnection::new(Arc::new(client_config), HOST.try_into()?)?;
    let mut socket = TcpStream::connect(format!("{}:{}", HOST, PORT))?;

    println!("Connected to IP: {}", socket.peer_addr()?.ip());

    // From https://github.com/rustls/rustls/blob/main/examples/src/bin/simpleclient.rs
    let mut tls = rustls::Stream::new(&mut connection, &mut socket);

    // https://geminiprotocol.net/docs/protocol-specification.gmi#requests
    // - Needs trailing `/` otherwise it redirects (status 3X)
    // - Must end with CRLF
    let request = format!("{}{}{}\r\n", PROTOCOL, HOST, PATH);
    println!("Request: {}", request.trim());
    tls.write_all(request.as_bytes())?;

    let mut header = Vec::new();
    tls.read_until(b'\n', &mut header)?;

    let space_pos = header.iter().position(|&c| c == b' ').unwrap();
    let header = String::from_utf8(header)?;

    // https://geminiprotocol.net/docs/protocol-specification.gmi#responses
    // - {status}{SP}{mimetype|URI-reference|errormsg}{CRLF}{body?}
    let (status_str, meta) = header.split_at(space_pos);
    let status = status_str[..1].parse::<u8>()?;
    println!("Response Status: {}X", status);

    match status {
        1 | 3 | 6 => println!("Unsupported feature..."),
        2 => {
            let mime = meta.trim();
            if !mime.starts_with("text/") {
                println!("Unsupported mimetype: {}", mime)
            }

            let mut body = String::new();
            tls.read_to_string(&mut body)?;

            println!("Opening the browser window...");
            browser::run_app(request, body);
        }
        _ => println!("Error: {}", meta),
    }

    Ok(())
}
