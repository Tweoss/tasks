use std::io;

use crop::Rope;
use ratatui::{
    crossterm::{
        cursor::SetCursorStyle,
        event::{KeyCode, KeyEvent},
    },
    layout::{Constraint, Layout},
    style::Style,
    widgets::Widget,
};

use crate::{
    storage::{
        Task,
        editing::{EditErr, EditOp, Pos},
    },
    tui::task::scrollbar::ScrollbarWidget,
};

pub struct EditorTui {
    view_offset: usize,
    last_state: BufferState,
}

struct BufferState {
    cursor: Pos,
}

pub enum Action {
    Unhandled,
}

macro_rules! unwrap {
    ($v:expr, $event:expr, $cursor:expr, $msg: expr, $file: expr, $line: expr) => {
        match $v {
            Err(e) => {
                log::error!(
                    "[{}:{}] invalid editor logic encountered '{}' while handling '{}' at {:?}. {}",
                    $file,
                    $line,
                    e,
                    $event,
                    $cursor,
                    $msg
                );
                return None;
            }
            Ok(v) => v,
        }
    };
    ($v:expr, $event:expr, $cursor:expr, $msg: expr) => {
        unwrap!($v, $event, $cursor, $msg, file!(), line!())
    };
    ($v:expr, $event:expr, $cursor:expr) => {
        unwrap!($v, $event, $cursor, "", file!(), line!())
    };
}

impl EditorTui {
    pub fn new() -> Self {
        Self {
            view_offset: 0,
            last_state: BufferState {
                cursor: (0, 0).into(),
            },
        }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        focus: &mut EditorFocus,
        task: Option<&mut Task>,
    ) -> Option<Action> {
        match focus {
            EditorFocus::Unlocked => match key_event.code {
                KeyCode::Enter => {
                    *focus = EditorFocus::Locked;
                    return None;
                }
                _ => {
                    return Some(Action::Unhandled);
                }
            },
            EditorFocus::Locked => {}
        }

        assert!(matches!(focus, EditorFocus::Locked));

        let Some(task) = task else {
            return Some(Action::Unhandled);
        };

        let last_state = &mut self.last_state;
        match key_event.code {
            KeyCode::Esc => *focus = EditorFocus::Unlocked,
            KeyCode::Up => {
                let cursor = &mut last_state.cursor;
                unwrap!(Err(EditErr::OutOfBounds), key_event.code, cursor);
                if cursor.line > 0 {
                    cursor.line -= 1;
                    let line_len =
                        unwrap!(task.get_line_char_len(cursor.line), key_event.code, cursor);
                    cursor.column = cursor.column.min(line_len);
                }
            }
            KeyCode::Down => {
                let cursor = &mut last_state.cursor;
                if cursor.line + 1 < task.context().line_len() {
                    cursor.line += 1;
                    let line_len =
                        unwrap!(task.get_line_char_len(cursor.line), key_event.code, cursor);
                    cursor.column = cursor.column.min(line_len);
                } else if cursor.line + 1 == task.context().line_len()
                    && task.is_simulated_final_newline((cursor.line + 1, 0).into())
                {
                    cursor.line += 1;
                    cursor.column = 0;
                }
            }
            KeyCode::Left => {
                let cursor = &mut last_state.cursor;
                if cursor.column > 0 {
                    cursor.column -= 1;
                    return None;
                }
                if cursor.line == 0 {
                    return None;
                }
                cursor.line -= 1;
                let line_len = unwrap!(task.get_line_char_len(cursor.line), key_event.code, cursor);
                cursor.column = line_len;
            }
            KeyCode::Right => {
                let cursor = &mut last_state.cursor;
                let line_len = unwrap!(task.get_line_char_len(cursor.line), key_event.code, cursor);
                if cursor.column + 1 < line_len {
                    cursor.column += 1;
                    return None;
                }
                let next_line: Pos = (cursor.line + 1, 0).into();
                if next_line.line < task.context().line_len()
                    || task.is_simulated_final_newline(next_line)
                {
                    *cursor = next_line;
                }
            }
            KeyCode::Char(c) => {
                unwrap!(
                    task.apply_edit(EditOp::Insert {
                        pos: last_state.cursor,
                        text: c.to_string(),
                    }),
                    key_event.code,
                    last_state.cursor
                );
                last_state.cursor.column += 1;
            }
            KeyCode::Enter => {
                unwrap!(
                    task.apply_edit(EditOp::Insert {
                        pos: last_state.cursor,
                        text: "\n".to_string(),
                    }),
                    key_event.code,
                    last_state.cursor
                );
                last_state.cursor.line += 1;
                last_state.cursor.column = 0;
            }
            KeyCode::Backspace => {
                let cursor = &mut last_state.cursor;
                let start = if cursor.column > 0 {
                    cursor.with_column(cursor.column - 1)
                } else if cursor.line > 0 {
                    let next_line = cursor.line - 1;
                    let line_len =
                        unwrap!(task.get_line_char_len(next_line), key_event.code, cursor);
                    (cursor.line - 1, line_len).into()
                } else {
                    *cursor
                };
                let end = *cursor;
                unwrap!(
                    task.apply_edit(EditOp::Delete { start, end }),
                    key_event.code,
                    cursor
                );
                *cursor = start;
            }
            _ => return None,
        }
        None
    }

