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
        Block, Paragraph, ScrollDirection, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Widget, Wrap,
    },
};

use dioscuri::SkipServerVerification;

const PROTOCOL: &str = "gemini://";
const HOST: &str = "geminiprotocol.net";
const PORT: usize = 1965;
const PATH: &str = "/news/";

const TICK_RATE: Duration = Duration::from_millis(300);

#[derive(Default)]
struct Scroll {
    value: usize,
    max: usize,
    state: ScrollbarState,
}

#[derive(Default)]
struct App {
    request: String,
    body: String,
    scroll: Scroll,
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
        terminal.draw(|frame| self.draw(frame))?;

        let mut last_tick = Instant::now();

        while !self.exit {
            let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());

            if self.handle_events(timeout)? {
                terminal.draw(|frame| self.draw(frame))?;
            }

            if last_tick.elapsed() >= TICK_RATE {
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self, timeout: Duration) -> Result<bool> {
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    match key_event.code {
                        KeyCode::Esc => {
                            self.exit = true;
                        }
                        KeyCode::Down => {
                            if self.scroll.value < self.scroll.max {
                                self.scroll.state.scroll(ScrollDirection::Forward);
                                self.scroll.value = self.scroll.value.saturating_add(1);
                                return Ok(true);
                            }
                        }
                        KeyCode::Up => {
                            if self.scroll.value > 0 {
                                self.scroll.state.scroll(ScrollDirection::Backward);
                                self.scroll.value = self.scroll.value.saturating_sub(1);
                                return Ok(true);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => return Ok(true),
                _ => {}
            }
        }

        Ok(false)
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let app_title = Line::from(" dioscuri ".blue().bold());
        let request_block = Block::bordered().title(app_title);
        let request = Text::from(self.request.clone());

        let body_block = Block::bordered();

        let [top, bottom] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        Paragraph::new(request)
            .block(request_block)
            .render(top, buf);

        let body_paragraph = Paragraph::new(self.body.replace("\t", " "))
            .block(body_block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll.value as u16, 0));

        let body_lines = body_paragraph.line_count(bottom.width - 2);
        let body_height = (bottom.height - 2) as usize;

        let pages = body_lines / body_height;
        let reminder_lines = body_lines % body_height;

        self.scroll.max =
            (body_height * pages.saturating_sub(1)) + if pages > 0 { reminder_lines } else { 0 };

        self.scroll.state = self.scroll.state.content_length(self.scroll.max);

        body_paragraph.render(bottom, buf);

        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(
            bottom,
            buf,
            &mut self.scroll.state,
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
