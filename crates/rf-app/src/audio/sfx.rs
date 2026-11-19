//! [audio14] Sound effects: announcement stingers from the `SOUND` raws, the alert ping,
//! savage-area one-shots, mining sounds and the positional model
//! (docs/mechanics/audio14.md section 4). Pure.

use super::catalog::Catalog;
use super::director::SplitMix;
use std::collections::BTreeMap;

/// One stinger per SOUND id per this many seconds (ESTIMATE).
pub const SFX_DEDUP_S: f64 = 2.0;
/// Most stingers started in one frame (ESTIMATE).
pub const MAX_STINGERS_PER_FRAME: usize = 8;
/// Savage surroundings from this savagery on (ESTIMATE cut-off: top third).
pub const SAVAGE_MIN: i32 = 66;
/// Seconds between savage one-shots (ESTIMATE).
pub const SAVAGE_GAP: (f64, f64) = (45.0, 150.0);
/// Mining: concurrent voices and minimum spacing (ESTIMATE).
pub const DIG_MAX_VOICES: usize = 3;
pub const DIG_MIN_GAP_S: f64 = 0.12;
/// Extra combat sounds per second (ESTIMATE).
pub const STRIKE_MAX_PER_S: usize = 6;

/// DF announcement tokens a Rustfort announcement stands for (its `ui::round9` class id and
/// text). Only these can trigger SOUND objects; mods binding other tokens work the same way.
pub fn tokens_for(class: &str, text: &str, out: &mut Vec<&'static str>) {
    let t = text.to_ascii_lowercase();
    match class {
        "SIEGE" => {
            // A goblin siege has no raws sound (only the music event); necromancers do.
            if t.contains("undead") || t.contains("the dead") {
                out.push("UNDEAD_ATTACK");
            }
        }
        "AMBUSH" => out.push("AMBUSH_AMBUSHER"),
        "THIEF" => out.push("AMBUSH_THIEF"),
        "SNATCHER" => out.push("AMBUSH_SNATCHER"),
        "ABDUCTED" => out.push("CITIZEN_SNATCHED"),
        "MEGABEAST" => {
            out.push("MEGABEAST_ARRIVAL");
            if t.contains("werebeast") {
                out.push("WEREBEAST_ARRIVAL");
            }
        }
        "DEATH" => out.push("CITIZEN_DEATH"),
        "PET_DEATH" => out.push("PET_DEATH"),
        "MIGRANTS" => out.extend(["MIGRANT_ARRIVAL", "D_MIGRANTS_ARRIVAL"]),
        "CARAVAN" => out.push("CARAVAN_ARRIVAL"),
        "VISITOR" => out.push("GUEST_ARRIVAL"),
        "MOOD" => out.push("STRANGE_MOOD"),
        "MOOD_CLAIM" => out.extend(["MOOD_BUILDING_CLAIMED", "ARTIFACT_BEGUN"]),
        "ARTIFACT" => out.push("MADE_ARTIFACT"),
        "MASTERPIECE" => out.push("MASTERPIECE_CRAFTED"),
        "BIRTH" if t.contains("has given birth") => out.push("BIRTH_CITIZEN"),
        "TANTRUM" => out.extend(["CITIZEN_TANTRUM", "BERSERK_CITIZEN"]),
        "NOBLES" => out.push("NEW_MANDATE"),
        "GHOST" => out.push("GHOST_ATTACK"),
        "CAVE_IN" => out.push("CAVE_COLLAPSE"),
        "TRAPS" => out.push("ANIMAL_TRAP_CATCH"),
        "JOB" => out.push("CANCEL_JOB"),
        _ => {}
    }
}

/// Announcement stingers with de-duplication.
#[derive(Clone, Debug, Default)]
pub struct Stingers {
    last: BTreeMap<usize, f64>,
    frame_n: usize,
}

impl Stingers {
    /// Start of a frame (resets the per-frame cap).
    pub fn begin_frame(&mut self) {
        self.frame_n = 0;
    }

