use std::io;

use chumsky::text::Char;
use crop::Rope;
use ratatui::{
    crossterm::{
        cursor::SetCursorStyle,
        event::{KeyCode, KeyEvent, KeyModifiers},
    },
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::Widget,
};

use crate::{
    storage::{Task, editing::Pos},
    tui::task::scrollbar::ScrollbarWidget,
};

pub struct EditorTui {
    view_offset: usize,
    last_state: BufferState,
}

struct BufferState {
    cursor: Pos,
}

pub enum Action {
    Unhandled,
}

impl EditorTui {
    pub fn new() -> Self {
        Self {
            view_offset: 0,
            last_state: BufferState {
                cursor: (0, 0).into(),
            },
        }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        focus: &mut EditorFocus,
        task: Option<&mut Task>,
    ) -> Option<Action> {
        match focus {
            EditorFocus::Unlocked => match key_event.code {
                KeyCode::Enter => {
                    *focus = EditorFocus::Locked;
                    return None;
                }
                _ => {
                    return Some(Action::Unhandled);
                }
            },
            EditorFocus::Locked => {}
        }

        assert!(matches!(focus, EditorFocus::Locked));

        let Some(task) = task else {
            return Some(Action::Unhandled);
        };

        let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);
        match key_event.code {
            KeyCode::Esc => *focus = EditorFocus::Unlocked,
            KeyCode::Char('j') if ctrl => self.scroll_down(task.context().line_len() + 1),
            KeyCode::Char('k') if ctrl => self.scroll_up(),
            _ => {
                if let Some(new_pos) = task.handle_key_event(self.last_state.cursor, key_event) {
                    self.last_state.cursor = new_pos;
                }
            }
        }
        None
    }

    fn scroll_up(&mut self) {
        self.view_offset = self.view_offset.saturating_sub(1);
    }
    fn scroll_down(&mut self, line_count: usize) {
        // Weird math to avoid panic.
        self.view_offset = (self.view_offset + 2).min(line_count).saturating_sub(1)
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum EditorFocus {
    #[default]
    Unlocked,
    Locked,
}

pub struct EditorWidget<'a> {
    pub editor: &'a mut EditorTui,
    pub text: &'a Rope,
    pub switched_text: bool,
    pub cursor_buf_pos: &'a mut Option<(u16, u16)>,
    pub focus: Option<EditorFocus>,
}

impl Widget for EditorWidget<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let layout = Layout::horizontal([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);
        let (scroll_area, text_area) = (layout[0], layout[2]);

        let width = text_area.width as usize;
        let height = text_area.height as usize;
        if self.switched_text {
            let line_count = self.text.line_len();
            // Put cursor at end of task.
            self.editor.last_state = BufferState {
                cursor: (
                    line_count.saturating_sub(1),
                    self.text
                        .lines()
                        .next_back()
                        .map(|l| l.chars().count())
                        .unwrap_or(0),
                )
                    .into(),
            };
        }
        // Scroll the cursor into view.
        let cursor = self.editor.last_state.cursor;
        if cursor.line < self.editor.view_offset {
            self.editor.view_offset = cursor.line;
        }
        if cursor.line >= self.editor.view_offset + height {
            self.editor.view_offset += 1 + cursor.line - self.editor.view_offset - height;
        }
        if let Some(EditorFocus::Locked) = self.focus {
            *self.cursor_buf_pos = Some((
                (text_area.x as usize + cursor.column) as u16,
                (text_area.y as usize + cursor.line - self.editor.view_offset) as u16,
            ));
            if let Err(e) = ratatui::crossterm::execute!(io::stdout(), SetCursorStyle::SteadyBar) {
                log::error!("failed to set cursor style {e}");
            }
        }

        let visible_lines = self
            .text
            .raw_lines()
            .skip(self.editor.view_offset)
            .take(height);
        for (y, l) in visible_lines.enumerate() {
            let rope_slice = l.to_string();
            let mut l = rope_slice.as_str();
            let mut x_offset = 0;
            let y = text_area.y + y as u16;
            while x_offset < width {
                let space_style = Style::new().fg(Color::DarkGray);
                let x = (text_area.x as usize + x_offset) as u16;
                let Some(first_char) = l.chars().next() else {
                    break;
                };
                if first_char.is_whitespace() {
                    if let Some(i) = l.find(|c: char| !c.is_whitespace()) {
                        let (whitespace, rest) = l.split_at(i);
                        l = rest;
                        buf.set_string(x, y, whitespace.replace(" ", "·"), space_style);
                        x_offset += whitespace.chars().count();
                    } else {
                        buf.set_string(x, y, l.replace(" ", "·"), space_style);
                        break;
                    }
                } else if let Some(i) = l.find(|c: char| c.is_whitespace()) {
                    let (chars, rest) = l.split_at(i);
                    l = rest;
                    buf.set_string(x, y, chars, Style::new());
                    x_offset += chars.chars().count();
                } else {
                    buf.set_string(x, y, l, Style::new());
                    break;
                }
            }
        }

        ScrollbarWidget {
            view_offset: self.editor.view_offset,
            total_lines: self.text.line_len(),
        }
        .render(scroll_area, buf);
    }
}
