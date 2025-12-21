use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Widget};

use crate::filter::FilteredData;
use crate::storage::BoxState;

pub struct TaskTui {}

pub enum Action {
    Exit,
    Unhandled,
}

impl TaskTui {
    pub fn new() -> Self {
        Self {}
    }
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        focus: &mut TaskFocus,
    ) -> Option<Action> {
        let i = focus.to_i8();
        let new_i = match key_event.code {
            KeyCode::Up => i - 1,
            KeyCode::Down => i + 1,
            KeyCode::Esc => return Some(Action::Exit),
            _ => return Some(Action::Unhandled),
        };
        *focus = TaskFocus::from_i8_wrapped(new_i);
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskFocus {
    Boxes,
    Context,
}

impl TaskFocus {
    const TASK_COUNT: i8 = 2;
    fn to_i8(self) -> i8 {
        match self {
            TaskFocus::Boxes => 0,
            TaskFocus::Context => 1,
        }
    }
    fn from_i8_wrapped(v: i8) -> Self {
        match v.rem_euclid(Self::TASK_COUNT) {
            0 => TaskFocus::Boxes,
            1 => TaskFocus::Context,
            _ => unreachable!(),
        }
    }
}

pub struct TaskWidget<'a, 'b>(
    pub &'a TaskTui,
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
        fn style_focused<'a>(
            block: Block<'a>,
            focus: &Option<TaskFocus>,
            target: TaskFocus,
        ) -> Block<'a> {
            if Some(target) == *focus {
                block.border_style(Style::new().fg(Color::LightBlue))
            } else {
                block
            }
        }

        let title_block = Block::bordered().title("Title");
        Text::raw(v.title.clone()).render(title_block.inner(title_area), buf);
        title_block.render(title_area, buf);
        let context_block = style_focused(
            Block::bordered().title("Context"),
            &focus,
            TaskFocus::Context,
        );

        Text::raw(
            v.context
                .raw_lines()
                .rev()
                .take(context_area.height as usize)
                .rev()
                .map(|r| r.to_string())
                .collect::<String>(),
        )
        .render(context_block.inner(context_area), buf);
        context_block.render(context_area, buf);
        let boxes_block = style_focused(Block::bordered().title("Boxes"), &focus, TaskFocus::Boxes);
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
