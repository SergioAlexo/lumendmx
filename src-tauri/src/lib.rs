mod engine;
mod io;
mod output;
mod show;
mod showfile;
mod timing;

use engine::manager::EngineManager;
use output::input::DmxInputManager;
use output::manager::OutputManager;
use show::commands::AppSink;
use show::service::ShowService;
use showfile::store::ShowStore;
use timing::manager::TimingManager;
use std::time::Duration;
use tauri::Manager;

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);

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

            // After the engine and outputs: loading the last session
            // configures universes and hands the engine its patch.
            let store = app.path().app_data_dir().ok().map(ShowStore::new);
            app.manage(ShowService::new(Box::new(AppSink { app: app.handle().clone() }), store));

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(AUTOSAVE_INTERVAL);
                handle.state::<ShowService>().autosave_if_dirty();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            output::commands::list_serial_ports,
            output::commands::output_get_universe_data,
            output::commands::output_universe_status,
            output::commands::output_all_statuses,
            output::commands::output_set_blackout,
            output::commands::output_get_blackout,
            output::commands::output_get_universe_config,
            output::commands::output_driver_defaults,
            output::commands::output_driver_label,
            output::commands::output_discover_artnet_nodes,
            output::commands::output_detect_controllers,
            output::commands::input_start_artnet,
            output::commands::input_get_artnet_frame,
            output::commands::input_start_sacn,
            output::commands::input_stop_sacn,
            output::commands::input_get_sacn_frame,
            engine::commands::engine_get_universe_data,
            engine::commands::engine_set_grand_master,
            engine::commands::engine_grand_master,
            engine::commands::engine_tempo_status,
            engine::commands::engine_set_bpm,
            engine::commands::engine_tap_tempo,
            timing::commands::timing_status,
            timing::commands::timing_configure,
            timing::commands::timing_list_audio_inputs,
            timing::commands::timing_list_midi_inputs,
            show::commands::show_get,
            show::commands::show_fixture_types,
            show::commands::show_set_library,
            show::commands::show_add_fixtures,
            show::commands::show_update_fixture,
            show::commands::show_remove_fixtures,
            show::commands::show_create_group,
            show::commands::show_update_group,
            show::commands::show_delete_group,
            show::commands::show_undo,
            show::commands::show_redo,
            show::commands::show_add_universe,
            show::commands::show_update_universe,
            show::commands::show_remove_universe,
            show::commands::show_rename,
            show::commands::show_new,
            show::commands::show_open,
            show::commands::show_save,
            show::commands::show_recent,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
