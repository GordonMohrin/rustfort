//! [audio14] The music director: title music, context pools with FREQUENCY weights, gaps
//! between songs, card interludes, and event songs that override the current one
//! (docs/mechanics/audio14.md section 3). Pure: time and randomness are injected, the caller
//! turns the returned commands into playback.

use super::catalog::Catalog;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

/// Fade-out of the current music when an event song or a mode change takes over (ESTIMATE).
pub const FADE_OUT_S: f64 = 1.5;
/// Neutral card instead of a song (ESTIMATE).
pub const NEUTRAL_INTERLUDE_P: f64 = 0.25;
/// Retry when no song fits the context (ESTIMATE).
pub const RETRY_S: f64 = 5.0;
/// First song after loading a fort: 5..15 s (ESTIMATE).
pub const FIRST_SLOT_S: (f64, f64) = (5.0, 15.0);

pub const TICKS_PER_DAY: i64 = rf_core::TICKS_PER_DAY as i64;
pub const TICKS_PER_MONTH: i64 = rf_core::TICKS_PER_MONTH as i64;
pub const TICKS_PER_YEAR: i64 = rf_core::TICKS_PER_YEAR as i64;
/// Death spiral: this many citizen deaths ... (ESTIMATE)
pub const DEATH_SPIRAL_DEATHS: usize = 5;
/// ... within this much game time (ESTIMATE) ...
pub const DEATH_SPIRAL_WINDOW: u64 = 2 * TICKS_PER_MONTH as u64;
/// ... then not again for a season (ESTIMATE).
pub const DEATH_SPIRAL_COOLDOWN: u64 = TICKS_PER_YEAR as u64 / 4;
/// Tavern music: audience of at least this many (ESTIMATE) ...
pub const TAVERN_CROWD: u16 = 5;
/// ... at most once a season (ESTIMATE).
pub const TAVERN_COOLDOWN: u64 = TICKS_PER_YEAR as u64 / 4;

/// Small deterministic RNG of the app (splitmix64). Audio is presentation: it is seeded from
/// wall time in the app and explicitly in tests, and never touches the sim's streams.
#[derive(Clone, Debug)]
pub struct SplitMix(pub u64);

impl SplitMix {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }
}

/// Gap between songs: uniform in [avg * 3/4, avg * 5/4] (240 s gives DF's 180..300 s).
pub fn gap(avg_secs: f64, r: &mut SplitMix) -> f64 {
    r.range(avg_secs * 0.75, avg_secs * 1.25)
}

/// Where the music plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    Title,
    Fort {
        /// 0..=11 (0 = Granite, the first spring month).
        month: u8,
        /// Ticks since the fort was founded (`None` = unknown: treated as older than a year).
        age_ticks: Option<i64>,
        caverns: bool,
    },
}

/// Does a raw `CONTEXT` hold?
pub fn context_true(c: &str, ctx: &Ctx) -> bool {
    match (c, ctx) {
        ("ANY", _) => true,
        (_, Ctx::Title) => matches!(c, "SPRING" | "SUMMER" | "AUTUMN"),
        ("MAIN", Ctx::Fort { .. }) => true,
        ("FIRST_YEAR", Ctx::Fort { age_ticks, .. }) => age_ticks.map(|a| a < TICKS_PER_YEAR).unwrap_or(false),
        ("SECOND_YEAR_PLUS", Ctx::Fort { age_ticks, .. }) => age_ticks.map(|a| a >= TICKS_PER_YEAR).unwrap_or(true),
        ("CAVERNS_OPENED", Ctx::Fort { caverns, .. }) => *caverns,
        ("SPRING", Ctx::Fort { month, .. }) => month / 3 == 0,
        ("SUMMER", Ctx::Fort { month, .. }) => month / 3 == 1,
        ("AUTUMN", Ctx::Fort { month, .. }) => month / 3 == 2,
        ("WINTER", Ctx::Fort { month, .. }) => month / 3 == 3,
        _ => false,
    }
}

