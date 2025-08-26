use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use gemini_client::GeminiClient;
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{
        Block, Paragraph, ScrollDirection, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Widget, Wrap,
    },
};

const DEFAULT_URL: &str = "gemini://geminiprotocol.net/";
const UPDATE_TICK_RATE: Duration = Duration::from_millis(300);

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

enum Status {
    Running(bool),
    Exit,
}

struct Input {
    value: String,
    index: usize,
    mode: InputMode,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            value: String::from(DEFAULT_URL),
            mode: InputMode::Editing,
            index: DEFAULT_URL.len(),
        }
    }
}

struct App {
    client: GeminiClient,
    body: String,
    scroll: Scroll,
    input: Input,
}

impl App {
    pub fn new(client: GeminiClient) -> App {
        App {
            client,
            body: String::new(),
            scroll: Scroll::default(),
            input: Input::default(),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut last_tick = Instant::now();

        self.render(terminal)?;

        while let Status::Running(should_render) =
            self.handle_events(UPDATE_TICK_RATE.saturating_sub(last_tick.elapsed()))?
        {
            if should_render {
                self.render(terminal)?;
            }

            if last_tick.elapsed() >= UPDATE_TICK_RATE {
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn render(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal.draw(|frame| self.draw_ui(frame))?;
        Ok(())
    }

    fn draw_ui(&mut self, frame: &mut Frame) {
        let [top, bottom] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(frame.area());

        self.draw_address_bar(frame, top);
        self.draw_body(frame.buffer_mut(), bottom);
    }

    fn draw_address_bar(&mut self, frame: &mut Frame, area: Rect) {
        let title = Line::from(" dioscuri ".blue().bold());
        let block = Block::bordered().title(title);
        let url = Text::from(self.input.value.clone());

        let address_bar = if self.input.mode == InputMode::Editing {
            Paragraph::new(url).block(block).blue()
        } else {
            Paragraph::new(url).block(block)
        };

        address_bar.render(area, frame.buffer_mut());

        if self.input.mode == InputMode::Editing {
            frame.set_cursor_position(Position::new(
                area.x + self.input.index as u16 + 1,
                area.y + 1,
            ));
        }
    }

    fn draw_body(&mut self, buffer: &mut Buffer, area: Rect) {
        let instructions = if self.input.mode == InputMode::Normal {
            " <SLASH> - Edit the address "
        } else {
            " <ENTER> - Request address | <ESC> - Focus the body "
        };
        let instructions = Line::from(instructions.bold()).alignment(Alignment::Right);

        let block = if self.input.mode == InputMode::Normal {
            Block::bordered()
                .title_bottom(instructions)
                .border_style(Style::new().blue())
        } else {
            Block::bordered().title_bottom(instructions)
        };

        let paragraph = Paragraph::new(self.body.replace("\t", " "))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll.value as u16, 0));

        let lines = paragraph.line_count(area.width - 2);
        let height = (area.height - 2) as usize;

        let pages = lines / height;
        let reminder = lines % height;

        self.scroll.max = (height * pages.saturating_sub(1)) + if pages > 0 { reminder } else { 0 };
        self.scroll.state = self.scroll.state.content_length(self.scroll.max);

        paragraph.render(area, buffer);

        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(
            area,
            buffer,
            &mut self.scroll.state,
        );
    }

    fn handle_events(&mut self, timeout: Duration) -> Result<Status> {
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    match self.input.mode {
                        InputMode::Normal => match key_event.code {
                            KeyCode::Up => self.scroll_up(),
                            KeyCode::Down => self.scroll_down(),
                            KeyCode::Char('/') => self.enter_editing_mode(),
                            KeyCode::Esc => return Ok(Status::Exit),
                            _ => return Ok(Status::Running(false)),
                        },
                        InputMode::Editing => match key_event.code {
                            KeyCode::Enter => self.request_url()?,
                            KeyCode::Char(char) => self.enter_char(char),
                            KeyCode::Backspace => self.delete_char(),
                            KeyCode::Left => self.move_cursor_left(),
                            KeyCode::Right => self.move_cursor_right(),
                            KeyCode::Esc => self.exit_editing_mode(),
                            _ => return Ok(Status::Running(false)),
                        },
                    };
                    return Ok(Status::Running(true));
                }
                Event::Resize(_, _) => return Ok(Status::Running(true)),
                _ => return Ok(Status::Running(false)),
            }
        }

        Ok(Status::Running(false))
    }

    fn scroll_up(&mut self) {
        if self.scroll.value > 0 {
            self.scroll.state.scroll(ScrollDirection::Backward);
            self.scroll.value = self.scroll.value.saturating_sub(1);
        }
    }

    fn scroll_down(&mut self) {
        if self.scroll.value < self.scroll.max {
            self.scroll.state.scroll(ScrollDirection::Forward);
            self.scroll.value = self.scroll.value.saturating_add(1);
        }
    }

    fn exit_editing_mode(&mut self) {
        if !self.body.is_empty() {
            self.input.mode = InputMode::Normal;
        }
    }

    fn enter_editing_mode(&mut self) {
        self.input.mode = InputMode::Editing;
        self.reset_cursor();
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.input.index.saturating_sub(1);
        self.input.index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.input.index.saturating_add(1);
        self.input.index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, char: char) {
        let index = self.byte_index();
        self.input.value.insert(index, char);
        self.move_cursor_right();
    }

    fn byte_index(&self) -> usize {
        self.input
            .value
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.input.index)
            .unwrap_or(self.input.value.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.input.index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.input.index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self.input.value.chars().take(from_left_to_current_index);
            let after_char_to_delete = self.input.value.chars().skip(current_index);

            self.input.value = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.value.len())
    }

    fn reset_cursor(&mut self) {
        self.input.index = self.input.value.len();
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