    /// SOUND objects to play for these tokens now (indices into `cat.sounds`).
    pub fn trigger(&mut self, cat: &Catalog, tokens: &[&str], now: f64, out: &mut Vec<usize>) {
        for (i, s) in cat.sounds.iter().enumerate() {
            if !s.announcements.iter().any(|a| tokens.contains(&a.as_str())) || cat.file(&s.file).is_none() {
                continue;
            }
            if self.frame_n >= MAX_STINGERS_PER_FRAME || out.contains(&i) {
                continue;
            }
            if self.last.get(&i).map(|&t| now - t < SFX_DEDUP_S).unwrap_or(false) {
                continue;
            }
            self.last.insert(i, now);
            self.frame_n += 1;
            out.push(i);
        }
    }
}

/// Savage-area howls and giant steps at random intervals.
#[derive(Clone, Debug, Default)]
pub struct Savage {
    next: Option<f64>,
}

impl Savage {
    /// `active`: savage surroundings and the view at or above the surface. Returns a SOUND
    /// index and a stereo pan when one plays.
    pub fn step(&mut self, now: f64, active: bool, cat: &Catalog, r: &mut SplitMix) -> Option<(usize, f32)> {
        if !active {
            self.next = None;
            return None;
        }
        let next = *self.next.get_or_insert_with(|| now + r.range(SAVAGE_GAP.0, SAVAGE_GAP.1));
        if now < next {
            return None;
        }
        self.next = Some(now + r.range(SAVAGE_GAP.0, SAVAGE_GAP.1));
        let pool: Vec<usize> = (0..cat.sounds.len()).filter(|&i| cat.sounds[i].savage && cat.file(&cat.sounds[i].file).is_some()).collect();
        if pool.is_empty() {
            return None;
        }
        let k = pool[r.below(pool.len() as u64) as usize];
        Some((k, r.range(-0.6, 0.6) as f32))
    }
}

/// Gain of a positional sound (4.6): z distance to the view and horizontal tiles outside the
/// visible rectangle. 0 = inaudible.
pub fn positional_gain(dz: i32, d_tiles: f32, hearing: f32) -> f32 {
    let zg = match dz.unsigned_abs() {
        0 => 1.0,
        1 => 0.45,
        2 => 0.2,
        _ => return 0.0,
    };
    zg * (1.0 - d_tiles.max(0.0) / hearing.max(1.0)).max(0.0)
}

/// Stereo pan from the view centre (-0.8 .. 0.8).
pub fn pan(src_x: f32, center_x: f32, half_w: f32) -> f32 {
    ((src_x - center_x) / half_w.max(1.0)).clamp(-1.0, 1.0) * 0.8
}

