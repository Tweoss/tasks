use ratatui::style::Color;

pub mod app;
mod filter;
pub mod popup;
mod table;
pub mod task;

pub const FOCUSED_BORDER: Color = Color::LightBlue;
pub const FOCUSED_BACKGROUND: Color = Color::Blue;
pub const UNFOCUSED_BORDER: Color = Color::DarkGray;
