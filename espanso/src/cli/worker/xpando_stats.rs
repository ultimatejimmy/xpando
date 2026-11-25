//! xpando's usage statistics: what the engine writes for the app's "Your savings" page.
//!
//! Active only when the app starts the engine with `XPANDO_STATS_DIR` set, so a stock espanso
//! (which has its own `stats` feature) is untouched. Two small files, both private to this
//! computer and never holding what was typed:
//!
//! * `expansions.log`: one line per expansion, `unix_ms<TAB>chars<TAB>trigger`.
//! * `typing.txt`: `chars active_ms`, a rolling total the app turns into words per minute.
//!
//! Typing speed is measured passively from key presses the engine already receives. Only the
//! time between presses and a count of printable characters are kept, never which keys. A pause
//! longer than two seconds ends a burst and is not counted, and presses closer than 25 ms
//! (a held key, a macro) are ignored. Nothing is measured while the engine is typing an
//! expansion itself.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use espanso_engine::process::{StatsRecord, StatsRecorder};

const PAUSE: Duration = Duration::from_millis(2000);
const TOO_FAST: Duration = Duration::from_millis(25);
const FLUSH_EVERY: Duration = Duration::from_secs(60);
/// When the totals pass this, both are halved so the speed follows recent typing.
const HALVE_AT_CHARS: u64 = 40_000;

struct Typing {
    dir: PathBuf,
    chars: u64,
    active_ms: u64,
    last_key: Option<Instant>,
    quiet_until: Instant,
    last_flush: Instant,
    dirty: bool,
}

static TYPING: Mutex<Option<Typing>> = Mutex::new(None);

fn dir_from_env() -> Option<PathBuf> {
    let dir = std::env::var_os("XPANDO_STATS_DIR").map(PathBuf::from)?;
    if dir.as_os_str().is_empty() {
        return None;
    }
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Whether the app asked for statistics. Call once at start-up; then installs the recorder.
pub fn init() -> bool {
    let Some(dir) = dir_from_env() else {
        return false;
    };
    let (chars, active_ms) = fs::read_to_string(dir.join("typing.txt"))
        .ok()
        .and_then(|t| {
            let mut it = t.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .unwrap_or((0, 0));
    let now = Instant::now();
    *TYPING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Typing {
        dir,
        chars,
        active_ms,
        last_key: None,
        quiet_until: now,
        last_flush: now,
        dirty: false,
    });
    espanso_engine::process::set_global_recorder(Box::new(XpandoStatsRecorder));
    true
}

fn off(dir: &std::path::Path) -> bool {
    dir.join("off").exists()
}

pub struct XpandoStatsRecorder;

impl StatsRecorder for XpandoStatsRecorder {
    fn record(&self, record: StatsRecord) {
        let mut guard = TYPING.lock().unwrap_or_else(|e| e.into_inner());
        let Some(t) = guard.as_mut() else { return };
        // The engine is about to type: do not take its own key presses for the user's.
        t.quiet_until = Instant::now() + Duration::from_millis(1500 + record.chars as u64 * 40);
        t.last_key = None;
        if off(&t.dir) {
            return;
        }
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let trigger: String = record
            .trigger
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(t.dir.join("expansions.log"))
        {
            let _ = writeln!(f, "{ms}\t{}\t{trigger}", record.chars);
        }
    }
}

/// A key was pressed. `value` is the character it types, if any; shortcuts and navigation keys
/// have none or a control character and are not counted.
pub fn key_pressed(value: Option<&str>) {
    let Some(v) = value else { return };
    let mut chars = v.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return;
    };
    if c.is_control() {
        return;
    }
    let now = Instant::now();
    let mut guard = TYPING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(t) = guard.as_mut() else { return };
    if now < t.quiet_until {
        return;
    }
    if let Some(prev) = t.last_key {
        let gap = now.duration_since(prev);
        if gap < TOO_FAST {
            return;
        }
        if gap <= PAUSE {
            t.active_ms += gap.as_millis() as u64;
            t.chars += 1;
            t.dirty = true;
        }
    }
    t.last_key = Some(now);
    if t.chars > HALVE_AT_CHARS {
        t.chars /= 2;
        t.active_ms /= 2;
    }
    if t.dirty && now.duration_since(t.last_flush) >= FLUSH_EVERY && !off(&t.dir) {
        let _ = fs::write(
            t.dir.join("typing.txt"),
            format!("{} {}\n", t.chars, t.active_ms),
        );
        t.last_flush = now;
        t.dirty = false;
    }
}
