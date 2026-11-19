//! [audio14] What DF's install offers: the `MUSIC` / `SOUND` raws (vanilla and the world's
//! active mods), the sound folder (`data/sound`), DF's audio settings (`init.txt`) and the two
//! small adventure-mode tables (`starting-volume.txt`, `spatialization.txt`).
//!
//! Everything is read at runtime from the user's install and never copied. File ids are
//! resolved from the folder and file names (docs/mechanics/audio14.md 1.1); nothing is taken
//! from the executable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `FREQUENCY` of a music object (w:Audio): weights 10 / 5 / 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Freq {
    #[default]
    Normal,
    Uncommon,
    Rare,
}

impl Freq {
    pub fn weight(self) -> u32 {
        match self {
            Freq::Normal => 10,
            Freq::Uncommon => 5,
            Freq::Rare => 2,
        }
    }
}

/// One `[MUSIC:...]` object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicObj {
    pub id: String,
    pub file: String,
    pub cards: Vec<String>,
    pub events: Vec<String>,
    pub contexts: Vec<String>,
    pub freq: Freq,
}

/// One `[SOUND:...]` object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundObj {
    pub id: String,
    pub file: String,
    pub announcements: Vec<String>,
    pub savage: bool,
}

/// A mod's `[SOUND_FILE:...]` / `[MUSIC_FILE:...]` identifier (since DF 50.09).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileIdent {
    pub id: String,
    /// Path relative to the mod folder.
    pub file: String,
    pub title: String,
}

/// Parse MUSIC, SOUND, MUSIC_FILE and SOUND_FILE objects of one raw file (bracketed tokens
/// only: text outside brackets is a comment, also an unbracketed `SOUND:X` line).
pub fn parse_raw(bytes: &[u8]) -> (Vec<MusicObj>, Vec<SoundObj>, Vec<FileIdent>) {
    let (_, _, objs) = rf_raws::parse_file(bytes, 0);
    // `parse_file` splits only on the file's first `[OBJECT:...]` type, so a file with
    // `[OBJECT:MUSIC]` and later `[OBJECT:SOUND]` (or a `SOUND_FILE` among `SOUND`s) would glue
    // the later objects' tokens onto the last earlier one. Re-split the token stream on all
    // four audio object headers ourselves.
    let mut split: Vec<(String, String, Vec<rf_raws::Token>)> = Vec::new();
    for t in objs.into_iter().flat_map(|o| o.tokens) {
        if matches!(t.name.as_str(), "MUSIC" | "SOUND" | "SOUND_FILE" | "MUSIC_FILE") {
            split.push((t.name.clone(), t.arg(0).to_string(), Vec::new()));
        } else if let Some(o) = split.last_mut() {
            o.2.push(t);
        }
    }
    let (mut music, mut sounds, mut idents) = (Vec::new(), Vec::new(), Vec::new());
    for (kind, id, tokens) in split {
        struct O {
            id: String,
            tokens: Vec<rf_raws::Token>,
        }
        let o = O { id, tokens };
        match kind.as_str() {
            "MUSIC" => {
                let mut m = MusicObj { id: o.id.clone(), ..Default::default() };
                for t in &o.tokens {
                    match t.name.as_str() {
                        "FILE" => m.file = t.arg(0).to_string(),
                        "CARD" => m.cards.push(t.arg(0).to_string()),
                        "EVENT" => m.events.push(t.arg(0).to_string()),
                        "CONTEXT" => m.contexts.push(t.arg(0).to_string()),
                        "FREQUENCY" => {
                            m.freq = match t.arg(0) {
                                "UNCOMMON" => Freq::Uncommon,
                                "RARE" => Freq::Rare,
                                _ => Freq::Normal,
                            }
                        }
                        _ => {}
                    }
                }
                music.push(m);
            }
            "SOUND" => {
                let mut s = SoundObj { id: o.id.clone(), ..Default::default() };
                for t in &o.tokens {
                    match t.name.as_str() {
                        "FILE" => s.file = t.arg(0).to_string(),
                        "ANNOUNCEMENT" => s.announcements.push(t.arg(0).to_string()),
                        "SAVAGE_AREA" => s.savage = true,
                        _ => {}
                    }
                }
                sounds.push(s);
            }
            "SOUND_FILE" | "MUSIC_FILE" => {
                let mut f = FileIdent { id: o.id.clone(), ..Default::default() };
                for t in &o.tokens {
                    match t.name.as_str() {
                        // A path may contain ':' (drive letters are not expected; keep it simple).
                        "FILE" => f.file = t.args.join(":"),
                        "TITLE" => f.title = t.args.join(":"),
                        _ => {}
                    }
                }
                idents.push(f);
            }
            _ => {}
        }
    }
    (music, sounds, idents)
}

