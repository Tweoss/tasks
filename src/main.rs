mod config;
mod popup;
mod storage;

use std::process::Command;

use chrono::Local;
use popup::{AddAction, AddDialog, Popup, SaveAction, SaveDialog};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
};
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, get_default_app_data_path},
    popup::ErrorDialog,
    storage::{BoxState, Data, Task},
};

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
                     \t-e, --edit: edit the config file\n\
                     \t-p, --print: print the loaded configuration\n\
                     ",
                    args[0]
                );
            }
            "-e" | "--edit" => {
                Command::new(std::env::var("EDITOR").unwrap_or("/usr/bin/vim".to_string()))
                    .arg(config::get_config_path().unwrap())
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            "-p" | "--print" => {
                let config = match Config::load() {
                    Ok(c) => c,
                    Err((c, r)) => {
                        eprintln!("failed to load config, continuing with default\n{r:?}");
                        c
                    }
                };
                println!("{config:?}");
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

#[derive(Debug)]
pub struct App<'a> {
    data: Data,
    visible: Vec<usize>,
    focus: FocusState<'a>,
    exit: bool,
    table_state: TableState,
}

impl Default for App<'_> {
    fn default() -> Self {
        Self {
            data: Data::new(get_default_app_data_path(), vec![]),
            visible: vec![],
            focus: FocusState::List,
            exit: false,
            table_state: TableState::new(),
        }
    }
}

#[derive(Debug, Default, Clone)]
enum FocusState<'a> {
    Filter,
    #[default]
    List,
    Task(TaskFocus),
    WritePopup(SaveDialog),
    AddNew(AddDialog<'a>),
    Error(ErrorDialog<'a>),
}
#[derive(Debug, Deserialize, Serialize, Clone)]
enum TaskFocus {
    Boxes,
    Context,
    Deletion,
}

impl App<'_> {
    pub fn load() -> Option<Self> {
        let config = match Config::load() {
            Ok(c) => c,
            Err((c, r)) => {
                eprintln!("failed to load config, continuing with default\n{r:?}");
                c
            }
        };
        let mut focus = FocusState::default();
        let data = match Data::load(
            shellexpand::tilde(&config.data_path.to_string_lossy())
                .into_owned()
                .into(),
        ) {
            Ok(d) => d,
            Err((d, e)) => {
                focus = FocusState::Error(ErrorDialog::from_error_focus(&e, focus));
                let error = format!("{:?}", e.wrap_err("Error loading data"));
                eprintln!("{error}");
                d
            }
        };
        let visible = (0..(data.tasks().len())).collect();
        let app: App = App {
            data,
            exit: false,
            visible,
            focus,
            ..Default::default()
        };
        Some(app)
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
            FocusState::Error(error) => error.render(frame, frame.area()),
        }
    }
    fn draw_visible(&mut self, frame: &mut Frame, area: Rect) {
        let max_boxes = self
            .visible
            .iter()
            .map(|t| self.tasks()[*t].boxes.len())
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
                let text_cell = Cell::from(self.tasks()[*t].title.clone());
                let completed_cell = Cell::from(
                    self.tasks()[*t]
                        .completed
                        .map(|d| d.format("%Y-%m-%m %H:%M").to_string())
                        .unwrap_or_default(),
                )
                .rapid_blink();
                let box_cell = Cell::from(
                    Text::raw(
                        self.tasks()[*t]
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
        let t = Table::new(rows, list_split.iter())
            .row_highlight_style(selected_row_style)
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().gray())
            .header(
                Row::new(vec!["Task".bold(), "Completed At".bold(), "Time".bold()])
                    .bottom_margin(1),
            );
        frame.render_stateful_widget(t, area, &mut self.table_state);
    }
    fn draw_selected(&mut self, frame: &mut Frame, area: Rect) {
        let Some(index) = *self.table_state.selected_mut() else {
            return;
        };
        if let Some(v) = self.visible.get(index) {
            // TODO: use text area
            let constraints = [Constraint::Max(3), Constraint::Fill(1), Constraint::Fill(2)];
            let layout = Layout::new(Direction::Vertical, constraints);
            let [title_area, context_area, boxes_area] = layout.areas(area);
            let title_block = Block::bordered().title("Title");
            frame.render_widget(
                Text::raw(self.tasks()[*v].title.clone()),
                title_block.inner(title_area),
            );
            frame.render_widget(title_block, title_area);
            let context_block = Block::bordered().title("Context");
            frame.render_widget(
                Text::raw(self.tasks()[*v].context.to_string()),
                context_block.inner(context_area),
            );
            frame.render_widget(context_block, context_area);
            let boxes_block = Block::bordered().title("Boxes");
            frame.render_widget(
                Text::raw(
                    self.tasks()[*v]
                        .boxes
                        .iter()
                        .map(|b| match b {
                            BoxState::Checked(date_time) => format!("Checked at {}\n", date_time),
                            BoxState::Started => "Started\n".to_string(),
                            BoxState::Empty => "Empty\n".to_string(),
                        })
                        .collect::<String>()
                        .clone(),
                ),
                boxes_block.inner(boxes_area),
            );
            frame.render_widget(boxes_block, boxes_area);
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
                        self.data
                            .set_completed(self.visible[i], Some(Local::now().naive_local()));
                    }
                }
                _ => {}
            },
            FocusState::WritePopup(ref mut save) => {
                match (save.handle_key(key_event), key_event.code) {
                    (SA::ExitNoWrite, _) => self.exit(),
                    (SA::Write, _) => {
                        if let Err(e) = self.data.write_dirty() {
                            self.focus = FocusState::Error(ErrorDialog::from_error_focus(
                                &e,
                                self.focus.clone(),
                            ));
                        } else {
                            self.focus = FocusState::List;
                        }
                    }
                    (SA::Exit, _) => {
                        if let Err(e) = self.data.write_dirty() {
                            self.focus = FocusState::Error(ErrorDialog::from_error_focus(
                                &e,
                                self.focus.clone(),
                            ));
                        } else {
                            self.exit();
                        }
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
                    self.visible.push(self.tasks().len());
                    self.data.push(t);
                    self.focus = FocusState::List;
                }
                (None, _) => {}
            },
            FocusState::Filter => todo!(),
            FocusState::Error(ref mut dialog) => match dialog.handle_key(key_event) {
                popup::ErrorAction::Okay => {
                    self.focus = *dialog.previous_state.take().unwrap_or_default()
                }
            },
        }
    }
    fn next_row(&mut self) {
        if self.data.tasks().is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i + 1 >= self.visible.len() {
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
        if self.data.tasks().is_empty() {
            return;
        }
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
        self.data.push_box(self.visible[i]);
    }

    fn handle_box_step(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        if let Some(BoxState::Started) = self
            .data
            .step_box_state(self.visible[i], Local::now().naive_local())
        {
            std::thread::spawn(|| {
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
            });
        };
    }

    fn remove_empty(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        self.data.remove_empty_state(self.visible[i]);
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn tasks(&self) -> &[Task] {
        self.data.tasks()
    }
}
