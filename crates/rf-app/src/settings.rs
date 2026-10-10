//! [round 6] Player settings, kept in a small JSON file next to the saves
//! (`<save dir>/settings.json`, i.e. `saves/settings.json`).
//!
//! The file is optional: a missing or broken file gives the defaults. Unknown fields are
//! ignored and missing ones take their default, so old files keep working.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Autosave choices offered in the settings (game days; 0 = off). DF offers seasonal and yearly.
pub const AUTOSAVE_CHOICES: &[(u32, &str)] =
    &[(0, "Off"), (7, "Every week"), (28, "Every month"), (84, "Every season"), (336, "Every year")];
/// Frame caps offered in the settings (DF's `G_FPS_CAP`).
pub const GFPS_CHOICES: &[u32] = &[30, 50, 60, 75, 100, 120, 144, 165, 240];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// UI scale (1 = DF's 100 % interface).
    pub ui_scale: f32,
    /// Frame cap when vsync is off.
    pub gfps: u32,
    /// Vsync (applies at the next start).
    pub vsync: bool,
    /// Autosave interval in game days (0 = off). Default: one season, like DF's `AUTOSAVE:SEASONAL`.
    pub autosave_days: u32,
    /// Autosaves kept per fort (older ones are deleted).
    pub autosave_keep: u32,
    /// Population cap for newly imported forts (`None` = keep the save's value).
    pub pop_cap: Option<i32>,
    /// Invasions for newly imported forts (`None` = keep the save's value).
    pub invasions: Option<bool>,
    /// DF install folder (empty = `RUSTFORT_DF_DIR` or the Steam default).
    pub df_dir: String,
    /// Ids of the first-time hints that were dismissed.
    pub hints_seen: Vec<String>,
    /// All hints switched off.
    pub hints_off: bool,
    /// [app8] The minimap is shown.
    pub minimap: bool,
    /// [app9] Announcement types changed in the game (DF `announcements.txt` flags); types not
    /// listed use the DF files' values.
    pub announcements: Vec<AnnSetting>,
}

/// [app9] One changed announcement type (`ui::round9::ANN_TYPES` id and its flags).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AnnSetting {
    pub id: String,
    /// DF `BOX`: a pop-up box that pauses.
    pub popup: bool,
    /// DF `P`.
    pub pause: bool,
    /// DF `R`.
    pub recenter: bool,
    /// DF `D_D`.
    pub show: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ui_scale: 1.0,
            gfps: 60,
            vsync: true,
            autosave_days: 84,
            autosave_keep: 3,
            pop_cap: None,
            invasions: None,
            df_dir: String::new(),
            hints_seen: Vec::new(),
            hints_off: false,
            minimap: false,
            announcements: Vec::new(),
        }
    }
}

/// `Every season`, `Off`, `Every 10 days`.
pub fn autosave_label(days: u32) -> String {
    match AUTOSAVE_CHOICES.iter().find(|(d, _)| *d == days) {
        Some((_, l)) => l.to_string(),
        None => format!("Every {days} days"),
    }
}

/// `saves/settings.json` (or `$RUSTFORT_SETTINGS`).
pub fn path() -> PathBuf {
    std::env::var_os("RUSTFORT_SETTINGS").map(PathBuf::from).unwrap_or_else(|| crate::save::save_dir().join("settings.json"))
}

impl Settings {
    pub fn load() -> Settings {
        Self::load_from(&path())
    }

    pub fn load_from(p: &std::path::Path) -> Settings {
        let Ok(t) = std::fs::read_to_string(p) else { return Settings::default() };
        let mut s: Settings = serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or_default();
        s.sanitize();
        s
    }

    fn sanitize(&mut self) {
        if !self.ui_scale.is_finite() {
            self.ui_scale = 1.0;
        }
        self.ui_scale = self.ui_scale.clamp(0.5, 4.0);
        self.gfps = self.gfps.clamp(10, 1000);
        self.autosave_keep = self.autosave_keep.clamp(1, 50);
    }

    /// Write the file (best effort; errors are returned for the status line).
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&path())
    }

    pub fn save_to(&self, p: &std::path::Path) -> Result<(), String> {
        if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d).map_err(|e| format!("create {}: {e}", d.display()))?;
        }
        let t = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(p, t + "\n").map_err(|e| format!("write {}: {e}", p.display()))
    }

    /// Point `rf_gfx::df_dir()` at the configured install (only when one is set).
    pub fn apply_df_dir(&self) {
        if !self.df_dir.trim().is_empty() {
            std::env::set_var("RUSTFORT_DF_DIR", self.df_dir.trim());
        }
    }

    pub fn hint_seen(&self, id: &str) -> bool {
        self.hints_off || self.hints_seen.iter().any(|h| h == id)
    }

    pub fn autosave_label(&self) -> String {
        autosave_label(self.autosave_days)
    }

    /// Next autosave choice (cycles).
    pub fn cycle_autosave(&mut self) {
        let i = AUTOSAVE_CHOICES.iter().position(|(d, _)| *d == self.autosave_days).map(|i| i + 1).unwrap_or(0);
        self.autosave_days = AUTOSAVE_CHOICES[i % AUTOSAVE_CHOICES.len()].0;
    }

    /// Step the frame cap through [`GFPS_CHOICES`].
    pub fn step_gfps(&mut self, up: bool) {
        let i = GFPS_CHOICES.iter().position(|&g| g >= self.gfps).unwrap_or(GFPS_CHOICES.len() - 1);
        let j = if up { (i + 1).min(GFPS_CHOICES.len() - 1) } else { i.saturating_sub(1) };
        self.gfps = GFPS_CHOICES[j];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("rf-settings-{}", std::process::id()));
        let p = dir.join("settings.json");
        assert_eq!(Settings::load_from(&p), Settings::default());
        let mut s = Settings { ui_scale: 1.5, autosave_days: 28, pop_cap: Some(120), ..Default::default() };
        s.hints_seen.push("welcome".into());
        s.announcements.push(AnnSetting { id: "DEATH".into(), popup: false, pause: true, recenter: false, show: true }); // [app9]
        s.save_to(&p).unwrap();
        let l = Settings::load_from(&p);
        assert_eq!(l, s);
        assert!(l.hint_seen("welcome") && !l.hint_seen("help"));
        // Old or partial files keep working; garbage gives the defaults.
        std::fs::write(&p, r#"{"ui_scale": 9, "unknown": 1}"#).unwrap();
        let l = Settings::load_from(&p);
        assert_eq!((l.ui_scale, l.autosave_days, l.gfps), (4.0, 84, 60));
        std::fs::write(&p, "not json").unwrap();
        assert_eq!(Settings::load_from(&p), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cycles() {
        let mut s = Settings::default();
        assert_eq!(s.autosave_label(), "Every season");
        s.cycle_autosave();
        assert_eq!(s.autosave_days, 336);
        s.cycle_autosave();
        assert_eq!(s.autosave_days, 0);
        s.step_gfps(true);
        assert_eq!(s.gfps, 75);
        s.gfps = 61;
        s.step_gfps(false);
        assert_eq!(s.gfps, 60);
    }
}
