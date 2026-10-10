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
        .plugin(tauri_plugin_http::init())
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
            engine::commands::engine_get_universe_sources,
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
            engine::commands::engine_assign_playback,
            engine::commands::engine_playback_go_to,
            engine::commands::engine_playback_release_all,
            engine::commands::engine_set_page,
            engine::commands::engine_set_release_on_page_change,
            engine::commands::engine_update_cue,
            engine::commands::engine_console_status,
            engine::commands::engine_set_grand_master,
            engine::commands::engine_set_rate_master,
            engine::commands::engine_set_blind,
            engine::commands::engine_set_highlight,
            engine::commands::engine_tempo_status,
            engine::commands::engine_set_bpm,
            engine::commands::engine_tap_tempo,
            engine::commands::engine_set_cue_timecode,
            timing::commands::timing_status,
            timing::commands::timing_configure,
            timing::commands::timing_list_audio_inputs,
            timing::commands::timing_list_midi_inputs,
            timing::commands::timing_list_triggers,
            timing::commands::timing_upsert_trigger,
            timing::commands::timing_delete_trigger,
            timing::commands::timing_timecode_play,
            timing::commands::timing_timecode_pause,
            timing::commands::timing_timecode_stop,
            timing::commands::timing_timecode_seek,
            timing::commands::timing_set_timecode_learn,
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
