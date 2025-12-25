use crate::{
    storage::{keyboard_edit::KeyboardEditable, text_edit::TextOp},
    tui::{
        FOCUSED_BORDER, UNFOCUSED_BORDER,
        task::editor::{EditorFocus, EditorTui, EditorWidget},
    },
};
use chumsky::text::Char;
use crop::Rope;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    style::Style,
    widgets::{Block, Widget},
};

pub struct FilterTui {
    editor: EditorTui,
    textbox: KeyboardEditable,
}

pub enum Action {
    Exit,
    Updated(String),
}

impl FilterTui {
    pub fn new() -> Self {
        Self {
            editor: EditorTui::new(),
            textbox: KeyboardEditable::from_rope(Rope::new(), true),
        }
    }
    pub fn handle_key(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            KeyCode::Enter => Some(Action::Updated(self.textbox.inner().to_string())),
            KeyCode::Esc => Some(Action::Exit),
            _ => {
                let text_op = KeyboardEditable::map_key_event(key_event)?;
                match text_op {
                    TextOp::InsertText(ref cow) => {
                        if !cow.contains(|c: char| c.is_newline()) {
                            self.textbox.apply_text_op(text_op);
                        }
                    }
                    _ => {
                        self.textbox.apply_text_op(text_op);
                    }
                }
                None
            }
        }
    }
}

pub struct FilterWidget<'a> {
    pub tui: &'a mut FilterTui,
    pub is_focused: bool,
    pub cursor_buf_pos: &'a mut Option<(u16, u16)>,
}

impl Widget for FilterWidget<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let mut filter_block = Block::bordered().title("Filter");
        filter_block = filter_block.border_style(Style::new().fg(if self.is_focused {
            FOCUSED_BORDER
        } else {
            UNFOCUSED_BORDER
        }));
        let outer_area = area;
        let area = filter_block.inner(area);
        filter_block.render(outer_area, buf);

        EditorWidget {
            editor: &mut self.tui.editor,
            text: &mut self.tui.textbox,
            cursor_buf_pos: self.cursor_buf_pos,
            focus: if self.is_focused {
                Some(EditorFocus::Locked)
            } else {
                None
            },
        }
        .render(area, buf);
    }
}
