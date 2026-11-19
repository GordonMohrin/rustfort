//! [audio14] Audio settings (saved in `settings.json`) and the mute / device policy.

use super::catalog::DfAudioInit;
use serde::{Deserialize, Serialize};

/// Volumes use DF's scale 0..=255 (255 = 100 %).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSettings {
    /// Take the volumes from DF's init files (true until the player changes one here).
    pub from_df: bool,
    /// Global mute.
    pub muted: bool,
    pub master: u8,
    pub music: u8,
    pub ambience: u8,
    pub sfx: u8,
    pub ui: u8,
    /// DF `AVERAGE_TIME_BETWEEN_SONGS` (10..=600 s).
    pub avg_song_secs: u16,
    /// Rustfort extension: adventure-mode sounds DF's fortress mode does not play (combat
    /// blows), default off.
    pub extra_sfx: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        let d = DfAudioInit::default();
        AudioSettings { from_df: true, muted: false, master: d.master, music: d.music, ambience: d.ambience, sfx: d.sfx, ui: d.ui, avg_song_secs: d.avg_song_secs, extra_sfx: false }
    }
}

impl AudioSettings {
    /// The values in effect: DF's when `from_df`, else these. `[SOUND:NO]` in DF mutes.
    pub fn effective(&self, df: &DfAudioInit) -> AudioSettings {
        let mut e = self.clone();
        if self.from_df {
            e.master = df.master;
            e.music = df.music;
            e.ambience = df.ambience;
            e.sfx = df.sfx;
            e.ui = df.ui;
            e.avg_song_secs = df.avg_song_secs;
            e.muted |= !df.sound;
        }
        e.avg_song_secs = e.avg_song_secs.clamp(10, 600);
        e
    }

    /// The Mute checkbox: flips what is in effect. With DF's `[SOUND:NO]` the effective state
    /// is muted even though `muted` is false, so the click must take the values over first,
    /// or the player could never unmute (docs/mechanics/audio14.md 2.2: "the player can
    /// unmute in Rustfort").
    pub fn toggle_mute(&mut self, df: &DfAudioInit) {
        let now_muted = self.effective(df).muted;
        self.take_over(df);
        self.muted = !now_muted;
    }

    /// Copy DF's values in before the first change here (so a slider starts from what plays).
    pub fn take_over(&mut self, df: &DfAudioInit) {
        if self.from_df {
            let m = self.muted;
            *self = self.effective(df);
            self.muted = m;
            self.from_df = false;
        }
    }
}

/// 0..=255 shown as 0..=100 %.
pub fn pct(v: u8) -> u32 {
    (v as u32 * 100 + 127) / 255
}

/// `RUSTFORT_MUTE` set to anything but empty / `0`.
pub fn env_muted() -> bool {
    std::env::var("RUSTFORT_MUTE").map(|v| !v.trim().is_empty() && v.trim() != "0").unwrap_or(false)
}

/// What the device rule looks at.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceInputs<'a> {
    pub cmd: &'a str,
    pub screenshot: bool,
    pub script: bool,
    pub shots_at: bool,
    pub mute_flag: bool,
    pub env_mute: bool,
    pub muted: bool,
    pub master: u8,
    pub has_sound: bool,
}

/// May the audio device be opened? `Err(reason)` otherwise (shown on the Audio page and
/// logged). Headless and test-driven runs never open it.
pub fn should_open_device(i: &DeviceInputs) -> Result<(), &'static str> {
    if i.cmd != "play" {
        return Err("not interactive");
    }
    if i.screenshot || i.shots_at {
        return Err("screenshot");
    }
    if i.script {
        return Err("scripted run");
    }
    if i.env_mute {
        return Err("muted by RUSTFORT_MUTE");
    }
    if i.mute_flag {
        return Err("muted by --mute");
    }
    if !i.has_sound {
        return Err("no DF sound folder");
    }
    if i.muted {
        return Err("muted");
    }
    if i.master == 0 {
        return Err("master volume 0");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_rule() {
        let ok = DeviceInputs { cmd: "play", master: 200, has_sound: true, ..Default::default() };
        assert_eq!(should_open_device(&ok), Ok(()));
        let cases: [(DeviceInputs, &str); 9] = [
            (DeviceInputs { cmd: "bench", ..ok }, "not interactive"),
            (DeviceInputs { cmd: "savebench", ..ok }, "not interactive"),
            (DeviceInputs { screenshot: true, ..ok }, "screenshot"),
            (DeviceInputs { shots_at: true, ..ok }, "screenshot"),
            (DeviceInputs { script: true, ..ok }, "scripted run"),
            (DeviceInputs { env_mute: true, ..ok }, "muted by RUSTFORT_MUTE"),
            (DeviceInputs { mute_flag: true, ..ok }, "muted by --mute"),
            (DeviceInputs { muted: true, ..ok }, "muted"),
            (DeviceInputs { master: 0, ..ok }, "master volume 0"),
        ];
        for (i, want) in cases {
            assert_eq!(should_open_device(&i), Err(want));
        }
        assert_eq!(should_open_device(&DeviceInputs { has_sound: false, ..ok }), Err("no DF sound folder"));
    }

    #[test]
    fn settings_follow_df_until_changed() {
        let mut df = DfAudioInit { master: 0, music: 113, ambience: 103, sfx: 108, ui: 204, avg_song_secs: 250, ..Default::default() };
        let s = AudioSettings::default();
        let e = s.effective(&df);
        assert_eq!((e.master, e.music, e.ambience, e.sfx, e.avg_song_secs), (0, 113, 103, 108, 250));
        df.sound = false;
        assert!(s.effective(&df).muted);
        df.sound = true;
        let mut s2 = s.clone();
        s2.take_over(&df);
        assert!(!s2.from_df);
        s2.master = 255;
        assert_eq!(s2.effective(&df).master, 255);
        assert_eq!((pct(255), pct(0), pct(128)), (100, 0, 50));
        // DF `[SOUND:NO]`: starts muted, and the checkbox can unmute (and mute again).
        let off = DfAudioInit { sound: false, ..Default::default() };
        let mut s3 = AudioSettings::default();
        assert!(s3.effective(&off).muted);
        s3.toggle_mute(&off);
        assert!(!s3.effective(&off).muted, "SOUND:NO could not be unmuted");
        s3.toggle_mute(&off);
        assert!(s3.effective(&off).muted);
        // Normal DF sound: a plain toggle.
        let mut s4 = AudioSettings::default();
        s4.toggle_mute(&df);
        assert!(s4.effective(&df).muted);
        // Serde: old settings files without the field get the defaults.
        let parsed: AudioSettings = serde_json::from_str(r#"{"muted": true}"#).unwrap();
        assert!(parsed.muted && parsed.from_df && parsed.master == 255);
    }
}
