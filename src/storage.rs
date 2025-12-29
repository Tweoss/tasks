pub mod editing;
pub mod keyboard_edit;
mod span_edit;
pub mod text_edit;

use std::{
    collections::HashSet,
    fmt::Display,
    fs::{self, OpenOptions, create_dir_all},
    io::{Read, Write},
    path::PathBuf,
};

use chrono::{DateTime, Datelike, Local, NaiveDateTime};
use crop::Rope;
use eyre::{Context, OptionExt, Result, eyre};

use crate::storage::{
    keyboard_edit::KeyboardEditable,
    parser::{Field, Value},
    text_edit::TextOp,
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
        let metadata = fs::metadata(&path).wrap_err("reading metadata")?;
        let created = metadata.created().context("reading created time")?;

        Task::from_string(DateTime::<Local>::from(created).naive_local(), path, buf)
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
                .join(format!("{}.md", urlencoding::encode(&task.title)))
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
    completed: Option<Date>,
    boxes: Vec<BoxState>,
    tags: HashSet<String>,
    context: KeyboardEditable,
    source_path: Option<PathBuf>,
    dirty: bool,
    extra_fields: Vec<Field>,
}

pub struct TaskEditableMut<'a> {
    dirty_bit: &'a mut bool,
    editable: &'a mut KeyboardEditable,
}

impl TaskEditableMut<'_> {
    pub fn apply_text_op(&mut self, op: TextOp) {
        match self.editable.apply_text_op(op) {
            editing::EditResult::Noop => {}
            editing::EditResult::Dirty => *self.dirty_bit = true,
        }
    }
}

impl Task {
    pub fn new(
        title: String,
        created: Date,
        boxes: Vec<BoxState>,
        tags: HashSet<String>,
        context: Rope,
        completed: Option<Date>,
    ) -> Self {
        Self {
            title,
            created,
            boxes,
            tags,
            context: KeyboardEditable::from_rope(context, true),
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
    pub fn tags(&self) -> &HashSet<String> {
        &self.tags
    }
    pub fn completed(&self) -> &Option<Date> {
        &self.completed
    }
    pub fn editable(&self) -> &KeyboardEditable {
        &self.context
    }
    pub fn editable_mut(&mut self) -> TaskEditableMut<'_> {
        TaskEditableMut {
            dirty_bit: &mut self.dirty,
            editable: &mut self.context,
        }
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_tags(&mut self, tags: Vec<String>) {
        self.dirty = true;
        self.tags = tags.into_iter().collect();
    }

    fn from_string(creation_date: Date, path: PathBuf, buf: String) -> Result<Self> {
        let title = urlencoding::decode(
            &path
                .file_stem()
                .ok_or_eyre("invalid name")?
                .to_string_lossy(),
        )
        .wrap_err("decoding path")?
        .to_string();

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
        let mut created = Ok(None);
        let mut boxes = Ok(None);
        let mut tags = Ok(None);
        let mut completed = Ok(None);

        let mut remaining = vec![];
        for field in fields {
            match (field.key.as_str(), field.value) {
                ("created", v) => {
                    // Last wins.
                    match v {
                        Value::Date(date) => {
                            created = Ok(Some(date));
                        }
                        t => {
                            created = Err(eyre!("created should be in date format, found {t}"));
                        }
                    }
                }
                ("boxes", v) => match v {
                    Value::BoxList(list) => {
                        boxes = Ok(Some(list));
                    }
                    Value::Unknown(s) if s.is_empty() => boxes = Ok(Some(vec![])),
                    t => {
                        boxes = Err(eyre!("boxes should be in list format, found {t}"));
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
                        t => {
                            completed = Err(eyre!(
                                "completed should be in date format or empty, found {t}"
                            ))
                        }
                    };
                }
                ("tags", v) => match v {
                    Value::TagList(list) => {
                        tags = Ok(Some(list.into_iter().collect()));
                    }
                    Value::Unknown(s) if s.is_empty() => tags = Ok(Some(HashSet::new())),
                    t => {
                        tags = Err(eyre!("tags should be in list format, found {t}"));
                    }
                },
                (k, value) => remaining.push(Field {
                    key: k.into(),
                    value,
                }),
            }
        }

        let mut dirty = false;

        let created = match created? {
            Some(v) => v,
            None => {
                dirty = true;
                log::warn!(
                    "using file metadata creation time for {} (missing created metadata)",
                    path.to_string_lossy()
                );
                creation_date
            }
        };

        let boxes = match boxes? {
            Some(v) => v,
            None => {
                dirty = true;
                log::warn!(
                    "using empty box list for {} (missing boxes metadata)",
                    path.to_string_lossy()
                );
                vec![]
            }
        };

        let tags = match tags? {
            Some(v) => v,
            None => {
                dirty = true;
                log::warn!(
                    "using empty tag list for {} (missing tags metadata)",
                    path.to_string_lossy()
                );
                HashSet::new()
            }
        };

        Ok(Self {
            title,
            created,
            boxes,
            completed: completed?,
            tags,
            context: KeyboardEditable::from_rope(context.into(), true),
            source_path: Some(path),
            dirty,
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
        writeln!(f, "created: {}", Value::Date(self.created))?;
        writeln!(
            f,
            "completed: {}",
            self.completed
                .map(Value::Date)
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
        )?;
        write!(f, "boxes:{}", Value::BoxList(self.boxes.clone()))?;
        write!(
            f,
            "tags:{}",
            Value::TagList(self.tags.iter().cloned().collect())
        )?;
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

    use crate::storage::{BoxState, Date, format_date};

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
        TagList(Vec<String>),
    }

    impl Display for Value {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Value::Unknown(s) => f.write_str(s),
                Value::Date(naive_date_time) => {
                    write!(f, "{}", format_date(naive_date_time))
                }
                Value::BoxList(box_states) => {
                    writeln!(f)?;
                    for b in box_states {
                        writeln!(f, "  - {}", b)?;
                    }
                    Ok(())
                }
                Value::TagList(items) => {
                    writeln!(f)?;
                    for t in items {
                        writeln!(f, "  - {}", t)?;
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
        let tag_list = newline()
            .ignore_then(
                just("  - ")
                    .ignore_then(line)
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .map(Value::TagList);
        let text = line.map(Value::Unknown);

        ident()
            .then_ignore(just(":"))
            .then_ignore(inline_whitespace())
            .then(choice((date_line, box_list, tag_list, text)))
            .map(|(key, value): (&str, _)| Field {
                key: key.to_string(),
                value,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
