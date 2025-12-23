use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    widgets::Widget,
};

use crate::{
    FocusState, PopupEnum,
    filter::FilteredData,
    popup::{AddAction, ErrorAction, ErrorDialog, Popup, SaveAction},
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
                    data.push(t);
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