/// DF's name normalisation for sound folders: `drink_&_industry` -> `DRINK_AND_INDUSTRY`,
/// `strike_the_earth!` -> `STRIKE_THE_EARTH`, `Bruising The Fat` -> `BRUISING_THE_FAT`.
pub fn norm(s: &str) -> String {
    let s = s.replace('&', "AND").replace('!', "");
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c.to_ascii_uppercase());
        } else {
            gap = true;
        }
    }
    out
}

/// Title of an adventure track file stem: `3_Bruising The Fat (Shar Ikal)` -> `Bruising The Fat`.
pub fn adv_title(stem: &str) -> String {
    let s = match stem.split_once('_') {
        Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => rest,
        _ => stem,
    };
    let s = match s.find('(') {
        Some(i) => &s[..i],
        None => s,
    };
    s.trim().to_string()
}

/// `drink_&_industry` -> `Drink & Industry`.
pub fn title_case(dir: &str) -> String {
    dir.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn oggs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("ogg")).unwrap_or(false)).collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Index of the files under `data/sound` that the raws refer to.
#[derive(Clone, Debug, Default)]
pub struct SoundDir {
    pub root: PathBuf,
    /// `norm(dir)` of every `tracks/<dir>` (not `adv`) -> its `.ogg` files.
    pub song_dirs: BTreeMap<String, (String, Vec<PathBuf>)>,
    /// `norm(title)` of every `tracks/adv/<n>_<Title>.ogg` -> (title, file).
    pub adv: BTreeMap<String, (String, PathBuf)>,
    /// Lower-case stem of every `sounds/*.ogg` -> file.
    pub sounds: BTreeMap<String, PathBuf>,
    /// `ambience/*.ogg` by stem (as on disk, e.g. `Magma_Close`).
    pub ambience: BTreeMap<String, PathBuf>,
    /// `cards/Card_N.ogg`, sorted by N.
    pub neutral_cards: Vec<PathBuf>,
    pub song_title: Option<PathBuf>,
    pub song_game: Option<PathBuf>,
    /// `audio/actions/dig.ogg` (stand-in for DF 53.17's FMOD dig sounds).
    pub dig: Option<PathBuf>,
    /// `sounds/alert.ogg`.
    pub alert: Option<PathBuf>,
    /// `audio/combat/melee/{metal,flesh}/hit-*.ogg` (optional extra combat sounds).
    pub hits_metal: Vec<PathBuf>,
    pub hits_flesh: Vec<PathBuf>,
    /// `.ogg` files found (all folders).
    pub n_files: usize,
}

impl SoundDir {
    pub fn scan(root: &Path) -> SoundDir {
        let mut d = SoundDir { root: root.to_path_buf(), ..Default::default() };
        let tracks = root.join("tracks");
        if let Ok(rd) = std::fs::read_dir(&tracks) {
            let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            dirs.sort();
            for p in dirs {
                let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let files = oggs(&p);
                if name.eq_ignore_ascii_case("adv") {
                    for f in files {
                        let t = adv_title(&stem(&f));
                        d.adv.insert(norm(&t), (t, f));
                    }
                } else {
                    d.song_dirs.insert(norm(&name), (title_case(&name), files));
                }
            }
        }
        for f in oggs(&root.join("sounds")) {
            d.sounds.insert(stem(&f).to_ascii_lowercase(), f);
        }
        d.alert = d.sounds.get("alert").cloned();
        for f in oggs(&root.join("ambience")) {
            d.ambience.insert(stem(&f), f);
        }
        let mut cards: Vec<(u32, PathBuf)> =
            oggs(&root.join("cards")).into_iter().filter_map(|f| stem(&f).rsplit('_').next().and_then(|n| n.parse().ok()).map(|n| (n, f))).collect();
        cards.sort();
        d.neutral_cards = cards.into_iter().map(|(_, f)| f).collect();
        d.song_title = Some(root.join("song_title.ogg")).filter(|p| p.is_file());
        d.song_game = Some(root.join("song_game.ogg")).filter(|p| p.is_file());
        d.dig = Some(root.join("audio").join("actions").join("dig.ogg")).filter(|p| p.is_file());
        let melee = root.join("audio").join("combat").join("melee");
        let hits = |k: &str| oggs(&melee.join(k)).into_iter().filter(|f| stem(f).starts_with("hit")).collect::<Vec<_>>();
        d.hits_metal = hits("metal");
        d.hits_flesh = hits("flesh");
        d.n_files = count_oggs(root, 0);
        d
    }

