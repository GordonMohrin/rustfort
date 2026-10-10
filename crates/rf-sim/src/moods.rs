//! Strange moods and artifacts (round 5).
//!
//! # DF rules implemented (wiki: Strange mood, Artifact, Insanity)
//! - **Trigger**: a counter starts at 1 000 and drops by 1 every 100 ticks (about 3 months to
//!   zero). Once it is zero, each of those steps has a 1 in 500 chance to start a mood (about
//!   2.4 % a day); the counter is then reset to 1 000. Only one mood at a time, only with at
//!   least 20 eligible dwarves, and only while the fort's artifact cap allows one more:
//!   `min(items / 100.1, revealed subterranean tiles / 2304)`.
//! - **Eligible**: adult dwarf citizens (creatures with STRANGE_MOODS: dwarves) who are not on
//!   duty as soldiers, can grasp, are not in a breakdown or insane and never had a mood or made
//!   an artifact. The moody dwarf is drawn by DF's profession lottery: 21 tickets for
//!   craftsdwarves, metalworkers, jewelers, weavers, clothiers, glassmakers, leatherworkers,
//!   bone carvers; 11 for carpenters, bowyers, stoneworkers, stone carvers, woodworkers; 6 for
//!   everyone else.
//! - **Type**: fey, secretive or possessed by luck; only highly stressed dwarves get fell or
//!   macabre moods. Announcements are DF's ("is taken by a fey mood!", "withdraws from
//!   society...", "has been possessed!", "looses a roaring laughter, fell and terrible!",
//!   "begins to stalk and brood...").
//! - **Skill and workshop**: the highest moodable skill (ties at random) picks the workshop:
//!   mason/miner/stone carver/stonecutter -> mason's workshop, stone crafter, engraver, bone
//!   carver, wood crafter -> craftsdwarf's workshop, carpenter -> carpenter's workshop, bowyer ->
//!   bowyer's workshop, gem cutter/setter -> jeweler's workshop, metal crafter/metalsmith/
//!   weaponsmith/armorsmith -> metalsmith's forge, glassmaker -> glass furnace, tanner/
//!   leatherworker -> leather works, weaver/clothier -> clothier's shop, mechanic -> mechanic's
//!   workshop. Without a moodable skill: craftsdwarf's workshop and crafts. The dwarf claims
//!   the nearest reachable one and throws out whoever works there.
//! - **Materials**: 1-3 units of the skill's base material (stoneworkers rock, woodworkers logs,
//!   metalworkers bars, gem workers rough gems, glassmakers raw glass, leatherworkers leather,
//!   weavers/clothiers cloth, bone carvers bones or shells) and up to 7 decorations drawn from
//!   logs, metal bars, cut gems, rock blocks, rough gems, boulders, bones, leather, cloth and
//!   raw glass. The dwarf collects them one after another from anywhere (e.g. a strange-mood
//!   stockpile) and brings them to the workshop. Waiting **exactly 50 000 ticks** for one
//!   demanded item drives the dwarf insane; every delivered item resets the clock.
//! - **No needs**: moody dwarves do not eat, drink or sleep.
//! - **Work**: "X has begun a mysterious construction!", then "X has created Y, a <material>
//!   <item>!" The artifact (quality artifact) is of a kind the skill can make (crafts for crafters,
//!   furniture for masons/carpenters, weapons/armor/furniture for smiths, clothing for weavers/
//!   clothiers, leather goods for leatherworkers, mechanisms for mechanics, ...), named, made of
//!   the first (base) material. The creator gets 20 000 experience in the mood skill (Legendary
//!   from nothing) except after a possessed mood, and a very strong happy thought (MadeArtifact).
//! - **Fell**: the dwarf murders the nearest dwarf on the spot, drags the corpse into the
//!   workshop (craftsdwarf's or leather works) and makes a bone or leather artifact. **Macabre**:
//!   like fell but demands bones instead of a victim.
//! - **Insanity**: stark raving mad, melancholy, catatonic (harmless; they stop eating and
//!   drinking and die) or berserk (attacks everyone in sight).
//!
//! # Estimates (not on the wiki)
//! - Type odds: highly stressed (stress >= 25 000): 1/3 fell, 1/3 macabre, 1/3 normal; normal:
//!   fey 40 %, secretive 40 %, possessed 20 %.
//! - Base units 1-3 and decorations 0-7, uniformly; the mysterious construction takes 6 000
//!   ticks (5 days); artifact value = 10 x material value x 120 (DF artifact quality multiplier)
//!   plus twice the decorations' value.
//! - "Items created" for the artifact cap is the current item count; dwarves who already made
//!   an artifact are recognized by a MadeArtifact thought in their exported emotions. The
//!   export has no mood counter: it starts at a random value 0..=1 000.
//! - Insanity kind: fey/secretive -> melancholy 1/3, stark raving 1/3, catatonic 1/6, berserk
//!   1/6; possessed -> stark raving or berserk; fell/macabre -> berserk or melancholy.
//! - Artifact names are made of random syllables plus an English epithet ("The Ruined Songs"
//!   style), not DF's language files.

use crate::path::Goals;
use crate::rng::Rng;
use crate::{AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{tile_flags, Pos};
mod saves10; // [saves10] save/load of mood progress

/// DF: insanity after exactly 50 000 ticks waiting for a demanded item.
pub const MOOD_WAIT_LIMIT: u64 = 50_000;
/// Work on the artifact (estimate).
pub const MOOD_WORK_TICKS: u64 = 6_000;
/// Counter reset value; it drops by one every 100 ticks (wiki).
const COUNTER_RESET: i32 = 1_000;
/// Eligible dwarves needed (wiki).
const MIN_ELIGIBLE: usize = 20;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub enum MoodKind {
    Fey,
    Secretive,
    Possessed,
    Macabre,
    Fell,
}

impl MoodKind {
    /// DF `mood_type` name.
    pub fn name(self) -> &'static str {
        match self {
            MoodKind::Fey => "Fey",
            MoodKind::Secretive => "Secretive",
            MoodKind::Possessed => "Possessed",
            MoodKind::Macabre => "Macabre",
            MoodKind::Fell => "Fell",
        }
    }
    fn start_text(self) -> &'static str {
        match self {
            MoodKind::Fey => "is taken by a fey mood!",
            MoodKind::Secretive => "withdraws from society...",
            MoodKind::Possessed => "has been possessed!",
            MoodKind::Fell => "looses a roaring laughter, fell and terrible!",
            MoodKind::Macabre => "begins to stalk and brood...",
        }
    }
    /// Unit sheet line (wiki).
    pub fn profile_text(self) -> &'static str {
        match self {
            MoodKind::Fey => "Has the aspect of one fey!",
            MoodKind::Secretive => "Peculiarly secretive...",
            MoodKind::Possessed => "Possessed by unknown forces!",
            MoodKind::Fell => "Has a horrible fell look!",
            MoodKind::Macabre => "Brooding darkly...",
        }
    }
    fn work_caption(self) -> &'static str {
        match self {
            MoodKind::Fey => "Strange Mood: works furiously!",
            MoodKind::Secretive => "Strange Mood: works secretly...",
            MoodKind::Possessed => "Strange Mood: keeps muttering...",
            MoodKind::Macabre => "Strange Mood: works, darkly brooding...",
            MoodKind::Fell => "Strange Mood: works with menacing fury!",
        }
    }
}

/// A material class a mood can demand.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub(crate) enum Mat {
    Boulder,
    Log,
    Bar,
    CutGem,
    Block,
    RoughGem,
    Bone,
    Shell,
    Leather,
    Cloth,
    RawGlass,
    Body,
}

