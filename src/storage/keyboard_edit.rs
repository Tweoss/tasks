use crop::Rope;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::storage::{
    editing::{EditResult, Pos},
    text_edit::{LeftRight, MoveDir, TextEditable, TextOp, Unit},
};

#[derive(Debug, Clone)]
pub struct KeyboardEditable {
    text: TextEditable,
    cursor: Pos,
}

impl KeyboardEditable {
    pub fn inner(&self) -> &Rope {
        self.text.inner()
    }
    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    pub fn from_rope(rope: Rope, cursor_at_end: bool) -> Self {
        Self {
            cursor: if cursor_at_end {
                let line_count = rope.line_len();
                // Put cursor at end of task.
                (
                    line_count.saturating_sub(1),
                    rope.lines()
                        .next_back()
                        .map(|l| l.chars().count())
                        .unwrap_or(0),
                )
                    .into()
            } else {
                (0, 0).into()
            },
            text: rope.into(),
        }
    }
    pub fn map_key_event(key_event: KeyEvent) -> Option<TextOp> {
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
            KeyCode::Char('z') if ctrl => TextOp::Undo,
            KeyCode::Char('r') if ctrl => TextOp::Redo,
            KeyCode::Char(c) => TextOp::InsertText(c.to_string().into()),
            _ => return None,
        };
        Some(op)
    }

    pub fn apply_text_op(&mut self, op: TextOp) -> EditResult {
        let (edit_result, new_pos) = self.text.handle_edit_event(self.cursor, op);
        if let Some(new_pos) = new_pos {
            self.cursor = new_pos;
        }
        edit_result
    }
}
