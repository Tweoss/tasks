use std::process::Command;

use chrono::Local;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState, Widget};

use crate::FocusState;
use crate::filter::FilteredData;
use crate::storage::BoxState;

const CHECK: &str = " ✔";
const STARTED: &str = "🌟";
const EMPTY: &str = " -";

pub struct TableTui {
    table_state: TableState,
}

pub enum Action {
    Unhandled,
    Add,
}

impl TableTui {
    pub fn new() -> Self {
        Self {
            table_state: TableState::new(),
        }
    }
    pub fn handle_key_event(
        &mut self,
        data: &mut FilteredData,
        key_event: KeyEvent,
    ) -> Option<Action> {
        let i = self.table_state.selected();
        match key_event.code {
            KeyCode::Down => self.next_row(data),
            KeyCode::Up => self.prev_row(data),
            KeyCode::Char('n') => {
                if let Some(i) = i {
                    data.push_box(i)
                }
            }
            KeyCode::Char('N') => {
                if let Some(i) = i
                    && let Some(BoxState::Started) =
                        data.step_box_state(i, Local::now().naive_local())
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
            KeyCode::Backspace => {
                if let Some(i) = self.table_state.selected() {
                    data.remove_empty_state(i);
                }
            }
            KeyCode::Char('F') => {
                if let Some(i) = self.table_state.selected() {
                    data.set_completed(i, Some(Local::now().naive_local()));
                }
            }
            KeyCode::Char('A') => return Some(Action::Add),
            _ => return Some(Action::Unhandled),
        };
        None
    }
    fn next_row(&mut self, data: &FilteredData) {
        if data.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i + 1 >= data.len() {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }
    pub fn prev_row(&mut self, data: &FilteredData) {
        if data.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    data.len() - 1
                } else {
                    i - 1
                }
            }
            None => data.len() - 1,
        };
        self.table_state.select(Some(i));
    }
    pub fn selected(&self) -> Option<usize> {
        self.table_state.selected()
    }
    pub fn set_selected(&mut self, index: usize) {
        *self.table_state.selected_mut() = Some(index);
    }
}

pub struct TableWidget<'a, 'b>(
    pub &'a mut TableTui,
    pub &'a FocusState<'a>,
    pub &'b FilteredData,
);

impl Widget for TableWidget<'_, '_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let TableWidget(table, focus, data) = self;
        let max_boxes =
            data.iter().map(|t| t.boxes().len()).max().unwrap_or(0) * CHECK.chars().count();
        let list_split = [
            Constraint::Fill(1),
            Constraint::Min(17),
            Constraint::Min(max_boxes.try_into().unwrap()),
        ];
        let rows = data
            .iter()
            .map(|t| {
                let text_cell = Cell::from(t.title());
                let completed_cell = Cell::from(
                    t.completed()
                        .map(|d| d.format("%Y-%m-%m %H:%M").to_string())
                        .unwrap_or_default(),
                )
                .rapid_blink();
                let box_cell = Cell::from(
                    Text::raw(
                        t.boxes()
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
        let selected_row_style = Style::default().fg(Color::White);
        let selected_row_style = match focus {
            FocusState::List => selected_row_style.bg(Color::Blue),
            _ => selected_row_style.bg(Color::DarkGray),
        };
        let t = Table::new(rows, list_split.iter())
            .row_highlight_style(selected_row_style)
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().gray())
            .header(
                Row::new(vec!["Task".bold(), "Completed At".bold(), "Time".bold()])
                    .bottom_margin(1),
            );

        StatefulWidget::render(t, area, buf, &mut table.table_state);
    }
}
