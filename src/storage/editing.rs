use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::storage::text_edit::{LeftRight, MoveDir, TextOp, Unit};

use super::Task;

impl Task {
    pub fn handle_key_event(&mut self, cursor: Pos, key_event: KeyEvent) -> Option<Pos> {
        let alt = key_event.modifiers.contains(KeyModifiers::ALT);
        let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);
        let op = match key_event.code {
            KeyCode::Up => TextOp::Move(MoveDir::Up),
            KeyCode::Down => TextOp::Move(MoveDir::Down),
            KeyCode::Char('a') if ctrl => TextOp::Move(MoveDir::Horizontal {
                unit: Unit::Line,
                dir: LeftRight::Left,
            }),
            KeyCode::Char('b') if alt => TextOp::Move(MoveDir::Horizontal {
                unit: Unit::Word,
                dir: LeftRight::Left,
            }),
            KeyCode::Left => TextOp::Move(MoveDir::Horizontal {
                unit: Unit::Char,
                dir: LeftRight::Left,
            }),
            KeyCode::Char('e') if ctrl => TextOp::Move(MoveDir::Horizontal {
                unit: Unit::Line,
                dir: LeftRight::Right,
            }),
            KeyCode::Char('f') if alt => TextOp::Move(MoveDir::Horizontal {
                unit: Unit::Word,
                dir: LeftRight::Right,
            }),
            KeyCode::Right => TextOp::Move(MoveDir::Horizontal {
                unit: Unit::Char,
                dir: LeftRight::Right,
            }),
            KeyCode::Enter => TextOp::InsertText("\n".into()),
            KeyCode::Char('u') if ctrl => TextOp::Delete {
                unit: Unit::Line,
                dir: LeftRight::Left,
            },
            KeyCode::Backspace if alt => TextOp::Delete {
                unit: Unit::Word,
                dir: LeftRight::Left,
            },
            KeyCode::Backspace => TextOp::Delete {
                unit: Unit::Char,
                dir: LeftRight::Left,
            },
            KeyCode::Delete => TextOp::Delete {
                unit: Unit::Line,
                dir: LeftRight::Right,
            },
            KeyCode::Char('d') if alt => TextOp::Delete {
                unit: Unit::Word,
                dir: LeftRight::Right,
            },
            KeyCode::Char('d') if ctrl => TextOp::Delete {
                unit: Unit::Char,
                dir: LeftRight::Right,
            },
            KeyCode::Char(c) => TextOp::InsertText(c.to_string().into()),
            _ => return None,
        };
        let (edit_result, new_pos) = self.context.handle_edit_event(cursor, op);
        match edit_result {
            EditResult::Noop => {}
            EditResult::Dirty => self.dirty = true,
        }
        new_pos
    }
}

pub enum EditResult {
    Noop,
    Dirty,
}

#[derive(Clone, Copy, Debug)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

impl Pos {
    pub fn with_line(self, line: usize) -> Self {
        Self {
            line,
            column: self.column,
        }
    }
    pub fn with_column(self, column: usize) -> Self {
        Self {
            line: self.line,
            column,
        }
    }
}

impl From<(usize, usize)> for Pos {
    fn from((line, column): (usize, usize)) -> Self {
        Self { line, column }
    }
}
