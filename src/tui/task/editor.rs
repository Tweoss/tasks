use std::io;

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
    storage::{Task, keyboard_edit::KeyboardEditable},
    tui::task::scrollbar::ScrollbarWidget,
};

pub struct EditorTui {
    view_offset: usize,
}

pub enum Action {
    Unhandled,
}

impl EditorTui {
    pub fn new() -> Self {
        Self { view_offset: 0 }
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
            KeyCode::Char('j') if ctrl => self.scroll_down(task.editable().inner().line_len() + 1),
            KeyCode::Char('k') if ctrl => self.scroll_up(),
            _ => {
                let mut editable = task.editable_mut();
                let op = KeyboardEditable::map_key_event(key_event);
                if let Some(op) = op {
                    editable.apply_text_op(op);
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

    pub fn set_text(&mut self, _text: &str) {
        self.view_offset = 0;
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
    pub text: &'a KeyboardEditable,
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
        // Scroll the cursor into view.
        let cursor = self.text.cursor();
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
            .inner()
            .raw_lines()
            .skip(self.editor.view_offset)
            .take(height);
        for (y, l) in visible_lines.enumerate() {
            let rope_slice = l.to_string();
            let mut l = rope_slice.as_str();
            let mut x_offset = 0;
            let y = text_area.y + y as u16;
            while x_offset < width {
                // Style spaces as dark gray.
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
            total_lines: self.text.inner().line_len(),
        }
        .render(scroll_area, buf);
    }
}
