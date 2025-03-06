use std::{
    error::Error,
    io::{BufRead, Read, Write},
    net::TcpStream,
    sync::Arc,
};

use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb,
};

use dioscuri::SkipServerVerification;

const PROTOCOL: &str = "gemini://";
const HOST: &str = "geminiprotocol.net";
const PORT: usize = 1965;
const PATH: &str = "/";

struct BrowserWindow {
    request: SharedString,
    content: SharedString,
}

impl Render for BrowserWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .text_base()
            .text_color(rgb(0xf7f7f7))
            .bg(rgb(0x212121))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px(px(20.0))
                    .py(px(5.0))
                    .border_b(px(1.))
                    // https://meyerweb.com/eric/tools/color-blend/#131618:F7F7F7:7:hex
                    .border_color(rgb(0x303234))
                    .bg(rgb(0x131618))
                    .child(format!("dioscuri | {}", self.request.trim()))
                    .child(
                        div()
                            .child("X")
                            .hover(|sr| sr.opacity(0.5).cursor_pointer())
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.quit()),
                    ),
            )
            .child(
                div()
                    .p(px(20.))
                    .max_w(px(1200.))
                    .child(self.content.to_owned()),
            )
    }
}

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

            // TODO: Handle body formatting with gpui
            let mut body = String::new();
            tls.read_to_string(&mut body)?;

            println!("Opening the browser window...");
            Application::new().run(|cx: &mut App| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Maximized(Bounds::maximized(None, cx))),
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|_| BrowserWindow {
                            content: body.into(),
                            request: request.into(),
                        })
                    },
                )
                .unwrap();
            });
        }
        _ => println!("Error: {}", meta),
    }

    Ok(())
}
