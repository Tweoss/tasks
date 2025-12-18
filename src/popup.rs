use chrono::Local;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Flex, Layout, Rect},
    text::Text,
    widgets::{Block, Clear},
};
use serde::{Deserialize, Serialize};
use tui_textarea::TextArea;

use crate::{FocusState, Task};

pub trait Popup {
    const TITLE: &str;
    type Action;
    fn draw_in_rect(&self, frame: &mut Frame, area: Rect);
    fn get_dimensions(&self, available_area: Rect) -> (u16, u16);

    fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered().title(Self::TITLE);

        let percent_x = 50;
        let percent_y = 30;
        let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        let (width, height) = self.get_dimensions(block.inner(area));
        frame.render_widget(Clear, area); //this clears out the background
        frame.render_widget(block, area);
        let [area] = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        self.draw_in_rect(frame, area);
    }

    fn handle_key(&mut self, key_event: KeyEvent) -> Self::Action;
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SaveDialog {}
pub enum SaveAction {
    Unhandled,
    ExitNoWrite,
    Write,
    Exit,
}

const POPUP_TEXT: &str = "write(w), write and exit(y), exit(n)\nESC to cancel";

impl Popup for SaveDialog {
    const TITLE: &str = "Exit Popup";
    type Action = SaveAction;

    fn draw_in_rect(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Text::raw("write(w), write and exit(y), exit(n)\nESC to cancel"),
            area,
        );
    }

    fn get_dimensions(&self, _: Rect) -> (u16, u16) {
        (
            POPUP_TEXT.lines().map(|l| l.chars().count()).max().unwrap() as u16,
            POPUP_TEXT.lines().count() as u16,
        )
    }

    fn handle_key(&mut self, key_event: KeyEvent) -> SaveAction {
        match key_event.code {
            KeyCode::Char('y') => SaveAction::Exit,
            KeyCode::Char('n') => SaveAction::ExitNoWrite,
            KeyCode::Char('w') => SaveAction::Write,
            _ => SaveAction::Unhandled,
        }
    }
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct AddDialog<'a> {
    #[serde(skip)]
    textbox: Box<TextArea<'a>>,
}
pub enum AddAction {
    Exit,
    Add(Task),
}

impl Popup for AddDialog<'_> {
    const TITLE: &'static str = "Add New Task";
    type Action = Option<AddAction>;

    fn draw_in_rect(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self.textbox.as_ref(), area);
    }

    fn get_dimensions(&self, available_area: Rect) -> (u16, u16) {
        (available_area.width, available_area.height)
    }

    fn handle_key(&mut self, key_event: KeyEvent) -> Self::Action {
        match key_event.code {
            KeyCode::Enter => self.textbox.lines().first().map(|title| {
                AddAction::Add(Task::new(
                    title.trim().to_string(),
                    Local::now().naive_local(),
                    vec![],
                    String::new().into(),
                    None,
                ))
            }),
            KeyCode::Esc => Some(AddAction::Exit),
            _ => {
                self.textbox.input(key_event);
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ErrorDialog<'a> {
    pub error: String,
    pub previous_state: Option<Box<FocusState<'a>>>,
}

pub enum ErrorAction {
    Okay,
}

impl<'a> Popup for ErrorDialog<'a> {
    const TITLE: &'static str = "Error Popup";
    type Action = ErrorAction;

    fn draw_in_rect(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::raw(self.error.clone()), area);
    }

    fn get_dimensions(&self, _: Rect) -> (u16, u16) {
        (
            self.error.lines().map(|l| l.chars().count()).max().unwrap() as u16,
            self.error.lines().count() as u16,
        )
    }

    fn handle_key(&mut self, _: KeyEvent) -> ErrorAction {
        ErrorAction::Okay
    }
}

const MAX_ERROR_WIDTH: usize = 80;

impl<'a> ErrorDialog<'a> {
    pub fn from_error_focus(error: &eyre::Report, focus: FocusState<'a>) -> Self {
        Self {
            error: textwrap::fill(&format!("{:?}", error), MAX_ERROR_WIDTH),
            previous_state: Some(Box::new(focus)),
        }
    }
}
