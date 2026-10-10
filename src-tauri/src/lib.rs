mod engine;
mod output;
mod reliability;
mod showfile;

use engine::manager::EngineManager;
use output::input::DmxInputManager;
use output::manager::OutputManager;
use reliability::logging::{self, LogRing};
use reliability::prefs::{self, PrefsStore};
use reliability::session::{LiveSessionStore, SessionGuard};
use reliability::watchdog::{self, WatchdogState};
use std::sync::Arc;
use tauri::{Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Logging first, so everything after it is recorded.
            let ring = Arc::new(LogRing::new());
            ring.attach(app.handle().clone());
            if let Ok(dir) = logging::log_dir(app.handle()) {
                if let Some(guard) = logging::init(&dir, ring.clone()) {
                    app.manage(guard);
                }
            }
            app.manage(ring);
            tracing::info!("LumenDMX {} starting", app.package_info().version);

            let data_dir = app.path().app_data_dir()?;
            let session = SessionGuard::begin(&data_dir);
            if session.was_unclean() {
                tracing::warn!("previous run did not shut down cleanly");
            }
            app.manage(session);
            app.manage(LiveSessionStore::default());
            app.manage(WatchdogState::new());

            let prefs_store = PrefsStore::load(data_dir.join("preferences.json"));
            let manager = OutputManager::new(app.handle().clone());
            app.manage(manager);
            app.manage(DmxInputManager::new());

            let engine = EngineManager::new();
            prefs::apply_to_engine(&engine, &prefs_store.get());
            app.manage(engine.show_lock().clone());
            watchdog::spawn(app.handle().clone(), engine.heartbeat());
            engine.start_tick(app.handle().clone());
            app.manage(engine);
            reliability::window::restore(app.handle(), &prefs_store);
            app.manage(prefs_store);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            reliability::show_mode::show_mode_get,
            reliability::show_mode::show_mode_set,
            reliability::prefs::prefs_get,
            reliability::prefs::prefs_set,
            reliability::watchdog::health_report,
            reliability::session::session_status,
            reliability::session::session_clean_shutdown,
            reliability::session::session_set_live,
            reliability::session::session_get_live,
            reliability::logging::log_recent,
            reliability::logging::log_clear,
            reliability::logging::log_write,
            reliability::logging::log_dir_path,
            reliability::diagnostics::diagnostics_export,
            reliability::window::window_monitors,
            reliability::window::window_move_to_monitor,
            reliability::window::window_set_fullscreen,
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
            engine::commands::engine_trigger_asset,
            engine::commands::engine_stop_asset,
            engine::commands::engine_execute_command,
            engine::commands::engine_record_group,
            engine::commands::engine_list_groups,
            engine::commands::engine_rename_group,
            engine::commands::engine_delete_group,
            engine::commands::engine_apply_group_master,
            engine::commands::engine_record_preset,
            engine::commands::engine_update_preset,
            engine::commands::engine_rename_preset,
            engine::commands::engine_delete_preset,
            engine::commands::engine_list_presets,
            engine::commands::engine_apply_preset,
            engine::commands::engine_create_cuelist,
            engine::commands::engine_list_cuelists,
            engine::commands::engine_delete_cuelist,
            engine::commands::engine_rename_cuelist,
            engine::commands::engine_set_cuelist_tracking,
            engine::commands::engine_set_current_cuelist,
            engine::commands::engine_record_cue,
            engine::commands::engine_delete_cue,
            engine::commands::engine_set_cue_preset_reference,
            engine::commands::engine_unlink_cue_preset,
            engine::commands::engine_export_show_data,
            engine::commands::engine_load_show_data,
            engine::commands::engine_show_data_revision,
            engine::commands::engine_set_chase_tempo,
            engine::commands::engine_create_playback,
            engine::commands::engine_delete_playback,
            engine::commands::engine_list_playbacks,
            engine::commands::engine_playback_go,
            engine::commands::engine_playback_go_back,
            engine::commands::engine_playback_release,
            engine::commands::engine_playback_set_paused,
            engine::commands::engine_playback_set_fader,
            engine::commands::engine_playback_set_fader_mode,
            engine::commands::engine_tempo_status,
            engine::commands::engine_set_bpm,
            engine::commands::engine_tap_tempo,
            engine::commands::engine_start_effect,
            engine::commands::engine_update_effect,
            engine::commands::engine_stop_effect,
            engine::commands::engine_list_effects,
            engine::commands::engine_record_effect_preset,
            engine::commands::engine_update_effect_preset,
            engine::commands::engine_create_pixel_map,
            engine::commands::engine_update_pixel_map,
            engine::commands::engine_delete_pixel_map,
            engine::commands::engine_list_pixel_maps,
            engine::commands::engine_import_pixel_map_media,
            engine::commands::engine_start_pixel_map,
            engine::commands::engine_stop_pixel_map,
            engine::commands::engine_set_pixel_map_intensity,
            engine::commands::engine_running_pixel_maps,
            engine::commands::engine_pixel_map_preview,
            showfile::commands::showfile_new,
            showfile::commands::showfile_open,
            showfile::commands::showfile_save,
            showfile::commands::showfile_autosave,
            showfile::commands::showfile_recent,
            showfile::commands::showfile_restore_session,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    reliability::window::save(window.app_handle(), &window.state::<PrefsStore>());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                tracing::info!("clean shutdown");
                app.state::<SessionGuard>().end();
            }
        });
}