/// Distance in tiles from a point to a rectangle (0 inside).
pub fn dist_to_rect(x: f32, y: f32, r: (f32, f32, f32, f32)) -> f32 {
    let dx = (r.0 - x).max(x - r.2).max(0.0);
    let dy = (r.1 - y).max(y - r.3).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Free a slot of a voice list: ended voices are cleared; returns an empty slot, `None` when
/// every slot still sounds.
pub fn free_slot<const N: usize>(slots: &mut [Option<u32>; N], finished: impl Fn(u32) -> bool) -> Option<usize> {
    for s in slots.iter_mut() {
        if s.map(&finished).unwrap_or(false) {
            *s = None;
        }
    }
    slots.iter().position(|s| s.is_none())
}

/// Rate limit for repeated one-shots (mining, blows).
#[derive(Clone, Debug, Default)]
pub struct Limiter {
    last: f64,
    window_start: f64,
    in_window: usize,
}

impl Limiter {
    /// May one start now? `min_gap` seconds since the last start and at most `per_s` per second.
    pub fn allow(&mut self, now: f64, min_gap: f64, per_s: usize) -> bool {
        if self.in_window > 0 && now - self.last < min_gap {
            return false;
        }
        if now - self.window_start >= 1.0 {
            self.window_start = now;
            self.in_window = 0;
        }
        if self.in_window >= per_s {
            return false;
        }
        self.in_window += 1;
        self.last = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::{parse_raw, tests as ct, Catalog, SoundDir};
    use super::*;

    fn cat(tag: &str) -> (Catalog, std::path::PathBuf) {
        let root = ct::fake_sound_dir(tag);
        let (m, s, i) = parse_raw(ct::RAWS.as_bytes());
        (Catalog::assemble(SoundDir::scan(&root), vec![(m, s, i, root.clone())]), root)
    }

    fn names(c: &Catalog, v: &[usize]) -> Vec<String> {
        v.iter().map(|&i| c.sounds[i].id.clone()).collect()
    }

    #[test]
    fn classes_map_to_sound_objects_with_dedup() {
        let (c, root) = cat("sfx");
        let mut st = Stingers::default();
        let play = |st: &mut Stingers, class: &str, text: &str, now: f64| {
            let mut tok = Vec::new();
            tokens_for(class, text, &mut tok);
            let mut out = Vec::new();
            st.begin_frame();
            st.trigger(&c, &tok, now, &mut out);
            names(&c, &out)
        };
        assert_eq!(play(&mut st, "MOOD", "Urist is taken by a fey mood!", 0.0), vec!["S_MOOD"]);
        // Within 2 s: once only.
        assert!(play(&mut st, "MOOD", "Another mood", 1.0).is_empty());
        assert_eq!(play(&mut st, "MOOD", "Another mood", 2.5), vec!["S_MOOD"]);
        assert_eq!(play(&mut st, "AMBUSH", "An ambush! Curse them!", 0.0), vec!["S_AMBUSH"]);
        // A lone thief has no sound (raws bind only the supported variants).
        assert!(play(&mut st, "THIEF", "Thief! Protect the hoard", 10.0).is_empty());
        assert_eq!(play(&mut st, "MEGABEAST", "The Forgotten Beast X has come!", 0.0), vec!["S_MEGA"]);
        assert!(play(&mut st, "SIEGE", "A vile force of darkness has arrived!", 0.0).is_empty());
        assert!(play(&mut st, "OTHER", "Urist cancels Eat", 0.0).is_empty());
        let mut tok = Vec::new();
        tokens_for("BIRTH", "Urist has grown up", &mut tok);
        assert!(tok.is_empty());
        tokens_for("BIRTH", "Urist has given birth to a boy", &mut tok);
        assert_eq!(tok, vec!["BIRTH_CITIZEN"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn savage_one_shots_only_when_active() {
        let (c, root) = cat("savage");
        let mut s = Savage::default();
        let mut r = SplitMix(9);
        let mut hits = 0;
        for t in 0..2000 {
            if let Some((k, p)) = s.step(t as f64, true, &c, &mut r) {
                assert!(c.sounds[k].savage && (-0.6..=0.6).contains(&p));
                hits += 1;
            }
        }
        // 2000 s at 45..150 s gaps.
        assert!((13..=45).contains(&hits), "{hits}");
        assert!(s.step(3000.0, false, &c, &mut r).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn positional_model() {
        assert_eq!(positional_gain(0, 0.0, 10.0), 1.0);
        assert_eq!(positional_gain(1, 0.0, 10.0), 0.45);
        assert_eq!(positional_gain(-2, 0.0, 10.0), 0.2);
        assert_eq!(positional_gain(3, 0.0, 10.0), 0.0);
        assert!((positional_gain(0, 5.0, 10.0) - 0.5).abs() < 1e-6);
        assert_eq!(positional_gain(0, 10.0, 10.0), 0.0);
        assert_eq!(positional_gain(0, 11.0, 10.0), 0.0);
        assert_eq!(pan(10.0, 10.0, 20.0), 0.0);
        assert_eq!(pan(100.0, 10.0, 20.0), 0.8);
        assert_eq!(dist_to_rect(5.0, 5.0, (0.0, 0.0, 10.0, 10.0)), 0.0);
        assert_eq!(dist_to_rect(13.0, 5.0, (0.0, 0.0, 10.0, 10.0)), 3.0);
        // Mining voices: three at once, a slot frees when its voice ends.
        let mut slots = [Some(1), Some(2), Some(3)];
        assert_eq!(free_slot(&mut slots, |_| false), None);
        assert_eq!(free_slot(&mut slots, |v| v == 2), Some(1));
        assert_eq!(slots, [Some(1), None, Some(3)]);
        let mut l = Limiter::default();
        assert!(l.allow(0.0, DIG_MIN_GAP_S, 6));
        assert!(!l.allow(0.05, DIG_MIN_GAP_S, 6));
        assert!(l.allow(0.2, DIG_MIN_GAP_S, 6));
        let mut l = Limiter::default();
        let n = (0..100).filter(|i| l.allow(*i as f64 * 0.001, 0.0, STRIKE_MAX_PER_S)).count();
        assert_eq!(n, STRIKE_MAX_PER_S);
    }
}
