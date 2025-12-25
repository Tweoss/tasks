use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Widget};

use crate::filter::{FilteredData, TaskID};
use crate::storage::{BoxState, Task};
use crate::tui::task::editor::{EditorFocus, EditorTui, EditorWidget};
use crate::tui::task::tags::{TagsTui, TagsWidget};
use crate::tui::{FOCUSED_BORDER, LOCKED_EDITOR_BORDER, UNFOCUSED_BORDER};

pub mod editor;
mod scrollbar;
mod tags;

pub struct TaskTui {
    editor: EditorTui,
    tags: TagsTui,
}

pub enum Action {
    Exit,
    Unhandled,
}

impl TaskTui {
    pub fn new() -> Self {
        Self {
            editor: EditorTui::new(),
            tags: TagsTui::new(),
        }
    }
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        focus: &mut TaskFocus,
        task: Option<(&mut Task, TaskID)>,
    ) -> Option<Action> {
        match focus {
            TaskFocus::Boxes => {}
            TaskFocus::Context(editor_focus) => {
                self.editor
                    .handle_key_event(key_event, editor_focus, task.map(|t| t.0))?;
            }
            TaskFocus::Tags(focus) => {
                self.tags.handle_key(key_event, focus, task)?;
            }
        }

        let i = focus.to_i8();
        let new_i = match key_event.code {
            KeyCode::Up => i - 1,
            KeyCode::Down => i + 1,
            KeyCode::Esc | KeyCode::Left => return Some(Action::Exit),
            _ => return Some(Action::Unhandled),
        };
        *focus = TaskFocus::from_i8_wrapped(new_i);
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskFocus {
    Context(EditorFocus),
    Tags(EditorFocus),
    Boxes,
}
const TASK_COUNT: i8 = 3;

impl TaskFocus {
    fn to_i8(self) -> i8 {
        match self {
            TaskFocus::Tags(_) => 0,
            TaskFocus::Context(_) => 1,
            TaskFocus::Boxes => 2,
        }
    }
    fn from_i8_wrapped(v: i8) -> Self {
        match v.rem_euclid(TASK_COUNT) {
            0 => TaskFocus::Tags(EditorFocus::default()),
            1 => TaskFocus::Context(EditorFocus::default()),
            2 => TaskFocus::Boxes,
            _ => unreachable!(),
        }
    }
    pub fn tags_locked() -> Self {
        Self::Tags(EditorFocus::Locked)
    }
    pub fn context_locked() -> Self {
        Self::Context(EditorFocus::Locked)
    }
    pub fn context_unlocked() -> Self {
        Self::Context(EditorFocus::Locked)
    }
    pub fn as_editor(self) -> Option<EditorFocus> {
        match self {
            TaskFocus::Context(editor_focus) => Some(editor_focus),
            _ => None,
        }
    }
    pub fn as_tags(self) -> Option<EditorFocus> {
        match self {
            TaskFocus::Tags(editor_focus) => Some(editor_focus),
            _ => None,
        }
    }
}

pub struct TaskWidget<'a, 'b> {
    pub task: &'a mut TaskTui,
    pub data: &'b mut FilteredData,
    pub id: Option<TaskID>,
    pub focus: Option<TaskFocus>,
    pub cursor_buf_pos: &'a mut Option<(u16, u16)>,
}

impl Widget for TaskWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let TaskWidget {
            task: tui,
            data,
            id,
            focus,
            cursor_buf_pos,
        } = self;

        let Some(id) = id else {
            return;
        };
        let Some(v) = data.get_mut(id) else {
            return;
        };

        let constraints = [
            Constraint::Max(3),
            Constraint::Max(3),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ];
        let layout = Layout::new(Direction::Vertical, constraints);
        let [title_area, tags_area, context_area, boxes_area] = layout.areas(area);

        let title_block = Block::bordered()
            .title("Title")
            .border_style(UNFOCUSED_BORDER);
        Text::raw(v.title()).render(title_block.inner(title_area), buf);
        title_block.render(title_area, buf);

        TagsWidget {
            tui: &mut tui.tags,
            focus: self.focus.and_then(|f| f.as_tags()),
            cursor_buf_pos,
            task_id: id,
            task: v,
        }
        .render(tags_area, buf);

        let context_block =
            (Block::bordered().title("Context")).border_style(Style::new().fg(match focus {
                Some(TaskFocus::Context(EditorFocus::Unlocked)) => FOCUSED_BORDER,
                Some(TaskFocus::Context(EditorFocus::Locked)) => LOCKED_EDITOR_BORDER,
                _ => UNFOCUSED_BORDER,
            }));

        EditorWidget {
            editor: &mut tui.editor,
            text: v.editable(),
            cursor_buf_pos,
            focus: self.focus.and_then(|f| f.as_editor()),
        }
        .render(context_block.inner(context_area), buf);
        context_block.render(context_area, buf);

        let boxes_block = Block::bordered()
            .title("Boxes")
            .border_style(Style::new().fg(match focus {
                Some(TaskFocus::Boxes) => FOCUSED_BORDER,
                _ => UNFOCUSED_BORDER,
            }));
        Text::raw(
            v.boxes()
                .iter()
                .map(|b| match b {
                    BoxState::Checked(date_time) => {
                        format!("Checked at {}\n", date_time.format("%Y-%m-%d %H:%M:%S"))
                    }
                    BoxState::Started => "Started\n".to_string(),
                    BoxState::Empty => "Empty\n".to_string(),
                })
                .collect::<String>()
                .clone(),
        )
        .render(boxes_block.inner(boxes_area), buf);
        boxes_block.render(boxes_area, buf);
    }
}
