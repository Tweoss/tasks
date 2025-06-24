mod popup;

use std::{path::PathBuf, process::Command};

use chrono::{DateTime, Local};
use popup::{AddAction, AddDialog, Popup, SaveAction, SaveDialog};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Borders, Cell, HighlightSpacing, Row, Table, TableState},
};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use tui_textarea::TextArea;

// Main Window
//
// --------------------------------------
// filter expression (default: not completed)
// --------------------------------------
// task list    | focused task details:
//              | - creation date
//              | - scrollable list of boxes (ticked with dates, pending)
//              | - scrollable text box for context
//              | - completed date if exists
//              | - deletion menu
//
//
// dialog for adding new task
// - name
//   (whether or not satisfies current filter)

// filter expression grammar:
// filter = '(' delimited(filter, '|') ')' | '(' delimited(filter, '&') ')' | 'not' filter | existence | comparison
// existence = 'completed' | 'ticked'[i]
// comparison = value operator reference
// value = 'created' | 'completed' | 'ticked'[i]
// operator = '>=' | '<=' | '='
// reference = date | 'today' | 'yesterday'
//
// maybe in future also, 'name' 'contains' string
//
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if let Some(arg) = args.get(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "Usage: {} [options]\n\
                     Run without options for tui.\n\n\
                     Options:\n\
                     \t-h, --help: print this help message\n\
                     \t-e, --edit: edit the data file\n\
                     \t-p, --print: print the loaded configuration\n\
                     ",
                    args[0]
                );
            }
            "-e" | "--edit" => {
                Command::new(std::env::var("EDITOR").unwrap_or("/usr/bin/vim".to_string()))
                    .arg(*App::get_path())
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            "-p" | "--print" => {
                let config = App::load().unwrap_or_default();
                println!("{}", config.format());
            }
            &_ => println!("Unknown command. Run with --help for more options."),
        }
        return;
    }

    let mut app = App::load().unwrap_or_default();
    let terminal = ratatui::init();
    app.run(terminal);
    ratatui::restore();
}

const CHECK: &str = " ✔";
const STARTED: &str = "🌟";
const EMPTY: &str = " -";

type Date = DateTime<Local>;
#[derive(Debug, Deserialize, Serialize)]
pub struct App<'a> {
    tasks: Vec<Task>,
    visible: Vec<usize>,
    #[serde(skip)]
    focus: FocusState<'a>,
    #[serde(skip)]
    exit: bool,
    #[serde(skip)]
    table_state: TableState,
}

