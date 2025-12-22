use crop::Rope;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout},
    style::Style,
    widgets::Widget,
};

use crate::tui::task::scrollbar::ScrollbarWidget;

pub struct EditorTui {
    view_offset: usize,
    last_line_count: Option<usize>,
}

pub enum Action {
    Unhandled,
}

impl EditorTui {
    pub fn new() -> Self {
        Self {
            view_offset: 0,
            last_line_count: None,
        }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        focus: &mut EditorFocus,
    ) -> Option<Action> {
        match focus {
            EditorFocus::Unlocked => {
                if key_event.code == KeyCode::Enter {
                    *focus = EditorFocus::Locked;
                } else {
                    return Some(Action::Unhandled);
                }
            }
            EditorFocus::Locked => match key_event.code {
                KeyCode::Esc => *focus = EditorFocus::Unlocked,
                KeyCode::Up => self.scroll_up(),
                KeyCode::Down => self.scroll_down(),
                _ => return Some(Action::Unhandled),
            },
        }
        None
    }

    fn scroll_up(&mut self) {
        self.view_offset = self.view_offset.saturating_sub(1);
    }
    fn scroll_down(&mut self) {
        let Some(lines) = self.last_line_count else {
            return;
        };
        // Weird math to avoid panic.
        self.view_offset = (self.view_offset + 2).min(lines).saturating_sub(1)
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

        let height = text_area.height as usize;
        if self.switched_text {
            // Scroll the bottommost text into view.
            self.editor.view_offset = self.text.line_len().max(height) - height;
        }
        self.editor.last_line_count = Some(self.text.line_len());

        let visible_lines = self
            .text
            .raw_lines()
            .skip(self.editor.view_offset)
            .take(height);
        for (y, l) in visible_lines.enumerate() {
            buf.set_string(
                text_area.x,
                text_area.y + y as u16,
                l.to_string(),
                Style::new(),
            );
        }

        ScrollbarWidget {
            view_offset: self.editor.view_offset,
            total_lines: self.text.line_len(),
        }
        .render(scroll_area, buf);
    }
}
