use std::borrow::Cow;

use chumsky::text::Char;
use crop::Rope;

use crate::storage::{
    editing::{EditResult, Pos},
    span_edit::{EditErr, EditOp, SpanEditable},
};

macro_rules! unwrap {
    ($v:expr, $event:expr, $cursor:expr, $file: expr, $line: expr, $text: expr) => {
        match $v {
            Err(e) => {
                log::error!(
                    "[{}:{}] invalid editor logic encountered '{}' while handling '{:?}' at {:?}. text has value (\"{}\")",
                    $file,
                    $line,
                    e,
                    $event,
                    $cursor,
                    $text.inner().to_string().escape_debug(),
                );
                return (crate::storage::editing::EditResult::Noop, None);
            }
            Ok(v) => v,
        }
    };
    ($v:expr, $event:expr, $cursor:expr, $text:expr) => {
        unwrap!($v, $event, $cursor, file!(), line!(), $text)
    };
}

#[derive(Debug, Clone)]
pub struct TextEditable(SpanEditable);

impl TextEditable {
    pub fn inner(&self) -> &Rope {
        self.0.inner()
    }

    pub fn handle_edit_event(&mut self, mut cursor: Pos, op: TextOp) -> (EditResult, Option<Pos>) {
        match op {
            TextOp::Move(move_dir) => {
                let text = &mut self.0;
                match move_dir {
                    MoveDir::Up => {
                        if cursor.line > 0 {
                            cursor.line -= 1;
                            let char_len =
                                unwrap!(text.get_line_char_len(cursor.line), op, cursor, text);
                            cursor.column = cursor.column.min(char_len);
                        }
                    }
                    MoveDir::Down => {
                        if cursor.line < text.inner().line_len() {
                            cursor.line += 1;
                            let char_len =
                                unwrap!(text.get_line_char_len(cursor.line), op, cursor, text);
                            cursor.column = cursor.column.min(char_len);
                        }
                    }
                    MoveDir::Horizontal { unit, dir } => {
                        cursor = unwrap!(
                            self.saturating_offset(cursor, unit, dir),
                            op,
                            cursor,
                            self.0
                        );
                    }
                }

                (EditResult::Noop, Some(cursor))
            }
            TextOp::InsertText(ref t) => {
                let text = &mut self.0;
                unwrap!(
                    text.apply_edit(EditOp::Insert {
                        pos: cursor,
                        text: t.clone().to_string()
                    }),
                    op,
                    cursor,
                    text
                );
                let new_lines = t.chars().filter(|c| c.is_newline()).count();
                let new_column = if new_lines == 0 {
                    cursor.column + t.chars().count()
                } else {
                    t.lines().last().unwrap().chars().count()
                };
                cursor.line += new_lines;
                cursor.column = new_column;
                (EditResult::Dirty, Some(cursor))
            }
            TextOp::Delete { unit, dir } => {
                let other = unwrap!(
                    self.saturating_offset(cursor, unit, dir),
                    op,
                    cursor,
                    self.0
                );
                let (start, end) = match dir {
                    LeftRight::Left => (other, cursor),
                    LeftRight::Right => (cursor, other),
                };
                let text = &mut self.0;
                unwrap!(
                    text.apply_edit(EditOp::Delete { start, end }),
                    op,
                    cursor,
                    text
                );
                (
                    EditResult::Dirty,
                    Some(match dir {
                        LeftRight::Left => other,
                        LeftRight::Right => cursor,
                    }),
                )
            }
        }
    }

    fn saturating_offset(&self, cursor: Pos, unit: Unit, dir: LeftRight) -> Result<Pos, EditErr> {
        match unit {
            Unit::Char => self.saturating_char_offset(cursor, dir),
            Unit::Word => self.saturating_word_boundary(cursor, dir),
            Unit::Line => self.saturating_line_boundary(cursor, dir),
        }
    }

