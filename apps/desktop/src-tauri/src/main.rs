// Release builds on Windows run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    shotmaxxing_desktop::run();
}
