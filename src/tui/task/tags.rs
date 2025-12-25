use std::collections::HashMap;

use chumsky::{Parser, text::Char};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    style::Style,
    widgets::{Block, Widget},
};

use crate::{
    filter::TaskID,
    storage::{Task, keyboard_edit::KeyboardEditable, text_edit::TextOp},
    tui::{
        FOCUSED_BORDER, LOCKED_EDITOR_BORDER, UNFOCUSED_BORDER,
        task::{
            editor::{EditorFocus, EditorTui, EditorWidget},
            tags::parse::inline_tags,
        },
    },
};

pub struct TagsTui {
    task_to_editor: HashMap<TaskID, (EditorTui, KeyboardEditable)>,
}

pub enum Action {
    Unhandled,
}

impl TagsTui {
    pub fn new() -> Self {
        Self {
            task_to_editor: HashMap::new(),
        }
    }

    pub fn handle_key(
        &mut self,
        key_event: KeyEvent,
        focus: &mut EditorFocus,
        task: Option<(&mut Task, TaskID)>,
    ) -> Option<Action> {
        let Some((task, task_id)) = task else {
            return Some(Action::Unhandled);
        };
        if matches!(focus, EditorFocus::Unlocked) {
            if let KeyCode::Enter = key_event.code {
                *focus = EditorFocus::Locked;
                return None;
            }
            return Some(Action::Unhandled);
        }
        let (_, textbox) = self
            .task_to_editor
            .entry(task_id)
            .or_insert_with(|| derive_editable(task));
        match key_event.code {
            KeyCode::Enter => {
                *focus = EditorFocus::Unlocked;
                let inner = textbox.inner().to_string();
                match inline_tags().parse(&inner).into_result() {
                    Ok(v) => task.set_tags(v),
                    Err(e) => {
                        log::warn!("error parsing tags {e:?}")
                    }
                }
                // TODO: highlight bad character?
                None
            }
            KeyCode::Esc => {
                *focus = EditorFocus::Unlocked;
                None
            }
            _ => {
                let text_op = KeyboardEditable::map_key_event(key_event)?;
                // let (_, textbox) = self
                //     .task_to_editor
                //     .entry(task_id)
                //     .or_insert_with(|| derive_editable(task));
                match text_op {
                    TextOp::InsertText(ref cow) => {
                        if !cow.contains(|c: char| c.is_newline()) {
                            textbox.apply_text_op(text_op);
                        }
                    }
                    _ => {
                        textbox.apply_text_op(text_op);
                    }
                }
                None
            }
        }
    }
}

fn derive_editable(task: &mut Task) -> (EditorTui, KeyboardEditable) {
    (
        EditorTui::new(),
        KeyboardEditable::from_rope(
            task.tags()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
                .into(),
            true,
        ),
    )
}

pub struct TagsWidget<'a> {
    pub tui: &'a mut TagsTui,
    pub task: &'a mut Task,
    pub focus: Option<EditorFocus>,
    pub cursor_buf_pos: &'a mut Option<(u16, u16)>,
    pub task_id: TaskID,
}

impl<'a> Widget for TagsWidget<'a> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let mut filter_block = Block::bordered().title("Tags");
        filter_block = filter_block.border_style(Style::new().fg(match self.focus {
            Some(EditorFocus::Unlocked) => FOCUSED_BORDER,
            Some(EditorFocus::Locked) => LOCKED_EDITOR_BORDER,
            _ => UNFOCUSED_BORDER,
        }));
        let outer_area = area;
        let area = filter_block.inner(area);
        filter_block.render(outer_area, buf);

        let (editor, text) = self
            .tui
            .task_to_editor
            .entry(self.task_id)
            .or_insert_with(|| derive_editable(self.task));

        EditorWidget {
            editor,
            text,
            cursor_buf_pos: self.cursor_buf_pos,
            focus: self.focus,
        }
        .render(area, buf);
    }
}

mod parse {
    use chumsky::{Parser, error::Rich, extra, prelude::*};

    pub fn inline_tags<'src>()
    -> impl Parser<'src, &'src str, Vec<String>, extra::Err<Rich<'src, char>>> {
        any()
            .filter(|c: &char| *c != '(' && *c != ')' && *c != ',')
            .repeated()
            .at_least(1)
            .collect::<String>()
            .padded()
            .separated_by(just(","))
            .collect()
    }
}
