//! "Your savings": reads what the engine logged and works out time and money saved.
//!
//! The engine (see `espanso/src/cli/worker/xpando_stats.rs`) appends one line per expansion to
//! `expansions.log` (`unix_ms<TAB>chars<TAB>trigger`) and keeps a rolling typing total in
//! `typing.txt` (`chars active_ms`). Creating a file called `off` in that folder pauses logging.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

const LOG: &str = "expansions.log";
const TYPING: &str = "typing.txt";
const OFF: &str = "off";
const SETTINGS_FILE: &str = "stats-settings.yml";
const MS_PER_DAY: i64 = 86_400_000;
/// Typing speed to assume until enough typing has been measured.
pub const FALLBACK_WPM: f64 = 40.0;
/// Characters of typing needed before the measured speed is trusted.
const MIN_SAMPLE_CHARS: u64 = 1_000;
/// Days shown in the chart.
const CHART_DAYS: i64 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PayType {
    #[default]
    Hourly,
    Salaried,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpeedMode {
    #[default]
    Auto,
    Manual,
}

/// What the user entered. Stored beside the app's other settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsSettings {
    #[serde(default)]
    pub pay_type: PayType,
    #[serde(default)]
    pub hourly_rate: f64,
    #[serde(default)]
    pub annual_salary: f64,
    #[serde(default = "default_hours")]
    pub hours_per_week: f64,
    #[serde(default)]
    pub speed_mode: SpeedMode,
    #[serde(default = "default_manual")]
    pub manual_wpm: f64,
}

fn default_hours() -> f64 {
    40.0
}
fn default_manual() -> f64 {
    FALLBACK_WPM
}

impl Default for StatsSettings {
    fn default() -> Self {
        Self {
            pay_type: PayType::Hourly,
            hourly_rate: 0.0,
            annual_salary: 0.0,
            hours_per_week: default_hours(),
            speed_mode: SpeedMode::Auto,
            manual_wpm: default_manual(),
        }
    }
}

impl StatsSettings {
    /// Keeps entries sane: finite, not negative, and in a range a person could mean.
    pub fn cleaned(mut self) -> Self {
        let fix = |v: f64, max: f64| {
            if v.is_finite() {
                v.clamp(0.0, max)
            } else {
                0.0
            }
        };
        self.hourly_rate = fix(self.hourly_rate, 10_000.0);
        self.annual_salary = fix(self.annual_salary, 100_000_000.0);
        self.hours_per_week = fix(self.hours_per_week, 168.0);
        self.manual_wpm = fix(self.manual_wpm, 400.0).max(1.0);
        self
    }

    /// Pay per hour, or `None` when none was entered.
    pub fn hourly(&self) -> Option<f64> {
        let rate = match self.pay_type {
            PayType::Hourly => self.hourly_rate,
            PayType::Salaried => {
                if self.hours_per_week > 0.0 {
                    self.annual_salary / (self.hours_per_week * 52.0)
                } else {
                    0.0
                }
            }
        };
        (rate > 0.0).then_some(rate)
    }
}

pub fn load_settings(dir: &Path) -> Result<StatsSettings, String> {
    match fs::read_to_string(dir.join(SETTINGS_FILE)) {
        Ok(t) if t.trim().is_empty() => Ok(StatsSettings::default()),
        Ok(t) => Ok(serde_norway::from_str::<StatsSettings>(&t)
            .map_err(|e| format!("The savings settings could not be understood ({e})."))?
            .cleaned()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(StatsSettings::default()),
        Err(e) => Err(format!("Could not read the savings settings: {e}")),
    }
}

pub fn save_settings(dir: &Path, s: &StatsSettings) -> Result<StatsSettings, String> {
    let s = s.clone().cleaned();
    fs::create_dir_all(dir).map_err(|e| format!("Could not create the settings folder: {e}"))?;
    let text = serde_norway::to_string(&s).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&tmp, text).map_err(|e| format!("Could not save the savings settings: {e}"))?;
    fs::rename(&tmp, dir.join(SETTINGS_FILE)).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Could not save the savings settings: {e}")
    })?;
    Ok(s)
}

/// Whether the engine is logging.
pub fn is_enabled(stats_dir: &Path) -> bool {
    !stats_dir.join(OFF).exists()
}