impl Mat {
    pub fn label(self) -> &'static str {
        match self {
            Mat::Boulder => "rock",
            Mat::Log => "logs",
            Mat::Bar => "metal bars",
            Mat::CutGem => "cut gems",
            Mat::Block => "rock blocks",
            Mat::RoughGem => "rough gems",
            Mat::Bone => "bones",
            Mat::Shell => "shells",
            Mat::Leather => "leather",
            Mat::Cloth => "cloth",
            Mat::RawGlass => "raw glass",
            Mat::Body => "a body",
        }
    }
}

/// Decoration demands (wiki).
const DECOR: [Mat; 10] = [Mat::Log, Mat::Bar, Mat::CutGem, Mat::Block, Mat::RoughGem, Mat::Boulder, Mat::Bone, Mat::Leather, Mat::Cloth, Mat::RawGlass];

/// Moodable skills (DF `job_skill` names).
const MOODABLE: [&str; 22] = [
    "FORGE_ARMOR", "BONE_CARVE", "BOWYER", "CARPENTRY", "CLOTHESMAKING", "ENGRAVE_STONE", "CUT_GEM", "ENCRUST_GEM", "GLASSMAKER",
    "LEATHERWORK", "MASONRY", "MECHANICS", "METALCRAFT", "FORGE_FURNITURE", "MINING", "CARVE_STONE", "STONECRAFT", "CUT_STONE",
    "TANNER", "FORGE_WEAPON", "WEAVING", "WOODCRAFT",
];

