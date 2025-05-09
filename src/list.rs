use std::process::Command;

use chrono::Local;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
};
use serde::{Deserialize, Serialize};

use crate::{BoxState, CHECK, EMPTY, STARTED, Task};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct List {
    pub list: Vec<Task>,
    #[serde(skip)]
    table_state: TableState,
}

pub enum ListAction {
    MarkCompleted(usize),
    Unhandled,
    Handled,
}

impl List {
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self
            .list
            .iter()
            .map(|l| {
                let text_cell = Cell::from("\n".to_string() + &l.text + "\n").gray();
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
            })
            .collect::<Vec<_>>();

        const TIME: &str = "Time";
        let max_boxes = (self.list.iter().map(|t| t.boxes.len()).max().unwrap_or(0)
            * CHECK.chars().count())
        .max(TIME.chars().count() + 1);
        let widths = [
            Constraint::Percentage(100),
            Constraint::Min(max_boxes.try_into().unwrap()),
        ];
        let selected_row_style = Style::default().fg(Color::White).bg(Color::Blue);
        let bar = " █ ";
        let t = Table::new(
            rows, // TODO: handle?
            widths,
        )
        .row_highlight_style(selected_row_style)
        .highlight_symbol(Text::from(vec!["".into(), bar.into(), "".into()]))
        .highlight_spacing(HighlightSpacing::Always)
        .block(Block::bordered().gray())
        .header(Row::new(vec!["Task".bold(), TIME.bold()]).bottom_margin(1));
        frame.render_stateful_widget(t, area, &mut self.table_state);
    }

    pub fn handle_key(&mut self, key_event: KeyEvent) -> ListAction {
        match key_event.code {
            KeyCode::Down => self.next_row(),
            KeyCode::Up => self.prev_row(),
            KeyCode::Char(' ') if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.handle_new_empty()
            }
            KeyCode::Char(' ') => self.handle_box_step(),
            KeyCode::Char('F') => {
                if let Some(i) = self.table_state.selected() {
                    return ListAction::MarkCompleted(i);
                }
            }
            _ => return ListAction::Unhandled,
        }
        ListAction::Handled
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

    fn handle_new_empty(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        self.list[i].boxes.push(BoxState::Empty);
    }

    fn handle_box_step(&mut self) {
        let Some(i) = self.table_state.selected() else {
            return;
        };
        let last_mut = self.list[i]
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
}
