pub mod utils;
#[macro_use]
pub mod macros;
#[macro_use]
extern crate log;

mod alt_tap;
mod app;
mod config;
mod foreground;
mod keyboard;
mod painter;
mod startup;
mod trayicon;
mod window_cycle;

pub use crate::app::start;
pub use crate::config::{load_config, Config};