    fn saturating_char_offset(&self, cursor: Pos, dir: LeftRight) -> Result<Pos, EditErr> {
        let text = &self.0;
        let byte = text.get_byte(cursor)?;
        match dir {
            LeftRight::Left => {
                let prev_char_byte_len = text
                    .inner()
                    .byte_slice(..byte)
                    .chars()
                    .next_back()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
                text.pos_from_byte(byte - prev_char_byte_len)
            }
            LeftRight::Right => {
                let next_char_byte_len = text
                    .inner()
                    .byte_slice(byte..)
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
                text.pos_from_byte(byte + next_char_byte_len)
            }
        }
    }

    fn saturating_word_boundary(&self, cursor: Pos, dir: LeftRight) -> Result<Pos, EditErr> {
        fn count_bytes_till_boundary(it: impl Iterator<Item = char> + Clone) -> usize {
            let mut count = 0;

            // First, skip either repeated newlines or repeated whitespace
            // (but not both).
            if it.clone().next().is_some_and(|c| c.is_newline()) {
                count += it
                    .clone()
                    .take_while(|c| c.is_newline())
                    .map(|c| c.len_utf8())
                    .sum::<usize>();
            } else {
                count += it
                    .clone()
                    .take_while(|c| c.is_inline_whitespace())
                    .map(|c| c.len_utf8())
                    .sum::<usize>();
            }

            let it = it.skip(count);
            // Then, delete while we have the same "type" of character
            #[derive(PartialEq, Clone, Copy)]
            enum CharType {
                Alphanumeric,
                Whitespace,
                Newline,
                Other,
            }
            let mut last_type = None;
            for c in it {
                let char_type = if c.is_alphanumeric() {
                    CharType::Alphanumeric
                } else if c.is_newline() {
                    CharType::Newline
                } else if c.is_whitespace() {
                    CharType::Whitespace
                } else {
                    CharType::Other
                };
                let Some(last) = last_type else {
                    count += c.len_utf8();
                    last_type = Some(char_type);
                    continue;
                };
                if char_type != last {
                    break;
                }
                count += c.len_utf8();
            }
            count
        }
        let byte = self.0.get_byte(cursor)?;
        match dir {
            LeftRight::Left => self.0.pos_from_byte(
                byte - count_bytes_till_boundary(self.0.inner().byte_slice(..byte).chars().rev()),
            ),
            LeftRight::Right => self.0.pos_from_byte(
                byte + count_bytes_till_boundary(self.0.inner().byte_slice(byte..).chars()),
            ),
        }
    }

    fn saturating_line_boundary(&self, cursor: Pos, dir: LeftRight) -> Result<Pos, EditErr> {
        fn count_bytes_till_boundary(mut it: impl Iterator<Item = char> + Clone) -> usize {
            // delete until we hit a newline, or delete the newline
            if it.clone().next().is_some_and(|c| c.is_newline()) {
                return it.next().unwrap().len_utf8();
            }
            it.take_while(|c| !c.is_newline())
                .map(|c| c.len_utf8())
                .sum()
        }
        let byte = self.0.get_byte(cursor)?;
        match dir {
            LeftRight::Left => self.0.pos_from_byte(
                byte - count_bytes_till_boundary(self.0.inner().byte_slice(..byte).chars().rev()),
            ),
            LeftRight::Right => self.0.pos_from_byte(
                byte + count_bytes_till_boundary(self.0.inner().byte_slice(byte..).chars()),
            ),
        }
    }
}

impl From<Rope> for TextEditable {
    fn from(value: Rope) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone)]
pub enum TextOp {
    Move(MoveDir),
    InsertText(Cow<'static, str>),
    Delete { unit: Unit, dir: LeftRight },
}

#[derive(Debug, Clone, Copy)]
pub enum MoveDir {
    Horizontal { unit: Unit, dir: LeftRight },
    Up,
    Down,
}

#[derive(Debug, Clone, Copy)]
pub enum Unit {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone, Copy)]
pub enum LeftRight {
    Left,
    Right,
}
