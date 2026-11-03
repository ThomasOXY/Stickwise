#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod edits;
mod jobs;
mod library_cmds;
mod peer;
mod player_cmds;
mod sidecar;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(jobs::Jobs::default())
        .manage(peer::OfferAsk::default())
        .manage(peer::MirrorAsk::default())
        .setup(|app| {
            use tauri::Manager;
            let dir = app.path().app_data_dir()?;
            app.manage(edits::Edits::load(dir.join("cue-edits.json")));
            app.manage(commands::VerifyCache::load(dir.join("verify-cache.json")));
            let lib = library::Library::open(&dir.join("library.sqlite"))?;
            app.manage(library_cmds::LibState(std::sync::Mutex::new(lib)));
            app.manage(peer::start_node(app.handle(), dir.join("peer")));
            app.manage(sidecar::Sidecar::new(dir, app.path().resource_dir().ok()));
            peer::start_fingerprints(app.handle().clone());
            Ok(())
        })
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
            commands::save_cues,
            commands::revert_cues,
            commands::pending_cues,
            commands::write_cues,
            commands::cue_palette,
            commands::smart_cue,
            commands::smart_cue_apply,
            commands::ai_status,
            commands::ai_analyze,
            library_cmds::library_data,
            library_cmds::library_import,
            library_cmds::library_create_playlist,
            library_cmds::library_rename_playlist,
            library_cmds::library_delete_playlist,
            library_cmds::library_move_playlist,
            library_cmds::library_add_folders,
            library_cmds::library_set_grid,
            library_cmds::library_reset_grid,
            library_cmds::library_analyze,
            library_cmds::library_order_playlists,
            library_cmds::library_set_tracks,
            library_cmds::library_track_detail,
            library_cmds::library_save_cues,
            library_cmds::library_cue_summary,
            library_cmds::library_smart_cue,
            library_cmds::library_smart_cue_apply,
            library_cmds::library_ai_status,
            library_cmds::library_ai_analyze,
            library_cmds::sync_selection,
            library_cmds::sync_plan,
            library_cmds::sync_start,
            library_cmds::engine_status,
            library_cmds::plan_set,
            library_cmds::apply_set_plan,
            library_cmds::set_cue_backups,
            library_cmds::restore_set_cues,
            player_cmds::settings_schema,
            player_cmds::settings_get,
            player_cmds::settings_save,
            player_cmds::settings_write,
            player_cmds::list_disks,
            player_cmds::format_start,
            peer::peer_settings,
            peer::peer_set_music_dir,
            peer::peer_list,
            peer::peer_pair,
            peer::peer_answer,
            peer::peer_unpair,
            peer::peer_ping,
            peer::peer_send,
            peer::peer_answer_offer,
            peer::peer_mirror,
            peer::peer_mirror_answer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Stickwise");
}