/// Music objects that may be picked at random now (a CONTEXT holds and the file resolved).
pub fn candidates(cat: &Catalog, ctx: &Ctx, out: &mut Vec<usize>) {
    out.clear();
    for (i, m) in cat.music.iter().enumerate() {
        // Title screen: ANY only (the season contexts there are for legends / main menu in
        // DF; on Rustfort's title screen they would add winter-less seasonal songs; keep ANY).
        let ok = match ctx {
            Ctx::Title => m.contexts.iter().any(|c| c == "ANY"),
            _ => m.contexts.iter().any(|c| context_true(c, ctx)),
        };
        if ok && cat.playable(i) {
            out.push(i);
        }
    }
}

/// Music events (w:Audio), highest priority first (ESTIMATE order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    JustEmbarked,
    TavernMusic,
    TavernDance,
    FortLevel,
    FirstGhost,
    FirstCavernOpened,
    DeathSpiral,
    MegabeastAttack,
    ForgottenBeastAttack,
    Siege,
    LostFort,
}

impl Event {
    pub fn token(self) -> &'static str {
        match self {
            Event::JustEmbarked => "JUST_EMBARKED",
            Event::TavernMusic => "TAVERN_MUSIC_PRESENT",
            Event::TavernDance => "TAVERN_DANCE_PRESENT",
            Event::FortLevel => "FORT_LEVEL",
            Event::FirstGhost => "FIRST_GHOST",
            Event::FirstCavernOpened => "FIRST_CAVERN_OPENED",
            Event::DeathSpiral => "DEATH_SPIRAL",
            Event::MegabeastAttack => "MEGABEAST_ATTACK",
            Event::ForgottenBeastAttack => "FORGOTTEN_BEAST_ATTACK",
            Event::Siege => "SIEGE",
            Event::LostFort => "LOST_FORT",
        }
    }
    /// Priority (higher wins); FB and megabeast share one.
    pub fn priority(self) -> u8 {
        match self {
            Event::JustEmbarked => 0,
            Event::TavernMusic | Event::TavernDance => 1,
            Event::FortLevel => 2,
            Event::FirstGhost => 3,
            Event::FirstCavernOpened => 4,
            Event::DeathSpiral => 5,
            Event::MegabeastAttack | Event::ForgottenBeastAttack => 6,
            Event::Siege => 7,
            Event::LostFort => 8,
        }
    }
}

/// Music event of an announcement (its `ui::round9` class id and text).
pub fn event_of_announcement(class: &str, text: &str) -> Option<Event> {
    let t = text.to_ascii_lowercase();
    match class {
        "SIEGE" => Some(Event::Siege),
        "MEGABEAST" if t.contains("forgotten beast") => Some(Event::ForgottenBeastAttack),
        "MEGABEAST" => Some(Event::MegabeastAttack),
        // [undead13] necromancer sieges (ESTIMATE wording match).
        _ if (t.contains("the dead walk") || t.contains("undead")) && (t.contains("arrived") || t.contains("approach")) => Some(Event::Siege),
        _ => None,
    }
}

/// What kind of piece plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Title,
    Song,
    Card,
    Interlude,
}

/// A queued or playing piece of music.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub title: String,
    pub music: Option<usize>,
    pub piece: Piece,
}

/// What the caller must do.
#[derive(Clone, Debug, PartialEq)]
pub enum MusicCmd {
    /// Stop whatever plays (fading remnants too) and start this from the beginning.
    Start(Item),
    /// Fade the current music out over [`FADE_OUT_S`].
    FadeOut,
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Silent { until: f64 },
    Playing,
    Fading { until: f64 },
}

