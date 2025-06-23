use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Flex, Layout, Rect},
    text::Text,
    widgets::{Block, Clear},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Popup {}
pub enum PopupAction {
    Unhandled,
    Handled,
    ExitNoWrite,
    Write,
    Exit,
}

impl Popup {
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered().title("Exit Popup");

        let percent_x = 60;
        let percent_y = 20;
        let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        frame.render_widget(Clear, area); //this clears out the background
        frame.render_widget(block, area);
        let text = Text::raw("write(w), write and exit(y), exit(n)\nESC to cancel");
        let [area] = Layout::horizontal([Constraint::Length(text.width() as u16)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([Constraint::Length(text.height() as u16)])
            .flex(Flex::Center)
            .areas(area);
        frame.render_widget(text, area);
    }

    pub fn handle_key(&mut self, key_event: KeyEvent) -> PopupAction {
        match key_event.code {
            KeyCode::Char('y') => PopupAction::Exit,
            KeyCode::Char('n') => PopupAction::ExitNoWrite,
            KeyCode::Char('w') => PopupAction::Write,
            _ => PopupAction::Unhandled,
        }
    }
}