/// (item kind, subtype, noun)
type ArtKind = (&'static str, &'static str, &'static str);
const CRAFTS: &[ArtKind] = &[
    ("AMULET", "", "amulet"),
    ("BRACELET", "", "bracelet"),
    ("CROWN", "", "crown"),
    ("FIGURINE", "", "figurine"),
    ("RING", "", "ring"),
    ("EARRING", "", "earring"),
    ("SCEPTER", "", "scepter"),
];
const FURNITURE: &[ArtKind] = &[
    ("TABLE", "", "table"),
    ("CHAIR", "", "throne"),
    ("DOOR", "", "door"),
    ("CABINET", "", "cabinet"),
    ("BOX", "", "coffer"),
    ("STATUE", "", "statue"),
    ("COFFIN", "", "sarcophagus"),
];
const WOOD_FURNITURE: &[ArtKind] = &[("TABLE", "", "table"), ("CHAIR", "", "chair"), ("DOOR", "", "door"), ("CABINET", "", "cabinet"), ("BOX", "", "chest"), ("BED", "", "bed")];
const WEAPONS: &[ArtKind] = &[
    ("WEAPON", "ITEM_WEAPON_AXE_BATTLE", "battle axe"),
    ("WEAPON", "ITEM_WEAPON_SWORD_SHORT", "short sword"),
    ("WEAPON", "ITEM_WEAPON_SPEAR", "spear"),
    ("WEAPON", "ITEM_WEAPON_MACE", "mace"),
    ("WEAPON", "ITEM_WEAPON_HAMMER_WAR", "war hammer"),
];
const ARMOR: &[ArtKind] = &[
    ("ARMOR", "ITEM_ARMOR_BREASTPLATE", "breastplate"),
    ("ARMOR", "ITEM_ARMOR_MAIL_SHIRT", "mail shirt"),
    ("HELM", "ITEM_HELM_HELM", "helm"),
    ("PANTS", "ITEM_PANTS_GREAVES", "greaves"),
    ("SHIELD", "ITEM_SHIELD_SHIELD", "shield"),
];
const CLOTHES: &[ArtKind] = &[
    ("ARMOR", "ITEM_ARMOR_CLOAK", "cloak"),
    ("ARMOR", "ITEM_ARMOR_ROBE", "robe"),
    ("PANTS", "ITEM_PANTS_TROUSERS", "trousers"),
    ("HELM", "ITEM_HELM_HOOD", "hood"),
    ("GLOVES", "ITEM_GLOVES_GLOVES", "gloves"),
    ("SHOES", "ITEM_SHOES_SOCKS", "socks"),
    ("BAG", "", "bag"),
];
const LEATHER_GOODS: &[ArtKind] = &[
    ("ARMOR", "ITEM_ARMOR_LEATHER", "leather armor"),
    ("HELM", "ITEM_HELM_CAP", "cap"),
    ("PANTS", "ITEM_PANTS_PANTS", "pants"),
    ("SHOES", "ITEM_SHOES_BOOTS", "boots"),
    ("GLOVES", "ITEM_GLOVES_GLOVES", "gloves"),
    ("BAG", "", "bag"),
    ("QUIVER", "", "quiver"),
];
const BOWS: &[ArtKind] = &[("WEAPON", "ITEM_WEAPON_CROSSBOW", "crossbow"), ("WEAPON", "ITEM_WEAPON_BOW", "bow")];
const MECHS: &[ArtKind] = &[("TRAPPARTS", "", "mechanism")];
const GEMS: &[ArtKind] = &[("SMALLGEM", "", "gem"), ("AMULET", "", "amulet"), ("CROWN", "", "crown"), ("RING", "", "ring"), ("FIGURINE", "", "figurine")];
const GLASS: &[ArtKind] = &[("GOBLET", "", "goblet"), ("FIGURINE", "", "figurine"), ("AMULET", "", "amulet"), ("TABLE", "", "table")];

/// Workshop (building kind, subtype, DF label), base material and artifact kinds of a skill.
fn skill_spec(skill: &str) -> (&'static str, &'static str, &'static str, Mat, &'static [ArtKind]) {
    match skill {
        "MASONRY" | "MINING" => ("Workshop", "Masons", "Mason's Workshop", Mat::Boulder, FURNITURE),
        "CARVE_STONE" | "CUT_STONE" => ("Workshop", "Masons", "Mason's Workshop", Mat::Boulder, CRAFTS),
        "STONECRAFT" | "ENGRAVE_STONE" => ("Workshop", "Craftsdwarfs", "Craftsdwarf's Workshop", Mat::Boulder, CRAFTS),
        "WOODCRAFT" => ("Workshop", "Craftsdwarfs", "Craftsdwarf's Workshop", Mat::Log, CRAFTS),
        "BONE_CARVE" => ("Workshop", "Craftsdwarfs", "Craftsdwarf's Workshop", Mat::Bone, CRAFTS),
        "CARPENTRY" => ("Workshop", "Carpenters", "Carpenter's Workshop", Mat::Log, WOOD_FURNITURE),
        "BOWYER" => ("Workshop", "Bowyers", "Bowyer's Workshop", Mat::Log, BOWS),
        "CUT_GEM" | "ENCRUST_GEM" => ("Workshop", "Jewelers", "Jeweler's Workshop", Mat::RoughGem, GEMS),
        "FORGE_WEAPON" => ("Workshop", "MetalsmithsForge", "Metalsmith's Forge", Mat::Bar, WEAPONS),
        "FORGE_ARMOR" => ("Workshop", "MetalsmithsForge", "Metalsmith's Forge", Mat::Bar, ARMOR),
        "FORGE_FURNITURE" => ("Workshop", "MetalsmithsForge", "Metalsmith's Forge", Mat::Bar, FURNITURE),
        "METALCRAFT" => ("Workshop", "MetalsmithsForge", "Metalsmith's Forge", Mat::Bar, CRAFTS),
        "GLASSMAKER" => ("Furnace", "GlassFurnace", "Glass Furnace", Mat::RawGlass, GLASS),
        "LEATHERWORK" | "TANNER" => ("Workshop", "Leatherworks", "Leather Works", Mat::Leather, LEATHER_GOODS),
        "WEAVING" | "CLOTHESMAKING" => ("Workshop", "Clothiers", "Clothier's Shop", Mat::Cloth, CLOTHES),
        "MECHANICS" => ("Workshop", "Mechanics", "Mechanic's Workshop", Mat::Boulder, MECHS),
        _ => ("Workshop", "Craftsdwarfs", "Craftsdwarf's Workshop", Mat::Boulder, CRAFTS),
    }
}

/// Profession lottery tickets (wiki).
fn tickets(profession: &str) -> u32 {
    match profession {
        "ARMORER" | "BLACKSMITH" | "BONE_CARVER" | "CLOTHIER" | "CRAFTSMAN" | "JEWELER" | "GEM_CUTTER" | "GEM_SETTER" | "GLASSMAKER"
        | "LEATHERWORKER" | "METALCRAFTER" | "METALSMITH" | "STONECRAFTER" | "WEAPONSMITH" | "WEAVER" | "WOODCRAFTER" => 21,
        "BOWYER" | "CARPENTER" | "STONEWORKER" | "STONE_CARVER" | "WOODWORKER" => 11,
        _ => 6,
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
enum Step {
    Claim,
    Hunt(u32),
    Fetch(i32),
    Carry(i32),
    Wait,
    Work(u64),
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct Active {
    slot: usize,
    uid: i32,
    kind: MoodKind,
    #[serde(with = "crate::saves11::interned")] // [saves11]
    skill: crate::saves11::SStr,
    #[serde(with = "crate::saves11::interned")] // [saves11]
    shop_kind: crate::saves11::SStr,
    #[serde(with = "crate::saves11::interned")] // [saves11]
    shop_sub: crate::saves11::SStr,
    #[serde(with = "crate::saves11::interned")] // [saves11]
    shop_label: crate::saves11::SStr,
    shop_bid: i32,
    site: u32,
    demands: Vec<Mat>,
    all_demands: Vec<Mat>,
    collected: Vec<i32>,
    step: Step,
    wait_since: Option<u64>,
    started: u64,
    needs: (i32, i32, i32),
    #[serde(with = "crate::saves11::interned")] // [saves11]
    art: ArtKind,
    record: usize,
}

/// One strange mood, for reports.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct MoodRecord {
    pub unit_id: i32,
    pub name: String,
    pub kind: String,
    pub skill: String,
    pub workshop: String,
    pub demands: Vec<String>,
    pub started: u64,
    pub ended: Option<u64>,
    /// "created <artifact>", "insane: Melancholy", "died", ...
    pub outcome: String,
    pub artifact_id: i32,
    pub victim: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)] // [saves11]
pub(crate) struct Moods {
    pub counter: i32,
    pub active: Option<Active>,
    pub records: Vec<MoodRecord>,
    /// Unit ids who had a mood or made an artifact (sorted).
    had: Vec<i32>,
    pub off: bool,
    inited: bool,
    pub artifacts_made: u32,
    pub insane: u32,
}

impl Default for Moods {
    fn default() -> Self {
        Moods { counter: COUNTER_RESET, active: None, records: Vec::new(), had: Vec::new(), off: false, inited: false, artifacts_made: 0, insane: 0 }
    }
}

fn is_insane(mood: &str) -> bool {
    matches!(mood, "Melancholy" | "Raving" | "Catatonic")
}

impl Sim {
    pub(crate) fn init_moods(&mut self) {
        if self.moods.inited {
            return;
        }
        self.moods.inited = true;
        let mut r = Rng::for_unit(self.seed, 11, 0, 0x300D);
        self.moods.counter = r.below(COUNTER_RESET as u32 + 1) as i32;
        let mut had: Vec<i32> = self.world.units.iter().filter(|u| u.emotions.iter().any(|e| e.thought == "MadeArtifact")).map(|u| u.id).collect();
        had.sort_unstable();
        self.moods.had = had;
    }

    /// [moods] Every 100 ticks: the mood counter, starting moods, watching the moody dwarf.
    pub(crate) fn moods_pass(&mut self, now: u64) {
        if self.moods.off {
            return;
        }
        self.init_moods();
        if let Some(a) = &self.moods.active {
            let s = a.slot;
            if s >= self.ai.len() || self.ai[s].kind == Kind::Dead {
                self.end_mood(now, "died".into());
            }
            return;
        }
        if self.moods.counter > 0 {
            self.moods.counter -= 1;
            return;
        }
        let mut r = Rng::for_unit(self.seed, 11, now, 0x300E);
        if !r.chance(1, 500) {
            return;
        }
        self.try_start_mood(now, &mut r);
    }

    fn mood_eligible(&self, s: usize) -> bool {
        let ai = &self.ai[s];
        if ai.kind != Kind::Worker {
            return false;
        }
        let u = &self.world.units[ai.uidx];
        if u.race != "DWARF" || u.age < 18.0 || !u.mood.is_empty() || u.profession == "CHILD" || u.profession == "BABY" {
            return false;
        }
        if self.moods.had.binary_search(&u.id).is_ok() || self.social_no_work(s) {
            return false;
        }
        if let Some(f) = self.combat.f.get(s) {
            if f.duty != crate::military::Duty::None || f.role != crate::combat::Role::None {
                return false;
            }
        }
        s >= self.combat.f.len() || self.grasp_ok(s)
    }

    /// Artifact cap: min(items / 100.1, revealed subterranean tiles / 2304) (wiki).
    fn artifact_cap(&self) -> u32 {
        let items = self.world.items.len() as f64 / 100.1;
        let revealed = self.world.tiles.iter().filter(|t| t.flags & tile_flags::SUBTERRANEAN != 0 && t.flags & tile_flags::HIDDEN == 0).count() as f64 / 2304.0;
        items.min(revealed) as u32
    }

    fn try_start_mood(&mut self, now: u64, r: &mut Rng) -> bool {
        let cands: Vec<usize> = (0..self.ai.len()).filter(|&s| self.mood_eligible(s)).collect();
        if cands.len() < MIN_ELIGIBLE {
            return false;
        }
        let made = self.moods.had.len() as u32 + self.moods.artifacts_made;
        if made >= self.artifact_cap() {
            return false;
        }
        let total: u32 = cands.iter().map(|&s| tickets(&self.world.units[self.ai[s].uidx].profession)).sum();
        let mut pick = r.below(total.max(1));
        let mut chosen = cands[0];
        for &s in &cands {
            let t = tickets(&self.world.units[self.ai[s].uidx].profession);
            if pick < t {
                chosen = s;
                break;
            }
            pick -= t;
        }
        self.start_mood(chosen, None, now);
        true
    }

    /// Start a strange mood on unit `s` (kind random by DF rules unless given).
    pub(crate) fn start_mood(&mut self, s: usize, kind: Option<MoodKind>, now: u64) {
        let mut r = Rng::for_unit(self.seed, s, now, 0x300F);
        let uidx = self.ai[s].uidx;
        let (stress, name, uid) = {
            let u = &self.world.units[uidx];
            (u.stress, u.name.clone(), u.id)
        };
        let kind = kind.unwrap_or_else(|| {
            if stress >= 25_000 && !r.chance(1, 3) {
                if r.chance(1, 2) {
                    MoodKind::Fell
                } else {
                    MoodKind::Macabre
                }
            } else {
                match r.below(10) {
                    0..=3 => MoodKind::Fey,
                    4..=7 => MoodKind::Secretive,
                    _ => MoodKind::Possessed,
                }
            }
        });
        // Mood skill: the highest moodable skill, ties at random.
        let skill: &'static str = match kind {
            MoodKind::Fell => {
                if r.chance(1, 2) {
                    "BONE_CARVE"
                } else {
                    "LEATHERWORK"
                }
            }
            MoodKind::Macabre => "BONE_CARVE",
            _ => {
                let u = &self.world.units[uidx];
                let best = MOODABLE.iter().map(|k| u.skill(k).clamp(0, 20)).max().unwrap_or(0);
                if best == 0 {
                    ["STONECRAFT", "WOODCRAFT", "BONE_CARVE"][r.below(3) as usize]
                } else {
                    let tied: Vec<&'static str> = MOODABLE.iter().copied().filter(|k| u.skill(k).clamp(0, 20) == best).collect();
                    tied[r.below(tied.len() as u32) as usize]
                }
            }
        };
        let (shop_kind, shop_sub, shop_label, base, arts) = skill_spec(skill);
        let mut demands = Vec::new();
        let mut art = arts[r.below(arts.len() as u32) as usize];
        match kind {
            MoodKind::Fell => {
                demands.push(Mat::Body);
                art = if skill == "BONE_CARVE" { CRAFTS[r.below(CRAFTS.len() as u32) as usize] } else { LEATHER_GOODS[r.below(LEATHER_GOODS.len() as u32) as usize] };
            }
            _ => {
                let base = if skill == "BONE_CARVE" && kind != MoodKind::Macabre && r.chance(1, 3) { Mat::Shell } else { base };
                for _ in 0..1 + r.below(3) {
                    demands.push(base);
                }
                for _ in 0..r.below(8) {
                    demands.push(DECOR[r.below(DECOR.len() as u32) as usize]);
                }
            }
        }
        // Mood takes over: drop the current task, no more needs.
        self.cancel_task(s);
        let ai = &self.ai[s];
        let needs = (ai.thirst.min(crate::needs::THIRST_CONSIDER - 1), ai.hunger.min(crate::needs::HUNGER_CONSIDER - 1), ai.sleep.min(crate::needs::SLEEP_CONSIDER - 1));
        self.world.units[uidx].mood = kind.name().into();
        if let Err(p) = self.moods.had.binary_search(&uid) {
            self.moods.had.insert(p, uid);
        }
        let mut seen = Vec::new();
        for d in &demands {
            if !seen.contains(&d.label()) {
                seen.push(d.label());
            }
        }
        self.moods.records.push(MoodRecord {
            unit_id: uid,
            name: name.clone(),
            kind: kind.name().into(),
            skill: skill.into(),
            workshop: shop_label.into(),
            demands: seen.iter().map(|s| s.to_string()).collect(),
            started: now,
            ended: None,
            outcome: String::new(),
            artifact_id: -1,
            victim: None,
        });
        let record = self.moods.records.len() - 1;
        self.moods.active = Some(Active {
            slot: s,
            uid,
            kind,
            skill,
            shop_kind,
            shop_sub,
            shop_label,
            shop_bid: -1,
            site: NONE,
            all_demands: demands.clone(),
            demands,
            collected: Vec::new(),
            step: Step::Claim,
            wait_since: None,
            started: now,
            needs,
            art,
            record,
        });
        let pos = self.world.units[uidx].pos;
        self.announce(now, format!("{name} {}", kind.start_text()), Some(pos), AnnouncementKind::Warning);
        self.ai[s].task = Task::Mood;
        self.ai[s].wake = now + 1;
    }

    /// [moods] Hook at the top of `decide`: moody dwarves follow their mood, insane ones wander
    /// without eating or drinking.
    pub(crate) fn moods_decide(&mut self, s: usize, now: u64) -> bool {
        if let Some(a) = &self.moods.active {
            if a.slot == s {
                self.ai[s].task = Task::Mood;
                self.mood_step(s, now);
                return true;
            }
        }
        let mood = &self.world.units[self.ai[s].uidx].mood;
        if mood.is_empty() || !is_insane(mood) {
            return false;
        }
        let catatonic = mood == "Catatonic";
        let mut r = Rng::for_unit(self.seed, s, now, 0x3010);
        self.ai[s].task = Task::Idle;
        if catatonic {
            self.ai[s].wake = now + 200;
        } else {
            self.wander(s, now, &mut r);
        }
        true
    }

    /// [moods] Path arrival / wake of a unit with `Task::Mood`.
    pub(crate) fn mood_arrive(&mut self, s: usize, now: u64) {
        let ok = self.moods.active.as_ref().map(|a| a.slot == s).unwrap_or(false);
        if !ok {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        }
        self.mood_step(s, now);
    }

    /// [moods] `cancel_task` of a moody dwarf (path failure, combat taking over).
    pub(crate) fn mood_cancel(&mut self, s: usize) {
        let now = self.ticks;
        let Some(a) = self.moods.active.as_mut() else { return };
        if a.slot != s {
            return;
        }
        match a.step {
            Step::Fetch(id) => {
                a.step = Step::Wait;
                self.release_reagent(id);
                if let Some(&i) = self.item_idx.get(&id) {
                    self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                }
            }
            Step::Carry(id) => {
                a.step = Step::Wait;
                let here = self.ai[s].pos;
                self.drop_item(s, id, here);
                self.release_reagent(id);
            }
            _ => {}
        }
    }

    fn mood_site_goals(&self, site: u32, c: u32) -> Goals {
        let mut g = Goals::default();
        let t = site as usize;
        if self.nav.walkable(t) && (c == 0 || self.nav.comp_root(t) == c) {
            g.push(site);
            return g;
        }
        let (x, y, z) = self.nav.xyz(t);
        for &(ddx, ddy) in &crate::nav::DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if self.nav.walkable(q) && (c == 0 || self.nav.comp_root(q) == c) {
                g.push(q as u32);
            }
        }
        g
    }

    fn at_site(&self, s: usize, site: u32) -> bool {
        self.world.pos_of(self.ai[s].pos as usize).dist(self.world.pos_of(site as usize)) <= 1
    }

    /// The moody dwarf's state machine.
    fn mood_step(&mut self, s: usize, now: u64) {
        let Some(a) = self.moods.active.clone() else { return };
        // No needs while in a mood (wiki).
        let ai = &mut self.ai[s];
        ai.thirst = a.needs.0;
        ai.hunger = a.needs.1;
        ai.sleep = a.needs.2;
        ai.sleeping = false;
        ai.last = now;
        ai.task = Task::Mood;
        match a.step {
            Step::Claim => self.mood_claim(s, now),
            Step::Hunt(v) => self.mood_hunt(s, v, now),
            Step::Fetch(id) => {
                if self.item_reachable_here(s, id) {
                    let carried = self.mood_take(s, id);
                    let c = self.nav.comp_near(self.ai[s].pos as usize);
                    let g = self.mood_site_goals(a.site, c);
                    self.moods.active.as_mut().unwrap().step = Step::Carry(carried);
                    self.request_path(s, g, now);
                } else {
                    self.release_reagent(id);
                    if let Some(&i) = self.item_idx.get(&id) {
                        self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                    }
                    self.moods.active.as_mut().unwrap().step = Step::Wait;
                    self.mood_next(s, now);
                }
            }
            Step::Carry(id) => {
                if !self.at_site(s, a.site) {
                    let c = self.nav.comp_near(self.ai[s].pos as usize);
                    let g = self.mood_site_goals(a.site, c);
                    if g.n == 0 {
                        self.mood_cancel(s);
                        self.ai[s].wake = now + 200;
                        return;
                    }
                    self.request_path(s, g, now);
                    return;
                }
                let tile = if self.nav.walkable(a.site as usize) { a.site } else { self.ai[s].pos };
                self.mood_place(s, id, tile);
                let m = self.moods.active.as_mut().unwrap();
                m.collected.push(id);
                if !m.demands.is_empty() {
                    m.demands.remove(0);
                }
                m.wait_since = None;
                m.step = Step::Wait;
                self.mood_next(s, now);
            }
            Step::Wait => self.mood_next(s, now),
            Step::Work(until) => {
                if now >= until {
                    self.finish_mood(s, now);
                } else {
                    self.ai[s].wake = until.min(now + 1_000);
                }
            }
        }
    }

    fn mood_claim(&mut self, s: usize, now: u64) {
        let a = self.moods.active.clone().unwrap();
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, usize::MAX);
        for (si, sh) in self.prod.shops.iter().enumerate() {
            let Some(b) = self.world.buildings.get(sh.bi) else { continue };
            if b.id != sh.bid || b.kind != a.shop_kind || b.subtype != a.shop_sub || !b.built {
                continue;
            }
            let g = self.mood_site_goals(sh.site, c);
            if c == 0 || g.n == 0 {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(sh.site as usize));
            if d < best.0 {
                best = (d, si);
            }
        }
        if best.1 == usize::MAX {
            // No such workshop: wait (the insanity clock runs).
            self.mood_wait(s, now);
            return;
        }
        let si = best.1;
        let (bid, site, busy) = (self.prod.shops[si].bid, self.prod.shops[si].site, self.prod.shops[si].busy);
        if busy != NONE && busy as usize != s {
            // Throw the worker out.
            let w = busy as usize;
            self.cancel_task(w);
            self.ai[w].wake = now + 1;
        }
        self.prod.shops[si].busy = s as u32;
        let name = self.world.units[self.ai[s].uidx].name.clone();
        let label = a.shop_label;
        let p = self.world.pos_of(site as usize);
        self.announce(now, format!("{name} has claimed a {label}."), Some(p), AnnouncementKind::Job);
        {
            let m = self.moods.active.as_mut().unwrap();
            m.shop_bid = bid;
            m.site = site;
            m.wait_since = None;
            m.step = Step::Wait;
        }
        if a.kind == MoodKind::Fell {
            self.moods.active.as_mut().unwrap().step = Step::Hunt(NONE);
            self.mood_hunt(s, NONE, now);
        } else {
            self.mood_next(s, now);
        }
    }

    fn mood_wait(&mut self, s: usize, now: u64) {
        let since = *self.moods.active.as_mut().unwrap().wait_since.get_or_insert(now);
        if now.saturating_sub(since) >= MOOD_WAIT_LIMIT {
            self.mood_insanity(s, now);
            return;
        }
        self.moods.active.as_mut().unwrap().step = if self.moods.active.as_ref().unwrap().shop_bid < 0 { Step::Claim } else { Step::Wait };
        let site = self.moods.active.as_ref().unwrap().site;
        if site != NONE && !self.at_site(s, site) {
            let c = self.nav.comp_near(self.ai[s].pos as usize);
            let g = self.mood_site_goals(site, c);
            if g.n > 0 {
                self.request_path(s, g, now);
                return;
            }
        }
        let left = MOOD_WAIT_LIMIT - now.saturating_sub(since);
        self.ai[s].wake = now + 200.min(left.max(1));
    }

    /// Next demanded item, or start working when everything is there.
    fn mood_next(&mut self, s: usize, now: u64) {
        let a = self.moods.active.clone().unwrap();
        if a.shop_bid < 0 || self.world.building(a.shop_bid).is_none() {
            // The workshop is gone: claim another.
            let m = self.moods.active.as_mut().unwrap();
            m.shop_bid = -1;
            m.site = NONE;
            m.step = Step::Claim;
            self.mood_claim(s, now);
            return;
        }
        let Some(&want) = a.demands.first() else {
            // Everything is there.
            let until = now + MOOD_WORK_TICKS;
            self.moods.active.as_mut().unwrap().step = Step::Work(until);
            if !self.at_site(s, a.site) {
                let c = self.nav.comp_near(self.ai[s].pos as usize);
                let g = self.mood_site_goals(a.site, c);
                if g.n > 0 {
                    // Walk back first; work starts on arrival (Work is re-checked there).
                    self.moods.active.as_mut().unwrap().step = Step::Wait;
                    self.request_path(s, g, now);
                    return;
                }
            }
            let name = self.world.units[self.ai[s].uidx].name.clone();
            let p = self.world.pos_of(a.site as usize);
            self.announce(now, format!("{name} has begun a mysterious construction!"), Some(p), AnnouncementKind::Job);
            self.ai[s].wake = until.min(now + 1_000);
            return;
        };
        if want == Mat::Body {
            self.moods.active.as_mut().unwrap().step = Step::Hunt(NONE);
            self.mood_hunt(s, NONE, now);
            return;
        }
        match self.mood_find_item(s, want, a.site, now) {
            Some(id) => {
                if let Some(&i) = self.item_idx.get(&id) {
                    let st = self.world.items[i as usize].stack.max(1);
                    self.imeta[i as usize].reserved += 1.min(st);
                }
                self.moods.active.as_mut().unwrap().step = Step::Fetch(id);
                let g = self.item_goals(id);
                self.request_path(s, g, now);
            }
            None => self.mood_wait(s, now),
        }
    }

    fn mood_matches(&self, idx: usize, want: Mat) -> bool {
        let it = &self.world.items[idx];
        let m = self.world.material(it.mat);
        let tok = m.map(|m| m.token.as_str()).unwrap_or("");
        match want {
            Mat::Boulder => it.kind == "BOULDER" && m.map(|m| m.is_stone && !m.is_ore).unwrap_or(false),
            Mat::Log => it.kind == "WOOD",
            Mat::Bar => it.kind == "BAR" && m.map(|m| m.is_metal).unwrap_or(false),
            Mat::CutGem => it.kind == "SMALLGEM",
            Mat::Block => it.kind == "BLOCKS" && m.map(|m| m.is_stone).unwrap_or(false),
            Mat::RoughGem => it.kind == "ROUGH" && !m.map(|m| m.is_glass).unwrap_or(false),
            Mat::RawGlass => it.kind == "ROUGH" && m.map(|m| m.is_glass).unwrap_or(false),
            Mat::Bone => {
                matches!(it.kind.as_str(), "CORPSEPIECE" | "REMAINS")
                    && (tok.ends_with(":BONE") || tok.contains(":SKULL") || it.desc.contains("bone") || it.desc.contains("skull") || it.kind == "REMAINS")
            }
            Mat::Shell => it.kind == "CORPSEPIECE" && (tok.ends_with(":SHELL") || it.desc.contains("shell")),
            Mat::Leather => it.kind == "SKIN_TANNED",
            Mat::Cloth => it.kind == "CLOTH",
            Mat::Body => false,
        }
    }

    fn mood_find_item(&mut self, s: usize, want: Mat, site: u32, now: u64) -> Option<i32> {
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return None;
        }
        let sp = self.nav.xyz(site as usize);
        let mut best = (i32::MAX, -1);
        for idx in 0..self.world.items.len() {
            if !self.mood_matches(idx, want) || !self.item_free(idx, now) || self.world.items[idx].flags.artifact {
                continue;
            }
            let it = &self.world.items[idx];
            let Some(t) = self.world.idx(it.pos) else { continue };
            let d = crate::manhattan(sp, self.nav.xyz(t));
            if (d, it.id) >= best {
                continue;
            }
            if self.nav.comp_near(t) != c {
                continue;
            }
            best = (d, it.id);
        }
        (best.1 >= 0).then_some(best.1)
    }

    /// Pick up one unit of an item (splitting stacks).
    pub(crate) fn mood_take(&mut self, s: usize, id: i32) -> i32 {
        let idx = self.item_idx[&id] as usize;
        self.imeta[idx].reserved = (self.imeta[idx].reserved - 1).max(0);
        let uid = self.ai[s].id;
        let here = self.world.pos_of(self.ai[s].pos as usize);
        let carried = if self.world.items[idx].stack > 1 {
            self.world.items[idx].stack -= 1;
            let mut it = self.world.items[idx].clone();
            it.id = self.next_item_id;
            self.next_item_id += 1;
            it.stack = 1;
            it.container = -1;
            it.holder = uid;
            it.flags.on_ground = false;
            it.flags.in_container = false;
            it.flags.in_inventory = true;
            it.pos = here;
            let nid = it.id;
            self.push_item(it);
            nid
        } else {
            let it = &mut self.world.items[idx];
            let (cont, pos, loose) = (it.container, it.pos, it.container < 0 && it.flags.on_ground);
            it.container = -1;
            it.flags.in_container = false;
            it.flags.on_ground = false;
            it.flags.in_inventory = true;
            it.holder = uid;
            if cont >= 0 {
                if let Some(v) = self.contents.get_mut(&cont) {
                    v.retain(|&x| x != id);
                }
            } else if loose {
                if let Some(t) = self.world.idx(pos) {
                    self.pile_occ_add(t as u32, -1);
                }
            }
            self.haul_remove(id);
            id
        };
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(carried);
        self.ai[s].carrying = carried;
        carried
    }

    /// Put a carried item down in the workshop, kept for the mood.
    fn mood_place(&mut self, s: usize, id: i32, tile: u32) {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        self.ai[s].carrying = -1;
        let p = self.world.pos_of(tile as usize);
        self.set_item_pos(id, p);
        if let Some(&i) = self.item_idx.get(&id) {
            let it = &mut self.world.items[i as usize];
            it.flags.on_ground = true;
            it.flags.in_inventory = false;
            it.flags.in_job = true;
            it.holder = -1;
            self.imeta[i as usize].hauling = false;
            self.imeta[i as usize].reserved = it.stack.max(1);
            self.haul_remove(id);
            self.pile_occ_add(tile, 1);
        }
    }

    /// Fell mood: find, reach and murder the nearest dwarf, then drag the corpse in.
    fn mood_hunt(&mut self, s: usize, v: u32, now: u64) {
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let mut v = v;
        if v == NONE || v as usize >= self.ai.len() || self.ai[v as usize].kind != Kind::Worker {
            let up = self.nav.xyz(self.ai[s].pos as usize);
            let mut best = (i32::MAX, NONE);
            for (t, ai) in self.ai.iter().enumerate() {
                if t == s || ai.kind != Kind::Worker || c == 0 || self.nav.comp_near(ai.pos as usize) != c {
                    continue;
                }
                if self.world.units[ai.uidx].race != "DWARF" {
                    continue;
                }
                let d = crate::manhattan(up, self.nav.xyz(ai.pos as usize));
                if (d, t as u32) < best {
                    best = (d, t as u32);
                }
            }
            v = best.1;
            if v == NONE {
                // Isolated from everybody: the mood fails in time.
                self.moods.active.as_mut().unwrap().step = Step::Hunt(NONE);
                let since = *self.moods.active.as_mut().unwrap().wait_since.get_or_insert(now);
                if now.saturating_sub(since) >= MOOD_WAIT_LIMIT {
                    self.mood_insanity(s, now);
                } else {
                    self.ai[s].wake = now + 200;
                }
                return;
            }
            self.moods.active.as_mut().unwrap().step = Step::Hunt(v);
        }
        let vt = v as usize;
        let (ps, pv) = (self.world.pos_of(self.ai[s].pos as usize), self.world.pos_of(self.ai[vt].pos as usize));
        if ps.dist(pv) <= 1 {
            // Murder on the spot (wiki: no fight).
            let name = self.world.units[self.ai[s].uidx].name.clone();
            let vname = self.world.units[self.ai[vt].uidx].name.clone();
            let n_items = self.world.items.len();
            self.combat_sync_units();
            self.combat_kill(vt, now, &format!("has been murdered by {name}"));
            let corpse = self.world.items[n_items..].iter().rev().find(|i| i.kind == "CORPSE").map(|i| i.id);
            {
                let m = self.moods.active.as_mut().unwrap();
                let r = m.record;
                self.moods.records[r].victim = Some(vname);
            }
            match corpse {
                Some(cid) => {
                    let carried = self.mood_take(s, cid);
                    let site = self.moods.active.as_ref().unwrap().site;
                    let cc = self.nav.comp_near(self.ai[s].pos as usize);
                    let g = self.mood_site_goals(site, cc);
                    self.moods.active.as_mut().unwrap().step = Step::Carry(carried);
                    self.request_path(s, g, now);
                }
                None => {
                    self.moods.active.as_mut().unwrap().step = Step::Hunt(NONE);
                    self.ai[s].wake = now + 10;
                }
            }
            return;
        }
        let ai = &self.ai[s];
        let end_ok = ai.path_i < ai.path.len() && self.world.pos_of(*ai.path.last().unwrap() as usize).dist(pv) <= 1;
        if end_ok && ps.dist(pv) > 6 {
            self.step(s, now);
        } else {
            let g = Goals::one(self.ai[vt].pos);
            self.request_path(s, g, now);
        }
    }

    /// The artifact is done.
    fn finish_mood(&mut self, s: usize, now: u64) {
        let a = self.moods.active.clone().unwrap();
        let mut r = Rng::for_unit(self.seed, s, now, 0x3011);
        let base_id = a.collected.first().copied().unwrap_or(-1);
        let base_mat = base_id_mat(self, base_id);
        let fell_mat = if a.kind == MoodKind::Fell {
            // Dwarf bone or dwarf leather.
            let tok = if a.skill == "BONE_CARVE" { "CREATURE:DWARF:BONE" } else { "CREATURE:DWARF:SKIN" };
            Some(self.ensure_material(tok))
        } else {
            None
        };
        let mat = fell_mat.unwrap_or(base_mat);
        let mut mat_name = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_else(|| "strange".into());
        if a.kind == MoodKind::Fell {
            mat_name = if a.skill == "BONE_CARVE" { "dwarf bone".into() } else { "dwarf leather".into() };
        }
        let mval = self.world.material(mat).map(|m| m.value).unwrap_or(1).max(1);
        let mut deco_value = 0;
        for &id in a.collected.iter().skip(1) {
            if let Some(&i) = self.item_idx.get(&id) {
                deco_value += self.world.items[i as usize].value.max(1);
            }
        }
        for &id in &a.collected {
            self.destroy_item(id);
        }
        let (kind, subtype, noun) = a.art;
        let art_name = artifact_name(&mut r);
        let tile = if self.nav.walkable(a.site as usize) { a.site as usize } else { self.ai[s].pos as usize };
        let desc = format!("{art_name}, a {mat_name} {noun}");
        let id = self.create_item(kind, mat, tile, desc.clone());
        if let Some(&i) = self.item_idx.get(&id) {
            let it = &mut self.world.items[i as usize];
            it.subtype = subtype.into();
            it.quality = 6;
            it.flags.artifact = true;
            it.name = art_name.clone();
            it.value = 10 * mval * 120 + 2 * deco_value;
        }
        let name = self.world.units[self.ai[s].uidx].name.clone();
        let p = self.world.pos_of(tile);
        self.announce(now, format!("{name} has created {art_name}, a {mat_name} {noun}!"), Some(p), AnnouncementKind::Info);
        if a.kind != MoodKind::Possessed {
            self.add_xp(s, a.skill, 20_000);
        }
        self.feel(s, "MadeArtifact", crate::social::Class::PosStrong, 800, -1, 0, now);
        self.moods.artifacts_made += 1;
        let rec = &mut self.moods.records[a.record];
        rec.artifact_id = id;
        self.end_mood(now, format!("created {desc}"));
        self.ai[s].task = Task::Idle;
        self.ai[s].wake = now + 1;
    }

    fn mood_insanity(&mut self, s: usize, now: u64) {
        let a = self.moods.active.clone().unwrap();
        let mut r = Rng::for_unit(self.seed, s, now, 0x3012);
        let m = match a.kind {
            MoodKind::Fey | MoodKind::Secretive => match r.below(6) {
                0 | 1 => "Melancholy",
                2 | 3 => "Raving",
                4 => "Catatonic",
                _ => "Berserk",
            },
            MoodKind::Possessed => {
                if r.chance(1, 2) {
                    "Raving"
                } else {
                    "Berserk"
                }
            }
            _ => {
                if r.chance(1, 2) {
                    "Berserk"
                } else {
                    "Melancholy"
                }
            }
        };
        let text = match m {
            "Melancholy" => "has been overtaken by melancholy.",
            "Raving" => "has gone stark raving mad!",
            "Catatonic" => "has become catatonic.",
            _ => "has gone berserk!",
        };
        let uidx = self.ai[s].uidx;
        let name = self.world.units[uidx].name.clone();
        let pos = self.world.units[uidx].pos;
        self.announce(now, format!("{name} {text}"), Some(pos), AnnouncementKind::Warning);
        self.moods.insane += 1;
        self.end_mood(now, format!("insane: {m}"));
        self.world.units[uidx].mood = m.into();
        self.ai[s].task = Task::Idle;
        self.ai[s].wake = now + 1;
        if m == "Berserk" {
            // Attacks everyone in sight: the combat AI runs it as a hostile.
            let uid = self.ai[s].id;
            self.ai[s].kind = Kind::Other;
            self.make_hostile(uid);
        }
    }

    /// Clear the active mood: free the workshop and the items still held.
    fn end_mood(&mut self, now: u64, outcome: String) {
        let Some(a) = self.moods.active.take() else { return };
        if let Some(sh) = self.prod.shops.iter_mut().find(|sh| sh.bid == a.shop_bid) {
            if sh.busy == a.slot as u32 {
                sh.busy = NONE;
            }
        }
        for &id in &a.collected {
            if self.item_idx.contains_key(&id) {
                self.release_reagent(id);
            }
        }
        match a.step {
            Step::Fetch(id) => self.release_reagent(id),
            Step::Carry(id) => {
                if a.slot < self.ai.len() && self.ai[a.slot].carrying == id {
                    let here = self.ai[a.slot].pos;
                    self.drop_item(a.slot, id, here);
                }
                self.release_reagent(id);
            }
            _ => {}
        }
        if a.slot < self.ai.len() {
            let uidx = self.ai[a.slot].uidx;
            self.world.units[uidx].mood.clear();
        }
        let rec = &mut self.moods.records[a.record];
        rec.ended = Some(now);
        rec.outcome = outcome;
        let _ = a.uid;
        let _ = a.started;
        self.moods.counter = COUNTER_RESET;
    }

    /// [moods] Caption for the unit sheet.
    pub(crate) fn mood_caption(&self, s: usize) -> (&'static str, Option<Pos>) {
        let Some(a) = self.moods.active.as_ref().filter(|a| a.slot == s) else { return ("No Job", None) };
        let site = (a.site != NONE).then(|| self.world.pos_of(a.site as usize));
        match a.step {
            Step::Claim => ("Strange Mood: looking for a workshop", None),
            Step::Hunt(_) => ("Strange Mood: stalking", None),
            Step::Fetch(id) | Step::Carry(id) => ("Strange Mood: gathering materials", self.item_idx.get(&id).map(|&i| self.world.items[i as usize].pos)),
            Step::Wait => ("Strange Mood: waiting for materials", site),
            Step::Work(_) => (a.kind.work_caption(), site),
        }
    }

    pub(crate) fn moods_hash(&self, mix: &mut dyn FnMut(u64)) {
        mix(self.moods.counter as u64);
        mix(self.moods.records.len() as u64);
        if let Some(a) = &self.moods.active {
            mix(a.slot as u64 ^ ((a.collected.len() as u64) << 32));
        }
    }

    // ------------------------------------------------------------------ public API

    /// All strange moods so far (active one last, with `ended == None`).
    pub fn mood_records(&self) -> &[MoodRecord] {
        &self.moods.records
    }

    /// Artifacts created in this simulation and dwarves driven insane by failed moods.
    pub fn mood_counts(&self) -> (u32, u32) {
        (self.moods.artifacts_made, self.moods.insane)
    }

    /// Switch strange moods on or off (default on).
    pub fn set_moods_enabled(&mut self, on: bool) {
        self.moods.off = !on;
    }

    /// Debug/UI and tests: strike a unit with a strange mood now (`kind`: `Fey`, `Secretive`,
    /// `Possessed`, `Macabre`, `Fell`, or "" for DF's random choice). False if a mood is active
    /// or the unit is no citizen.
    pub fn force_mood(&mut self, unit_id: i32, kind: &str) -> bool {
        self.init_moods();
        if self.moods.active.is_some() {
            return false;
        }
        let Some(&s) = self.unit_slot.get(&unit_id) else { return false };
        if self.ai[s].kind != Kind::Worker {
            return false;
        }
        let k = match kind {
            "Fey" => Some(MoodKind::Fey),
            "Secretive" => Some(MoodKind::Secretive),
            "Possessed" => Some(MoodKind::Possessed),
            "Macabre" => Some(MoodKind::Macabre),
            "Fell" => Some(MoodKind::Fell),
            _ => None,
        };
        self.combat_sync_units();
        let now = self.ticks;
        self.start_mood(s, k, now);
        true
    }

    pub(crate) fn mood_info_impl(&self, unit_id: i32) -> crate::MoodInfo {
        let mood = self.world.unit(unit_id).map(|u| u.mood.clone()).unwrap_or_default();
        match self.moods.active.as_ref().filter(|a| a.uid == unit_id) {
            Some(a) => {
                let mut needs: Vec<String> = Vec::new();
                for d in &a.demands {
                    let l = d.label().to_string();
                    if !needs.contains(&l) {
                        needs.push(l);
                    }
                }
                let ticks_left = match a.step {
                    Step::Wait | Step::Claim | Step::Hunt(_) => a.wait_since.map(|w| MOOD_WAIT_LIMIT.saturating_sub(self.ticks.saturating_sub(w))),
                    _ => None,
                };
                crate::MoodInfo { mood: a.kind.name().into(), workshop: a.shop_bid, needs, ticks_left }
            }
            None => crate::MoodInfo { mood, workshop: -1, ..Default::default() },
        }
    }

    /// The full demand list of the active mood (base material first), for tests and the UI.
    pub fn mood_demands(&self) -> Vec<String> {
        self.moods.active.as_ref().map(|a| a.all_demands.iter().map(|d| d.label().to_string()).collect()).unwrap_or_default()
    }
}