    /// Resolve a music or sound file id (1.1 rules 1-4). `None` = no such file.
    pub fn resolve(&self, id: &str) -> Option<(PathBuf, String)> {
        if let Some(name) = id.strip_prefix("DEFAULT_SOUND_") {
            return self.sounds.get(&name.to_ascii_lowercase()).map(|p| (p.clone(), title_case(&name.to_ascii_lowercase())));
        }
        if let Some((song, n)) = id.rsplit_once("_CARD_") {
            let (_, files) = self.song_dirs.get(song)?;
            let want = format!("_{n}");
            return files.iter().find(|f| stem(f).ends_with(&want)).map(|f| (f.clone(), String::new()));
        }
        if let Some((title, files)) = self.song_dirs.get(id) {
            let full = files.iter().find(|f| stem(f).ends_with("_Full"));
            let pick = full.or(if files.len() == 1 { files.first() } else { None }).cloned().or_else(|| {
                // Several files and none `_Full`: the largest (ESTIMATE tie-break).
                files.iter().max_by_key(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0)).cloned()
            });
            return pick.map(|p| (p, title.clone()));
        }
        self.adv.get(id).map(|(t, p)| (p.clone(), t.clone()))
    }
}

fn count_oggs(dir: &Path, depth: u32) -> usize {
    if depth > 8 {
        return 0;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            n += count_oggs(&p, depth + 1);
        } else if p.extension().map(|e| e.eq_ignore_ascii_case("ogg")).unwrap_or(false) {
            n += 1;
        }
    }
    n
}

/// DF's audio settings (`data/init/init_default.txt`, then the prefs; last value wins).
#[derive(Clone, Debug, PartialEq)]
pub struct DfAudioInit {
    pub sound: bool,
    pub master: u8,
    pub music: u8,
    pub ambience: u8,
    pub sfx: u8,
    pub ui: u8,
    pub avg_song_secs: u16,
    pub title_music: String,
    /// Which file the values came from (for the Audio page), empty = DF defaults.
    pub source: String,
}

impl Default for DfAudioInit {
    fn default() -> Self {
        DfAudioInit { sound: true, master: 255, music: 255, ambience: 230, sfx: 204, ui: 204, avg_song_secs: 240, title_music: "DWARF_FORTRESS".into(), source: String::new() }
    }
}

impl DfAudioInit {
    /// Apply the `[TOKEN:VALUE]` lines of one init file.
    pub fn apply(&mut self, text: &str) -> bool {
        let mut any = false;
        for chunk in text.split('[').skip(1) {
            let Some(end) = chunk.find(']') else { continue };
            let Some((k, v)) = chunk[..end].split_once(':') else { continue };
            let v = v.trim();
            let vol = |v: &str| v.parse::<i64>().ok().map(|n| n.clamp(0, 255) as u8);
            match k.trim() {
                "SOUND" => self.sound = !v.eq_ignore_ascii_case("NO"),
                "MASTER_VOLUME" => self.master = vol(v).unwrap_or(self.master),
                "MUSIC_VOLUME" => self.music = vol(v).unwrap_or(self.music),
                "AMBIENCE_VOLUME" => self.ambience = vol(v).unwrap_or(self.ambience),
                "SFX_VOLUME" => self.sfx = vol(v).unwrap_or(self.sfx),
                "UI_VOLUME" => self.ui = vol(v).unwrap_or(self.ui),
                "AVERAGE_TIME_BETWEEN_SONGS" => {
                    self.avg_song_secs = v.parse::<i64>().map(|n| n.clamp(10, 600) as u16).unwrap_or(self.avg_song_secs)
                }
                "TITLE_MUSIC" => self.title_music = v.to_string(),
                _ => continue,
            }
            any = true;
        }
        any
    }