impl Default for App<'_> {
    fn default() -> Self {
        let tasks = vec![Task {
            created: Local::now(),
            title: "whoop".to_string(),
            boxes: vec![],
            context: String::new(),
            completed: Some(Local::now()),
        }];
        Self {
            tasks: tasks.clone(),
            visible: (0..tasks.len()).collect(),
            focus: FocusState::List,
            exit: false,
            table_state: TableState::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Task {
    created: Date,
    boxes: Vec<BoxState>,
    title: String,
    context: String,
    completed: Option<Date>,
}
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
enum BoxState {
    Checked(DateTime<Local>),
    Started,
    Empty,
}
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
enum FocusState<'a> {
    Filter,
    #[default]
    List,
    Task(TaskFocus),
    WritePopup(SaveDialog),
    AddNew(AddDialog<'a>),
}
#[derive(Debug, Deserialize, Serialize, Clone)]
enum TaskFocus {
    Boxes,
    Context,
    Deletion,
}

impl App<'_> {
    fn get_path() -> Box<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "Tweoss", "Task List")
            .clone()
            .unwrap();
        Box::new(dirs.data_dir().join("data.ron"))
    }
    pub fn load() -> Option<Self> {
        let path = Self::get_path();
        let contents = std::fs::read_to_string(*path.clone())
            .map_err(|_| {
                println!(
                    "Failed to read data from {}, continuing with default",
                    path.display()
                )
            })
            .ok()?;

        let mut app: Self = ron::from_str(&contents)
            .map_err(|e| {
                println!(
                    "Failed to parse data, continuing with default. \nError at: {}:{e}",
                    path.display()
                );
            })
            .ok()?;
        app.exit = false;
        println!("Successfully loaded data from {}", path.display());
        Some(app)
    }

    pub fn store(&self) {
        let path = Self::get_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(*path.clone(), self.format()).unwrap();
    }

    fn format(&self) -> String {
        ron::ser::to_string_pretty(self, PrettyConfig::new()).unwrap()
    }

    fn run(&mut self, mut terminal: DefaultTerminal) {
        loop {
            terminal.draw(|frame| self.draw(frame)).unwrap();
            self.handle_events();
            if self.exit {
                break;
            }
        }
    }
    fn draw(&mut self, frame: &mut Frame) {
        let task_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(frame.area());
        self.draw_visible(frame, task_split[0]);
        self.draw_selected(frame, task_split[1]);
        match &mut self.focus {
            FocusState::WritePopup(save) => save.render(frame, frame.area()),
            FocusState::Filter => todo!(),
            FocusState::List => {}
            FocusState::Task(task_focus) => {}
            FocusState::AddNew(add) => add.render(frame, frame.area()),
        }
    }
    fn draw_visible(&mut self, frame: &mut Frame, area: Rect) {
        let max_boxes = self
            .visible
            .iter()
            .map(|t| self.tasks[*t].boxes.len())
            .max()
            .unwrap_or(0)
            * CHECK.chars().count();
        let list_split = [
            Constraint::Fill(1),
            Constraint::Min(17),
            Constraint::Min(max_boxes.try_into().unwrap()),
        ];
        let rows = self
            .visible
            .iter()
            .map(|t| {
                let text_cell = Cell::from(self.tasks[*t].title.clone());
                let completed_cell = Cell::from(
                    self.tasks[*t]
                        .completed
                        .map(|d| d.format("%Y-%m-%m %H:%M").to_string())
                        .unwrap_or_default(),
                )
                .rapid_blink();
                let box_cell = Cell::from(
                    Text::raw(
                        self.tasks[*t]
                            .boxes
                            .iter()
                            .rev()
                            .map(|b| match b {
                                BoxState::Checked(_) => CHECK,
                                BoxState::Started => STARTED,
                                BoxState::Empty => EMPTY,
                            })
                            .collect::<String>(),
                    )
                    .left_aligned(),
                );
                Row::new(vec![text_cell, completed_cell, box_cell])
                    .style(Style::new().bg(Color::Reset))
            })
            .collect::<Vec<_>>();
        let selected_row_style = Style::default().fg(Color::White).bg(Color::Blue);
        let bar = " █ ";
        let t = Table::new(rows, list_split.iter())
            .row_highlight_style(selected_row_style)
            .highlight_symbol(Text::from(bar))
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().gray())
            .header(
                Row::new(vec!["Task".bold(), "Completed At".bold(), "Time".bold()])
                    .bottom_margin(1),
            );
        frame.render_stateful_widget(t, area, &mut self.table_state);
    }
    fn draw_selected(&mut self, frame: &mut Frame, area: Rect) {
        let surrounding_block = Block::default()
            .borders(Borders::ALL)
            .title("Selected Task");
        if let Some(v) = self
            .visible
            .get(*self.table_state.selected_mut().get_or_insert_default())
        {
            frame.render_widget(
                Text::raw(self.tasks[*v].title.clone()),
                surrounding_block.inner(area),
            );
            frame.render_widget(surrounding_block, area);
            // TODO: draw task
        }
    }
    fn handle_events(&mut self) {
        match event::read().unwrap() {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
    }
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        use AddAction as AA;
        use SaveAction as SA;
        match self.focus {
            FocusState::List => match key_event.code {
                KeyCode::Down => self.next_row(),
                KeyCode::Up => self.prev_row(),
                KeyCode::Char('q') => self.focus = FocusState::WritePopup(SaveDialog {}),
                KeyCode::Char('n') => self.handle_new_empty(),
                KeyCode::Char('N') => self.handle_box_step(),
                KeyCode::Char('A') => self.focus = FocusState::AddNew(Default::default()),
                KeyCode::Backspace => self.remove_empty(),
                KeyCode::Char('F') => {
                    if let Some(i) = self.table_state.selected() {
                        self.tasks[self.visible[i]].completed = Some(Local::now());
                    }
                }
                _ => {}
            },
            FocusState::WritePopup(ref mut save) => {
                match (save.handle_key(key_event), key_event.code) {
                    (SA::ExitNoWrite, _) => self.exit(),
                    (SA::Write, _) => {
                        self.store();
                        self.focus = FocusState::List;
                    }
                    (SA::Exit, _) => {
                        self.store();
                        self.exit();
                    }
                    (SA::Unhandled, KeyCode::Esc) => self.focus = FocusState::List,
                    (SA::Unhandled, _) => {}
                }
            }
            FocusState::Task(_) => todo!(),
            FocusState::AddNew(ref mut add) => match (add.handle_key(key_event), key_event.code) {
                (Some(AA::Exit), _) => self.focus = FocusState::List,
                (Some(AA::Add(t)), _) => {
                    // TODO: properly recalculate visible
                    self.visible.push(self.tasks.len());
                    self.tasks.push(t);
                    self.focus = FocusState::List;
                }
                (None, _) => {}
            },
            FocusState::Filter => todo!(),
        }
    }
    fn next_row(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= self.visible.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }
    fn prev_row(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.visible.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.visible.len() - 1,
        };
        self.table_state.select(Some(i));
    }

    fn handle_new_empty(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        self.tasks[self.visible[i]].boxes.push(BoxState::Empty);
    }

    fn handle_box_step(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        let last_mut = self.tasks[self.visible[i]]
            .boxes
            .iter_mut()
            .find(|b| !matches!(b, BoxState::Checked(_)));
        if let Some(last_mut) = last_mut {
            match last_mut {
                BoxState::Empty => {
                    *last_mut = BoxState::Started;
                    Command::new("/usr/bin/osascript")
                        .args([
                            "-e",
                            r#"tell application "Menubar Countdown"
                                	set hours to "0"
                                    set minutes to "25"
                                 	set seconds to "0"
                                    set play notification sound to false
                                    set repeat alert sound to false
                                	start timer
                                end tell"#,
                        ])
                        .output()
                        .unwrap();
                }
                BoxState::Started => *last_mut = BoxState::Checked(Local::now()),
                _ => (),
            }
        };
    }

    fn remove_empty(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        let Some(box_i) = self.tasks[self.visible[i]]
            .boxes
            .iter()
            .rposition(|b| matches!(b, BoxState::Empty))
        else {
            return;
        };
        self.tasks[self.visible[i]].boxes.remove(box_i);
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}
