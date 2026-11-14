// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(unix)]
    if let Some(code) = privavault_lib::cli::main() {
        std::process::exit(code);
    }
    privavault_lib::run()
}
