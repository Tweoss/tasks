use std::borrow::Cow;

use chumsky::text::Char;
use crop::Rope;

use crate::storage::{
    editing::{EditResult, Pos},
    span_edit::{EditErr, EditOp, LogEntry, SpanEditable},
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
pub struct TextEditable {
    inner: SpanEditable,
    log: Log,
}

impl TextEditable {
    pub fn inner(&self) -> &Rope {
        self.inner.inner()
    }

    pub fn handle_edit_event(&mut self, mut cursor: Pos, op: TextOp) -> (EditResult, Option<Pos>) {
        match op {
            TextOp::Move(move_dir) => {
                let text = &mut self.inner;
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
                            self.inner
                        );
                    }
                }

                (EditResult::Noop, Some(cursor))
            }
            TextOp::InsertText(ref t) => {
                let text = &mut self.inner;
                let edit_op = EditOp::Insert {
                    pos: cursor,
                    text: t.clone().to_string(),
                };
                let new_pos = Self::calc_cursor_pos(&edit_op);
                let entry = unwrap!(text.apply_edit(edit_op), op, cursor, text);
                self.log.push_entry(entry);

                (EditResult::Dirty, Some(new_pos))
            }
            TextOp::Delete { unit, dir } => {
                let other = unwrap!(
                    self.saturating_offset(cursor, unit, dir),
                    op,
                    cursor,
                    self.inner
                );
                let (start, end) = match dir {
                    LeftRight::Left => (other, cursor),
                    LeftRight::Right => (cursor, other),
                };
                let text = &mut self.inner;
                let edit_op = EditOp::Delete { start, end };
                let new_pos = Self::calc_cursor_pos(&edit_op);
                let entry = unwrap!(text.apply_edit(edit_op), op, cursor, text);
                self.log.push_entry(entry);
                (EditResult::Dirty, Some(new_pos))
            }
            TextOp::Redo => {
                if let Some(edit_op) = self.log.redo() {
                    let new_pos = Self::calc_cursor_pos(&edit_op);
                    unwrap!(self.inner.apply_edit(edit_op), op, cursor, self.inner);
                    (EditResult::Dirty, Some(new_pos))
                } else {
                    (EditResult::Noop, None)
                }
            }
            TextOp::Undo => {
                if let Some(edit_op) = self.log.undo() {
                    let new_pos = Self::calc_cursor_pos(&edit_op);
                    unwrap!(self.inner.apply_edit(edit_op), op, cursor, self.inner);
                    (EditResult::Dirty, Some(new_pos))
                } else {
                    (EditResult::Noop, None)
                }
            }
        }
    }

    // Calculate the position of cursor after an edit.
    fn calc_cursor_pos(op: &EditOp) -> Pos {
        match op {
            EditOp::Insert {
                pos: cursor,
                text: t,
            } => {
                let mut cursor = *cursor;
                let new_lines = t.chars().filter(|c| c.is_newline()).count();
                let new_column = if new_lines == 0 {
                    cursor.column + t.chars().count()
                } else {
                    t.lines().last().unwrap().chars().count()
                };
                cursor.line += new_lines;
                cursor.column = new_column;
                cursor
            }
            EditOp::Delete { start, end: _ } => *start,
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
        let text = &self.inner;
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
        let byte = self.inner.get_byte(cursor)?;
        match dir {
            LeftRight::Left => self.inner.pos_from_byte(
                byte - count_bytes_till_boundary(
                    self.inner.inner().byte_slice(..byte).chars().rev(),
                ),
            ),
            LeftRight::Right => self.inner.pos_from_byte(
                byte + count_bytes_till_boundary(self.inner.inner().byte_slice(byte..).chars()),
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
        let byte = self.inner.get_byte(cursor)?;
        match dir {
            LeftRight::Left => self.inner.pos_from_byte(
                byte - count_bytes_till_boundary(
                    self.inner.inner().byte_slice(..byte).chars().rev(),
                ),
            ),
            LeftRight::Right => self.inner.pos_from_byte(
                byte + count_bytes_till_boundary(self.inner.inner().byte_slice(byte..).chars()),
            ),
        }
    }
}

impl From<Rope> for TextEditable {
    fn from(value: Rope) -> Self {
        Self {
            inner: value.into(),
            log: Log {
                entries: vec![],
                next_index: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum TextOp {
    Move(MoveDir),
    InsertText(Cow<'static, str>),
    Delete { unit: Unit, dir: LeftRight },
    Redo,
    Undo,
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

#[derive(Debug, Clone)]
struct Log {
    entries: Vec<LogEntry>,
    next_index: usize,
}

impl Log {
    fn push_entry(&mut self, entry: LogEntry) {
        // If we undid some stuff and are now making new edits,
        // then we are branching into a new "timeline". So,
        // delete the old redo information.
        self.entries.truncate(self.next_index);
        self.entries.push(entry);
        self.next_index += 1;
    }
    fn undo(&mut self) -> Option<EditOp> {
        if self.next_index == 0 {
            return None;
        }
        self.next_index -= 1;
        Some(self.entries[self.next_index].undo.clone())
    }
    fn redo(&mut self) -> Option<EditOp> {
        let out = self.entries.get(self.next_index);
        if out.is_some() {
            self.next_index += 1;
        }
        out.cloned().map(|e| e.edit)
    }
}
