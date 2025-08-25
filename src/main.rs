use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use gemini_client::GeminiClient;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Position},
    style::Stylize,
    text::{Line, Text},
    widgets::{
        Block, Paragraph, ScrollDirection, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Widget, Wrap,
    },
};

const DEFAULT_URL: &str = "gemini://geminiprotocol.net/";
const TICK_RATE: Duration = Duration::from_millis(300);

#[derive(Default)]
struct Scroll {
    value: usize,
    max: usize,
    state: ScrollbarState,
}

#[derive(PartialEq, Eq)]
enum InputMode {
    Normal,
    Editing,
}

struct Input {
    value: String,
    character_index: usize,
    mode: InputMode,
}

struct App {
    client: GeminiClient,
    body: String,
    scroll: Scroll,
    input: Input,
    exit: bool,
}

impl App {
    pub fn new(client: GeminiClient) -> App {
        App {
            client,
            body: String::new(),
            scroll: Scroll::default(),
            input: Input {
                value: String::from(DEFAULT_URL),
                mode: InputMode::Editing,
                character_index: DEFAULT_URL.len(),
            },
            exit: false,
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
        let app_title = Line::from(" dioscuri ".blue().bold());
        let url_block = Block::bordered().title(app_title);
        let url = Text::from(self.input.value.clone());

        let body_block = Block::bordered();

        let [top, bottom] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(frame.area());

        let address_bar = Paragraph::new(url).block(url_block);

        if self.input.mode == InputMode::Editing {
            address_bar.blue().render(top, frame.buffer_mut());
            frame.set_cursor_position(Position::new(
                top.x + self.input.character_index as u16 + 1,
                top.y + 1,
            ));
        } else {
            address_bar.render(top, frame.buffer_mut());
        }

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

        if self.input.mode == InputMode::Normal {
            body_paragraph.blue().render(bottom, frame.buffer_mut());
        } else {
            body_paragraph.render(bottom, frame.buffer_mut());
        }

        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(
            bottom,
            frame.buffer_mut(),
            &mut self.scroll.state,
        );
    }

    fn handle_events(&mut self, timeout: Duration) -> Result<bool> {
        if event::poll(timeout)? {
            match self.input.mode {
                InputMode::Normal => match event::read()? {
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
                            KeyCode::Char('/') => {
                                self.input.mode = InputMode::Editing;
                                self.input.character_index = self.input.value.len();
                                return Ok(true);
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(_, _) => return Ok(true),
                    _ => {}
                },
                InputMode::Editing => match event::read()? {
                    Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                        match key_event.code {
                            KeyCode::Enter => {
                                self.request_url()?;
                            }
                            KeyCode::Char(to_insert) => self.enter_char(to_insert),
                            KeyCode::Backspace => self.delete_char(),
                            KeyCode::Left => self.move_cursor_left(),
                            KeyCode::Right => self.move_cursor_right(),
                            KeyCode::Esc => {
                                if self.body.is_empty() {
                                    self.exit = true;
                                } else {
                                    self.input.mode = InputMode::Normal;
                                }
                            }
                            _ => {
                                return Ok(false);
                            }
                        };

                        return Ok(true);
                    }
                    Event::Resize(_, _) => return Ok(true),
                    _ => {}
                },
            }
        }

        Ok(false)
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.input.character_index.saturating_sub(1);
        self.input.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.input.character_index.saturating_add(1);
        self.input.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.value.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can be contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_index(&self) -> usize {
        self.input
            .value
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.input.character_index)
            .unwrap_or(self.input.value.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.input.character_index != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.input.character_index;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.value.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.value.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input.value = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.value.len())
    }

    fn reset_cursor(&mut self) {
        self.input.character_index = 0;
    }

    fn request_url(&mut self) -> Result<()> {
        self.body = self.client.request(&self.input.value)?;
        self.reset_cursor();
        self.input.mode = InputMode::Normal;

        Ok(())
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let gemini_client = GeminiClient::new();
    let mut terminal = ratatui::init();

    let mut app = App::new(gemini_client);
    let result = app.run(&mut terminal);

    ratatui::restore();

    result
}
