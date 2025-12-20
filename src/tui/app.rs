use std::cell::RefMut;

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout},
    widgets::Widget,
};

use crate::{
    FocusState, PopupEnum,
    filter::FilteredData,
    popup::{ErrorDialog, SaveDialog},
    tui::{
        popup::{self, PopupTui, PopupWidget},
        table::{TableTui, TableWidget},
        task::{TaskTui, TaskWidget},
    },
};

pub struct AppTui<'a> {
    focus: FocusState<'a>,
    table: TableTui,
    task: TaskTui,
    popup: PopupTui,
}

pub enum Action {
    Exit,
    Unhandled,
}

impl AppTui<'_> {
    pub fn new() -> Self {
        Self {
            focus: FocusState::List,
            table: TableTui::new(),
            task: TaskTui::new(),
            popup: PopupTui::new(),
        }
    }

    pub fn set_error_focus(&mut self, error: eyre::Report) {
        self.focus = FocusState::Popup(PopupEnum::Error(ErrorDialog::from_error_focus(
            &error,
            self.focus.clone(),
        )));
    }

    pub fn handle_key_event(
        &mut self,
        data: &mut FilteredData,
        key_event: KeyEvent,
    ) -> Option<Action> {
        match &self.focus {
            FocusState::List => match self.table.handle_key_event(data, key_event)? {
                super::table::Action::Add => {
                    self.focus = FocusState::Popup(PopupEnum::AddNew(Default::default()))
                }
                super::table::Action::Unhandled => {
                    if let KeyCode::Char('q') = key_event.code {
                        self.focus = FocusState::Popup(PopupEnum::WritePopup(SaveDialog {}))
                    }
                }
            },
            FocusState::Task(task_focus) => todo!(),
            FocusState::Popup(_) => {
                match self
                    .popup
                    .handle_key_event(&mut self.focus, data, key_event)?
                {
                    popup::Action::Exit => return Some(Action::Exit),
                    popup::Action::Unhandled => return Some(Action::Unhandled),
                }
            }
        }
        None
    }
}

impl Default for AppTui<'_> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AppWidget<'a, 'b>(pub RefMut<'a, AppTui<'b>>, pub &'a FilteredData);

impl Widget for AppWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let AppWidget(mut app, data) = self;
        // In future, can match on focus to change layout.
        let task_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(area);
        TableWidget(&mut app.table, data).render(task_split[0], buf);
        TaskWidget(&app.task, data, app.table.selected()).render(task_split[1], buf);

        match app.focus.clone() {
            FocusState::List => {}
            FocusState::Task(task_focus) => {
                todo!()
            }
            FocusState::Popup(p) => PopupWidget(&p).render(area, buf),
        }
    }
}
