pub mod activity;
pub mod adapter;
pub mod app;
pub mod cli;
pub mod clipboard;
pub mod event;
pub mod group;
pub mod session;
pub mod state;
pub mod time;
pub mod tmux;
pub mod tool_name;
pub mod ui;

pub const SPINNER_ICON: &str = "●";
pub const SPINNER_PULSE: &[u8] = &[82, 78, 114, 150, 186, 150, 114, 78];
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
