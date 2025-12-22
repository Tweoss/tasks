use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Widget};

use crate::filter::FilteredData;
use crate::storage::BoxState;
use crate::tui::task::editor::{EditorFocus, EditorTui, EditorWidget};

mod editor;
mod scrollbar;

pub struct TaskTui {
    editor: EditorTui,
    last_index: Option<usize>,
}

pub enum Action {
    Exit,
    Unhandled,
}

impl TaskTui {
    pub fn new() -> Self {
        Self {
            last_index: None,
            editor: EditorTui::new(),
        }
    }
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        focus: &mut TaskFocus,
    ) -> Option<Action> {
        match focus {
            TaskFocus::Boxes => {}
            TaskFocus::Context(editor_focus) => {
                self.editor.handle_key_event(key_event, editor_focus)?;
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
    Boxes,
    Context(EditorFocus),
}

impl TaskFocus {
    const TASK_COUNT: i8 = 2;
    fn to_i8(self) -> i8 {
        match self {
            TaskFocus::Boxes => 0,
            TaskFocus::Context(_) => 1,
        }
    }
    fn from_i8_wrapped(v: i8) -> Self {
        match v.rem_euclid(Self::TASK_COUNT) {
            0 => TaskFocus::Boxes,
            1 => TaskFocus::Context(EditorFocus::default()),
            _ => unreachable!(),
        }
    }
    pub fn context() -> Self {
        Self::Context(EditorFocus::Unlocked)
    }
}

pub struct TaskWidget<'a, 'b>(
    pub &'a mut TaskTui,
    pub &'b FilteredData,
    pub Option<usize>,
    pub Option<TaskFocus>,
);

impl Widget for TaskWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let TaskWidget(task, data, index, focus) = self;

        let Some(index) = index else {
            return;
        };
        let Some(v) = data.get(index) else {
            return;
        };

        let constraints = [Constraint::Max(3), Constraint::Fill(1), Constraint::Fill(2)];
        let layout = Layout::new(Direction::Vertical, constraints);
        let [title_area, context_area, boxes_area] = layout.areas(area);

        let title_block = Block::bordered().title("Title");
        Text::raw(v.title.clone()).render(title_block.inner(title_area), buf);
        title_block.render(title_area, buf);

        let context_block =
            (Block::bordered().title("Context")).border_style(Style::new().fg(match focus {
                Some(TaskFocus::Context(EditorFocus::Unlocked)) => Color::Blue,
                Some(TaskFocus::Context(EditorFocus::Locked)) => Color::Green,
                _ => Color::Reset,
            }));

        let switched_text = task.last_index != Some(index);
        task.last_index = Some(index);
        EditorWidget {
            editor: &mut task.editor,
            text: &v.context,
            switched_text,
        }
        .render(context_block.inner(context_area), buf);
        context_block.render(context_area, buf);

        let boxes_block = Block::bordered()
            .title("Boxes")
            .border_style(Style::new().fg(match focus {
                Some(TaskFocus::Boxes) => Color::Blue,
                _ => Color::Reset,
            }));
        Text::raw(
            v.boxes
                .iter()
                .map(|b| match b {
                    BoxState::Checked(date_time) => format!("Checked at {}\n", date_time),
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
