use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    style::Style,
    widgets::Widget,
};
use tui_textarea::TextArea;

pub struct FilterTui<'a> {
    textbox: TextArea<'a>,
    reset_cursor_style: Style,
}

pub enum Action {
    Exit,
    Updated(String),
}

impl FilterTui<'_> {
    pub fn new() -> Self {
        let textbox = TextArea::new(vec![]);
        Self {
            reset_cursor_style: textbox.cursor_style(),
            textbox,
        }
    }
    pub fn handle_key(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            KeyCode::Enter => Some(Action::Updated(
                self.textbox.lines().first().cloned().unwrap_or_default(),
            )),
            KeyCode::Esc => Some(Action::Exit),
            _ => {
                self.textbox.input(key_event);
                None
            }
        }
    }
}

pub struct FilterWidget<'a, 'b> {
    pub filter: &'a mut FilterTui<'b>,
    pub is_focused: bool,
}

impl Widget for FilterWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        if self.is_focused {
            self.filter
                .textbox
                .set_cursor_style(self.filter.reset_cursor_style);
        } else {
            self.filter
                .textbox
                .set_cursor_style(self.filter.textbox.cursor_line_style());
        }
        self.filter.textbox.render(area, buf);
    }
}
