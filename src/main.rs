mod config;
mod filter;
mod popup;
mod storage;
mod tui;

use std::{
    cell::RefCell, fs::create_dir_all, path::PathBuf, process::Command, rc::Rc, time::SystemTime,
};

use chrono::{Datelike, Local};
use eyre::Context;
use popup::{AddDialog, SaveDialog};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyEvent, KeyEventKind},
    widgets::Widget,
};

use crate::{
    config::{Config, get_default_app_data_path},
    filter::FilteredData,
    popup::ErrorDialog,
    storage::{Data, Task},
    tui::{
        app::{AppTui, AppWidget},
        task::TaskFocus,
    },
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

    let (mut app, tui, config) = App::load();
    setup_logger(&config).expect("setting up logger");
    let terminal = ratatui::init();
    app.run(terminal, tui);
    ratatui::restore();
}

pub struct App {
    data: FilteredData,
    exit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            data: FilteredData::new(Data::new(get_default_app_data_path(), vec![])),
            exit: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub enum FocusState<'a> {
    // Filter,
    #[default]
    List,
    Task(TaskFocus),
    Popup(PopupEnum<'a>),
}

impl FocusState<'_> {
    fn as_task(&self) -> Option<TaskFocus> {
        match self {
            FocusState::Task(task_focus) => Some(*task_focus),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PopupEnum<'a> {
    WritePopup(SaveDialog),
    AddNew(AddDialog<'a>),
    Error(ErrorDialog<'a>),
}

impl App {
    pub fn load<'a>() -> (Self, AppTui<'a>, Config) {
        let config = match Config::load() {
            Ok(c) => c,
            Err((c, r)) => {
                eprintln!("failed to load config, continuing with default\n{r:?}");
                c
            }
        };
        let mut tui = AppTui::new();
        let data = match Data::load(
            shellexpand::tilde(&config.data_path.to_string_lossy())
                .into_owned()
                .into(),
        ) {
            Ok(d) => d,
            Err((d, e)) => {
                let e = e.wrap_err("Error loading data");
                let error = format!("{:?}", e);
                tui.set_error_focus(e);
                eprintln!("{error}");
                d
            }
        };
        let data = FilteredData::new(data);
        let app: App = App { data, exit: false };
        (app, tui, config)
    }

    fn run(&mut self, mut terminal: DefaultTerminal, tui: AppTui) {
        // Terminal draw needs multiple tui handles.
        let tui = Rc::new(RefCell::new(tui));
        loop {
            let tui = tui.clone();
            terminal
                .draw(|frame| self.draw(frame, tui.clone()))
                .unwrap();
            self.handle_events(tui);
            if self.exit {
                break;
            }
        }
    }
    fn draw<'a>(&mut self, frame: &mut Frame, tui: Rc<RefCell<AppTui<'a>>>) {
        let app_widget = AppWidget(tui.clone(), &self.data);
        app_widget.render(frame.area(), frame.buffer_mut());
    }

    fn handle_events<'a>(&mut self, tui: Rc<RefCell<AppTui<'a>>>) {
        match event::read().unwrap() {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(tui, key_event)
            }
            _ => {}
        };
    }
    fn handle_key_event<'a>(&mut self, tui: Rc<RefCell<AppTui<'a>>>, key_event: KeyEvent) {
        match tui.borrow_mut().handle_key_event(&mut self.data, key_event) {
            Some(tui::app::Action::Exit) => self.exit(),
            Some(tui::app::Action::Unhandled) | None => {}
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

fn setup_logger(config: &Config) -> Result<(), eyre::Report> {
    let time = Local::now();
    let log_file = config.log_path.join(format!(
        "{:04}_{:02}_{:02}.log",
        time.year(),
        time.month(),
        time.day()
    ));
    let log_file = PathBuf::from(shellexpand::tilde(&log_file.to_string_lossy()).into_owned());
    let parent = log_file.parent().unwrap();
    create_dir_all(parent).wrap_err_with(|| format!("creating log parent {}", parent.display()))?;
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .chain(
            fern::log_file(&log_file)
                .wrap_err_with(|| format!("opening {}", log_file.display()))?,
        )
        .apply()?;
    log::info!("Starting logging");
    Ok(())
}