pub fn set_enabled(stats_dir: &Path, on: bool) -> Result<(), String> {
    if on {
        match fs::remove_file(stats_dir.join(OFF)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Could not turn statistics on: {e}")),
        }
    } else {
        fs::create_dir_all(stats_dir).map_err(|e| format!("Could not turn statistics off: {e}"))?;
        fs::write(stats_dir.join(OFF), b"")
            .map_err(|e| format!("Could not turn statistics off: {e}"))
    }
}

/// Deletes the logged expansions (the measured typing speed stays).
pub fn reset(stats_dir: &Path) -> Result<(), String> {
    match fs::remove_file(stats_dir.join(LOG)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Could not reset statistics: {e}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    ms: i64,
    chars: u64,
    trigger: String,
}

fn parse_log(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.splitn(3, '\t');
            let ms = it.next()?.parse().ok()?;
            let chars = it.next()?.parse().ok()?;
            Some(Entry {
                ms,
                chars,
                trigger: it.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Measured typing speed in words per minute, once enough has been typed.
pub fn measured_wpm(stats_dir: &Path) -> Option<f64> {
    let text = fs::read_to_string(stats_dir.join(TYPING)).ok()?;
    let mut it = text.split_whitespace();
    let chars: u64 = it.next()?.parse().ok()?;
    let active_ms: u64 = it.next()?.parse().ok()?;
    wpm_from(chars, active_ms)
}

fn wpm_from(chars: u64, active_ms: u64) -> Option<f64> {
    if chars < MIN_SAMPLE_CHARS || active_ms == 0 {
        return None;
    }
    let wpm = (chars as f64 / 5.0) / (active_ms as f64 / 60_000.0);
    Some(wpm.clamp(10.0, 250.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Range {
    Today,
    Week,
    Month,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayCount {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopPhrase {
    pub trigger: String,
    pub count: u32,
    pub seconds_saved: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    /// Logging is on.
    pub enabled: bool,
    pub expansions: u32,
    pub chars: u64,
    pub seconds_saved: f64,
    /// Time saved over the period just before this one (none for "all" and "today").
    pub previous_seconds_saved: Option<f64>,
    pub money_saved: Option<f64>,
    pub hourly_rate: Option<f64>,
    pub wpm: f64,
    /// `wpm` was measured (not typed in, not the starting guess).
    pub wpm_measured: bool,
    pub best_day: u32,
    /// The last 14 days (fewer when "all" and the log is younger), oldest first.
    pub days: Vec<DayCount>,
    pub top: Vec<TopPhrase>,
    pub settings: StatsSettings,
}

/// Seconds of typing an expansion replaced: the inserted characters minus the trigger typed.
fn saved_seconds(chars: u64, trigger: &str, wpm: f64) -> f64 {
    let typed = trigger.chars().count() as u64;
    let saved_chars = chars.saturating_sub(typed);
    (saved_chars as f64 / 5.0) / wpm * 60.0
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn date_string(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Builds the summary. `now_ms` is the current time and `tz_offset_min` the minutes the user's
/// clock is ahead of UTC, so days start at the user's midnight.
pub fn summarize(
    stats_dir: &Path,
    settings_dir: &Path,
    range: Range,
    now_ms: i64,
    tz_offset_min: i32,
    top_n: usize,
) -> Result<Summary, String> {
    let settings = load_settings(settings_dir)?;
    let text = match fs::read_to_string(stats_dir.join(LOG)) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read the statistics: {e}")),
    };
    Ok(build(
        &parse_log(&text),
        settings,
        measured_wpm(stats_dir),
        is_enabled(stats_dir),
        range,
        now_ms,
        tz_offset_min,
        top_n,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build(
    entries: &[Entry],
    settings: StatsSettings,
    measured: Option<f64>,
    enabled: bool,
    range: Range,
    now_ms: i64,
    tz_offset_min: i32,
    top_n: usize,
) -> Summary {
    let (wpm, wpm_measured) = match (settings.speed_mode, measured) {
        (SpeedMode::Manual, _) => (settings.manual_wpm, false),
        (SpeedMode::Auto, Some(w)) => (w, true),
        (SpeedMode::Auto, None) => (FALLBACK_WPM, false),
    };
    let day_of = |ms: i64| (ms + i64::from(tz_offset_min) * 60_000).div_euclid(MS_PER_DAY);
    let today = day_of(now_ms);
    let span_days: Option<i64> = match range {
        Range::Today => Some(1),
        Range::Week => Some(7),
        Range::Month => Some(30),
        Range::All => None,
    };
    let start_day = span_days.map(|n| today - n + 1);
    let current: Vec<&Entry> = entries
        .iter()
        .filter(|e| {
            let d = day_of(e.ms);
            d <= today && start_day.is_none_or(|s| d >= s)
        })
        .collect();

    let mut chars = 0u64;
    let mut seconds = 0.0;
    let mut per_trigger: HashMap<&str, (u32, f64)> = HashMap::new();
    let mut per_day: HashMap<i64, u32> = HashMap::new();
    for e in &current {
        chars += e.chars;
        let s = saved_seconds(e.chars, &e.trigger, wpm);
        seconds += s;
        let slot = per_trigger.entry(e.trigger.as_str()).or_default();
        slot.0 += 1;
        slot.1 += s;
        *per_day.entry(day_of(e.ms)).or_default() += 1;
    }

    let previous = match (span_days, start_day) {
        (Some(n), Some(s)) if n > 1 => Some(
            entries
                .iter()
                .filter(|e| {
                    let d = day_of(e.ms);
                    d >= s - n && d < s
                })
                .map(|e| saved_seconds(e.chars, &e.trigger, wpm))
                .sum(),
        ),
        _ => None,
    };

    // The chart always shows the last 14 days, whatever the range; "all" starts at the first
    // logged day when that is more recent.
    let mut chart_first = today - (CHART_DAYS - 1);
    if range == Range::All {
        if let Some(first) = entries.iter().map(|e| day_of(e.ms)).min() {
            chart_first = chart_first.max(first.min(today));
        }
    }
    let mut chart_counts: HashMap<i64, u32> = HashMap::new();
    for e in entries {
        *chart_counts.entry(day_of(e.ms)).or_default() += 1;
    }
    let days = (chart_first..=today)
        .map(|d| DayCount {
            date: date_string(d),
            count: chart_counts.get(&d).copied().unwrap_or(0),
        })
        .collect();

    let mut top: Vec<TopPhrase> = per_trigger
        .into_iter()
        .filter(|(t, _)| !t.is_empty())
        .map(|(t, (count, secs))| TopPhrase {
            trigger: t.to_string(),
            count,
            seconds_saved: secs,
        })
        .collect();
    top.sort_by(|a, b| {
        b.seconds_saved
            .total_cmp(&a.seconds_saved)
            .then(b.count.cmp(&a.count))
            .then(a.trigger.cmp(&b.trigger))
    });
    top.truncate(top_n);

    let hourly = settings.hourly();
    Summary {
        enabled,
        expansions: current.len() as u32,
        chars,
        seconds_saved: seconds,
        previous_seconds_saved: previous,
        money_saved: hourly.map(|r| seconds / 3600.0 * r),
        hourly_rate: hourly,
        wpm,
        wpm_measured,
        best_day: per_day.values().copied().max().unwrap_or(0),
        days,
        top,
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = MS_PER_DAY;
    // 2026-11-25 12:00 UTC
    const NOW: i64 = 20_782 * DAY + 12 * 3_600_000;

    fn e(day_offset: i64, chars: u64, trigger: &str) -> Entry {
        Entry {
            ms: NOW + day_offset * DAY,
            chars,
            trigger: trigger.to_string(),
        }
    }

    fn manual(rate: f64) -> StatsSettings {
        StatsSettings {
            hourly_rate: rate,
            speed_mode: SpeedMode::Manual,
            manual_wpm: 50.0,
            ..Default::default()
        }
    }

    fn run(entries: &[Entry], s: StatsSettings, range: Range, tz: i32, top: usize) -> Summary {
        build(entries, s, None, true, range, NOW, tz, top)
    }

    #[test]
    fn dates() {
        assert_eq!(date_string(20_782), "2026-11-25");
        assert_eq!(date_string(0), "1970-01-01");
        assert_eq!(date_string(19_782), "2024-02-29");
    }

    #[test]
    fn parses_and_skips_bad_lines() {
        let got = parse_log("1\t10\t;sig\nnonsense\n2\t5\t\n3\tx\t;a\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].trigger, "");
    }

    #[test]
    fn time_and_money() {
        // 260 chars inserted, trigger ";sig" (4) typed: 256 / 5 = 51.2 words at 50 wpm.
        let s = run(&[e(0, 260, ";sig")], manual(60.0), Range::Today, 0, 5);
        assert_eq!(s.expansions, 1);
        assert!((s.seconds_saved - 61.44).abs() < 0.01);
        assert!((s.money_saved.unwrap() - 61.44 / 3600.0 * 60.0).abs() < 1e-9);
        assert!(!s.wpm_measured);
    }

    #[test]
    fn ranges_and_previous() {
        let entries = [
            e(0, 100, ";a"),
            e(-3, 100, ";a"),
            e(-10, 100, ";b"),
            e(-40, 100, ";b"),
        ];
        let week = run(&entries, manual(0.0), Range::Week, 0, 5);
        assert_eq!(week.expansions, 2);
        assert!(week.previous_seconds_saved.unwrap() > 0.0);
        assert_eq!(run(&entries, manual(0.0), Range::Month, 0, 5).expansions, 3);
        let all = run(&entries, manual(0.0), Range::All, 0, 5);
        assert_eq!(all.expansions, 4);
        assert!(all.previous_seconds_saved.is_none());
        assert!(week.money_saved.is_none());
    }

    #[test]
    fn timezone_moves_the_day() {
        // 23:30 UTC the day before "now"; at UTC+2 it is already today.
        let late = Entry {
            ms: NOW - 12 * 3_600_000 - 30 * 60_000,
            chars: 50,
            trigger: ";x".into(),
        };
        let utc = run(std::slice::from_ref(&late), manual(0.0), Range::Today, 0, 5);
        let plus2 = run(&[late], manual(0.0), Range::Today, 120, 5);
        assert_eq!(utc.expansions, 0);
        assert_eq!(plus2.expansions, 1);
    }

    #[test]
    fn top_is_sorted_and_skips_untyped() {
        let entries = [
            e(0, 500, ";big"),
            e(0, 50, ";small"),
            e(0, 50, ";small"),
            e(0, 900, ""),
        ];
        let s = run(&entries, manual(0.0), Range::Today, 0, 2);
        assert_eq!(s.top.len(), 2);
        assert_eq!(s.top[0].trigger, ";big");
        assert_eq!(s.best_day, 4);
        assert_eq!(s.days.len(), 14);
        assert_eq!(s.days.last().unwrap().count, 4);
    }

    #[test]
    fn typing_speed_needs_a_sample() {
        assert_eq!(wpm_from(500, 60_000), None);
        // 3000 chars = 600 words in 10 minutes = 60 wpm.
        assert!((wpm_from(3_000, 600_000).unwrap() - 60.0).abs() < 1e-9);
        assert_eq!(wpm_from(10_000, 1), Some(250.0));
    }

    #[test]
    fn auto_uses_measured_then_falls_back() {
        let auto = StatsSettings::default();
        let s = build(&[], auto.clone(), Some(72.0), true, Range::All, NOW, 0, 5);
        assert_eq!((s.wpm, s.wpm_measured), (72.0, true));
        let s = build(&[], auto, None, true, Range::All, NOW, 0, 5);
        assert_eq!((s.wpm, s.wpm_measured), (FALLBACK_WPM, false));
    }

    #[test]
    fn salary_becomes_an_hourly_rate() {
        let s = StatsSettings {
            pay_type: PayType::Salaried,
            annual_salary: 83_200.0,
            hours_per_week: 40.0,
            ..Default::default()
        };
        assert!((s.hourly().unwrap() - 40.0).abs() < 1e-9);
        let none = StatsSettings {
            hours_per_week: 0.0,
            ..s
        };
        assert!(none.hourly().is_none());
    }

    #[test]
    fn settings_are_cleaned() {
        let s = StatsSettings {
            hourly_rate: f64::NAN,
            manual_wpm: -3.0,
            hours_per_week: 999.0,
            ..Default::default()
        }
        .cleaned();
        assert_eq!(s.hourly_rate, 0.0);
        assert_eq!(s.manual_wpm, 1.0);
        assert_eq!(s.hours_per_week, 168.0);
    }

    #[test]
    fn files_round_trip() {
        let dir = std::env::temp_dir().join(format!("xpando-stats-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let saved = save_settings(&dir, &manual(25.0)).unwrap();
        assert_eq!(load_settings(&dir).unwrap(), saved);
        assert!(is_enabled(&dir));
        set_enabled(&dir, false).unwrap();
        assert!(!is_enabled(&dir));
        set_enabled(&dir, true).unwrap();
        assert!(is_enabled(&dir));
        fs::write(dir.join(LOG), "1\t5\t;a\n").unwrap();
        reset(&dir).unwrap();
        assert!(!dir.join(LOG).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