/// Music state machine.
#[derive(Clone, Debug)]
pub struct Director {
    state: State,
    pub current: Option<Item>,
    current_event: Option<Event>,
    queue: VecDeque<Item>,
    /// Song played last (no immediate repeat).
    last: Option<usize>,
    /// Songs whose cards were played in full already (this fort session).
    heard: BTreeSet<usize>,
    /// Shuffled card deck per song (indices into `MusicObj::cards`).
    decks: BTreeMap<usize, Vec<usize>>,
    pub rng: SplitMix,
    pub avg_secs: f64,
    title_mode: bool,
    /// After a fade: stay silent until then (entering a fort).
    pending_silence: Option<f64>,
    scratch: Vec<usize>,
}

impl Director {
    pub fn new(seed: u64, avg_secs: f64) -> Director {
        Director {
            state: State::Silent { until: 0.0 },
            current: None,
            current_event: None,
            queue: VecDeque::new(),
            last: None,
            heard: BTreeSet::new(),
            decks: BTreeMap::new(),
            rng: SplitMix(seed),
            avg_secs,
            title_mode: true,
            pending_silence: None,
            scratch: Vec::new(),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.state == State::Playing
    }

    /// The title screen opens: the title music from the start, then the ANY pool.
    pub fn enter_title(&mut self, now: f64, cat: &Catalog, title_id: &str) -> Option<MusicCmd> {
        self.title_mode = true;
        self.pending_silence = None;
        self.queue.clear();
        self.current_event = None;
        let item = match cat.dir.resolve(title_id).or_else(|| cat.files.get(title_id).cloned()) {
            Some((p, t)) => Some(Item { path: p, title: t, music: None, piece: Piece::Title }),
            None => cat.dir.song_title.clone().map(|p| Item { path: p, title: "Title".into(), music: None, piece: Piece::Title }),
        };
        match item {
            Some(it) => {
                self.queue.push_back(it);
                if self.current.is_some() {
                    self.state = State::Fading { until: now + FADE_OUT_S };
                    Some(MusicCmd::FadeOut)
                } else {
                    self.state = State::Silent { until: now };
                    None
                }
            }
            None => {
                self.state = State::Silent { until: now + 1.0 };
                None
            }
        }
    }

    /// A fort starts: fade the title music out; first song after 5..15 s (or the
    /// JUST_EMBARKED event, which the caller sends separately).
    pub fn enter_fort(&mut self, now: f64) -> Option<MusicCmd> {
        self.title_mode = false;
        self.queue.clear();
        self.current_event = None;
        self.heard.clear();
        self.decks.clear();
        let first = now + self.rng.range(FIRST_SLOT_S.0, FIRST_SLOT_S.1);
        if self.current.is_some() {
            self.state = State::Fading { until: now + FADE_OUT_S };
            // After the fade: silence until the first slot.
            self.pending_silence = Some(first);
            Some(MusicCmd::FadeOut)
        } else {
            self.state = State::Silent { until: first };
            None
        }
    }

    /// An event happened. Returns a command when its song takes over.
    pub fn on_event(&mut self, now: f64, cat: &Catalog, ev: Event) -> Option<MusicCmd> {
        if self.title_mode {
            return None;
        }
        if let Some(cur) = self.current_event {
            // The same event, or a lower one, while an event song plays: ignored.
            if matches!(self.state, State::Playing | State::Fading { .. }) && ev.priority() <= cur.priority() {
                return None;
            }
        }
        let songs: Vec<usize> = (0..cat.music.len()).filter(|&i| cat.music[i].events.iter().any(|e| e == ev.token()) && cat.playable(i)).collect();
        if songs.is_empty() {
            return None;
        }
        // Uniform among the event's songs (FREQUENCY only weighs random picks: ESTIMATE).
        let m = songs[self.rng.below(songs.len() as u64) as usize];
        let item = Item { path: cat.file(&cat.music[m].file).cloned().unwrap_or_default(), title: cat.title(m), music: Some(m), piece: Piece::Song };
        self.queue.clear();
        self.queue.push_back(item);
        self.current_event = Some(ev);
        self.last = Some(m);
        self.pending_silence = None;
        match self.state {
            State::Playing => {
                self.state = State::Fading { until: now + FADE_OUT_S };
                Some(MusicCmd::FadeOut)
            }
            State::Fading { .. } => None,
            State::Silent { .. } => {
                self.state = State::Silent { until: now };
                None
            }
        }
    }

    /// Advance. `finished`: the current piece ended on its own.
    pub fn step(&mut self, now: f64, cat: &Catalog, ctx: &Ctx, finished: bool) -> Option<MusicCmd> {
        match self.state.clone() {
            State::Playing => {
                if !finished {
                    return None;
                }
                self.current = None;
                if let Some(it) = self.queue.pop_front() {
                    return Some(self.start(it));
                }
                self.current_event = None;
                self.state = State::Silent { until: now + gap(self.avg_secs, &mut self.rng) };
                None
            }
            State::Fading { until } => {
                if now < until {
                    return None;
                }
                self.current = None;
                if let Some(t) = self.pending_silence.take() {
                    self.state = State::Silent { until: t };
                    return None;
                }
                match self.queue.pop_front() {
                    Some(it) => Some(self.start(it)),
                    None => {
                        self.state = State::Silent { until: now + gap(self.avg_secs, &mut self.rng) };
                        None
                    }
                }
            }
            State::Silent { until } => {
                if now < until {
                    return None;
                }
                if let Some(it) = self.queue.pop_front() {
                    return Some(self.start(it));
                }
                self.pick(cat, ctx);
                match self.queue.pop_front() {
                    Some(it) => Some(self.start(it)),
                    None => {
                        self.state = State::Silent { until: now + RETRY_S };
                        None
                    }
                }
            }
        }
    }

    fn start(&mut self, it: Item) -> MusicCmd {
        self.state = State::Playing;
        self.current = Some(it.clone());
        MusicCmd::Start(it)
    }

    /// Fill the queue with the next random slot (3.5 steps 1-4).
    fn pick(&mut self, cat: &Catalog, ctx: &Ctx) {
        let mut cands = std::mem::take(&mut self.scratch);
        candidates(cat, ctx, &mut cands);
        if cands.len() >= 2 {
            if let Some(l) = self.last {
                cands.retain(|&c| c != l);
            }
        }
        let fort = matches!(ctx, Ctx::Fort { .. });
        if fort && !cat.dir.neutral_cards.is_empty() && self.rng.unit() < NEUTRAL_INTERLUDE_P {
            let k = self.rng.below(cat.dir.neutral_cards.len() as u64) as usize;
            self.queue.push_back(Item { path: cat.dir.neutral_cards[k].clone(), title: String::new(), music: None, piece: Piece::Interlude });
            self.scratch = cands;
            return;
        }
        if cands.is_empty() {
            // Classic DF without tracks/: song_game in a fort.
            if fort {
                if let Some(p) = cat.dir.song_game.clone() {
                    self.queue.push_back(Item { path: p, title: "Dwarf Fortress".into(), music: None, piece: Piece::Song });
                }
            }
            self.scratch = cands;
            return;
        }
        let m = weighted(cat, &cands, &mut self.rng);
        self.scratch = cands;
        self.last = Some(m);
        let cards: Vec<PathBuf> = cat.music[m].cards.iter().filter_map(|c| cat.file(c).cloned()).collect();
        if !cards.is_empty() {
            if self.heard.insert(m) {
                // First time in this fort: all cards in order, then the song.
                for p in cards {
                    self.queue.push_back(Item { path: p, title: cat.title(m), music: Some(m), piece: Piece::Card });
                }
            } else {
                let n = 1 + self.rng.below(3) as usize;
                for _ in 0..n.min(cards.len()) {
                    let k = self.draw_card(m, cards.len());
                    self.queue.push_back(Item { path: cards[k].clone(), title: cat.title(m), music: Some(m), piece: Piece::Card });
                }
            }
        }
        if let Some(p) = cat.file(&cat.music[m].file) {
            self.queue.push_back(Item { path: p.clone(), title: cat.title(m), music: Some(m), piece: Piece::Song });
        }
    }

    /// Next card of a song's shuffled deck (refilled and reshuffled when empty).
    pub fn draw_card(&mut self, m: usize, n: usize) -> usize {
        let deck = self.decks.entry(m).or_default();
        if deck.is_empty() {
            deck.extend(0..n);
            for i in (1..deck.len()).rev() {
                let j = self.rng.below(i as u64 + 1) as usize;
                deck.swap(i, j);
            }
        }
        deck.pop().unwrap_or(0)
    }
}

/// Weighted pick by FREQUENCY (10 / 5 / 2).
pub fn weighted(cat: &Catalog, cands: &[usize], r: &mut SplitMix) -> usize {
    let total: u64 = cands.iter().map(|&c| cat.music[c].freq.weight() as u64).sum();
    let mut x = r.below(total.max(1));
    for &c in cands {
        let w = cat.music[c].freq.weight() as u64;
        if x < w {
            return c;
        }
        x -= w;
    }
    cands[cands.len() - 1]
}

/// Facts the transition events are derived from (see `rf_sim::audio14_api::AudioFacts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub ticks: u64,
    pub caverns_mask: u32,
    pub citizens: u32,
    pub ghosts: u32,
    pub land_rank: u8,
}

