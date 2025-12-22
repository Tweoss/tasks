use std::{error::Error, fmt::Display};

use chumsky::text::Char;
use crop::Rope;

use super::Task;

impl Task {
    pub fn apply_edit(&mut self, op: EditOp) -> Result<(), EditErr> {
        self.dirty = true;
        match op {
            EditOp::Insert { pos, text } => {
                self.context.insert(get_byte(&self.context, pos)?, text)
            }
            EditOp::Delete { start, end } => self
                .context
                .delete(get_byte(&self.context, start)?..get_byte(&self.context, end)?),
        };

        Ok(())
    }

    pub fn get_line_char_len(&self, line: usize) -> Result<usize, EditErr> {
        let line_len = self.context.line_len();
        Ok(if line == line_len {
            0
        } else if line > line_len {
            return Err(EditErr::OutOfBounds);
        } else {
            self.context.line(line).chars().count()
        })
    }
}
fn get_byte(rope: &Rope, pos: Pos) -> Result<usize, EditErr> {
    // If the buffer is of the form "...\n" and the cursor is right after the
    // last newline character, then this is a valid position (even though the
    // cursor is not in an actual "line").
    if pos.line == rope.line_len()
        && pos.column == 0
        && rope
            .raw_lines()
            .next_back()
            .is_none_or(|l| l.chars().next_back().is_none_or(|c| c.is_newline()))
    {
        return Ok(rope.byte_len());
    }

    if pos.line >= rope.line_len() {
        return Err(EditErr::OutOfBounds);
    }
    let line_start = rope.byte_of_line(pos.line);
    let line = rope.line(pos.line);
    let len = line.chars().count();
    // It is valid to be at the very end of a line.
    if pos.column > len {
        return Err(EditErr::OutOfBounds);
    }
    let col_offset: usize = line.chars().take(pos.column).map(|c| c.len_utf8()).sum();
    log::debug!("col_offset {:?} line_start {:?}", col_offset, line_start);

    Ok(line_start + col_offset)
}

pub enum EditOp {
    Insert { pos: Pos, text: String },
    Delete { start: Pos, end: Pos },
}

#[derive(Debug)]
pub enum EditErr {
    OutOfBounds,
}

impl Error for EditErr {}

impl Display for EditErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditErr::OutOfBounds => f.write_fmt(format_args!("{:?}: out of bounds indexing", self)),
        }
    }
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
