use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
};

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
        task::{TaskFocus, TaskTui, TaskWidget},
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
        match &mut self.focus {
            FocusState::List => match self.table.handle_key_event(data, key_event)? {
                super::table::Action::Add => {
                    self.focus = FocusState::Popup(PopupEnum::AddNew(Default::default()))
                }
                super::table::Action::Unhandled => match key_event.code {
                    KeyCode::Char('q') => {
                        self.focus = FocusState::Popup(PopupEnum::WritePopup(SaveDialog {}))
                    }
                    KeyCode::Enter => self.focus = FocusState::Task(TaskFocus::Context),
                    _ => (),
                },
            },
            FocusState::Task(task_focus) => {
                match self.task.handle_key_event(key_event, task_focus)? {
                    super::task::Action::Exit => self.focus = FocusState::List,
                    super::task::Action::Unhandled => {}
                }
            }
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

pub struct AppWidget<'a, 'b>(pub Rc<RefCell<AppTui<'a>>>, pub &'b FilteredData);
// pub struct AppWidget<'a, 'b>(pub RefMut<'a, AppTui<'b>>, pub &'a FilteredData);

impl Widget for AppWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let AppWidget(app, data) = self;
        // In future, can match on focus to change layout.
        let task_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(area);
        {
            let app = app.clone();
            let (mut table, focus) =
                RefMut::map_split(app.borrow_mut(), |a| (&mut a.table, &mut a.focus));
            TableWidget(&mut table, &focus, data).render(task_split[0], buf);
        }
        let app = app.borrow_mut();
        TaskWidget(
            &app.task,
            data,
            app.table.selected(),
            app.focus.clone().as_task(),
        )
        .render(task_split[1], buf);

        match app.focus.clone() {
            FocusState::List => {}
            FocusState::Task(_) => {}
            FocusState::Popup(p) => PopupWidget(&p).render(area, buf),
        }
    }
}