/// Detects the app-side events: FIRST_CAVERN_OPENED (+ a CAVERN_BREAK sound per new layer),
/// FIRST_GHOST, LOST_FORT, FORT_LEVEL, DEATH_SPIRAL and TAVERN_MUSIC_PRESENT. The first
/// snapshot after a load only sets the baseline, so loading never fires anything.
#[derive(Clone, Debug, Default)]
pub struct Detector {
    prev: Option<Facts>,
    deaths: VecDeque<u64>,
    spiral_ready: u64,
    tavern_ready: u64,
}

impl Detector {
    /// New facts. Pushes events into `out`; returns the number of newly discovered cavern
    /// layers (each plays CAVERN_BREAK).
    pub fn on_facts(&mut self, f: Facts, out: &mut Vec<Event>) -> u32 {
        let Some(p) = self.prev.replace(f) else { return 0 };
        let new = (f.caverns_mask & !p.caverns_mask).count_ones();
        if p.caverns_mask == 0 && f.caverns_mask != 0 {
            out.push(Event::FirstCavernOpened);
        }
        if p.ghosts == 0 && f.ghosts > 0 {
            out.push(Event::FirstGhost);
        }
        if p.citizens > 0 && f.citizens == 0 {
            out.push(Event::LostFort);
        }
        if f.land_rank > p.land_rank {
            out.push(Event::FortLevel);
        }
        new
    }

