use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    rc::Rc,
};

use crossterm::event::KeyCode;
use ratatui::{
    crossterm::event::{KeyEvent, KeyModifiers},
    layout::{Constraint, Layout},
    widgets::Widget,
};
use serde::{Deserialize, Serialize};

use crate::{
    FocusState, PopupEnum,
    filter::FilteredData,
    tui::{
        filter::{FilterTui, FilterWidget},
        popup::{
            self, PopupTui, PopupWidget,
            dialog::{ErrorDialog, SaveDialog},
        },
        table::{TableTui, TableWidget},
        task::{TaskFocus, TaskTui, TaskWidget},
    },
};

pub struct AppTui<'a> {
    focus: FocusState<'a>,
    filter: FilterTui,
    table: TableTui,
    task: TaskTui,
    popup: PopupTui,
    mode: Mode,
    keybinds: HashMap<Mode, HashMap<KeyCode, KeyAction>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum KeyAction {
    SetFilter(String),
}

pub enum Action {
    Exit,
    Unhandled,
}

#[derive(PartialEq, Eq, Hash, Debug, Deserialize, Serialize, Clone)]
pub enum Mode {
    Normal,
    Key(KeyCode),
}

impl AppTui<'_> {
    pub fn new(keybinds: HashMap<Mode, HashMap<KeyCode, KeyAction>>) -> Self {
        Self {
            filter: FilterTui::new(),
            focus: FocusState::List,
            table: TableTui::new(),
            task: TaskTui::new(),
            popup: PopupTui::new(),
            mode: Mode::Normal,
            keybinds,
        }
    }

    pub fn set_table_index(&mut self, index: usize) {
        self.table.set_selected(index);
    }

    pub fn set_error_focus(&mut self, error: eyre::Report) {
        self.focus = FocusState::Popup {
            popup: PopupEnum::Error(ErrorDialog::from_error_focus(&error)),
            last_focus: FocusState::default().into(),
        };
    }

    pub fn handle_key_event(
        &mut self,
        data: &mut FilteredData,
        key_event: KeyEvent,
    ) -> Option<Action> {
        if key_event.code == KeyCode::Char('c')
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Some(Action::Exit);
        }

        match &mut self.focus {
            FocusState::List => match self.table.handle_key_event(data, key_event)? {
                super::table::Action::Add => {
                    self.focus = FocusState::Popup {
                        popup: PopupEnum::AddNew(Default::default()),
                        last_focus: self.focus.clone().into(),
                    }
                }
                super::table::Action::Unhandled => match key_event.code {
                    KeyCode::Char(' ') => {
                        self.focus = FocusState::Popup {
                            popup: PopupEnum::WritePopup(SaveDialog {}),
                            last_focus: self.focus.clone().into(),
                        }
                    }
                    KeyCode::Char('f') => self.focus = FocusState::Filter,
                    KeyCode::Char('t') => self.focus = FocusState::Task(TaskFocus::tags_locked()),
                    KeyCode::Enter => self.focus = FocusState::Task(TaskFocus::context_locked()),
                    KeyCode::Right => self.focus = FocusState::Task(TaskFocus::context_unlocked()),
                    _ => {
                        if let Some(action) = self
                            .keybinds
                            .get(&self.mode)
                            .and_then(|m| m.get(&key_event.code))
                        {
                            match action {
                                KeyAction::SetFilter(s) => {
                                    self.filter.set_text(s.clone());
                                    if let Err(e) = data.set_filter(s) {
                                        log::error!("encountered err {e} while updating filter");
                                    }
                                }
                            }
                        }
                    }
                },
            },
            FocusState::Filter => match self.filter.handle_key(key_event)? {
                super::filter::Action::Exit => self.focus = FocusState::List,
                super::filter::Action::Updated(f) => {
                    if let Err(e) = data.set_filter(&f) {
                        log::error!("encountered err {e} while updating filter");
                    } else {
                        self.focus = FocusState::List
                    }
                }
            },

            FocusState::Task(task_focus) => {
                match self.task.handle_key_event(
                    key_event,
                    task_focus,
                    self.table.selected().and_then(|i| {
                        let task_id = data.get_id(i);
                        data.get_mut(task_id).map(|t| (t, task_id))
                    }),
                )? {
                    super::task::Action::Exit => self.focus = FocusState::List,
                    super::task::Action::Unhandled => match key_event.code {
                        KeyCode::Char(' ') => {
                            self.focus = FocusState::Popup {
                                popup: PopupEnum::WritePopup(SaveDialog {}),
                                last_focus: self.focus.clone().into(),
                            }
                        }
                        _ => return Some(Action::Unhandled),
                    },
                }
            }
            FocusState::Popup { .. } => {
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
        Self::new(HashMap::new())
    }
}

pub struct AppWidget<'a, 'b> {
    pub app: Rc<RefCell<AppTui<'a>>>,
    pub data: &'b mut FilteredData,
    pub cursor_buf_pos: &'b mut Option<(u16, u16)>,
}

impl Widget for AppWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let AppWidget {
            app,
            data,
            cursor_buf_pos,
        } = self;

        let split = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).split(area);
        let [filter_area, area] = [split[0], split[1]];

        let app = app.clone();
        let is_focused = matches!(app.borrow().focus, FocusState::Filter);
        FilterWidget {
            tui: &mut app.borrow_mut().filter,
            is_focused,
            cursor_buf_pos,
        }
        .render(filter_area, buf);

        let task_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(area);

        {
            let app = app.clone();
            let (mut table, focus) =
                RefMut::map_split(app.borrow_mut(), |a| (&mut a.table, &mut a.focus));
            TableWidget(&mut table, &focus, data).render(task_split[0], buf);
        }

        let mut app = app.borrow_mut();
        let selected = app.table.selected();
        let focus_state = app.focus.clone();
        let id = selected.map(|i| data.get_id(i));
        TaskWidget {
            task: &mut app.task,
            data,
            id,
            focus: focus_state.as_task(),
            cursor_buf_pos,
        }
        .render(task_split[1], buf);

        if let FocusState::Popup {
            popup: p,
            last_focus: _,
        } = app.focus.clone()
        {
            PopupWidget(&p).render(area, buf)
        }
    }
}