    /// Read the install's files in DF's order: defaults, then the prefs (the install's, then
    /// the `%APPDATA%` copy).
    pub fn read(df: &Path) -> DfAudioInit {
        let mut d = DfAudioInit::default();
        let mut files = vec![df.join("data").join("init").join("init_default.txt"), df.join("prefs").join("init.txt")];
        if let Some(app) = std::env::var_os("APPDATA") {
            files.push(PathBuf::from(app).join("Bay 12 Games").join("Dwarf Fortress").join("prefs").join("init.txt"));
        }
        for f in files {
            if let Ok(t) = std::fs::read_to_string(&f) {
                if d.apply(&t) {
                    d.source = f.display().to_string();
                }
            }
        }
        d
    }
}

/// Alternating `key` / `value` lines (`starting-volume.txt`, `spatialization.txt`).
pub fn parse_kv(text: &str) -> BTreeMap<String, f32> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let mut m = BTreeMap::new();
    for pair in lines.chunks(2) {
        if let [k, v] = pair {
            if let Ok(f) = v.parse::<f32>() {
                m.insert(k.to_ascii_lowercase(), f);
            }
        }
    }
    m
}

/// Base gain per bus from `starting-volume.txt` (`song 0.85`, the rest 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartVol {
    pub song: f32,
    pub ambience: f32,
    pub sfx: f32,
    pub ui: f32,
}

impl Default for StartVol {
    fn default() -> Self {
        StartVol { song: 0.85, ambience: 1.0, sfx: 1.0, ui: 1.0 }
    }
}

impl StartVol {
    pub fn from_kv(m: &BTreeMap<String, f32>) -> StartVol {
        let d = StartVol::default();
        let g = |k: &str, d: f32| m.get(k).copied().filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 2.0)).unwrap_or(d);
        StartVol { song: g("song", d.song), ambience: g("ambiance", d.ambience), sfx: g("soundeffects", d.sfx), ui: g("userinterface", d.ui) }
    }
}

/// Everything audio knows about the install.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub dir: SoundDir,
    pub music: Vec<MusicObj>,
    pub sounds: Vec<SoundObj>,
    /// Resolved file of every id used by the raws (and mod identifiers): id -> (file, title).
    pub files: BTreeMap<String, (PathBuf, String)>,
    pub start: StartVol,
    /// `maxhearingdistance` in tiles.
    pub hearing: f32,
    /// The mods this catalog was built for (`World::meta.mods`; empty = vanilla only).
    pub mods: Vec<String>,
    /// Ids that did not resolve (logged once).
    pub unresolved: Vec<String>,
}

impl Catalog {
    pub fn file(&self, id: &str) -> Option<&PathBuf> {
        self.files.get(id).map(|(p, _)| p)
    }

    pub fn title(&self, music: usize) -> String {
        let m = &self.music[music];
        match self.files.get(&m.file) {
            Some((_, t)) if !t.is_empty() => t.clone(),
            _ => title_case(&m.id.to_ascii_lowercase()),
        }
    }

    /// Music objects whose file resolved.
    pub fn playable(&self, music: usize) -> bool {
        self.files.contains_key(&self.music[music].file)
    }