    /// A citizen died at `tick`.
    pub fn on_death(&mut self, tick: u64, out: &mut Vec<Event>) {
        while self.deaths.front().map(|&t| t + DEATH_SPIRAL_WINDOW < tick).unwrap_or(false) {
            self.deaths.pop_front();
        }
        self.deaths.push_back(tick);
        if self.deaths.len() >= DEATH_SPIRAL_DEATHS && tick >= self.spiral_ready {
            out.push(Event::DeathSpiral);
            self.spiral_ready = tick + DEATH_SPIRAL_COOLDOWN;
            self.deaths.clear();
        }
    }

    /// A performance (sim cue). Music with a crowd in a tavern or a temple (the sim performs
    /// only there; spec 3.3: "in a tavern/temple") -> TAVERN_MUSIC_PRESENT.
    pub fn on_perform(&mut self, tick: u64, _tavern: bool, music: bool, audience: u16, out: &mut Vec<Event>) {
        if music && audience >= TAVERN_CROWD && tick >= self.tavern_ready {
            out.push(Event::TavernMusic);
            self.tavern_ready = tick + TAVERN_COOLDOWN;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::{tests as ct, Catalog, SoundDir};
    use super::*;

    fn cat() -> (Catalog, PathBuf) {
        let root = ct::fake_sound_dir(&format!("dir-{:?}", std::thread::current().id()).replace(['(', ')'], ""));
        let (m, s, i) = super::super::catalog::parse_raw(ct::RAWS.as_bytes());
        (Catalog::assemble(SoundDir::scan(&root), vec![(m, s, i, root.clone())]), root)
    }

    fn ids(c: &Catalog, v: &[usize]) -> Vec<String> {
        let mut x: Vec<String> = v.iter().map(|&i| c.music[i].id.clone()).collect();
        x.sort();
        x
    }

    #[test]
    fn contexts_pick_the_right_pools() {
        let (c, root) = cat();
        let mut v = Vec::new();
        candidates(&c, &Ctx::Title, &mut v);
        assert_eq!(ids(&c, &v), vec!["SONG_C", "SONG_D"]);
        candidates(&c, &Ctx::Fort { month: 10, age_ticks: Some(1000), caverns: true }, &mut v);
        assert_eq!(ids(&c, &v), vec!["SONG_A", "SONG_B", "SONG_C", "SONG_D", "SONG_E", "SONG_F"]);
        candidates(&c, &Ctx::Fort { month: 4, age_ticks: Some(2 * TICKS_PER_YEAR), caverns: false }, &mut v);
        assert_eq!(ids(&c, &v), vec!["SONG_B", "SONG_C", "SONG_D", "SONG_G"]);
        // Unknown founding date counts as an older fort.
        candidates(&c, &Ctx::Fort { month: 0, age_ticks: None, caverns: false }, &mut v);
        assert!(ids(&c, &v).contains(&"SONG_G".to_string()) && !ids(&c, &v).contains(&"SONG_F".to_string()));
        assert!(context_true("SPRING", &Ctx::Title) && !context_true("WINTER", &Ctx::Title) && !context_true("MAIN", &Ctx::Title));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn frequency_weights() {
        let (c, root) = cat();
        let n = |id: &str| c.music.iter().position(|m| m.id == id).unwrap();
        let (normal, unc, rare) = (n("SONG_D"), n("SONG_B"), n("SONG_C"));
        let mut r = SplitMix(42);
        let mut cnt = [0u32; 3];
        for _ in 0..100_000 {
            let p = weighted(&c, &[normal, unc, rare], &mut r);
            cnt[[normal, unc, rare].iter().position(|&x| x == p).unwrap()] += 1;
        }
        let (u, ra) = (cnt[1] as f64 / cnt[0] as f64, cnt[2] as f64 / cnt[0] as f64);
        assert!((u - 0.5).abs() < 0.02, "{u}");
        assert!((ra - 0.2).abs() < 0.02, "{ra}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn gap_range_and_mean() {
        let mut r = SplitMix(7);
        let mut sum = 0.0;
        for _ in 0..10_000 {
            let g = gap(240.0, &mut r);
            assert!((180.0..=300.0).contains(&g));
            sum += g;
        }
        assert!((sum / 10_000.0 - 240.0).abs() < 2.0);
    }

    #[test]
    fn decks_cycle_through_every_card() {
        let mut d = Director::new(3, 240.0);
        let mut seen = BTreeSet::new();
        for _ in 0..5 {
            seen.insert(d.draw_card(0, 5));
        }
        assert_eq!(seen.len(), 5);
        let mut seen = BTreeSet::new();
        for _ in 0..5 {
            seen.insert(d.draw_card(0, 5));
        }
        assert_eq!(seen.len(), 5);
    }

    fn run_until_start(d: &mut Director, c: &Catalog, ctx: &Ctx, now: &mut f64) -> Item {
        for _ in 0..10_000 {
            *now += 1.0;
            if let Some(MusicCmd::Start(it)) = d.step(*now, c, ctx, false) {
                return it;
            }
        }
        panic!("nothing started");
    }

    #[test]
    fn first_pick_plays_all_cards_then_the_song_and_no_immediate_repeat() {
        let (mut c, root) = cat();
        c.dir.neutral_cards.clear(); // no interludes for this test
        let ctx = Ctx::Fort { month: 0, age_ticks: Some(10), caverns: true };
        // Only SONG_A (two cards) and SONG_D fit: restrict the catalog to them.
        c.music.retain(|m| m.id == "SONG_A" || m.id == "SONG_D");
        let a = c.music.iter().position(|m| m.id == "SONG_A").unwrap();
        let mut d = Director::new(1, 240.0);
        d.enter_fort(0.0);
        let mut now = 0.0;
        let mut plays = Vec::new();
        let mut a_runs = Vec::new();
        for _ in 0..12 {
            let it = run_until_start(&mut d, &c, &ctx, &mut now);
            // follow the chain until the song itself
            let mut chain = vec![it.clone()];
            while chain.last().unwrap().piece == Piece::Card {
                match d.step(now, &c, &ctx, true) {
                    Some(MusicCmd::Start(n)) => chain.push(n),
                    other => panic!("{other:?}"),
                }
            }
            let song = chain.last().unwrap().music.unwrap();
            if song == a {
                a_runs.push(chain.len() - 1);
            }
            plays.push(song);
            assert_eq!(d.step(now, &c, &ctx, true), None); // song ends -> silent gap
        }
        assert!(plays.windows(2).all(|w| w[0] != w[1]), "{plays:?}");
        // First SONG_A: both cards; later: 1..=2 (of 2 cards; 1..=3 capped by the deck).
        assert_eq!(a_runs[0], 2);
        assert!(a_runs[1..].iter().all(|&n| (1..=2).contains(&n)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn events_override_by_priority() {
        let (mut c, root) = cat();
        c.dir.neutral_cards.clear();
        let ctx = Ctx::Fort { month: 0, age_ticks: Some(10), caverns: false };
        let mut d = Director::new(5, 240.0);
        d.enter_fort(0.0);
        let mut now = 0.0;
        run_until_start(&mut d, &c, &ctx, &mut now);
        // A siege fades the song out, then its song starts.
        assert_eq!(d.on_event(now, &c, Event::Siege), Some(MusicCmd::FadeOut));
        assert_eq!(d.step(now + 0.5, &c, &ctx, false), None);
        let Some(MusicCmd::Start(it)) = d.step(now + FADE_OUT_S + 0.1, &c, &ctx, false) else { panic!() };
        assert_eq!(c.music[it.music.unwrap()].id, "SONG_C");
        // Same event, or a lower one: ignored. A higher one is not in the raws: ignored too.
        assert_eq!(d.on_event(now + 2.0, &c, Event::Siege), None);
        assert_eq!(d.on_event(now + 2.0, &c, Event::ForgottenBeastAttack), None);
        assert_eq!(d.on_event(now + 2.0, &c, Event::LostFort), None);
        // Song ends: the gap timer restarts, then a lower event plays again.
        assert_eq!(d.step(now + 3.0, &c, &ctx, true), None);
        assert!(!d.is_playing());
        assert_eq!(d.on_event(now + 4.0, &c, Event::ForgottenBeastAttack), None);
        let Some(MusicCmd::Start(it)) = d.step(now + 4.0, &c, &ctx, false) else { panic!() };
        assert_eq!(c.music[it.music.unwrap()].id, "SONG_H");
        // Title screen: events are ignored.
        let mut t = Director::new(1, 240.0);
        t.enter_title(0.0, &c, "DWARF_FORTRESS");
        assert_eq!(t.on_event(0.0, &c, Event::Siege), None);
        let Some(MusicCmd::Start(it)) = t.step(0.0, &c, &Ctx::Title, false) else { panic!() };
        assert_eq!(it.piece, Piece::Title);
        assert!(it.path.ends_with("Dwarf_Fortress.ogg"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn announcements_map_to_events() {
        assert_eq!(event_of_announcement("SIEGE", "A vile force of darkness has arrived!"), Some(Event::Siege));
        assert_eq!(event_of_announcement("MEGABEAST", "The Forgotten Beast Ur has come! A huge thing."), Some(Event::ForgottenBeastAttack));
        assert_eq!(event_of_announcement("MEGABEAST", "The dragon has come!"), Some(Event::MegabeastAttack));
        assert_eq!(event_of_announcement("OTHER", "Urist has made a cheese."), None);
    }

    #[test]
    fn transitions_fire_once_and_not_on_load() {
        let mut det = Detector::default();
        let mut ev = Vec::new();
        // First snapshot after a load: caverns already discovered, ghosts present -> nothing.
        assert_eq!(det.on_facts(Facts { ticks: 0, caverns_mask: 0b10, citizens: 7, ghosts: 2, land_rank: 1 }, &mut ev), 0);
        assert!(ev.is_empty());
        assert_eq!(det.on_facts(Facts { ticks: 10, caverns_mask: 0b110, citizens: 7, ghosts: 2, land_rank: 2 }, &mut ev), 1);
        assert_eq!(ev, vec![Event::FortLevel]); // not FIRST_CAVERN (one was known)
        ev.clear();
        let mut fresh = Detector::default();
        fresh.on_facts(Facts { citizens: 7, ..Default::default() }, &mut ev);
        assert_eq!(fresh.on_facts(Facts { caverns_mask: 0b10, citizens: 7, ghosts: 1, ..Default::default() }, &mut ev), 1);
        assert_eq!(ev, vec![Event::FirstCavernOpened, Event::FirstGhost]);
        ev.clear();
        fresh.on_facts(Facts { caverns_mask: 0b10, citizens: 0, ghosts: 1, ..Default::default() }, &mut ev);
        assert_eq!(ev, vec![Event::LostFort]);
    }

    #[test]
    fn death_spiral_window_and_cooldown() {
        let mut det = Detector::default();
        let mut ev = Vec::new();
        // 4 deaths, then a gap longer than the window: no spiral.
        for t in [0, 100, 200, 300] {
            det.on_death(t, &mut ev);
        }
        det.on_death(300 + DEATH_SPIRAL_WINDOW + 400, &mut ev);
        assert!(ev.is_empty());
        let base = 1_000_000;
        for k in 0..5 {
            det.on_death(base + k * 1000, &mut ev);
        }
        assert_eq!(ev, vec![Event::DeathSpiral]);
        // Five more within the cooldown: nothing.
        for k in 0..5 {
            det.on_death(base + 10_000 + k, &mut ev);
        }
        assert_eq!(ev.len(), 1);
        for k in 0..5 {
            det.on_death(base + DEATH_SPIRAL_COOLDOWN + 20_000 + k, &mut ev);
        }
        assert_eq!(ev.len(), 2);
        // Tavern music: crowd threshold and cooldown.
        let mut ev = Vec::new();
        det.on_perform(10, true, true, TAVERN_CROWD - 1, &mut ev);
        det.on_perform(10, true, false, 50, &mut ev);
        assert!(ev.is_empty());
        det.on_perform(10, true, true, TAVERN_CROWD, &mut ev);
        det.on_perform(20, true, true, TAVERN_CROWD, &mut ev);
        assert_eq!(ev, vec![Event::TavernMusic]);
        // A temple performance counts too (spec 3.3: tavern/temple).
        let mut t = Detector::default();
        let mut ev = Vec::new();
        t.on_perform(10, false, true, TAVERN_CROWD, &mut ev);
        assert_eq!(ev, vec![Event::TavernMusic]);
    }
}
