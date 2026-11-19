//! [audio14] DF audio: music, ambience and event sounds read at runtime from the DF install
//! (`data/sound`, the `MUSIC` / `SOUND` raws, `init.txt`). See docs/mechanics/audio14.md.
//!
//! The audio system lives on the main thread (rodio's output stream is not `Send`) in a
//! thread-local, so the title screen and the game share it. It opens a device only for an
//! interactive `play` that is not muted (`--mute`, `RUSTFORT_MUTE=1`, the Mute setting,
//! master volume 0) and not a screenshot or script run; bench, savebench and tests never
//! construct one with a real backend.

pub mod ambience;
pub mod catalog;
pub mod director;
pub mod out;
pub mod settings;
pub mod sfx;

pub use settings::AudioSettings;

use ambience::{AmbInputs, Sky, LAYERS, N_LAYERS};
use catalog::{Catalog, DfAudioInit};
use director::{Ctx, Detector, Director, Event, Facts, Item, MusicCmd, SplitMix};
use out::{Backend, Bus, PlayOpts, VoiceId};
use rf_sim::audio14_api::{AudioCue, DigSound, StrikeSound};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// What the Audio page shows.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// "playing", or why there is no sound.
    pub line: String,
    pub now_playing: String,
    pub sound_dir: String,
    pub n_files: usize,
    /// Where DF's values come from (init file), empty = DF defaults.
    pub df_source: String,
    pub df: Option<DfAudioInit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    None,
    Title,
    Fort,
}

pub struct AudioSystem {
    /// Permanent reason from the command line / environment (screenshot, --mute, ...).
    block: Option<&'static str>,
    df: Option<PathBuf>,
    dfinit: DfAudioInit,
    cat: Catalog,
    building: Option<Receiver<Catalog>>,
    backend: Option<Box<dyn Backend>>,
    device_error: Option<String>,
    eff: AudioSettings,
    dir: Director,
    det: Detector,
    music: Option<VoiceId>,
    fading: Option<(VoiceId, f64, f32)>,
    music_gain_now: f32,
    stingers: sfx::Stingers,
    savage: sfx::Savage,
    dig_lim: sfx::Limiter,
    /// Mining voices still sounding (at most `DIG_MAX_VOICES`, spec 4.5).
    dig_voices: [Option<VoiceId>; sfx::DIG_MAX_VOICES],
    strike_lim: sfx::Limiter,
    adamantine_done: bool,
    rng: SplitMix,
    amb_voice: [Option<VoiceId>; N_LAYERS],
    amb_gain: [f32; N_LAYERS],
    amb_target: [f32; N_LAYERS],
    amb_next: f64,
    alt: bool,
    alt_next: f64,
    duck: f32,
    last_now: f64,
    mode: Mode,
    facts_next: f64,
    combat_until: f64,
    cues: Vec<AudioCue>,
    events: Vec<Event>,
    tokens: Vec<&'static str>,
    sounds: Vec<usize>,
    surface_depth: i32,
    caverns: bool,
}

thread_local! {
    static AUDIO: RefCell<Option<AudioSystem>> = const { RefCell::new(None) };
}

/// Run `f` on the audio system, if there is one.
pub fn with<R>(f: impl FnOnce(&mut AudioSystem) -> R) -> Option<R> {
    AUDIO.with(|a| a.borrow_mut().as_mut().map(f))
}

