use ratatui::crossterm::event::KeyEvent;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Widget};

use crate::filter::FilteredData;
use crate::storage::BoxState;

pub struct TaskTui {}

impl TaskTui {
    pub fn new() -> Self {
        Self {}
    }
    pub fn handle_key_event(&mut self, key_event: KeyEvent) {}
}

pub struct TaskWidget<'a, 'b>(pub &'a TaskTui, pub &'b FilteredData, pub Option<usize>);

impl Widget for TaskWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let TaskWidget(task, data, index) = self;

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
        let context_block = Block::bordered().title("Context");

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
        let boxes_block = Block::bordered().title("Boxes");
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
