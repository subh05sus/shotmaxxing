// Release builds on Windows run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A copy the running app starts to read its own Screen Recording permission answers and ends here, before the
    // single-instance check or any window could treat it as a second launch.
    if shotmaxxing_desktop::answer_permission_probe() {
        return;
    }
    shotmaxxing_desktop::run();
}