/// Start the audio system for `play` (call once on the main thread).
pub fn init(args: &crate::Args, s: &AudioSettings) {
    let block = settings::should_open_device(&settings::DeviceInputs {
        cmd: &args.cmd,
        screenshot: args.screenshot.is_some(),
        script: args.script.is_some(),
        shots_at: !args.shots_at.is_empty(),
        mute_flag: args.mute,
        env_mute: settings::env_muted(),
        muted: false,
        master: 255,
        has_sound: true,
    })
    .err();
    let df = rf_raws::df_dir_status().0;
    let dfinit = df.as_deref().map(DfAudioInit::read).unwrap_or_default();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let eff = s.effective(&dfinit);
    let mut sys = AudioSystem {
        block,
        df: df.clone(),
        dir: Director::new(seed, eff.avg_song_secs as f64),
        dfinit,
        cat: Catalog::default(),
        building: None,
        backend: None,
        device_error: None,
        eff,
        det: Detector::default(),
        music: None,
        fading: None,
        music_gain_now: 0.0,
        stingers: Default::default(),
        savage: Default::default(),
        dig_lim: Default::default(),
        dig_voices: [None; sfx::DIG_MAX_VOICES],
        strike_lim: Default::default(),
        adamantine_done: false,
        rng: SplitMix(seed ^ 0x5151),
        amb_voice: [None; N_LAYERS],
        amb_gain: [0.0; N_LAYERS],
        amb_target: [0.0; N_LAYERS],
        amb_next: 0.0,
        alt: false,
        alt_next: 0.0,
        duck: 1.0,
        last_now: 0.0,
        mode: Mode::None,
        facts_next: 0.0,
        combat_until: f64::MIN,
        cues: Vec::with_capacity(rf_sim::audio14_api::CUE_CAP),
        events: Vec::new(),
        tokens: Vec::new(),
        sounds: Vec::new(),
        surface_depth: 0,
        caverns: false,
    };
    sys.rebuild(&[]);
    match sys.block {
        Some(r) => eprintln!("audio: off ({r})"),
        None => eprintln!("audio: on (Settings > Audio; --mute or RUSTFORT_MUTE=1 for silence)"),
    }
    AUDIO.with(|a| *a.borrow_mut() = Some(sys));
}

/// Settings changed (Audio page).
pub fn apply(s: &AudioSettings) {
    with(|a| {
        a.eff = s.effective(&a.dfinit);
        a.dir.avg_secs = a.eff.avg_song_secs as f64;
        a.device_error = None;
    });
}

/// DF's audio values (for the Audio page and "take over").
pub fn df_init() -> DfAudioInit {
    with(|a| a.dfinit.clone()).unwrap_or_default()
}

pub fn status() -> Status {
    with(|a| a.status()).unwrap_or_else(|| Status { line: "off (not started)".into(), ..Default::default() })
}

/// The game replaced its world (a save was loaded in the game): start the fort logic over,
/// so the event detector takes a new baseline instead of comparing the new fort with the old
/// one (which fired FIRST_CAVERN_OPENED, FIRST_GHOST or FORT_LEVEL on load).
pub fn fort_changed() {
    with(|a| {
        if a.mode == Mode::Fort {
            a.mode = Mode::None;
        }
    });
}

/// One title-screen frame.
pub fn menu_frame() {
    with(|a| {
        let now = macroquad::time::get_time();
        if a.mode != Mode::Title {
            a.mode = Mode::Title;
            a.stop_ambience();
            if let Some(c) = a.dir.enter_title(now, &a.cat, &a.dfinit.title_music.clone()) {
                a.run_music(c, now);
            }
        }
        a.pump(now, &Ctx::Title);
    });
}