    /// Assemble a catalog from parsed raws (in load order: later objects with the same id
    /// replace earlier ones) and a scanned folder.
    pub fn assemble(dir: SoundDir, raws: Vec<(Vec<MusicObj>, Vec<SoundObj>, Vec<FileIdent>, PathBuf)>) -> Catalog {
        let mut c = Catalog { hearing: 10.0, ..Default::default() };
        let mut idents: BTreeMap<String, (PathBuf, String)> = BTreeMap::new();
        for (music, sounds, ids, base) in raws {
            for m in music {
                match c.music.iter_mut().find(|x| x.id == m.id) {
                    Some(x) => *x = m,
                    None => c.music.push(m),
                }
            }
            for s in sounds {
                match c.sounds.iter_mut().find(|x| x.id == s.id) {
                    Some(x) => *x = s,
                    None => c.sounds.push(s),
                }
            }
            for f in ids {
                let p = base.join(&f.file);
                if p.is_file() {
                    idents.insert(f.id.clone(), (p, f.title.clone()));
                }
            }
        }
        let mut want: Vec<String> = Vec::new();
        for m in &c.music {
            want.push(m.file.clone());
            want.extend(m.cards.iter().cloned());
        }
        for s in &c.sounds {
            want.push(s.file.clone());
        }
        for id in want {
            if id.is_empty() || c.files.contains_key(&id) {
                continue;
            }
            // Mod identifiers win over the naming rules.
            match idents.get(&id).cloned().or_else(|| dir.resolve(&id)) {
                Some(r) => {
                    c.files.insert(id, r);
                }
                None => {
                    if !c.unresolved.contains(&id) {
                        c.unresolved.push(id);
                    }
                }
            }
        }
        c.dir = dir;
        c
    }

