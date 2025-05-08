mod list;
mod popup;

use std::{path::PathBuf, process::Command};

use chrono::{DateTime, Local};
use list::{List, ListAction};
use popup::{Popup, PopupAction};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
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

const CHECK: &str = " ✔ ";
const STARTED: &str = "🌟 ";
const EMPTY: &str = " - ";

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct App {
    #[serde(flatten)]
    list: List,
    popup: Popup,
    completed: Vec<(Task, DateTime<Local>)>,
    #[serde(skip)]
    focus: FocusState,
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
#[derive(Default, Debug, Deserialize, Serialize, Clone, Copy)]
enum FocusState {
    #[default]
    List,
    SavePopup,
    TaskDetails,
    CompletedList,
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
    fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
        let [area] = Layout::horizontal([horizontal])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
        area
    }

    fn draw(&mut self, frame: &mut Frame) {
        let vertical = &Layout::vertical([Constraint::Max(3), Constraint::Fill(1)]);
        let rects = vertical.split(frame.area());
        let text = Text::raw("\nTasks List. hit 1 to focus list, 2 to focus completed");
        frame.render_widget(text, rects[0]);

        let max_boxes = self
            .list
            .list
            .iter()
            .map(|t| t.boxes.len())
            .max()
            .unwrap_or(0)
            * CHECK.chars().count();
        let widths = [
            Constraint::Percentage(100),
            Constraint::Min(max_boxes.try_into().unwrap()),
        ];

        match self.focus {
            FocusState::List => self.list.render(frame, rects[1]),
            FocusState::SavePopup => {
                self.list.render(frame, rects[1]);
                self.popup.render(frame, frame.area());
            }
            FocusState::TaskDetails => {
                let layout = Layout::new(
                    Direction::Horizontal,
                    [Constraint::Fill(1), Constraint::Fill(1)],
                )
                .split(rects[1]);
                self.list.render(frame, layout[0]);
                // self.render_task(frame, layout[1]);
            }
            FocusState::CompletedList => {
                let rows = self
                    .completed
                    .iter()
                    .map(|(t, d)| {
                        let text_cell = Cell::from("\n".to_string() + &t.text + "\n");
                        let box_cell = Cell::from(
                            Text::raw(
                                "\n".to_string()
                                    + &t.boxes
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
                        let completed_cell =
                            Cell::from(d.format("\n%d/%m/%Y %H:%M").to_string()).rapid_blink();
                        Row::new(vec![text_cell, completed_cell, box_cell])
                            .style(Style::new().bg(Color::Reset))
                            .height(3)
                    })
                    .collect::<Vec<_>>();
                self.render_table(
                    frame,
                    rects[1],
                    rows.into_iter(),
                    &[widths[0], Constraint::Min(17), widths[1]],
                );
            }
        };
    }
    // TODO: extract table and expanded view to reuse for completed list
    fn render_table<'a>(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        rows: impl Iterator<Item = Row<'a>>,
        widths: &[Constraint],
    ) {
        let selected_row_style = Style::default().fg(Color::White).bg(Color::Blue);
        let bar = " █ ";
        let t = Table::new(rows, widths)
            .row_highlight_style(selected_row_style)
            .highlight_symbol(Text::from(vec!["".into(), bar.into(), "".into()]))
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().gray())
            .header(
                Row::new(vec!["Task".bold(), "Completed At".bold(), "Time".bold()])
                    .bottom_margin(1),
            );
        frame.render_stateful_widget(t, area, &mut self.table_state);
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
        use ListAction as LA;
        use PopupAction as PU;
        match self.focus {
            FocusState::List => match (self.list.handle_key(key_event), key_event.code) {
                (LA::Handled, _) => {}
                (LA::MarkCompleted(i), _) => self.handle_completed(i),

                (LA::Unhandled, KeyCode::Esc) => self.focus = FocusState::SavePopup,
                (LA::Unhandled, KeyCode::Char('2')) => self.focus = FocusState::CompletedList,
                (LA::Unhandled, _) => {}
            },
            FocusState::SavePopup => match (self.popup.handle_key(key_event), key_event.code) {
                (PU::Handled, _) => {}
                (PU::ExitNoWrite, _) => self.exit(),
                (PU::Write, _) => {
                    self.store();
                    self.focus = FocusState::List;
                }
                (PU::Exit, _) => {
                    self.store();
                    self.exit();
                }
                (PU::Unhandled, KeyCode::Esc) => self.focus = FocusState::List,
                (PU::Unhandled, _) => {}
            },
            FocusState::TaskDetails => todo!(),
            // TODO: separate row states
            FocusState::CompletedList => match key_event.code {
                KeyCode::Char('1') => self.focus = FocusState::List,
                _ => {}
            },
        }
    }
    fn exit(&mut self) {
        self.exit = true;
    }

    // fn render_task(&mut self, frame: &mut Frame<'_>, layout: Rect) {
    //     let Some(i) = self.table_state.selected() else {
    //         return;
    //     };
    //     // TODO: have a recent context editor
    //     let layout = Layout::new(
    //         Direction::Vertical,
    //         [Constraint::Length(5), Constraint::Min(0)],
    //     )
    //     .split(layout);
    //     let task = &self.list[i];
    //     let block = Block::default().title("Header").borders(Borders::ALL);
    //     // block.
    //     // frame.render_widget(widget, area);
    //     // task.text
    //     // se
    // }

    fn handle_completed(&mut self, i: usize) {
        let task = self.list.list.remove(i);
        let time = Local::now();
        self.completed.push((task, time));
    }
}
