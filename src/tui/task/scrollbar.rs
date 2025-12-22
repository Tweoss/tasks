use ratatui::{
    buffer::{Buffer, Cell},
    layout::Offset,
    style::Color,
    widgets::Widget,
};

pub struct ScrollbarWidget {
    pub view_offset: usize,
    pub total_lines: usize,
}

impl Widget for ScrollbarWidget {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        // Allow scrolling such that last line is at top (like VSCode)
        // => total line count for scrollbar is height + total_lines - 1
        // => visible line count is height
        // scrollbar should occupy at least one space.

        let height = area.height as usize;
        let ScrollbarWidget {
            view_offset,
            total_lines,
        } = self;
        // total_lines should be > 0, so this should be nonnegative.
        let scrollable_line_count = height + total_lines - 1;
        let visible_lines = height;

        let offset = view_offset as f32 / scrollable_line_count as f32;
        let fraction = visible_lines as f32 / scrollable_line_count as f32;

        let offset = (offset * height as f32).round() as i32;
        let fraction = ((fraction * height as f32) as u16).max(1);

        let scrollbar = area;
        let mut scrollbar = scrollbar.offset(Offset { x: 0, y: offset });
        scrollbar.height = fraction;

        buf.merge(&Buffer::filled(
            scrollbar,
            Cell::new("¦").set_fg(Color::LightBlue).clone(),
        ));
    }
}
