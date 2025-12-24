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