fn base_id_mat(sim: &Sim, id: i32) -> u16 {
    sim.item_idx.get(&id).map(|&i| sim.world.items[i as usize].mat).unwrap_or(rf_core::NO_MAT)
}

fn artifact_name(r: &mut Rng) -> String {
    const SYL: &[&str] = &["ngi", "lok", "ub", "ur", "os", "din", "eg", "en", "nist", "lun", "ub", "as", "kul", "al", "lil", "um", "ast", "ol", "zan", "rith", "dak", "ruth", "esh"];
    const ADJ: &[&str] = &["Ruined", "Cloudy", "Strange", "Revered", "Dim", "Steamy", "Wild", "Regal", "Cracked", "Golden", "Silent", "Bloody", "Ancient"];
    const NOUN: &[&str] = &["Songs", "Scream", "Mirth", "Gift", "Echo", "Peak", "Sack", "Church", "Riddles", "Insanities", "Ships", "Gluttons", "Cradles"];
    let mut word = String::new();
    for _ in 0..2 + r.below(2) {
        word.push_str(SYL[r.below(SYL.len() as u32) as usize]);
    }
    let mut c = word.chars();
    let word = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => word,
    };
    format!("{word}, The {} {}", ADJ[r.below(ADJ.len() as u32) as usize], NOUN[r.below(NOUN.len() as u32) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Building, Item, ItemFlags, World};

    /// Synthetic fort with a craftsdwarf's workshop in the hall and some materials.
    fn mood_world() -> (World, Pos) {
        let mut w = World::synthetic(48, 48, 20);
        let hall = w.units[0].pos;
        let c = Pos::new(hall.x - 1, hall.y + 3, hall.z);
        w.buildings.push(Building {
            id: 900,
            kind: "Workshop".into(),
            subtype: "Craftsdwarfs".into(),
            x1: c.x - 1,
            y1: c.y - 1,
            x2: c.x + 1,
            y2: c.y + 1,
            z: c.z,
            center: c,
            built: true,
            owner: -1,
            ..Default::default()
        });
        (w, c)
    }

    fn add_item(w: &mut World, kind: &str, mat: u16, p: Pos, n: usize) {
        for _ in 0..n {
            let id = w.items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
            w.items.push(Item { id, kind: kind.into(), mat, pos: p, stack: 1, container: -1, holder: -1, building: -1, flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() });
        }
    }

    fn run_until(sim: &mut Sim, max: u64, f: impl Fn(&Sim) -> bool) -> bool {
        for _ in 0..max / 100 {
            sim.run_ticks(100);
            if f(sim) {
                return true;
            }
        }
        false
    }

    #[test]
    fn fey_mood_collects_and_creates_artifact() {
        let (mut w, c) = mood_world();
        // Plenty of everything a craftsdwarf mood can demand.
        let p = Pos::new(c.x + 6, c.y - 3, c.z);
        let granite = 0u16;
        add_item(&mut w, "BOULDER", granite, p, 6);
        add_item(&mut w, "WOOD", 3, p, 4);
        add_item(&mut w, "BLOCKS", granite, p, 4);
        let mut sim = Sim::new(w, 7);
        sim.set_invasions(false);
        // Give the dwarf a stonecrafting skill so the mood picks stone crafts.
        let uid = sim.world.units[3].id;
        sim.world.units[3].skills.push(rf_core::Skill { id: "STONECRAFT".into(), rating: 3, xp: 0 });
        assert!(sim.force_mood(uid, "Fey"));
        let ann = sim.drain_announcements();
        assert!(ann.iter().any(|a| a.text.ends_with("is taken by a fey mood!")));
        assert_eq!(sim.mood_info(uid).mood, "Fey");
        // Restrict demands to what we have: rock base (stone crafter) + decorations we can satisfy.
        if let Some(a) = sim.moods.active.as_mut() {
            a.demands.retain(|d| matches!(d, Mat::Boulder | Mat::Log | Mat::Block));
            a.all_demands = a.demands.clone();
            assert_eq!(a.skill, "STONECRAFT");
        }
        let done = run_until(&mut sim, 60_000, |s| s.moods.active.is_none());
        assert!(done, "mood did not finish: {:?}", sim.mood_info(uid));
        let rec = &sim.mood_records()[0];
        assert!(rec.outcome.starts_with("created "), "{}", rec.outcome);
        let art = sim.world.item(rec.artifact_id).expect("artifact");
        assert!(art.flags.artifact && art.quality == 6 && art.value > 1000);
        assert!(sim.world.unit(uid).unwrap().skill("STONECRAFT") >= 15, "legendary");
        assert!(sim.world.unit(uid).unwrap().mood.is_empty());
        let ann = sim.drain_announcements();
        assert!(ann.iter().any(|a| a.text.contains("has claimed a Craftsdwarf's Workshop")), "{:?}", ann.iter().map(|a| &a.text).collect::<Vec<_>>());
        assert!(ann.iter().any(|a| a.text.contains("has begun a mysterious construction!")));
        assert!(ann.iter().any(|a| a.text.contains(" has created ") && a.text.contains(", a granite ")), "{:?}", ann.iter().map(|a| &a.text).collect::<Vec<_>>());
        // Needs did not grow during the mood: nobody died of thirst.
        assert!(!sim.world.unit(uid).unwrap().flags.dead);
    }

    #[test]
    fn missing_materials_drive_insane_after_50000_ticks() {
        let (w, _) = mood_world();
        let mut sim = Sim::new(w, 9);
        sim.set_invasions(false);
        let uid = sim.world.units[1].id;
        assert!(sim.force_mood(uid, "Secretive"));
        if let Some(a) = sim.moods.active.as_mut() {
            a.demands = vec![Mat::RoughGem];
        }
        sim.run_ticks(40_000);
        assert!(sim.moods.active.is_some(), "still waiting");
        let left = sim.mood_info(uid).ticks_left.expect("clock");
        assert!(left > 5_000 && left < 15_000, "{left}");
        assert_eq!(sim.mood_info(uid).needs, vec!["rough gems".to_string()]);
        sim.run_ticks(12_000);
        assert!(sim.moods.active.is_none());
        let rec = &sim.mood_records()[0];
        assert!(rec.outcome.starts_with("insane: "), "{}", rec.outcome);
        let waited = rec.ended.unwrap() - rec.started;
        assert!((50_000..50_500).contains(&waited), "{waited}");
    }

    #[test]
    fn fell_mood_murders_and_makes_bone_artifact() {
        let (w, _) = mood_world();
        let mut sim = Sim::new(w, 11);
        sim.set_invasions(false);
        let uid = sim.world.units[2].id;
        let n0 = sim.world.citizens().count();
        assert!(sim.force_mood(uid, "Fell"));
        if let Some(a) = sim.moods.active.as_mut() {
            a.skill = "BONE_CARVE";
            a.shop_kind = "Workshop";
            a.shop_sub = "Craftsdwarfs";
            a.shop_label = "Craftsdwarf's Workshop";
        }
        let done = run_until(&mut sim, 30_000, |s| s.moods.active.is_none());
        assert!(done, "{:?}", sim.mood_info(uid));
        assert_eq!(sim.world.citizens().count(), n0 - 1);
        let rec = &sim.mood_records()[0];
        assert!(rec.victim.is_some());
        assert!(rec.outcome.contains("dwarf bone"), "{}", rec.outcome);
    }

    #[test]
    fn moods_start_by_df_counter_with_enough_dwarves() {
        // 7 dwarves: below DF's 20 -> never.
        let (w, _) = mood_world();
        let mut sim = Sim::new(w, 13);
        sim.set_invasions(false);
        sim.moods.counter = 0;
        let mut r = Rng::for_unit(1, 1, 1, 1);
        assert!(!sim.try_start_mood(100, &mut r));
        // The lottery prefers craftsdwarves (21 tickets) over peasants (6).
        assert_eq!(tickets("CRAFTSMAN"), 21);
        assert_eq!(tickets("CARPENTER"), 11);
        assert_eq!(tickets("MINER"), 6);
    }
}
