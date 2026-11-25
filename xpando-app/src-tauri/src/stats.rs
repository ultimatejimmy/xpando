//! Commands for the "Your savings" page. The maths and the files live in `xpando_core::stats`;
//! this file only finds the folders. The engine writes the log (see `engine_run::stats_dir`).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};
use xpando_core::stats::{self, Range, StatsSettings, Summary};

use crate::engine_run::stats_dir;

fn settings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

/// Time and money saved for a period. `tz_offset_min` is how many minutes the user's clock is
/// ahead of UTC (the page knows it), so days start at the user's midnight.
#[tauri::command]
pub fn stats_summary(app: AppHandle, range: Range, tz_offset_min: i32) -> Result<Summary, String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    stats::summarize(
        &stats_dir(&app)?,
        &settings_dir(&app)?,
        range,
        now_ms,
        tz_offset_min.clamp(-14 * 60, 14 * 60),
        8,
    )
}

#[tauri::command]
pub fn stats_save_settings(
    app: AppHandle,
    settings: StatsSettings,
) -> Result<StatsSettings, String> {
    stats::save_settings(&settings_dir(&app)?, &settings)
}

/// Turns logging on or off. The engine looks for the switch on every expansion, so no restart.
#[tauri::command]
pub fn stats_set_enabled(app: AppHandle, on: bool) -> Result<(), String> {
    stats::set_enabled(&stats_dir(&app)?, on)
}

/// Deletes the logged expansions.
#[tauri::command]
pub fn stats_reset(app: AppHandle) -> Result<(), String> {
    stats::reset(&stats_dir(&app)?)
}