    fn scroll_up(&mut self) {
        self.view_offset = self.view_offset.saturating_sub(1);
    }
    fn scroll_down(&mut self, line_count: usize) {
        // Weird math to avoid panic.
        self.view_offset = (self.view_offset + 2).min(line_count).saturating_sub(1)
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum EditorFocus {
    #[default]
    Unlocked,
    Locked,
}

pub struct EditorWidget<'a> {
    pub editor: &'a mut EditorTui,
    pub text: &'a Rope,
    pub switched_text: bool,
    pub cursor_buf_pos: &'a mut Option<(u16, u16)>,
    pub focus: Option<EditorFocus>,
}

impl Widget for EditorWidget<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let layout = Layout::horizontal([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);
        let (scroll_area, text_area) = (layout[0], layout[2]);

        let height = text_area.height as usize;
        if self.switched_text {
            let line_count = self.text.line_len();
            // Put cursor at end of task.
            self.editor.last_state = BufferState {
                cursor: (
                    line_count.saturating_sub(1),
                    self.text
                        .lines()
                        .next_back()
                        .map(|l| l.chars().count())
                        .unwrap_or(0),
                )
                    .into(),
            };
        }
        // Scroll the cursor into view.
        let cursor = self.editor.last_state.cursor;
        if cursor.line < self.editor.view_offset {
            self.editor.view_offset = cursor.line;
        }
        if cursor.line >= self.editor.view_offset + height {
            self.editor.view_offset += 1 + cursor.line - self.editor.view_offset - height;
        }
        if let Some(EditorFocus::Locked) = self.focus {
            *self.cursor_buf_pos = Some((
                (text_area.x as usize + cursor.column) as u16,
                (text_area.y as usize + cursor.line - self.editor.view_offset) as u16,
            ));
            if let Err(e) = ratatui::crossterm::execute!(io::stdout(), SetCursorStyle::SteadyBar) {
                log::error!("failed to set cursor style {e}");
            }
        }

        let visible_lines = self
            .text
            .raw_lines()
            .skip(self.editor.view_offset)
            .take(height);
        for (y, l) in visible_lines.enumerate() {
            buf.set_string(
                text_area.x,
                text_area.y + y as u16,
                l.to_string(),
                Style::new(),
            );
        }

        ScrollbarWidget {
            view_offset: self.editor.view_offset,
            total_lines: self.text.line_len(),
        }
        .render(scroll_area, buf);
    }
}
