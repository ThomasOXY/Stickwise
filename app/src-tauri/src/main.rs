#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod jobs;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(jobs::Jobs::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_sticks,
            commands::open_stick,
            commands::cue_summary,
            commands::track_detail,
            commands::verify_stick,
            commands::start_extract,
            commands::cancel_job,
            commands::reveal,
            commands::eject,
            commands::home_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RB-EXPORT");
}
