use std::{path::PathBuf, process::Command};

use chrono::{DateTime, Local};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Cell, Clear, HighlightSpacing, Row, Table, TableState},
};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if let Some(arg) = args.get(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "Usage: {} [options]\nOptions:\n  -h, --help: print this help message\n  -e, --edit: edit the data file\n",
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
            &_ => println!("Unknown command. Run with --help for more options."),
        }
        return;
    }

    let mut app = App::load().unwrap_or_default();
    let terminal = ratatui::init();
    app.run(terminal);
    ratatui::restore();
    if app.should_save {
        app.store();
    }
}

// const INFO_TEXT: [&str; 2] = [
//     "(Esc) quit | (↑) move up | (↓) move down",
//     // TODO: set timer on new box
//     "(Shift + Enter) add new box",
// ];

#[derive(Debug, Deserialize, Serialize)]
pub struct App {
    list: Vec<Task>,
    #[serde(skip)]
    save_popup: bool,
    #[serde(skip)]
    should_save: bool,
    #[serde(skip)]
    exit: bool,
    #[serde(skip)]
    table_state: TableState,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct Task {
    text: String,
    boxes: Vec<BoxState>,
}
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
enum BoxState {
    Checked(DateTime<Local>),
    Started,
    Empty,
}
impl App {
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
            .map_err(|_| {
                println!(
                    "Failed to parse data from {}, continuing with default",
                    path.display()
                );
            })
            .ok()?;
        app.exit = false;
        println!("Successfully loaded data from {}", path.display());
        Some(app)
    }

    pub fn store(&mut self) {
        let path = Self::get_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            *path.clone(),
            ron::ser::to_string_pretty(self, PrettyConfig::new()).unwrap(),
        )
        .unwrap();
        println!("Stored data to {}", path.display());
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
    fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
        let [area] = Layout::horizontal([horizontal])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
        area
    }

    fn draw(&mut self, frame: &mut Frame) {
        let vertical = &Layout::vertical([Constraint::Min(5), Constraint::Length(4)]);
        let rects = vertical.split(frame.area());
        self.render_table(frame, rects[0]);
        if self.save_popup {
            let block = Block::bordered().title("Popup");
            let area = self.popup_area(frame.area(), 60, 20);
            frame.render_widget(Clear, area); //this clears out the background
            frame.render_widget(block, area);
            let text = Text::raw("exit and save (y/n)?\nESC to cancel");
            let area = Self::center(
                area,
                Constraint::Length(text.width() as u16),
                Constraint::Length(text.height() as u16),
            );
            frame.render_widget(text, area);
        }
    }
    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(Color::LightBlue);
        let bar = " █ ";
        const CHECK: &str = " ☑️ ";
        const STARTED: &str = " 🌟 ";
        const EMPTY: &str = " ➖ ";
        let max_boxes = self.list.iter().map(|t| t.boxes.len()).max().unwrap_or(0) * CHECK.len();
        let t = Table::new(
            self.list.iter().map(|l| {
                let text_cell = Cell::from("\n".to_string() + &l.text + "\n");
                let box_cell = Cell::from(
                    Text::raw(
                        "\n".to_string()
                            + &l.boxes
                                .iter()
                                .rev()
                                .map(|b| match b {
                                    BoxState::Checked(_) => CHECK,
                                    BoxState::Started => STARTED,
                                    BoxState::Empty => EMPTY,
                                })
                                .collect::<String>()
                            + "\n",
                    )
                    .left_aligned(),
                );
                Row::new(vec![text_cell, box_cell])
                    .style(Style::new().bg(Color::Reset))
                    .height(3)
            }),
            [
                Constraint::Percentage(100),
                Constraint::Min(max_boxes.try_into().unwrap()),
            ],
        )
        .row_highlight_style(selected_row_style)
        .highlight_symbol(Text::from(vec!["".into(), bar.into(), "".into()]))
        .highlight_spacing(HighlightSpacing::Always)
        .header(Row::new(vec!["Item"]));
        frame.render_stateful_widget(t, area, &mut self.table_state);
    }
    fn popup_area(&mut self, area: Rect, percent_x: u16, percent_y: u16) -> Rect {
        let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        area
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
        // TODO: have a menu to expand a single row
        if self.save_popup {
            match key_event.code {
                KeyCode::Char('y') => {
                    self.should_save = true;
                    self.exit();
                }
                KeyCode::Char('n') => {
                    self.should_save = false;
                    self.exit();
                }
                KeyCode::Esc => self.save_popup = false,
                _ => {}
            }
            return;
        }
        match key_event.code {
            KeyCode::Esc => self.save_popup = true,
            KeyCode::Down => self.next_row(),
            KeyCode::Up => self.prev_row(),
            KeyCode::Char(' ') => self.handle_box_space(),
            _ => {}
        }
    }
    fn exit(&mut self) {
        self.exit = true;
    }

    fn next_row(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= self.list.len() - 1 {
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
                    self.list.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.list.len() - 1,
        };
        self.table_state.select(Some(i));
    }

    fn handle_box_space(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        let last_mut = self.list[i].boxes.last_mut();
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
                    return;
                }
                BoxState::Started => {
                    *last_mut = BoxState::Checked(Local::now());
                    return;
                }
                _ => (),
            }
        };
        self.list[i].boxes.push(BoxState::Empty);
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            list: vec![
                Task {
                    text: "welcome".to_string(),
                    boxes: vec![BoxState::Empty],
                };
                1
            ],
            exit: false,
            table_state: TableState::default(),
            save_popup: false,
            should_save: false,
        }
    }
}
