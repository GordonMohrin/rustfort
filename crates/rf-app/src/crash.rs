//! [app10] Crash reporting: a panic hook that writes every panic (thread, place, message,
//! backtrace) to `saves/crash.log` and keeps the last report for the crash dialog.
//!
//! Panics of the simulation thread and of the interface code are caught (`simthread`,
//! `game::run`), so the window stays open and the crash dialog (`ui::round10`) can save the
//! last good in-memory snapshot of the fort instead of losing the game.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// The last panic report (set by the hook, read by the crash dialog).
static LAST: Mutex<Option<String>> = Mutex::new(None);

/// `saves/crash.log` (or `$RUSTFORT_SAVE_DIR/crash.log`).
pub fn log_path() -> PathBuf {
    crate::save::save_dir().join("crash.log")
}

/// Append a block of text to the crash log (best effort; never panics).
pub fn append_log(text: &str) {
    let p = log_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(text.as_bytes());
        if !text.ends_with('\n') {
            let _ = f.write_all(b"\n");
        }
    }
}

/// Install the panic hook (once, at start). The default hook still prints to stderr.
pub fn install_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown place".into());
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        let bt = std::backtrace::Backtrace::force_capture();
        let when = crate::save::fmt_unix(crate::save::now_unix());
        let report = format!("=== {when}: panic in thread '{thread}' at {place}\n{msg}\n");
        append_log(&format!("{report}backtrace:\n{bt}\n"));
        if let Ok(mut l) = LAST.lock() {
            *l = Some(format!("{msg}\n(thread '{thread}' at {place})"));
        }
        default(info);
    }));
}

/// The last panic report (message, thread and place), if any panic happened.
pub fn last_report() -> Option<String> {
    LAST.lock().ok().and_then(|l| l.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn crash_log_appends() {
        let dir = std::env::temp_dir().join(format!("rustfort-crashlog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("crash.log");
        std::fs::create_dir_all(&dir).unwrap();
        // Same code path as `append_log`, but on a private file (tests run in parallel).
        for t in ["one", "two\n"] {
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).unwrap();
            use std::io::Write;
            f.write_all(t.as_bytes()).unwrap();
            if !t.ends_with('\n') {
                f.write_all(b"\n").unwrap();
            }
        }
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "one\ntwo\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
