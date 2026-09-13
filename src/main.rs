#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod hotkey;
mod icon;
mod layout;
mod tray;
mod win;
mod window;

fn main() -> eframe::Result {
    app::run()
}