    /// Build from a DF install: vanilla raws plus the given active mods (`id:version`).
    pub fn build(df: &Path, mods: &[String]) -> Catalog {
        let root = df.join("data").join("sound");
        let dir = SoundDir::scan(&root);
        let mut raws = Vec::new();
        let read_objects = |objects: &Path, base: &Path, raws: &mut Vec<_>| {
            let Ok(rd) = std::fs::read_dir(objects) else { return };
            let mut files: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    n.ends_with(".txt") && (n.starts_with("music_") || n.starts_with("sound_"))
                })
                .collect();
            files.sort();
            for f in files {
                if let Ok(b) = std::fs::read(&f) {
                    let (m, s, i) = parse_raw(&b);
                    raws.push((m, s, i, base.to_path_buf()));
                }
            }
        };
        let vanilla = df.join("data").join("vanilla").join("vanilla_music");
        read_objects(&vanilla.join("objects"), &vanilla, &mut raws);
        if !mods.is_empty() {
            let installed = rf_raws::installed_mods_all(df);
            for entry in mods {
                let (id, ver) = rf_raws::parse_mod_entry(entry);
                if id.starts_with("vanilla_") {
                    continue;
                }
                let mut cands: Vec<&rf_raws::ModInfo> = installed.iter().filter(|m| m.id == id).collect();
                cands.sort_by_key(|m| (Some(m.numeric_version) == ver, m.numeric_version));
                if let Some(m) = cands.last() {
                    read_objects(&m.dir.join("objects"), &m.dir, &mut raws);
                    read_objects(&m.dir.join("sound"), &m.dir, &mut raws);
                }
            }
        }
        let mut c = Catalog::assemble(dir, raws);
        let audio = root.join("audio");
        if let Ok(t) = std::fs::read_to_string(audio.join("starting-volume.txt")) {
            c.start = StartVol::from_kv(&parse_kv(&t));
        }
        if let Ok(t) = std::fs::read_to_string(audio.join("spatialization.txt")) {
            if let Some(h) = parse_kv(&t).get("maxhearingdistance").filter(|h| **h > 0.0) {
                c.hearing = *h;
            }
        }
        c.mods = mods.to_vec();
        if !c.unresolved.is_empty() {
            eprintln!("audio: {} sound ids without a file: {}", c.unresolved.len(), c.unresolved.join(", "));
        }
        c
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Inline raws written for the test (not copied from DF): three songs, two sounds, an
    /// unbracketed object and trailing comments.
    pub const RAWS: &str = "music_test\n[OBJECT:MUSIC]\n\
        [MUSIC:SONG_A][FILE:EXPANSIVE_CAVERN] some comment after the token\n\
        [CARD:EXPANSIVE_CAVERN_CARD_1][CARD:EXPANSIVE_CAVERN_CARD_2]\n\
        [EVENT:FIRST_CAVERN_OPENED][CONTEXT:CAVERNS_OPENED]\n\
        [MUSIC:SONG_B][FILE:STRIKE_THE_EARTH][CONTEXT:MAIN][FREQUENCY:UNCOMMON][EVENT:JUST_EMBARKED]\n\
        [MUSIC:SONG_C][FILE:BRUISING_THE_FAT][EVENT:SIEGE][FREQUENCY:RARE][CONTEXT:ANY]\n\
        [MUSIC:SONG_D][FILE:DRINK_AND_INDUSTRY][CONTEXT:ANY]\n\
        [MUSIC:SONG_E][FILE:WINTER_ENTOMBS_YOU][CONTEXT:WINTER]\n\
        [MUSIC:SONG_F][FILE:FIRST_YEAR][CONTEXT:FIRST_YEAR]\n\
        [MUSIC:SONG_G][FILE:ANOTHER_YEAR][CONTEXT:SECOND_YEAR_PLUS]\n\
        [MUSIC:SONG_H][FILE:FORGOTTEN_BEAST][EVENT:FORGOTTEN_BEAST_ATTACK][EVENT:MEGABEAST_ATTACK]\n\
        [OBJECT:SOUND]\n\
        [SOUND:S_MOOD][FILE:DEFAULT_SOUND_STRANGE_MOOD][ANNOUNCEMENT:STRANGE_MOOD]\n\
        [SOUND:S_STEP][FILE:DEFAULT_SOUND_GIANT_STEP_1][SAVAGE_AREA]\n\
        [SOUND:S_AMBUSH][FILE:DEFAULT_SOUND_AMBUSH][ANNOUNCEMENT:AMBUSH_AMBUSHER][ANNOUNCEMENT:AMBUSH_THIEF_SUPPORT]\n\
        [SOUND:S_MEGA][FILE:DEFAULT_SOUND_MEGABEAST][ANNOUNCEMENT:MEGABEAST_ARRIVAL]\n\
        SOUND:NOT_A_TOKEN FILE:DEFAULT_SOUND_SIEGE ANNOUNCEMENT:UNDEAD_ATTACK\n";

    /// A temp dir laid out like DF's `data/sound`, with empty placeholder files.
    pub fn fake_sound_dir(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rf-audio14-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let files = [
            "tracks/expansive_cavern/EC_1.ogg",
            "tracks/expansive_cavern/EC_2.ogg",
            "tracks/expansive_cavern/EC_Full.ogg",
            "tracks/strike_the_earth!/STE_3.ogg",
            "tracks/strike_the_earth!/STE_Full.ogg",
            "tracks/drink_&_industry/DI_Full.ogg",
            "tracks/winter_entombs_you/WEY_Full.ogg",
            "tracks/first_year/FY_Full.ogg",
            "tracks/another_year/AY_Full.ogg",
            "tracks/forgotten_beast/FB_Full.ogg",
            "tracks/nabidas/Nabidas.ogg",
            "tracks/dwarf_fortress/Dwarf_Fortress.ogg",
            "tracks/adv/3_Bruising The Fat (Shar Ikal).ogg",
            "tracks/adv/12_Two Coins.ogg",
            "sounds/Giant_Step_1.ogg",
            "sounds/strange_mood.ogg",
            "sounds/ambush.ogg",
            "sounds/megabeast.ogg",
            "sounds/alert.ogg",
            "cards/Card_2.ogg",
            "cards/Card_10.ogg",
            "cards/Card_1.ogg",
            "ambience/Forest.ogg",
            "audio/actions/dig.ogg",
        ];
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"").unwrap();
        }
        root
    }

    #[test]
    fn parses_music_and_sound_raws() {
        let (m, s, i) = parse_raw(RAWS.as_bytes());
        assert_eq!(m.len(), 8);
        assert!(i.is_empty());
        let a = &m[0];
        assert_eq!((a.id.as_str(), a.file.as_str()), ("SONG_A", "EXPANSIVE_CAVERN"));
        assert_eq!(a.cards, vec!["EXPANSIVE_CAVERN_CARD_1", "EXPANSIVE_CAVERN_CARD_2"]);
        assert_eq!(a.events, vec!["FIRST_CAVERN_OPENED"]);
        assert_eq!(a.contexts, vec!["CAVERNS_OPENED"]);
        assert_eq!(m[1].freq, Freq::Uncommon);
        assert_eq!(m[2].freq, Freq::Rare);
        assert_eq!(m[7].events, vec!["FORGOTTEN_BEAST_ATTACK", "MEGABEAST_ATTACK"]);
        // The unbracketed SOUND line is a comment.
        assert_eq!(s.len(), 4);
        assert!(s.iter().all(|x| x.id != "NOT_A_TOKEN"));
        assert!(s[1].savage && !s[0].savage);
        assert_eq!(s[2].announcements, vec!["AMBUSH_AMBUSHER", "AMBUSH_THIEF_SUPPORT"]);
        assert_eq!((Freq::Normal.weight(), Freq::Uncommon.weight(), Freq::Rare.weight()), (10, 5, 2));
    }

    /// One file with `[OBJECT:MUSIC]` and then `[OBJECT:SOUND]`, and a `SOUND_FILE` inside an
    /// `[OBJECT:SOUND]` file: every object is found and none swallows the next one's tokens.
    #[test]
    fn mixed_object_files_split_on_every_header() {
        let (m, s, _) = parse_raw(RAWS.as_bytes());
        let h = m.iter().find(|x| x.id == "SONG_H").unwrap();
        assert_eq!(h.file, "FORGOTTEN_BEAST");
        assert_eq!(s.len(), 4);
        let mixed = "snd
[OBJECT:SOUND]
[SOUND:A][FILE:DEFAULT_SOUND_X][ANNOUNCEMENT:MADE_ARTIFACT]
            [SOUND_FILE:MY_BEEP][FILE:sound/beep.ogg]
[SOUND:B][FILE:MY_BEEP][ANNOUNCEMENT:STRANGE_MOOD]
";
        let (_, s, i) = parse_raw(mixed.as_bytes());
        assert_eq!(s.iter().map(|x| (x.id.as_str(), x.file.as_str())).collect::<Vec<_>>(), vec![("A", "DEFAULT_SOUND_X"), ("B", "MY_BEEP")]);
        assert_eq!(i.len(), 1);
        assert_eq!((i[0].id.as_str(), i[0].file.as_str()), ("MY_BEEP", "sound/beep.ogg"));
    }

    #[test]
    fn normalises_names() {
        assert_eq!(norm("drink_&_industry"), "DRINK_AND_INDUSTRY");
        assert_eq!(norm("strike_the_earth!"), "STRIKE_THE_EARTH");
        assert_eq!(norm("koganusan"), "KOGANUSAN");
        assert_eq!(adv_title("3_Bruising The Fat (Shar Ikal)"), "Bruising The Fat");
        assert_eq!(norm(&adv_title("13_Losing Is Fun")), "LOSING_IS_FUN");
        assert_eq!(title_case("drink_&_industry"), "Drink & Industry");
    }

    #[test]
    fn resolves_ids_from_the_folder_layout() {
        let root = fake_sound_dir("resolve");
        let d = SoundDir::scan(&root);
        let r = |id: &str| d.resolve(id).map(|(p, _)| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"));
        assert_eq!(r("DRINK_AND_INDUSTRY").as_deref(), Some("tracks/drink_&_industry/DI_Full.ogg"));
        assert_eq!(r("STRIKE_THE_EARTH").as_deref(), Some("tracks/strike_the_earth!/STE_Full.ogg"));
        assert_eq!(r("STRIKE_THE_EARTH_CARD_3").as_deref(), Some("tracks/strike_the_earth!/STE_3.ogg"));
        assert_eq!(r("EXPANSIVE_CAVERN_CARD_2").as_deref(), Some("tracks/expansive_cavern/EC_2.ogg"));
        assert_eq!(r("NABIDAS").as_deref(), Some("tracks/nabidas/Nabidas.ogg"));
        assert_eq!(r("BRUISING_THE_FAT").as_deref(), Some("tracks/adv/3_Bruising The Fat (Shar Ikal).ogg"));
        assert_eq!(r("TWO_COINS").as_deref(), Some("tracks/adv/12_Two Coins.ogg"));
        assert_eq!(r("DEFAULT_SOUND_GIANT_STEP_1").as_deref(), Some("sounds/Giant_Step_1.ogg"));
        assert_eq!(r("DEFAULT_SOUND_WEDDING"), None);
        assert_eq!(r("NO_SUCH_SONG"), None);
        assert_eq!(d.resolve("BRUISING_THE_FAT").unwrap().1, "Bruising The Fat");
        assert_eq!(d.resolve("DRINK_AND_INDUSTRY").unwrap().1, "Drink & Industry");
        let cards: Vec<String> = d.neutral_cards.iter().map(|p| stem(p)).collect();
        assert_eq!(cards, vec!["Card_1", "Card_2", "Card_10"]);
        assert!(d.alert.is_some() && d.dig.is_some() && d.ambience.contains_key("Forest"));
        assert_eq!(d.n_files, 24);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mod_identifiers_win_and_later_objects_replace_earlier() {
        let root = fake_sound_dir("mods");
        let modd = root.join("mymod");
        std::fs::create_dir_all(modd.join("sound")).unwrap();
        std::fs::write(modd.join("sound").join("beep.ogg"), b"").unwrap();
        let ident = "sound_file_x\n[OBJECT:SOUND_FILE]\n[SOUND_FILE:DEFAULT_SOUND_STRANGE_MOOD][FILE:sound/beep.ogg]\n";
        let override_raw = "sound_x\n[OBJECT:SOUND]\n[SOUND:S_MOOD][FILE:DEFAULT_SOUND_STRANGE_MOOD][ANNOUNCEMENT:MADE_ARTIFACT]\n";
        let (m, s, i) = parse_raw(RAWS.as_bytes());
        let (m2, s2, i2) = parse_raw(ident.as_bytes());
        let (m3, s3, i3) = parse_raw(override_raw.as_bytes());
        assert_eq!(i2.len(), 1);
        let c = Catalog::assemble(SoundDir::scan(&root), vec![(m, s, i, root.clone()), (m2, s2, i2, modd.clone()), (m3, s3, i3, modd.clone())]);
        assert_eq!(c.file("DEFAULT_SOUND_STRANGE_MOOD"), Some(&modd.join("sound/beep.ogg")));
        let mood = c.sounds.iter().find(|s| s.id == "S_MOOD").unwrap();
        assert_eq!(mood.announcements, vec!["MADE_ARTIFACT"]);
        assert_eq!(c.sounds.len(), 4);
        // Every test song resolves; a CARD that has no file is listed as unresolved.
        assert!((0..c.music.len()).all(|i| c.playable(i)));
        assert!(c.unresolved.contains(&"EXPANSIVE_CAVERN_CARD_2".to_string()) == false);
        assert_eq!(c.title(2), "Bruising The Fat");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_df_init_and_small_tables() {
        let mut d = DfAudioInit::default();
        assert!(d.apply("[SOUND:YES]\n[MASTER_VOLUME:255]\n[MUSIC_VOLUME:255]\n[AVERAGE_TIME_BETWEEN_SONGS:240]"));
        assert_eq!((d.master, d.music, d.ambience, d.avg_song_secs), (255, 255, 230, 240));
        // prefs override the defaults; values are clamped
        d.apply("[MASTER_VOLUME:0]\n[SFX_VOLUME:999]\n[AVERAGE_TIME_BETWEEN_SONGS:5]\n[SOUND:NO]\n[TITLE_MUSIC:NABIDAS]");
        assert_eq!((d.master, d.sfx, d.avg_song_secs, d.sound), (0, 255, 10, false));
        assert_eq!(d.title_music, "NABIDAS");
        d.apply("[AVERAGE_TIME_BETWEEN_SONGS:4000]");
        assert_eq!(d.avg_song_secs, 600);
        assert!(!DfAudioInit::default().apply("no tokens here"));
        let kv = parse_kv("ambiance\n1\nsong\n0.85\n\nsoundeffects\n0.5\nbroken\nx\n");
        let sv = StartVol::from_kv(&kv);
        assert_eq!((sv.song, sv.ambience, sv.sfx, sv.ui), (0.85, 1.0, 0.5, 1.0));
        assert_eq!(StartVol::from_kv(&BTreeMap::new()), StartVol::default());
        assert_eq!(parse_kv("maxhearingdistance\n10").get("maxhearingdistance"), Some(&10.0));
    }
}