/// One game frame, after the announcements were drained.
pub fn game_frame(g: &mut crate::game::Game, fresh: &[rf_sim::Announcement]) {
    let Some(blocked) = with(|a| a.block.is_some()) else { return };
    if blocked {
        return; // screenshot / script / --mute runs: no device, no cues, nothing to do
    }
    let now = macroquad::time::get_time();
    // Cue recording is presentation only (never saved, hashed or seeded).
    if !g.sim.audio_cues_on() {
        g.sim.set_audio_cues(true);
    }
    let mut cues = with(|a| std::mem::take(&mut a.cues)).unwrap_or_default();
    g.sim.drain_audio_cues(&mut cues);
    let ticks = g.sim.ticks();
    let w = &g.sim.world;
    let founded = w.meta.founded;
    let synthetic = w.meta.source == "synthetic";
    let cal = w.calendar;
    let age = match founded {
        Some(f) => Some((cal.year - f.year) as i64 * director::TICKS_PER_YEAR + (cal.year_tick - f.year_tick) as i64),
        None if synthetic => Some(ticks as i64),
        None => None,
    };
    let need_facts = with(|a| now >= a.facts_next).unwrap_or(false);
    let facts = need_facts.then(|| g.sim.audio_facts());
    // Classify the fresh lines (death lines need the world to tell citizens from pets).
    let classes: Vec<(&'static str, &str)> = fresh
        .iter()
        .map(|an| {
            let mut t = crate::ui::round9::ANN_TYPES[crate::ui::round9::classify(&an.text, an.kind)].id;
            if t == "DEATH" {
                t = crate::ui::round9::ANN_TYPES[crate::ui::round9::death_type(w, &an.text, an.pos)].id;
            }
            (t, an.text.as_str())
        })
        .collect();
    let amb_due = with(|a| now >= a.amb_next).unwrap_or(false);
    let (cam_x, cam_y, cam_z, zoom) = (g.cam.x, g.cam.y, g.cam.z, g.cam.zoom.max(0.05));
    let half_w = macroquad::window::screen_width() / (32.0 * zoom) / 2.0;
    let half_h = macroquad::window::screen_height() / (32.0 * zoom) / 2.0;
    let view = ((cam_x - half_w) as i32, (cam_y - half_h) as i32, (cam_x + half_w) as i32, (cam_y + half_h) as i32);
    let center = rf_core::Pos::new(cam_x as i16, cam_y as i16, cam_z as i16);
    let amb = amb_due.then(|| {
        let rep = g.sim.cavern_report();
        let caverns: Vec<(i16, i16)> = rep.layers.iter().filter(|l| !l.magma && l.discovered()).map(|l| (l.z_min, l.z_max)).collect();
        let sea = rep.layers.iter().find(|l| l.magma && l.discovered()).map(|l| (l.z_min, l.z_max));
        let mut inp = ambience::gather(w, center, view, &caverns, sea);
        inp.sky = match g.sim.weather_summary().kind {
            rf_sim::WeatherKind::Rain | rf_sim::WeatherKind::EvilRain => Sky::Rain,
            rf_sim::WeatherKind::Snow => Sky::Snow,
            _ => Sky::Clear,
        };
        inp.siege = g.hostiles.len() >= 6;
        inp.tavern = g.sim.location_at(center).and_then(|id| w.locations.iter().find(|l| l.id == id)).map(|l| l.kind.eq_ignore_ascii_case("tavern")).unwrap_or(false);
        inp
    });
    // Adamantine: the material of dug tiles.
    let adamantine: Vec<bool> = cues
        .iter()
        .map(|c| match c {
            AudioCue::Dig { mat, class: DigSound::Ore, .. } => w.material(*mat).map(|m| m.token.contains("ADAMANTINE")).unwrap_or(false),
            _ => false,
        })
        .collect();
    let savagery = w.climate.savagery;
    with(|a| {
        if a.cat.mods != w.meta.mods && a.building.is_none() && a.df.is_some() {
            a.rebuild(&w.meta.mods);
        }
        if a.mode != Mode::Fort {
            a.mode = Mode::Fort;
            a.det = Detector::default();
            a.adamantine_done = false;
            if let Some(c) = a.dir.enter_fort(now) {
                a.run_music(c, now);
            }
            if ticks == 0 && age.map(|x| x < director::TICKS_PER_DAY).unwrap_or(false) {
                a.events.push(Event::JustEmbarked);
            }
        }
        a.stingers.begin_frame();
        a.sounds.clear();
        let mut alert = false;
        for (class, text) in &classes {
            if let Some(e) = director::event_of_announcement(class, text) {
                a.events.push(e);
            }
            if *class == "DEATH" {
                a.det.on_death(ticks, &mut a.events);
            }
            a.tokens.clear();
            sfx::tokens_for(class, text, &mut a.tokens);
            let before = a.sounds.len();
            a.stingers.trigger(&a.cat, &a.tokens, now, &mut a.sounds);
            if matches!(*class, "SIEGE" | "AMBUSH" | "THIEF" | "SNATCHER" | "MEGABEAST") && a.sounds.len() == before {
                alert = true;
            }
        }
        if let Some(f) = facts {
            a.facts_next = now + 1.0;
            let fx = Facts { ticks: f.ticks, caverns_mask: f.caverns_mask, citizens: f.citizens_alive, ghosts: f.ghosts_risen, land_rank: f.land_rank };
            let new_caverns = a.det.on_facts(fx, &mut a.events);
            if new_caverns > 0 {
                a.stingers.trigger(&a.cat, &["FEATURE_DISCOVERY"], now, &mut a.sounds);
            }
            a.caverns = f.caverns_mask != 0;
        }
        for (i, c) in cues.iter().enumerate() {
            match *c {
                AudioCue::Dig { pos, .. } => {
                    if adamantine[i] && !a.adamantine_done {
                        a.adamantine_done = true;
                        a.stingers.trigger(&a.cat, &["STRUCK_DEEP_METAL"], now, &mut a.sounds);
                    }
                    let gain = sfx::positional_gain(pos.z as i32 - cam_z, sfx::dist_to_rect(pos.x as f32, pos.y as f32, (view.0 as f32, view.1 as f32, view.2 as f32, view.3 as f32)), a.cat.hearing);
                    if gain > 0.0 {
                        a.play_dig(now, gain, sfx::pan(pos.x as f32, cam_x, half_w), pos.z as i32 != cam_z);
                    }
                }
                AudioCue::Perform { tavern, music, audience, .. } => a.det.on_perform(ticks, tavern, music, audience, &mut a.events),
                AudioCue::Strike { pos, kind } => {
                    let inside = (pos.z as i32 - cam_z).abs() <= 1 && sfx::dist_to_rect(pos.x as f32, pos.y as f32, (view.0 as f32, view.1 as f32, view.2 as f32, view.3 as f32)) <= 0.0;
                    if inside {
                        a.combat_until = now + 3.0;
                        if a.eff.extra_sfx {
                            a.play_strike(now, kind, sfx::pan(pos.x as f32, cam_x, half_w));
                        }
                    }
                }
            }
        }
        if alert && a.sounds.is_empty() {
            if let Some(p) = a.cat.dir.alert.clone() {
                a.play_sfx(&p, 0.0, None, 1.0);
            }
        }
        for k in std::mem::take(&mut a.sounds) {
            if let Some(p) = a.cat.file(&a.cat.sounds[k].file).cloned() {
                a.play_sfx(&p, 0.0, None, 1.0);
            }
        }
        // Highest-priority event of the frame wins.
        if let Some(e) = a.events.iter().copied().max_by_key(|e| e.priority()) {
            if let Some(c) = a.dir.on_event(now, &a.cat, e) {
                a.run_music(c, now);
            }
        }
        a.events.clear();
        if let Some(mut inp) = amb {
            inp.combat = now < a.combat_until;
            a.surface_depth = inp.depth;
            a.set_ambience(now, &inp);
        }
        let savage_on = savagery >= sfx::SAVAGE_MIN && a.surface_depth == 0;
        if let Some((k, pan)) = a.savage.step(now, savage_on, &a.cat, &mut a.rng) {
            if let Some(p) = a.cat.file(&a.cat.sounds[k].file).cloned() {
                a.play_sfx(&p, pan, None, 1.0);
            }
        }
        cues.clear();
        a.cues = cues;
        let month = (cal.year_tick / rf_core::TICKS_PER_MONTH).clamp(0, 11) as u8;
        let ctx = Ctx::Fort { month, age_ticks: age, caverns: a.caverns };
        a.pump(now, &ctx);
    });
}

impl AudioSystem {
    fn rebuild(&mut self, mods: &[String]) {
        let Some(df) = self.df.clone() else { return };
        let mods = mods.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Catalog::build(&df, &mods));
        });
        self.building = Some(rx);
    }

    fn status(&self) -> Status {
        let line = if let Some(b) = self.block {
            format!("off ({b})")
        } else if self.df.is_none() {
            "off (Dwarf Fortress not found)".into()
        } else if self.cat.dir.n_files == 0 && self.building.is_none() {
            "off (no data/sound folder in the DF install)".into()
        } else if let Some(e) = &self.device_error {
            format!("off (no audio device: {e})")
        } else if self.eff.muted {
            if self.dfinit.sound || !self.eff.from_df {
                "muted".into()
            } else {
                "muted (DF sound is off: init.txt SOUND:NO)".into()
            }
        } else if self.eff.master == 0 {
            "silent (master volume 0)".into()
        } else if self.backend.is_some() {
            "on".into()
        } else {
            "starting".into()
        };
        let now_playing = match &self.dir.current {
            Some(Item { title, piece, .. }) if self.backend.is_some() => match piece {
                director::Piece::Interlude => "interlude".into(),
                director::Piece::Card => format!("{title} (card)"),
                _ => title.clone(),
            },
            _ => String::new(),
        };
        Status {
            line,
            now_playing,
            sound_dir: self.cat.dir.root.display().to_string(),
            n_files: self.cat.dir.n_files,
            df_source: self.dfinit.source.clone(),
            df: Some(self.dfinit.clone()),
        }
    }

    fn bus_gain(&self, bus: Bus) -> f32 {
        let e = &self.eff;
        let m = e.master as f32 / 255.0;
        let (v, base) = match bus {
            Bus::Music => (e.music, self.cat.start.song),
            Bus::Ambience => (e.ambience, self.cat.start.ambience),
            Bus::Sfx => (e.sfx, self.cat.start.sfx),
            Bus::Ui => (e.ui, self.cat.start.ui),
        };
        m * v as f32 / 255.0 * base
    }

    /// Open or close the device as the settings say.
    fn ensure_device(&mut self) {
        let want = self.block.is_none() && !self.eff.muted && self.eff.master > 0 && self.cat.dir.n_files > 0;
        if !want {
            if self.backend.take().is_some() {
                self.music = None;
                self.fading = None;
                self.amb_voice = [None; N_LAYERS];
                self.amb_gain = [0.0; N_LAYERS];
            }
            return;
        }
        if self.backend.is_some() || self.device_error.is_some() {
            return;
        }
        #[cfg(feature = "audio")]
        match out::RodioBackend::new() {
            Ok(b) => self.backend = Some(Box::new(b)),
            Err(e) => {
                eprintln!("audio: no output device ({e}); audio is off");
                self.device_error = Some(e);
            }
        }
        #[cfg(not(feature = "audio"))]
        {
            self.device_error = Some("built without the audio feature".into());
        }
    }

    fn run_music(&mut self, c: MusicCmd, now: f64) {
        let g = self.bus_gain(Bus::Music);
        let Some(b) = self.backend.as_mut() else { return };
        match c {
            MusicCmd::FadeOut => {
                if let Some(v) = self.music.take() {
                    if let Some((old, _, _)) = self.fading.take() {
                        b.stop(old);
                    }
                    self.fading = Some((v, now, self.music_gain_now));
                }
            }
            MusicCmd::Start(it) => {
                if let Some((old, _, _)) = self.fading.take() {
                    b.stop(old);
                }
                if let Some(v) = self.music.take() {
                    b.stop(v);
                }
                self.music = b.play(Bus::Music, &it.path, PlayOpts { gain: g, ..Default::default() });
                self.music_gain_now = g;
            }
        }
    }

    /// Per-frame upkeep: device, music state machine, fades, ambience ramps.
    fn pump(&mut self, now: f64, ctx: &Ctx) {
        if let Some(rx) = &self.building {
            if let Ok(c) = rx.try_recv() {
                self.cat = c;
                self.building = None;
                if self.mode == Mode::Title && self.dir.current.is_none() {
                    self.mode = Mode::None; // re-enter: the title music needs the catalog
                }
            }
        }
        self.ensure_device();
        let dt = (now - self.last_now).clamp(0.0, 0.25) as f32;
        self.last_now = now;
        let Some(_) = self.backend.as_ref() else { return };
        let finished = self.music.map(|v| self.backend.as_ref().map(|b| b.finished(v)).unwrap_or(true)).unwrap_or(true);
        if let Some(c) = self.dir.step(now, &self.cat, ctx, finished && self.dir.is_playing()) {
            self.run_music(c, now);
        }
        let mg = self.bus_gain(Bus::Music);
        self.music_gain_now = mg;
        // Ambience ducks under music.
        let playing = self.music.is_some() && self.dir.is_playing();
        let want_duck = if playing { ambience::AMBIENCE_DUCK } else { 1.0 };
        self.duck += (want_duck - self.duck).clamp(-dt, dt);
        let ag = self.bus_gain(Bus::Ambience) * self.duck;
        let b = self.backend.as_mut().unwrap();
        if let Some(v) = self.music {
            b.set_gain(v, mg);
        }
        if let Some((v, t0, g0)) = self.fading {
            let k = ((now - t0) / director::FADE_OUT_S) as f32;
            if k >= 1.0 {
                b.stop(v);
                self.fading = None;
            } else {
                b.set_gain(v, g0 * (1.0 - k));
            }
        }
        // Ramp to the targets; silent layers pause and resume where they left off.
        for i in 0..N_LAYERS {
            self.amb_gain[i] = ambience::ramp(self.amb_gain[i], self.amb_target[i], dt);
            let g = self.amb_gain[i];
            match self.amb_voice[i] {
                Some(v) => {
                    b.pause(v, g <= 0.0);
                    b.set_gain(v, g * ag);
                }
                None if g > 0.0 => {
                    if let Some(p) = self.cat.dir.ambience.get(LAYERS[i]) {
                        self.amb_voice[i] = b.play(Bus::Ambience, p, PlayOpts { looped: true, gain: g * ag, ..Default::default() });
                    }
                }
                None => {}
            }
        }
        b.tick();
    }

    fn set_ambience(&mut self, now: f64, inp: &AmbInputs) {
        self.amb_next = now + ambience::AMB_PERIOD_S;
        if now >= self.alt_next {
            self.alt = !self.alt;
            self.alt_next = now + self.rng.range(ambience::ALT_SWAP_S.0, ambience::ALT_SWAP_S.1);
        }
        self.amb_target = ambience::targets(inp, self.alt);
        // "Neutral_Winds_2" takes every other neutral phase.
        let (n1, n2) = (LAYERS.iter().position(|l| *l == "Neutral_Winds").unwrap(), LAYERS.iter().position(|l| *l == "Neutral_Winds_2").unwrap());
        if self.rng.0 & 1 == 1 && self.amb_target[n1] > 0.0 {
            self.amb_target.swap(n1, n2);
        }
    }

    fn stop_ambience(&mut self) {
        self.amb_target = [0.0; N_LAYERS];
        if let Some(b) = self.backend.as_mut() {
            for v in self.amb_voice.iter_mut().filter_map(|v| v.take()) {
                b.stop(v);
            }
        }
        self.amb_gain = [0.0; N_LAYERS];
    }

    fn play_sfx(&mut self, p: &std::path::Path, pan: f32, low_pass: Option<u32>, gain: f32) -> Option<VoiceId> {
        let g = self.bus_gain(Bus::Sfx) * gain;
        self.backend.as_mut().and_then(|b| b.play(Bus::Sfx, p, PlayOpts { gain: g, pan, low_pass, looped: false }))
    }

    fn play_dig(&mut self, now: f64, gain: f32, pan: f32, muffled: bool) {
        let Some(p) = self.cat.dir.dig.clone() else { return };
        // At most DIG_MAX_VOICES mining voices at once (counted per voice, not per bus: other
        // one-shots must not block or be blocked by mining).
        let Some(b) = self.backend.as_ref() else { return };
        let Some(slot) = sfx::free_slot(&mut self.dig_voices, |v| b.finished(v)) else { return };
        if self.dig_lim.allow(now, sfx::DIG_MIN_GAP_S, 8) {
            self.dig_voices[slot] = self.play_sfx(&p, pan, muffled.then_some(1200), gain);
        }
    }

    fn play_strike(&mut self, now: f64, kind: StrikeSound, pan: f32) {
        let pool = match kind {
            StrikeSound::Metal => &self.cat.dir.hits_metal,
            StrikeSound::Flesh => &self.cat.dir.hits_flesh,
        };
        if pool.is_empty() || !self.strike_lim.allow(now, 0.0, sfx::STRIKE_MAX_PER_S) {
            return;
        }
        let p = pool[self.rng.below(pool.len() as u64) as usize].clone();
        self.play_sfx(&p, pan, None, 0.7);
    }
}
