use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    widgets::Widget,
};

use crate::{
    FocusState, PopupEnum,
    filter::FilteredData,
    tui::popup::dialog::{AddAction, ErrorAction, ErrorDialog, Popup, SaveAction},
};

pub struct PopupTui {}

pub enum Action {
    Unhandled,
    Exit,
}

impl PopupTui {
    pub fn new() -> Self {
        Self {}
    }

    pub fn handle_key_event<'a>(
        &mut self,
        focus: &mut FocusState<'a>,
        data: &mut FilteredData,
        key_event: KeyEvent,
    ) -> Option<Action> {
        let FocusState::Popup {
            popup: p,
            last_focus,
        } = focus
        else {
            return None;
        };
        use AddAction as AA;
        use SaveAction as SA;
        match p {
            PopupEnum::WritePopup(save) => match (save.handle_key(key_event), key_event.code) {
                (SA::ExitNoWrite, _) => return Some(Action::Exit),
                (SA::Write, _) => {
                    if let Err(e) = data.write_dirty() {
                        *focus = FocusState::Popup {
                            popup: PopupEnum::Error(ErrorDialog::from_error_focus(&e)),
                            last_focus: focus.clone().into(),
                        };
                    } else {
                        *focus = *last_focus.clone();
                    }
                }
                (SA::Exit, _) => {
                    if let Err(e) = data.write_dirty() {
                        *focus = FocusState::Popup {
                            popup: PopupEnum::Error(ErrorDialog::from_error_focus(&e)),
                            last_focus: focus.clone().into(),
                        };
                    } else {
                        return Some(Action::Exit);
                    }
                }
                (SA::Unhandled, KeyCode::Esc) => *focus = *last_focus.clone(),
                (SA::Unhandled, _) => return Some(Action::Unhandled),
            },
            PopupEnum::AddNew(add) => match (add.handle_key(key_event), key_event.code) {
                (Some(AA::Exit), _) => *focus = *last_focus.clone(),
                (Some(AA::Add(t)), _) => {
                    data.push(*t);
                    *focus = *last_focus.clone();
                }
                (None, _) => {}
            },
            PopupEnum::Error(error) => match error.handle_key(key_event) {
                ErrorAction::Okay => *focus = *last_focus.clone(),
            },
        }
        None
    }
}

pub struct PopupWidget<'a>(pub &'a PopupEnum<'a>);

impl Widget for PopupWidget<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        match self.0 {
            PopupEnum::WritePopup(d) => d.render(area, buf),
            PopupEnum::AddNew(d) => d.render(area, buf),
            PopupEnum::Error(d) => d.render(area, buf),
        }
    }
}

pub mod dialog {
    use std::collections::HashSet;

    use chrono::Local;
    use ratatui::{
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent},
        layout::{Constraint, Flex, Layout, Rect},
        text::Text,
        widgets::{Block, Clear, Widget},
    };
    use tui_textarea::TextArea;

    use crate::storage::Task;

    pub trait Popup {
        const TITLE: &str;
        type Action;
        fn draw_in_rect(&self, area: Rect, buf: &mut Buffer);
        fn get_dimensions(&self, available_area: Rect) -> (u16, u16);
        fn handle_key(&mut self, key_event: KeyEvent) -> Self::Action;
    }
    fn render<T: Popup>(v: &T, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered().title(T::TITLE);

        let percent_x = 50;
        let percent_y = 30;
        let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        let (width, height) = v.get_dimensions(block.inner(area));
        Clear.render(area, buf);
        block.render(area, buf);
        let [area] = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        v.draw_in_rect(area, buf);
    }

    #[derive(Clone, Debug, Default)]
    pub struct SaveDialog {}
    pub enum SaveAction {
        Unhandled,
        ExitNoWrite,
        Write,
        Exit,
    }
    const POPUP_TEXT: &str = "write(,), write and exit(q), exit(Q)\nESC to cancel";
    impl Popup for SaveDialog {
        const TITLE: &str = "Exit Popup";
        type Action = SaveAction;

        fn draw_in_rect(&self, area: Rect, buf: &mut Buffer) {
            Text::raw(POPUP_TEXT).render(area, buf);
        }

        fn get_dimensions(&self, _: Rect) -> (u16, u16) {
            (
                POPUP_TEXT.lines().map(|l| l.chars().count()).max().unwrap() as u16,
                POPUP_TEXT.lines().count() as u16,
            )
        }

        fn handle_key(&mut self, key_event: KeyEvent) -> SaveAction {
            match key_event.code {
                KeyCode::Char('Q') => SaveAction::ExitNoWrite,
                KeyCode::Char('q') => SaveAction::Exit,
                KeyCode::Char(',') => SaveAction::Write,
                _ => SaveAction::Unhandled,
            }
        }
    }
    impl Widget for &SaveDialog {
        fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
            render(self, area, buf)
        }
    }

    #[derive(Clone, Default, Debug)]
    pub struct AddDialog<'a> {
        textbox: Box<TextArea<'a>>,
    }
    pub enum AddAction {
        Exit,
        Add(Box<Task>),
    }

    impl Popup for AddDialog<'_> {
        const TITLE: &'static str = "Add New Task";
        type Action = Option<AddAction>;

        fn draw_in_rect(&self, area: Rect, buf: &mut Buffer) {
            self.textbox.as_ref().render(area, buf);
        }

        fn get_dimensions(&self, available_area: Rect) -> (u16, u16) {
            (available_area.width, available_area.height)
        }

        fn handle_key(&mut self, key_event: KeyEvent) -> Self::Action {
            match key_event.code {
                KeyCode::Enter => self.textbox.lines().first().map(|title| {
                    AddAction::Add(
                        Task::new(
                            title.trim().to_string(),
                            Local::now().naive_local(),
                            vec![],
                            HashSet::new(),
                            String::new().into(),
                            None,
                        )
                        .into(),
                    )
                }),
                KeyCode::Esc => Some(AddAction::Exit),
                _ => {
                    self.textbox.input(key_event);
                    None
                }
            }
        }
    }
    impl Widget for &AddDialog<'_> {
        fn render(self, area: Rect, buf: &mut Buffer) {
            render(self, area, buf)
        }
    }

    #[derive(Debug, Clone)]
    pub struct ErrorDialog {
        pub error: String,
    }
    pub enum ErrorAction {
        Okay,
    }
    impl Popup for ErrorDialog {
        const TITLE: &'static str = "Error Popup";
        type Action = ErrorAction;

        fn draw_in_rect(&self, area: Rect, buf: &mut Buffer) {
            Text::raw(self.error.clone()).render(area, buf);
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
    impl Widget for &ErrorDialog {
        fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
            render(self, area, buf)
        }
    }

    const MAX_ERROR_WIDTH: usize = 80;

    impl ErrorDialog {
        pub fn from_error_focus(error: &eyre::Report) -> Self {
            Self {
                error: textwrap::fill(&format!("{:?}", error), MAX_ERROR_WIDTH),
            }
        }
    }
}
