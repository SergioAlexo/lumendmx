mod engine;
mod io;
mod output;
mod showfile;
mod timing;

use engine::manager::EngineManager;
use output::input::DmxInputManager;
use output::manager::OutputManager;
use timing::manager::TimingManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let manager = OutputManager::new(app.handle().clone());
            app.manage(manager);
            app.manage(DmxInputManager::new());

            let engine = EngineManager::new();
            engine.start_tick(app.handle().clone());
            app.manage(TimingManager::new(engine.timing_handles()));
            app.manage(engine);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            output::commands::list_serial_ports,
            output::commands::output_configure_universe,
            output::commands::output_remove_universe,
            output::commands::output_update_universe_data,
            output::commands::output_get_universe_data,
            output::commands::output_universe_status,
            output::commands::output_all_statuses,
            output::commands::output_set_blackout,
            output::commands::output_get_blackout,
            output::commands::output_get_universe_config,
            output::commands::output_driver_defaults,
            output::commands::output_driver_label,
            output::commands::output_discover_artnet_nodes,
            output::commands::input_start_artnet,
            output::commands::input_get_artnet_frame,
            output::commands::input_start_sacn,
            output::commands::input_stop_sacn,
            output::commands::input_get_sacn_frame,
            engine::commands::engine_set_htp_channels,
            engine::commands::engine_set_patch_index,
            engine::commands::engine_set_channels,
            engine::commands::engine_get_universe_data,
            engine::commands::engine_record_group,
            engine::commands::engine_list_groups,
            engine::commands::engine_rename_group,
            engine::commands::engine_delete_group,
            engine::commands::engine_export_show_data,
            engine::commands::engine_load_show_data,
            engine::commands::engine_show_data_revision,
            engine::commands::engine_set_grand_master,
            engine::commands::engine_grand_master,
            engine::commands::engine_tempo_status,
            engine::commands::engine_set_bpm,
            engine::commands::engine_tap_tempo,
            timing::commands::timing_status,
            timing::commands::timing_configure,
            timing::commands::timing_list_audio_inputs,
            timing::commands::timing_list_midi_inputs,
            showfile::commands::showfile_new,
            showfile::commands::showfile_open,
            showfile::commands::showfile_save,
            showfile::commands::showfile_autosave,
            showfile::commands::showfile_recent,
            showfile::commands::showfile_restore_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
