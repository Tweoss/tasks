pub mod editing;
mod span_edit;
mod text_edit;

use std::{
    fmt::Display,
    fs::{OpenOptions, create_dir_all},
    io::{Read, Write},
    path::PathBuf,
};

use chrono::{Datelike, NaiveDateTime};
use crop::Rope;
use eyre::{Context, OptionExt, Result, eyre};
use serde::{Deserialize, Serialize};

use crate::storage::{
    parser::{Field, Value},
    text_edit::TextEditable,
};

pub type Date = NaiveDateTime;

#[derive(Debug, Clone)]
pub struct Data {
    source_dir: PathBuf,
    tasks: Vec<Task>,
}

impl Data {
    pub fn new(source_dir: PathBuf, tasks: Vec<Task>) -> Self {
        Self { source_dir, tasks }
    }

    /// Reports first error encountered.
    pub fn load(path: PathBuf) -> Result<Self, (Self, eyre::Report)> {
        let mut out = Self {
            source_dir: path.clone(),
            tasks: vec![],
        };
        let result = out.load_dir(path);
        result.map_err(|e| (out.clone(), e))?;
        Ok(out)
    }

    fn load_dir(&mut self, path: PathBuf) -> Result<()> {
        let mut read_dir: Vec<_> = path
            .read_dir()
            .wrap_err_with(|| format!("listing directories in {}", path.display()))?
            .collect::<Result<Vec<_>, _>>()?;
        read_dir.sort_by_key(|entry| entry.path());
        let mut error = Ok(());
        for entry in read_dir {
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                error = error.and(self.load_dir(entry.path()));
            }
            if file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e.to_str() == Some("md"))
            {
                let wrap_err_with = self
                    .load_file(entry.path())
                    .wrap_err_with(|| format!("reading '{}'", entry.path().display()));
                let task = match wrap_err_with {
                    Ok(t) => t,
                    Err(e) => {
                        error = error.and(Err(e));
                        continue;
                    }
                };
                self.tasks.push(task);
            }
        }
        error
    }

    fn load_file(&mut self, path: PathBuf) -> Result<Task> {
        let mut buf = String::new();
        OpenOptions::new()
            .read(true)
            .open(&path)?
            .read_to_string(&mut buf)?;
        buf = buf.trim().to_owned();
        buf += "\n";
        Task::from_string(path, buf)
    }

    pub fn write_dirty(&mut self) -> Result<()> {
        for index in 0..self.tasks.len() {
            if self.tasks[index].dirty {
                self.write_file(index)?;
            }
        }
        Ok(())
    }

    fn write_file(&mut self, index: usize) -> Result<()> {
        let task = &self.tasks[index];
        let path = task.source_path.clone().unwrap_or_else(|| {
            self.source_dir
                .clone()
                .join(task.created.year().to_string())
                .join(format!("{:02}", task.created.month()))
                .join(format!("{}.md", task.title))
        });
        let parent = path.parent().unwrap();
        create_dir_all(parent).wrap_err(format!("creating parent '{}'", parent.display()))?;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .wrap_err(format!("opening '{}'", path.display()))?
            .write_all(task.to_string().as_bytes())?;
        self.clear_dirty(index);
        Ok(())
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut [Task] {
        self.tasks.as_mut_slice()
    }

    fn set_dirty(&mut self, index: usize) {
        self.tasks[index].dirty = true;
    }

    fn clear_dirty(&mut self, index: usize) {
        self.tasks[index].dirty = false;
    }

    pub fn set_completed(&mut self, index: usize, value: Option<Date>) {
        self.set_dirty(index);
        self.tasks[index].completed = value;
    }

    pub fn push_box(&mut self, index: usize) {
        self.set_dirty(index);
        self.tasks[index].boxes.push(BoxState::Empty);
    }

    /// Returns new state.
    pub fn step_box_state(&mut self, index: usize, time: Date) -> Option<BoxState> {
        self.set_dirty(index);
        let last_mut = self.tasks[index]
            .boxes
            .iter_mut()
            .find(|b| !matches!(b, BoxState::Checked(_)))?;
        *last_mut = match *last_mut {
            BoxState::Empty => BoxState::Started,
            BoxState::Started => BoxState::Checked(time),
            last_mut => last_mut,
        };
        Some(*last_mut)
    }

    pub fn remove_empty_state(&mut self, index: usize) {
        let Some(box_i) = self.tasks[index]
            .boxes
            .iter()
            .rposition(|b| matches!(b, BoxState::Empty))
        else {
            return;
        };
        self.set_dirty(index);
        self.tasks[index].boxes.remove(box_i);
    }

    pub fn push(&mut self, task: Task) {
        self.tasks.push(task);
        self.set_dirty(self.tasks.len() - 1);
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    title: String,
    created: Date,
    boxes: Vec<BoxState>,
    context: TextEditable,
    completed: Option<Date>,
    source_path: Option<PathBuf>,
    dirty: bool,
    extra_fields: Vec<Field>,
}
impl Task {
    pub fn new(
        title: String,
        created: Date,
        boxes: Vec<BoxState>,
        context: Rope,
        completed: Option<Date>,
    ) -> Self {
        Self {
            title,
            created,
            boxes,
            context: context.into(),
            completed,
            source_path: None,
            dirty: true,
            extra_fields: vec![],
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn created(&self) -> &Date {
        &self.created
    }
    pub fn boxes(&self) -> &[BoxState] {
        &self.boxes
    }
    pub fn context(&self) -> &Rope {
        self.context.inner()
    }
    pub fn completed(&self) -> &Option<Date> {
        &self.completed
    }

    fn from_string(path: PathBuf, buf: String) -> Result<Self> {
        let title = path
            .file_stem()
            .ok_or_eyre("invalid name")?
            .to_string_lossy()
            .into_owned();

        // Find front matter (wrapped by `---`).
        let mut line_offset = 0;
        let buf = buf
            .strip_prefix("---\n")
            .ok_or_eyre("missing frontmatter start marker")?;
        line_offset += 1;
        let (front_matter, context) = buf
            .split_once("---\n")
            .ok_or_eyre("missing frontmatter end marker")?;
        let fields: Vec<Field> = parser::parse_fields(front_matter, line_offset)?;
        let mut created = Err(eyre!("missing created field"));
        let mut boxes = Err(eyre!("missing boxes field"));
        let mut completed = Ok(None);

        let mut remaining = vec![];
        for field in fields {
            match (field.key.as_str(), field.value) {
                ("created", v) => {
                    // Last wins.
                    if let Value::Date(date) = v {
                        created = Ok(date);
                    } else {
                        created = Err(eyre!("created should be in date format"));
                    }
                }
                ("boxes", v) => match v {
                    Value::BoxList(list) => {
                        boxes = Ok(list);
                    }
                    Value::Unknown(s) if s.is_empty() => boxes = Ok(vec![]),
                    _ => {
                        created = Err(eyre!("boxes should be in list format"));
                    }
                },
                ("completed", v) => {
                    match v {
                        Value::Date(date) => {
                            completed = Ok(Some(date));
                        }
                        Value::Unknown(s) if s.is_empty() => {
                            completed = Ok(None);
                        }
                        _ => completed = Err(eyre!("completed should be in date format or empty")),
                    };
                }
                (k, value) => remaining.push(Field {
                    key: k.into(),
                    value,
                }),
            }
        }

        Ok(Self {
            title,
            created: created?,
            boxes: boxes?,
            completed: completed?,
            context: Rope::from(context).into(),
            source_path: Some(path),
            dirty: false,
            extra_fields: remaining,
        })
    }
}

fn format_date(date: &Date) -> String {
    date.format("%Y-%m-%dT%H:%M:%S").to_string()
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "---")?;
        writeln!(f, "created: {}", format_date(&self.created))?;
        writeln!(
            f,
            "completed: {}",
            self.completed.as_ref().map(format_date).unwrap_or_default()
        )?;
        writeln!(f, "boxes:")?;
        for b in &self.boxes {
            writeln!(f, "  - {}", b)?;
        }
        for field in &self.extra_fields {
            writeln!(f, "{}: {}", field.key, field.value)?;
        }
        writeln!(f, "---")?;
        writeln!(f, "{}", self.context.inner())?;
        Ok(())
    }
}

mod parser {
    use std::fmt::Display;

    use chrono::NaiveDateTime;
    use chumsky::{
        prelude::*,
        text::{Char, digits, ident, inline_whitespace, newline},
    };
    use eyre::eyre;

    use crate::storage::{BoxState, Date};

    #[derive(Debug, Clone)]
    pub struct Field {
        pub key: String,
        pub value: Value,
    }

    #[derive(Debug, Clone)]
    pub enum Value {
        Unknown(String),
        Date(Date),
        BoxList(Vec<BoxState>),
    }

    impl Display for Value {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Value::Unknown(s) => f.write_str(s),
                Value::Date(naive_date_time) => {
                    write!(f, "{}", naive_date_time.format("%Y-%m-%dT%H:%M:%S"))
                }
                Value::BoxList(box_states) => {
                    writeln!(f)?;
                    for b in box_states {
                        writeln!(f, "  - {}", b)?;
                    }
                    Ok(())
                }
            }
        }
    }

    pub fn parse_fields(frontmatter: &str, line_offset: usize) -> Result<Vec<Field>, eyre::Report> {
        fn get_location(s: &str, line_offset: usize) -> (usize, usize) {
            (
                line_offset + s.lines().count(),
                1 + s.lines().last().map(|l| l.chars().count()).unwrap_or(0),
            )
        }

        let result = field()
            .repeated()
            .collect::<Vec<_>>()
            .parse(frontmatter)
            .into_result()
            .map_err(|e| {
                let Some(e) = e.first() else {
                    return eyre!("missing error");
                };
                let (start, _end) = (e.span().start, e.span().end);
                let (line, col) = get_location(frontmatter.split_at(start).0, line_offset);

                eyre!("parsing fields. {} at {line}:{col}", e.reason())
            })?;
        Ok(result)
    }

    fn field<'src>() -> impl Parser<'src, &'src str, Field, extra::Err<Rich<'src, char>>> {
        let line = any()
            .filter(|c: &char| !c.is_newline())
            .repeated()
            .collect::<String>()
            .then_ignore(newline());
        let date = digits(10)
            .exactly(4)
            .then(just("-"))
            .then(digits(10).exactly(2))
            .then(just("-"))
            .then(digits(10).exactly(2))
            .then(just("T"))
            .then(digits(10).exactly(2))
            .then(just(":"))
            .then(digits(10).exactly(2))
            .then(just(":"))
            .then(digits(10).exactly(2))
            .to_slice()
            .try_map(|t: &str, span| {
                NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S")
                    .map_err(|e| Rich::custom(span, e))
            });
        let date_line = date.map(Value::Date).then_ignore(newline());
        let box_list = newline()
            .ignore_then(
                just("  - ")
                    .ignore_then(choice((
                        just("Started").to(BoxState::Started),
                        just("Empty").to(BoxState::Empty),
                        just("Checked")
                            .ignore_then(just("("))
                            .ignore_then(date)
                            .then_ignore(just(")"))
                            .map(BoxState::Checked),
                    )))
                    .then_ignore(newline())
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .map(Value::BoxList);
        let text = line.map(Value::Unknown);

        ident()
            .then_ignore(just(":"))
            .then_ignore(inline_whitespace())
            .then(choice((date_line, box_list, text)))
            .map(|(key, value): (&str, _)| Field {
                key: key.to_string(),
                value,
            })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub enum BoxState {
    Checked(Date),
    Started,
    Empty,
}

impl Display for BoxState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoxState::Checked(date_time) => {
                f.write_fmt(core::format_args!("Checked({})", format_date(date_time)))
            }
            BoxState::Started => write!(f, "Started"),
            BoxState::Empty => write!(f, "Empty"),
        }
    }
}
