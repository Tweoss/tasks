pub mod editing;
pub mod keyboard_edit;
mod span_edit;
pub mod text_edit;

use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fmt::Display,
    fs::{self, OpenOptions, create_dir_all},
    io::{Read, Write},
    path::PathBuf,
};

use chrono::{DateTime, Datelike, Local, NaiveDateTime};
use chumsky::{Parser, error::Rich, span::Spanned};
use crop::Rope;
use eyre::{Context, OptionExt, Result, eyre};

use crate::storage::{
    keyboard_edit::KeyboardEditable,
    parser::{Field, Frontmatter, Value, box_field, date_field, rename_field, tag_field},
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
        self.fix_path_conflicts();
        for index in 0..self.tasks.len() {
            if self.tasks[index].dirty {
                self.write_file(index)?;
            }
        }
        Ok(())
    }

    fn fix_path_conflicts(&mut self) {
        let mut path_to_index: HashMap<_, Vec<_>> = HashMap::new();
        for index in 0..self.tasks.len() {
            let t = &self.tasks[index];
            let path = self.get_task_path(t);
            let v = path_to_index.entry(path).or_default();
            v.push(index);
        }
        for (path, indices) in path_to_index {
            if indices.len() < 2 {
                continue;
            }
            // Manually override source path to avoid path conflicts.
            for (i, task_index) in indices.iter().enumerate() {
                let t = &mut self.tasks[*task_index];
                // We're not overwriting data because rename would have already
                // been used to set title.
                t.rename = Some(t.title.clone());
                let mut path = path.clone();
                path.set_file_name(
                    path.file_stem()
                        .expect("should have had file name")
                        .to_string_lossy()
                        .into_owned()
                        + &format!("_{i}.")
                        + &path
                            .extension()
                            .map(|e| e.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                );
                t.source_path = Some(path);
                t.dirty = true;
            }
        }
    }

    fn write_file(&mut self, index: usize) -> Result<()> {
        let task = &self.tasks[index];
        let path = self.get_task_path(task);
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

    fn get_task_path(&self, task: &Task) -> PathBuf {
        task.source_path.clone().unwrap_or_else(|| {
            self.source_dir
                .clone()
                .join(task.created.year().to_string())
                .join(format!("{:02}", task.created.month()))
                .join(format!("{}.md", urlencoding::encode(&task.title)))
        })
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
    rename: Option<String>,
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
            rename: None,
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
        let frontmatter = parser::parse_fields(front_matter, line_offset)?;
        let mut created = Ok(None);
        let mut boxes = Ok(None);
        let mut tags = Ok(None);
        let mut completed = Ok(None);
        let mut rename = Ok(None);

        fn format_error(
            name: &str,
            frontmatter: &Frontmatter,
            field: &Spanned<(String, String)>,
            errors: Vec<Rich<'_, char>>,
        ) -> eyre::Report {
            let e = errors.first().unwrap();
            dbg!(field.span);
            let (line, col) = frontmatter.get_location(field.span.start);
            let (end_line, end_col) = frontmatter.get_location(field.span.end);
            eyre!("{name} failed to parse, {e} between {line}:{col} and {end_line}:{end_col}")
        }
        fn run_parser<'src, T>(
            parser: impl Parser<'src, &'src str, T, chumsky::extra::Err<Rich<'src, char>>>,
            key: &str,
            value: &'src str,
            frontmatter: &Frontmatter,
            field: &Spanned<(String, String)>,
        ) -> Result<Option<T>, eyre::Report> {
            parser
                .parse(value)
                .into_result()
                .map(Some)
                .map_err(|e| format_error(key, frontmatter, field, e))
        }

        let mut remaining = vec![];
        for field in &frontmatter.parsed_fields {
            let key = field.0.as_str();
            let value = field.1.clone();
            match key {
                "created" => {
                    // Last wins.
                    created = run_parser(date_field(), key, &value, &frontmatter, field);
                }
                "boxes" if value.trim().is_empty() => boxes = Ok(None),
                "boxes" => boxes = run_parser(box_field(), key, &value, &frontmatter, field),
                "completed" => {
                    completed = run_parser(date_field(), key, &value, &frontmatter, field)
                }
                "tags" if value.trim().is_empty() => tags = Ok(None),
                "tags" => tags = run_parser(tag_field(), key, &value, &frontmatter, field),
                "rename" => rename = run_parser(rename_field(), key, &value, &frontmatter, field),
                _ => remaining.push(Field {
                    key: key.into(),
                    value: Value::Unknown(value),
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
            Some(v) => v.into_iter().collect(),
            None => {
                dirty = true;
                log::warn!(
                    "using empty tag list for {} (missing tags metadata)",
                    path.to_string_lossy()
                );
                HashSet::new()
            }
        };

        let rename = rename?;
        let title = match &rename {
            Some(v) => v.to_owned(),
            None => title,
        };

        Ok(Self {
            title,
            created,
            boxes,
            completed: completed?,
            tags,
            rename,
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
        if let Some(completed) = self.completed {
            writeln!(f, "completed: {}", Value::Date(completed))?;
        }
        write!(f, "boxes:{}", Value::BoxList(self.boxes.clone()))?;
        write!(
            f,
            "tags:{}",
            Value::TagList(self.tags.iter().cloned().collect())
        )?;
        if let Some(rename) = &self.rename {
            writeln!(f, "rename: {}", Value::Rename(rename.clone()))?;
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
        Rename(String),
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
                Value::Rename(t) => write!(f, "{}", t),
            }
        }
    }

    pub struct Frontmatter {
        line_offset: usize,
        text: String,
        pub parsed_fields: Vec<Spanned<(String, String)>>,
    }
    impl Frontmatter {
        pub fn get_location(&self, byte: usize) -> (usize, usize) {
            let s = dbg!(self.text.split_at(byte).0);
            let out = (
                self.line_offset + s.lines().count(),
                1 + s.lines().last().map(|l| l.chars().count()).unwrap_or(0),
            );
            // instead of reporting the last character of a line, report the next line.
            if s.ends_with(|s: char| s.is_newline()) && self.text.lines().count() > out.0 {
                return (out.0 + 1, 0);
            }
            out
        }
    }

    pub fn parse_fields(
        frontmatter: &str,
        line_offset: usize,
    ) -> Result<Frontmatter, eyre::Report> {
        let mut out = Frontmatter {
            line_offset,
            text: frontmatter.to_string(),
            parsed_fields: vec![],
        };

        out.parsed_fields = field()
            .spanned()
            .repeated()
            .collect::<Vec<_>>()
            .padded()
            .parse(frontmatter)
            .into_result()
            .map_err(|e| {
                let Some(e) = e.first() else {
                    return eyre!("missing error");
                };
                let (start, _end) = (e.span().start, e.span().end);
                let (line, col) = out.get_location(start);
                eyre!("parsing fields. {} at {line}:{col}", e.reason())
            })?;

        Ok(out)
    }

    fn field<'src>() -> impl Parser<'src, &'src str, (String, String), extra::Err<Rich<'src, char>>>
    {
        let any_field = line()
            .then(
                just("  ")
                    .map(|s| s.to_owned())
                    .then(line())
                    .repeated()
                    .collect::<Vec<(String, String)>>(),
            )
            .map(|(first_line, lines)| {
                first_line
                    + "\n"
                    + &lines
                        .into_iter()
                        .map(|(whitespace, line)| whitespace + &line + "\n")
                        .collect::<String>()
            });

        ident()
            .then_ignore(just(":"))
            .then_ignore(inline_whitespace())
            .then(any_field)
            .map(|(key, value): (&str, _)| (key.to_string(), value))
    }
    fn line<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
        any()
            .filter(|c: &char| !c.is_newline())
            .repeated()
            .collect::<String>()
            .then_ignore(newline())
    }
    fn date<'src>() -> impl Parser<'src, &'src str, NaiveDateTime, extra::Err<Rich<'src, char>>> {
        digits(10)
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
            })
    }
    pub fn date_field<'src>()
    -> impl Parser<'src, &'src str, NaiveDateTime, extra::Err<Rich<'src, char>>> {
        date().then_ignore(newline())
    }
    pub fn box_field<'src>()
    -> impl Parser<'src, &'src str, Vec<BoxState>, extra::Err<Rich<'src, char>>> {
        newline().ignore_then(
            just("  - ")
                .ignore_then(choice((
                    just("Started").to(BoxState::Started),
                    just("Empty").to(BoxState::Empty),
                    just("Checked")
                        .ignore_then(just("("))
                        .ignore_then(date())
                        .then_ignore(just(")"))
                        .map(BoxState::Checked),
                )))
                .then_ignore(newline())
                .repeated()
                .at_least(1)
                .collect::<Vec<_>>(),
        )
    }
    pub fn tag_field<'src>()
    -> impl Parser<'src, &'src str, Vec<String>, extra::Err<Rich<'src, char>>> {
        newline().ignore_then(
            just("  - ")
                .ignore_then(line())
                .repeated()
                .collect::<Vec<_>>(),
        )
    }
    pub fn rename_field<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>>
    {
        line()
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
