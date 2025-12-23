use std::{error::Error, fmt::Display};

use chumsky::text::Char;
use crop::Rope;

use crate::storage::editing::Pos;

#[derive(Debug, Clone)]
pub struct SpanEditable(Rope);

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

impl SpanEditable {
    pub fn inner(&self) -> &Rope {
        &self.0
    }
    pub fn apply_edit(&mut self, op: EditOp) -> Result<(), EditErr> {
        match op {
            EditOp::Insert { pos, text } => {
                let byte_offset = self.get_byte(pos)?;
                self.0.insert(byte_offset, text)
            }
            EditOp::Delete { start, end } => {
                let start = self.get_byte(start)?;
                let end = self.get_byte(end)?;
                self.0.delete(start..end)
            }
        };

        Ok(())
    }

    pub fn get_line_char_len(&self, line: usize) -> Result<usize, EditErr> {
        let line_len = self.0.line_len();
        Ok(
            if line == line_len
                && self
                    .0
                    .raw_lines()
                    .next_back()
                    .is_none_or(|c| c.chars().next_back().is_none_or(|c| c.is_newline()))
            {
                0
            } else if line >= line_len {
                return Err(EditErr::OutOfBounds);
            } else {
                self.0.line(line).chars().count()
            },
        )
    }

    pub fn get_byte(&self, pos: Pos) -> Result<usize, EditErr> {
        let rope = &self.0;
        if self.is_simulated_final_newline(pos) {
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

        Ok(line_start + col_offset)
    }

    pub fn is_simulated_final_newline(&self, pos: Pos) -> bool {
        // If the buffer is of the form "...\n" and the cursor is right after the
        // last newline character, then this is a valid position (even though the
        // cursor is not in an actual "line").
        // e.g. if the buffer has form "abc\n"
        // ```
        // abc
        // ```
        // then we are allowed to place the cursor below abc at "abc\n|"
        pos.line == self.0.line_len()
            && pos.column == 0
            && self
                .0
                .raw_lines()
                .next_back()
                .is_none_or(|l| l.chars().next_back().is_none_or(|c| c.is_newline()))
    }
    pub fn pos_from_byte(&self, byte_pos: usize) -> Result<Pos, EditErr> {
        if byte_pos > self.0.byte_len() {
            return Err(EditErr::OutOfBounds);
        }

        // If we're at the end.
        let out = if byte_pos == self.0.byte_len() {
            if self.0.chars().next_back().is_none_or(|c| c.is_newline()) {
                // Rope will not return the final empty simulated line in the iterator.
                (self.0.line_len(), 0).into()
            } else {
                (
                    self.0.line_len().saturating_sub(1),
                    self.0
                        .lines()
                        .next_back()
                        .map(|l| l.chars().count())
                        .unwrap_or(0),
                )
                    .into()
            }
        } else {
            let line = self.0.line_of_byte(byte_pos);
            let line_byte_offset = byte_pos - self.0.byte_of_line(line);
            let char_offset = self
                .0
                .line(line)
                .byte_slice(0..line_byte_offset)
                .chars()
                .count();
            (line, char_offset).into()
        };
        Ok(out)
    }
}

impl From<Rope> for SpanEditable {
    fn from(value: Rope) -> Self {
        Self(value)
    }
}
