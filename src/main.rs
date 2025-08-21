use std::{
    io::{BufRead, Read, Write},
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::{Result, eyre::Ok};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Stylize,
    text::{Line, Text},
    widgets::{
        Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget,
        Wrap,
    },
};

use dioscuri::SkipServerVerification;

const PROTOCOL: &str = "gemini://";
const HOST: &str = "geminiprotocol.net";
const PORT: usize = 1965;
const PATH: &str = "/news/";

#[derive(Default)]
struct App {
    request: String,
    body: String,
    vertical_scroll_state: ScrollbarState,
    vertical_scroll: usize,
    exit: bool,
}

impl App {
    pub fn new(request: String, body: String) -> App {
        App {
            request,
            body,
            ..Default::default()
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();

        while !self.exit {
            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            self.handle_events(timeout)?;
            if last_tick.elapsed() >= tick_rate {
                terminal.draw(|frame| self.draw(frame))?;
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self, timeout: Duration) -> Result<()> {
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    match key_event.code {
                        KeyCode::Esc => {
                            self.exit = true;
                        }
                        KeyCode::Down => {
                            self.vertical_scroll = self.vertical_scroll.saturating_add(1);
                            self.vertical_scroll_state =
                                self.vertical_scroll_state.position(self.vertical_scroll);
                        }
                        KeyCode::Up => {
                            self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
                            self.vertical_scroll_state =
                                self.vertical_scroll_state.position(self.vertical_scroll);
                        }
                        _ => {}
                    }
                }
                Event::Mouse(_) => {}
                _ => {}
            }
        }

        Ok(())
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let app_title = Line::from(" dioscuri ".blue().bold());
        let request_block = Block::bordered().title(app_title);
        let request = Text::from(self.request.clone());

        let body_block = Block::bordered();
        let body_lines: Vec<Line> = self
            .body
            .lines()
            .map(|l| Line::from(l.replace("\t", " ")))
            .collect();

        let [top, bottom] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        self.vertical_scroll_state = self.vertical_scroll_state.content_length(body_lines.len());

        Paragraph::new(request)
            .block(request_block)
            .render(top, buf);

        Paragraph::new(body_lines)
            .block(body_block)
            .wrap(Wrap { trim: false })
            .scroll((self.vertical_scroll as u16, 0))
            .render(bottom, buf);

        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(
            bottom,
            buf,
            &mut self.vertical_scroll_state,
        );
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

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

            let mut terminal = ratatui::init();
            App::new(request, body).run(&mut terminal)?;
            ratatui::restore();
        }
        _ => println!("Error: {}", meta),
    }

    Ok(())
}
