//! Social simulation: DF v50 psychology (thoughts, emotions, needs, stress, mental
//! breakdowns), dining rooms and bedrooms.
//!
//! # DF rules implemented
//! - **Stress buckets** (DFHack `Units::stress_cutoffs`, what DF's stress face shows):
//!   >= 50 000 breaking down, >= 25 000 very stressed, >= 10 000 stressed, >= -10 000 fine,
//!   >= -25 000 content, >= -50 000 happy, below that ecstatic. Short-term stress lives in
//!   `Unit::stress` (-100 000..=100 000, wiki "Stress"), long-term stress in
//!   `Unit::longterm_stress` (-50 000..=120 000; +25 000 stressed, +50 000 haggard,
//!   +100 000 harrowed). Long-term stress follows short-term stress by at most +20 160 and
//!   -43 564 per year (wiki), i.e. +60 / -130 per day.
//! - **Emotions** (wiki "Emotion"): every thought becomes an emotion picked from the unit's
//!   personality; the stress change is `strength / divider` with DF's divider table. As on the
//!   wiki, an emotion with divider +-2 cannot push a unit into the outermost bucket and one
//!   with +-4/+-8 not into the second-outermost: the change is capped at `100 000 / |divider|`.
//!   Thought names are DF's `unit_thought_type` names, captions DF's own (see `captions`).
//! - **Breakdowns** (wiki "Stress", "Tantrum"): only when both short-term and long-term
//!   stress are high. The personality picks the kind: ANGER_PROPENSITY -> tantrum ("is
//!   throwing a tantrum!": throws items, topples furniture, starts fist fights),
//!   DEPRESSION_PROPENSITY -> depression, ANXIETY_PROPENSITY -> obliviousness; all three stop
//!   working for several days. Tantrum-prone dwarves snap about once a month (wiki). Repeated
//!   breakdowns at harrowed long-term stress can end in insanity (berserk / melancholy / stark
//!   raving mad; `Unit::mood`).
//! - **Needs** (wiki "Need", DFHack `personality_needst`): focus falls by `need_level` per 100
//!   ticks (fits the wiki's "fulfil every 3 months at level 10, 6 months at level 5"), a
//!   satisfied need goes to 400. Focus states: >= 300 unfettered, >= 200 level-headed, >= 100
//!   untroubled, >= -999 not distracted, >= -9 999 unfocused, >= -99 999 distracted, else badly
//!   distracted. The overall focus (100 % = undistracted) scales effective skill by up to
//!   +-50 % (wiki) and so the work speed of designation and workshop jobs.
//! - **Satisfying needs**: drinking alcohol (DrinkAlcohol), eating a prepared meal worth 20+ or
//!   of quality (EatGoodMeal), praying in a temple (PrayOrMeditate), talking to others in a
//!   meeting hall / tavern / dining room (Socialize, BeWithFriends, BeWithFamily, MakeMerry),
//!   resting (TakeItEasy), work (StayOccupied, PracticeSkill, CraftObject, BeCreative), skill
//!   level-ups (LearnSomething), seeing tame animals (SeeAnimal), admiring fine furniture
//!   (AdmireArt). Idle dwarves with a pressing need walk to a matching meeting zone.
//! - **Dining** (wiki "Dining room"): a hungry dwarf takes the food to a free chair next to a
//!   table in a dining hall (or tavern) zone if one is reachable and eats there ->
//!   `DiningQuality` by room value; without one it eats where the food is -> `LackTables` /
//!   `NoDining`.
//! - **Bedrooms** (wiki "Bedroom", "Room"): beds inside an owned bedroom zone belong to the
//!   zone's owner. Sleeping in the own bedroom -> `BedroomQuality` by room value; in a bed
//!   outside any room -> `LackBedroom`; without a bed -> `SleptFloor` / `SleptRoughFloor` /
//!   `SleptGrass` / `SleptDirt` / `SleptRocks` / `SleptIce` by the ground.
//! - **Room value tiers** (wiki "Room", "Bedroom"): 0 meager, 100 modest, 250 (plain)
//!   quarters / dining room, 500 decent, 1 000 fine, 1 500 great, 2 500 grand, 10 000 royal.
//! - **Room value** (wiki "Room", "Engraving"): the furniture inside the zone plus the value of
//!   its floor tiles and of the walls bordering it. A tile is worth origin multiplier x material
//!   value plus its engraving (10 x material value x quality multiplier 1/2/3/4/5/12); floors:
//!   rough 1, smooth 4, constructed 7; walls: rough 1, smooth 5, constructed 9. Smoothing,
//!   engraving and constructions re-value the rooms (at most once per social pass).
//! - **Other thoughts**: good meals/drinks by quality, `SameFood`/`SameBooze`, `AteRotten`,
//!   `LackWell` (water from a natural source), `Thirsty`/`Hungry`/`Drowsy` and worse,
//!   `SatisfiedAtWork`, `ImproveSkill`, `Talked`, `Prayer`, `AdmireBuilding` /
//!   `AdmireOwnBuilding`, `SawDeadBody`, `WitnessDeath`, grief at the death of relations
//!   (`Death`, `LostPet`), `MajorInjuries`/`MinorInjuries` from wounds, `Jailed` when caged,
//!   `NeedsUnfulfilled`.
//!
//! # Estimates (not on the wiki)
//! - Thought strengths (50..600), the stress scale (`STRESS_K`: change = strength x 50 /
//!   |divider|), short-term decay towards 0 (0.5 %/day, faster the longer the unit went without
//!   distress / eustress, slower with high ANXIETY_PROPENSITY), the BRAVERY multiplier on stress
//!   gain (x0.5..x1.5) and the breakdown threshold (short-term >= 50 000 shifted by
//!   STRESS_VULNERABILITY +-25 000, long-term >= 25 000), breakdown durations (2-7 days),
//!   the insanity rule.
//! - Emotion choice: each thought has a valence class; the emotion within the class is picked
//!   from the dominant propensity (anger / depression / anxiety / cheer), like DF's
//!   personality-dependent reactions but with our own tables.
//! - Needs of units without exported needs (pre-round-3 exports) are generated from facets.
//!   Long-term stress missing in the export is estimated as 60 % of short-term stress.
//! - Temples, taverns and libraries are meeting-hall zones recognized by their name (DF
//!   locations are not exported yet): "Temple/Tempel/Kapelle/Chapel/Shrine", "Tavern",
//!   "Library/Bibliothek".
//! - Food is reserved where it lies and eaten at the table (the item is not carried).
//! - Each citizen thinks once per day (staggered over 12 slots of 100 ticks); events (meals,
//!   drinks, sleep, work, deaths) are handled when they happen.

mod captions;
mod saves10; // [saves10] save/load of breakdowns, timers and counters

use crate::path::Goals;
use crate::rng::Rng;
use crate::{needs, AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{Emotion, Pos, Shape, TtMaterial, TtSpecial, UnitNeed, NO_MAT};
use std::collections::BTreeMap;

/// Social pass period in ticks; each citizen is handled every `WHEEL` passes (= one day).
pub const SOCIAL_PERIOD: u64 = 100;
const WHEEL: u64 = 12;
const DAY: i32 = rf_core::TICKS_PER_DAY;
/// Stress points per strength point at divider 1 (estimate).
const STRESS_K: i32 = 50;
const MAX_EMOTIONS: usize = 64;
const ST_MIN: i32 = -100_000;
const ST_MAX: i32 = 100_000;
const LT_MIN: i32 = -50_000;
const LT_MAX: i32 = 120_000;
const FOCUS_MAX: i32 = 400;
const FOCUS_MIN: i32 = -200_000; // lowest focus level seen in the export
/// [skills9] Daily chance (1 in this) to relive the strongest bad memory, and its share.
const RECALL_IN: u32 = 6;
const RECALL_DIV: i32 = 2;

/// DF `need_type` names in enum order (subthought of `NeedsUnfulfilled`).
pub const NEED_NAMES: [&str; 30] = [
    "Socialize", "DrinkAlcohol", "PrayOrMeditate", "StayOccupied", "BeCreative", "Excitement", "LearnSomething",
    "BeWithFamily", "BeWithFriends", "HearEloquence", "UpholdTradition", "SelfExamination", "MakeMerry", "CraftObject",
    "MartialTraining", "PracticeSkill", "TakeItEasy", "MakeRomance", "SeeAnimal", "SeeGreatBeast", "AcquireObject",
    "EatGoodMeal", "Fight", "CauseTrouble", "Argue", "BeExtravagant", "Wander", "HelpSomebody", "ThinkAbstractly",
    "AdmireArt",
];
/// What a dwarf with this unmet need is "distracted after" (estimate of DF's wording).
const NEED_UNMET: [&str; 30] = [
    "being away from people", "being unable to drink", "being unable to pray", "being unoccupied",
    "being unable to do something creative", "a lack of excitement", "being unable to learn something",
    "being away from family", "being away from friends", "being unable to hear eloquent speech",
    "being unable to uphold tradition", "being unable to examine themselves", "being unable to make merry",
    "being unable to craft an object", "being unable to practice a martial art", "being unable to practice a skill",
    "being unable to take it easy", "being unable to make romance", "being unable to see an animal",
    "being unable to see a great beast", "being unable to acquire something", "being unable to eat a good meal",
    "being unable to fight", "being unable to cause trouble", "being unable to argue", "being unable to be extravagant",
    "being unable to wander", "being unable to help somebody", "being unable to think abstractly",
    "being unable to admire art",
];

fn need_index(id: &str) -> Option<usize> {
    NEED_NAMES.iter().position(|n| *n == id)
}

/// Emotion divider (wiki "Emotion"): stress change = strength / divider; 0 = no stress effect.
pub fn divider(emotion: &str) -> i32 {
    match emotion {
        "ACCEPTANCE" | "AROUSAL" | "CONTENTMENT" | "ENJOYMENT" | "ENTHUSIASM" | "EXPECTANCY" | "FONDNESS" | "INTEREST"
        | "LUST" | "NOSTALGIA" | "PATIENCE" | "SATISFACTION" | "SYMPATHY" | "WONDER" | "ADMIRATION" => -8,
        "AMUSEMENT" | "EAGERNESS" | "FREEDOM" | "GRATITUDE" | "JOLLINESS" | "OPTIMISM" | "PLEASURE" | "PRIDE" | "ZEAL" => -4,
        "AFFECTION" | "CARING" | "ELATION" | "EMPATHY" | "EXCITEMENT" | "EXHILARATION" | "GAIETY" | "GLEE" | "HAPPINESS"
        | "HOPE" | "JOVIALITY" | "PASSION" | "RELIEF" | "REPENTANCE" | "TENDERNESS" | "THRILL" | "TRIUMPH" => -2,
        "ADORATION" | "BLISS" | "DELIGHT" | "EUPHORIA" | "JOY" | "JUBILATION" | "LOVE" | "RAPTURE" => -1,
        "AGONY" | "ANGST" | "ANGUISH" | "DESPAIR" | "FEAR" | "HORROR" | "MISERY" | "PANIC" | "RAGE" | "SHAKEN" | "SHOCK"
        | "TERROR" | "WRATH" => 1,
        "ANGER" | "BITTERNESS" | "DEFEAT" | "DISMAY" | "DISTRESS" | "FEROCITY" | "FRIGHT" | "GRIEF" | "HATRED"
        | "HOPELESSNESS" | "LOATHING" | "MORTIFICATION" | "OUTRAGE" => 2,
        "AGITATION" | "AGGRAVATION" | "ALARM" | "ANXIETY" | "AVERSION" | "CONTEMPT" | "DEJECTION" | "DISGUST"
        | "EMPTINESS" | "GLOOM" | "GUILT" | "HUMILIATION" | "INSULT" | "ISOLATION" | "LONELINESS" | "REJECTION"
        | "REMORSE" | "SADNESS" | "SHAME" | "UNHAPPINESS" | "VENGEFULNESS" => 4,
        "ALIENATION" | "ANNOYANCE" | "BOREDOM" | "CONFUSION" | "DISAPPOINTMENT" | "DISILLUSIONMENT" | "DISLIKE"
        | "DISPLEASURE" | "DOUBT" | "EMBARRASSMENT" | "EXASPERATION" | "FRUSTRATION" | "GLUMNESS" | "GROUCHINESS"
        | "GRUMPINESS" | "IRRITATION" | "NERVOUSNESS" | "PESSIMISM" | "REGRET" | "RESENTMENT" | "RIGHTEOUS_INDIGNATION"
        | "SELF_PITY" | "SUSPICION" | "UNEASINESS" | "WORRY" | "RESTLESS" => 8,
        _ => 0,
    }
}

/// The `&'static str` for an emotion name we know (so history can be reused cheaply).
fn known_emotion(e: &str) -> Option<&'static str> {
    const ALL: &[&str] = &[
        "ANYTHING", "ACCEPTANCE", "ADORATION", "AFFECTION", "AGITATION", "AGGRAVATION", "AGONY", "ALARM", "ALIENATION",
        "AMAZEMENT", "AMBIVALENCE", "AMUSEMENT", "ANGER", "ANGST", "ANGUISH", "ANNOYANCE", "ANXIETY", "APATHY", "AROUSAL",
        "ASTONISHMENT", "AVERSION", "AWE", "BITTERNESS", "BLISS", "BOREDOM", "CARING", "CONFUSION", "CONTEMPT",
        "CONTENTMENT", "DEFEAT", "DEJECTION", "DELIGHT", "DESPAIR", "DISAPPOINTMENT", "DISGUST", "DISILLUSIONMENT",
        "DISLIKE", "DISMAY", "DISPLEASURE", "DISTRESS", "DOUBT", "EAGERNESS", "ELATION", "EMBARRASSMENT", "EMPATHY",
        "EMPTINESS", "ENJOYMENT", "ENTHUSIASM", "EUPHORIA", "EXASPERATION", "EXCITEMENT", "EXHILARATION", "EXPECTANCY",
        "FEAR", "FEROCITY", "FONDNESS", "FREEDOM", "FRIGHT", "FRUSTRATION", "GAIETY", "GLEE", "GLOOM", "GLUMNESS",
        "GRATITUDE", "GRIEF", "GRIM_SATISFACTION", "GROUCHINESS", "GRUMPINESS", "GUILT", "HAPPINESS", "HATRED", "HOPE",
        "HOPELESSNESS", "HORROR", "HUMILIATION", "INSULT", "INTEREST", "IRRITATION", "ISOLATION", "JOLLINESS",
        "JOVIALITY", "JOY", "JUBILATION", "LOATHING", "LONELINESS", "LOVE", "LUST", "MISERY", "MORTIFICATION",
        "NERVOUSNESS", "NOSTALGIA", "OPTIMISM", "OUTRAGE", "PANIC", "PATIENCE", "PASSION", "PESSIMISM", "PLEASURE",
        "PRIDE", "RAGE", "RAPTURE", "REJECTION", "RELIEF", "REGRET", "REMORSE", "REPENTANCE", "RESENTMENT",
        "RIGHTEOUS_INDIGNATION", "SADNESS", "SATISFACTION", "SELF_PITY", "SERVILE", "SHAKEN", "SHAME", "SHOCK",
        "SUSPICION", "SYMPATHY", "TENDERNESS", "TERROR", "THRILL", "TRIUMPH", "UNEASINESS", "UNHAPPINESS",
        "VENGEFULNESS", "WONDER", "WORRY", "WRATH", "ZEAL", "RESTLESS", "ADMIRATION",
    ];
    ALL.iter().find(|x| **x == e).copied()
}

/// Adjective DF-style sentences use ("felt satisfied ..."); falls back to the noun.
fn emotion_word(emotion: &str) -> String {
    let w = match emotion {
        "SATISFACTION" => "satisfied",
        "CONTENTMENT" => "content",
        "ENJOYMENT" => "enjoyment",
        "PLEASURE" => "pleasure",
        "AMUSEMENT" => "amused",
        "GRATITUDE" => "grateful",
        "JOLLINESS" => "jolly",
        "HAPPINESS" => "happy",
        "ELATION" => "elated",
        "GLEE" => "gleeful",
        "JOY" => "joy",
        "DELIGHT" => "delighted",
        "BLISS" => "bliss",
        "RAPTURE" => "rapture",
        "EUPHORIA" => "euphoric",
        "FONDNESS" => "fondness",
        "RELIEF" => "relieved",
        "ANNOYANCE" => "annoyed",
        "IRRITATION" => "irritated",
        "GROUCHINESS" => "grouchy",
        "DISPLEASURE" => "displeased",
        "DISAPPOINTMENT" => "disappointed",
        "GLUMNESS" => "glum",
        "UNEASINESS" => "uneasy",
        "WORRY" => "worried",
        "AGGRAVATION" => "aggravated",
        "AGITATION" => "agitated",
        "SADNESS" => "sad",
        "DEJECTION" => "dejected",
        "GLOOM" => "gloomy",
        "ANXIETY" => "anxious",
        "ALARM" => "alarmed",
        "UNHAPPINESS" => "unhappy",
        "ANGER" => "angry",
        "RAGE" => "enraged",
        "MISERY" => "miserable",
        "DESPAIR" => "despair",
        "FRIGHT" => "frightened",
        "TERROR" => "terror",
        "DISTRESS" => "distressed",
        "DISMAY" => "dismayed",
        "GRIEF" => "grief",
        "ANGUISH" => "anguish",
        "HORROR" => "horrified",
        "SHOCK" => "shocked",
        "FEAR" => "afraid",
        _ => "",
    };
    if w.is_empty() {
        emotion.to_lowercase().replace('_', " ")
    } else {
        w.to_string()
    }
}

/// Valence class of a thought: decides which emotions the personality can pick.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub(crate) enum Class {
    PosMild,
    Pos,
    PosStrong,
    /// Prayer after a long wait.
    Bliss,
    NegMild,
    Neg,
    NegStrong,
    Grief,
    Horror,
}

/// Personality facets that drive emotions and stress (0..=100, DF 50 = average).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct Facets {
    anger: i32,
    depression: i32,
    anxiety: i32,
    cheer: i32,
    bravery: i32,
    vulnerability: i32,
    gregarious: i32,
    immoderation: i32,
    activity: i32,
    friendliness: i32,
    art: i32,
    curious: i32,
}

impl Default for Facets {
    fn default() -> Self {
        Facets {
            anger: 50,
            depression: 50,
            anxiety: 50,
            cheer: 50,
            bravery: 50,
            vulnerability: 50,
            gregarious: 50,
            immoderation: 50,
            activity: 50,
            friendliness: 50,
            art: 50,
            curious: 50,
        }
    }
}

impl Facets {
    fn of(list: &[(String, i32)]) -> Facets {
        let mut f = Facets::default();
        for (k, v) in list {
            let v = (*v).clamp(0, 100);
            match k.as_str() {
                "ANGER_PROPENSITY" => f.anger = v,
                "DEPRESSION_PROPENSITY" => f.depression = v,
                "ANXIETY_PROPENSITY" => f.anxiety = v,
                "CHEER_PROPENSITY" => f.cheer = v,
                "BRAVERY" => f.bravery = v,
                "STRESS_VULNERABILITY" => f.vulnerability = v,
                "GREGARIOUSNESS" => f.gregarious = v,
                "IMMODERATION" => f.immoderation = v,
                "ACTIVITY_LEVEL" => f.activity = v,
                "FRIENDLINESS" => f.friendliness = v,
                "ART_INCLINED" => f.art = v,
                "CURIOUS" => f.curious = v,
                _ => {}
            }
        }
        f
    }
}

fn pick(r: &mut Rng, l: &[&'static str]) -> &'static str {
    l[r.below(l.len() as u32) as usize]
}

/// The emotion a unit feels about a thought of class `c` (None = it does not care).
fn pick_emotion(c: Class, f: &Facets, r: &mut Rng) -> Option<&'static str> {
    // Dominant negative propensity, if clearly above average.
    let neg = {
        let m = f.anger.max(f.depression).max(f.anxiety);
        if m < 55 {
            0
        } else if m == f.anger {
            1
        } else if m == f.depression {
            2
        } else {
            3
        }
    };
    let cheerful = f.cheer >= 60;
    Some(match c {
        Class::PosMild => {
            if cheerful {
                pick(r, &["PLEASURE", "AMUSEMENT", "SATISFACTION", "ENJOYMENT"])
            } else {
                pick(r, &["SATISFACTION", "CONTENTMENT", "ENJOYMENT", "SATISFACTION"])
            }
        }
        Class::Pos => {
            if cheerful {
                pick(r, &["JOLLINESS", "PLEASURE", "HAPPINESS"])
            } else {
                pick(r, &["PLEASURE", "GRATITUDE", "SATISFACTION", "PLEASURE"])
            }
        }
        Class::PosStrong => {
            if cheerful {
                pick(r, &["DELIGHT", "JOY", "ELATION"])
            } else {
                pick(r, &["HAPPINESS", "ELATION", "PLEASURE"])
            }
        }
        Class::Bliss => pick(r, &["BLISS", "RAPTURE", "EUPHORIA", "JOY"]),
        Class::NegMild => match neg {
            1 => pick(r, &["ANNOYANCE", "IRRITATION", "GROUCHINESS"]),
            2 => pick(r, &["DISAPPOINTMENT", "GLUMNESS", "ANNOYANCE"]),
            3 => pick(r, &["UNEASINESS", "WORRY", "ANNOYANCE"]),
            _ => pick(r, &["ANNOYANCE", "DISPLEASURE", "ANNOYANCE"]),
        },
        Class::Neg => match neg {
            1 => pick(r, &["AGGRAVATION", "AGITATION", "IRRITATION"]),
            2 => pick(r, &["SADNESS", "DEJECTION", "GLOOM"]),
            3 => pick(r, &["ANXIETY", "ALARM", "WORRY"]),
            _ => pick(r, &["UNHAPPINESS", "ANNOYANCE", "DISPLEASURE"]),
        },
        Class::NegStrong => match neg {
            1 => pick(r, &["ANGER", "RAGE", "ANGER"]),
            2 => pick(r, &["MISERY", "DESPAIR", "SADNESS"]),
            3 => pick(r, &["FRIGHT", "TERROR", "ANXIETY"]),
            _ => pick(r, &["DISTRESS", "DISMAY", "UNHAPPINESS"]),
        },
        Class::Grief => {
            if f.depression >= 75 {
                pick(r, &["ANGUISH", "MISERY", "GRIEF"])
            } else if f.anger >= 75 {
                pick(r, &["GRIEF", "ANGER", "GRIEF"])
            } else {
                pick(r, &["GRIEF", "GRIEF", "SADNESS"])
            }
        }
        Class::Horror => {
            if f.bravery >= 75 && f.anxiety <= 40 && r.chance(1, 2) {
                return None;
            }
            match neg {
                3 => pick(r, &["HORROR", "TERROR", "SHOCK"]),
                2 => pick(r, &["DISMAY", "HORROR", "SADNESS"]),
                1 => pick(r, &["SHOCK", "ANGER", "DISMAY"]),
                _ => pick(r, &["SHOCK", "HORROR", "DISMAY", "DISTRESS"]),
            }
        }
    })
}

/// Mental breakdown state (DF `soldier_mood` Tantrum / Depressed / Oblivious).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)] // [saves11]
pub enum Breakdown {
    #[default]
    None,
    Tantrum,
    Depressed,
    Oblivious,
}

impl Breakdown {
    pub fn label(self) -> &'static str {
        match self {
            Breakdown::None => "",
            Breakdown::Tantrum => "Tantrum",
            Breakdown::Depressed => "Depressed",
            Breakdown::Oblivious => "Oblivious",
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub(crate) enum RoomKind {
    Bedroom,
    Dormitory,
    Dining,
    Meeting,
    Tavern,
    Temple,
    Library,
    Other,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct Room {
    kind: RoomKind,
    owner: i32,
    value: i32,
    /// Furniture part of `value` (the rest is floor, walls and engravings).
    furn_value: i32,
    min_tier: i32,
    x1: i16,
    y1: i16,
    x2: i16,
    y2: i16,
    z: i16,
    extents: Vec<u8>,
}

impl Room {
    fn contains(&self, p: Pos) -> bool {
        if p.z != self.z || p.x < self.x1 || p.x > self.x2 || p.y < self.y1 || p.y > self.y2 {
            return false;
        }
        if self.extents.is_empty() {
            return true;
        }
        let w = (self.x2 - self.x1 + 1) as usize;
        let i = (p.y - self.y1) as usize * w + (p.x - self.x1) as usize;
        self.extents.get(i).copied().unwrap_or(0) != 0
    }
    /// Room value tier (wiki "Room"): 0 meager .. 7 royal; never below the level DF itself
    /// reported for this room (`min_tier`, from exported thoughts: DF's room value also counts
    /// engravings, which the export does not have).
    fn tier(&self) -> i32 {
        const T: [i32; 7] = [100, 250, 500, 1000, 1500, 2500, 10_000];
        (T.iter().filter(|&&t| self.value >= t).count() as i32).max(self.min_tier)
    }
}

const TIER_WORD: [&str; 8] = ["meager", "modest", "ordinary", "decent", "fine", "great", "grand", "royal"];
const QUALITY_WORD: [&str; 7] = ["", "well-crafted", "finely-crafted", "superior", "exceptional", "masterful", "artifact"];
const FOOD_WORD: [&str; 7] = ["", "well-prepared", "finely-prepared", "superior", "exceptional", "masterful", "legendary"];

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
struct Seat {
    tile: u32,
    room: u32,
    occupant: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
struct Furn {
    pos: Pos,
    bid: i32,
    quality: u8,
    room: u32,
    #[serde(with = "crate::saves11::interned")] // [saves11]
    kind: crate::saves11::SStr,
}

/// Per-unit social state that is not part of `rf_core::Unit` (rebuilt on load).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct Psy {
    facets: Facets,
    breakdown: Breakdown,
    until: u64,
    breakdowns: u16,
    seat: u32,
    meals: [u16; 2],
    drinks: [u16; 2],
    seen: Vec<i32>,
    next_admire: u64,
    next_need_thought: u64,
    next_injury: u64,
    without_distress: i32,
    without_eustress: i32,
    /// Overall focus in percent (100 = undistracted).
    focus: i32,
    ready: bool,
}

impl Default for Psy {
    fn default() -> Self {
        Psy {
            facets: Facets::default(),
            breakdown: Breakdown::None,
            until: 0,
            breakdowns: 0,
            seat: NONE,
            meals: [NO_MAT; 2],
            drinks: [NO_MAT; 2],
            seen: Vec::new(),
            next_admire: 0,
            next_need_thought: 0,
            next_injury: 0,
            without_distress: 0,
            without_eustress: 0,
            focus: 100,
            ready: false,
        }
    }
}

/// Stress distribution of the living citizens at the start of a month.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct MonthStress {
    pub year: i32,
    /// 0-based month.
    pub month: i32,
    /// Citizens per stress level, in `StressLevel` order (Ecstatic .. Breaking down).
    pub counts: [u32; 7],
    pub mean_stress: i32,
    pub mean_longterm: i32,
    pub mean_focus: i32,
}

/// Social statistics for reports and the benchmark.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct SocialReport {
    pub months: Vec<MonthStress>,
    /// Thought counts (DF `unit_thought_type` name), most frequent first.
    pub thoughts: Vec<(String, u64)>,
    pub tantrums: u64,
    pub depressions: u64,
    pub oblivious: u64,
    pub insane: u64,
    pub meals_at_table: u64,
    pub meals_without_table: u64,
    pub prayers: u64,
    pub conversations: u64,
    /// Dining seats (chair next to a table in a dining hall or tavern) and owned bedrooms.
    pub seats: usize,
    pub bedrooms: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Social {
    psy: Vec<Psy>,
    rooms: Vec<Room>,
    seats: Vec<Seat>,
    furn: Vec<Furn>,
    wells: Vec<Pos>,
    corpses: Vec<(i32, Pos)>,
    zone_kind: Vec<RoomKind>,
    zone_key: (usize, usize),
    known_buildings: usize,
    /// `Sim::detail_epoch` the room values were computed at.
    known_detail: u64,
    last_month: i32,
    months: Vec<MonthStress>,
    #[serde(with = "crate::saves11::interned")] // [saves11]
    counts: BTreeMap<&'static str, u64>,
    tantrums: u64,
    depressions: u64,
    oblivious: u64,
    insane: u64,
    meals_at_table: u64,
    meals_without_table: u64,
    prayers: u64,
    conversations: u64,
    has_dining: bool,
    /// Social simulation switched off (A/B measurements; `Sim::set_social_enabled`).
    off: bool,
}

fn salt(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

fn room_kind(subtype: &str, name: &str) -> RoomKind {
    let n = name.to_lowercase();
    match subtype {
        "Bedroom" => RoomKind::Bedroom,
        "Dormitory" | "Barracks" => RoomKind::Dormitory,
        "DiningHall" => RoomKind::Dining,
        s if s.contains("Meeting") => {
            if ["temple", "tempel", "kapelle", "chapel", "shrine", "schrein"].iter().any(|k| n.contains(k)) {
                RoomKind::Temple
            } else if n.contains("tavern") {
                RoomKind::Tavern
            } else if ["library", "bibliothek"].iter().any(|k| n.contains(k)) {
                RoomKind::Library
            } else {
                RoomKind::Meeting
            }
        }
        _ => RoomKind::Other,
    }
}

fn furniture_kind(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Bed" => "bed",
        "Table" => "table",
        "Chair" => "chair",
        "Door" => "door",
        "Cabinet" => "cabinet",
        "Box" => "chest",
        "Coffin" => "coffin",
        "Statue" => "statue",
        "Armorstand" => "armor stand",
        "Weaponrack" => "weapon rack",
        "Slab" => "slab",
        "Bookcase" => "bookcase",
        "Well" => "well",
        _ => return None,
    })
}

/// Contribution of one need to the overall focus, in tenths of a "neutral" need (40).
/// Fitted to DF's `current_focus` of the 88 citizens of the round-3 export (rms error 1.1
/// points; DF itself counts 4 per undistracted need).
fn focus_points(focus_level: i32) -> i32 {
    match focus_level {
        300.. => 60,
        200..=299 => 53,
        100..=199 => 46,
        -999..=99 => 40,
        -9_999..=-1_000 => 33,
        -99_999..=-10_000 => 27,
        _ => 19,
    }
}

/// DF's word for a need's focus state.
pub fn focus_state(focus_level: i32) -> &'static str {
    match focus_level {
        300.. => "Unfettered",
        200..=299 => "Level-headed",
        100..=199 => "Untroubled",
        -999..=99 => "Not distracted",
        -9_999..=-1_000 => "Unfocused",
        -99_999..=-10_000 => "Distracted",
        _ => "Badly distracted",
    }
}

impl Sim {
    // ------------------------------------------------------------------ init

    /// Build rooms, dining seats, admirable furniture; give beds in owned bedrooms to the
    /// owner; fill per-unit state. Called from `Sim::new` and whenever buildings change.
    pub(crate) fn init_social(&mut self) {
        self.social_rebuild_rooms();
        // Long-term stress: estimate if the export has none.
        let cit: Vec<usize> = (0..self.world.units.len()).filter(|&i| self.world.units[i].flags.citizen).collect();
        if !cit.is_empty() && cit.iter().all(|&i| self.world.units[i].longterm_stress == 0) {
            for &i in &cit {
                let u = &mut self.world.units[i];
                u.longterm_stress = (u.stress * 6 / 10).clamp(LT_MIN, LT_MAX);
            }
        }
        self.social_ensure();
    }

    fn social_rebuild_rooms(&mut self) {
        let sc = &mut self.social;
        sc.rooms.clear();
        sc.seats.clear();
        sc.furn.clear();
        sc.wells.clear();
        sc.known_buildings = self.world.buildings.len();
        sc.zone_key = (usize::MAX, 0);
        for b in &self.world.buildings {
            if b.kind == "Civzone" {
                let kind = room_kind(&b.subtype, &b.name);
                if kind != RoomKind::Other {
                    sc.rooms.push(Room {
                        kind,
                        owner: b.owner,
                        value: 0,
                        furn_value: 0,
                        min_tier: 0,
                        x1: b.x1,
                        y1: b.y1,
                        x2: b.x2,
                        y2: b.y2,
                        z: b.z,
                        extents: b.extents.clone(),
                    });
                }
            }
        }
        // Furniture: value into rooms, quality for admiration.
        for b in &self.world.buildings {
            let Some(kind) = furniture_kind(&b.kind) else { continue };
            if !b.built {
                continue;
            }
            if b.kind == "Well" {
                sc.wells.push(b.center);
            }
            let (mut q, mut v) = (0u8, 0i32);
            for id in &b.items {
                if let Some(&i) = self.item_idx.get(id) {
                    let it = &self.world.items[i as usize];
                    q = q.max(it.quality);
                    v += it.value.max(0);
                }
            }
            if b.items.is_empty() {
                v = 10;
            }
            let mut room = NONE;
            for (k, r) in sc.rooms.iter_mut().enumerate() {
                if r.contains(b.center) {
                    r.furn_value += v;
                    if room == NONE || r.kind == RoomKind::Bedroom || r.kind == RoomKind::Dining {
                        room = k as u32;
                    }
                }
            }
            sc.furn.push(Furn { pos: b.center, bid: b.id, quality: q, room, kind });
        }
        self.social_revalue_rooms();
        let sc = &mut self.social;
        // Room levels DF reported (thought severity = tier - 2, i.e. 0 = ordinary): an owner's
        // last BedroomQuality for its bedroom, the most common DiningQuality for dining rooms.
        let mut dining: BTreeMap<i32, u32> = BTreeMap::new();
        for u in &self.world.units {
            if let Some(e) = u.emotions.iter().rev().find(|e| e.thought == "DiningQuality") {
                *dining.entry(e.severity).or_default() += 1;
            }
            if let Some(e) = u.emotions.iter().rev().find(|e| e.thought == "BedroomQuality") {
                for r in sc.rooms.iter_mut().filter(|r| r.kind == RoomKind::Bedroom && r.owner == u.id) {
                    r.min_tier = (e.severity + 2).clamp(0, 7);
                }
            }
        }
        if let Some((&sev, _)) = dining.iter().max_by_key(|(sev, n)| (**n, -**sev)) {
            for r in sc.rooms.iter_mut().filter(|r| matches!(r.kind, RoomKind::Dining | RoomKind::Tavern)) {
                r.min_tier = (sev + 2).clamp(0, 7);
            }
        }
        // Dining seats: a chair next to a table, in a dining hall or tavern.
        for f in &sc.furn {
            if f.kind != "chair" || f.room == NONE {
                continue;
            }
            let rk = sc.rooms[f.room as usize].kind;
            if !matches!(rk, RoomKind::Dining | RoomKind::Tavern) {
                continue;
            }
            let table = sc.furn.iter().any(|t| t.kind == "table" && t.pos.z == f.pos.z && t.pos.dist(f.pos) == 1);
            if let (true, Some(t)) = (table, self.world.idx(f.pos)) {
                sc.seats.push(Seat { tile: t as u32, room: f.room, occupant: NONE });
            }
        }
        sc.has_dining = !sc.seats.is_empty();
        // Beds inside an owned bedroom belong to its owner (DF v50 room assignment).
        for bed in &mut self.beds {
            if bed.owner >= 0 || bed.tile >= self.world.tiles.len() as u32 {
                continue;
            }
            let p = self.world.pos_of(bed.tile as usize);
            if let Some(r) = sc.rooms.iter().find(|r| r.kind == RoomKind::Bedroom && r.owner >= 0 && r.contains(p)) {
                bed.owner = r.owner;
            }
        }
        for s in &mut sc.psy {
            s.seat = NONE;
        }
    }

    /// Room value = furniture + floor tiles + bordering walls + engravings (wiki "Room",
    /// "Engraving"). Called after a rebuild and when smoothing/engraving/constructions changed
    /// tiles.
    pub(crate) fn social_revalue_rooms(&mut self) {
        self.social.known_detail = self.detail_epoch();
        let mut rooms = std::mem::take(&mut self.social.rooms);
        let mut walls: Vec<u32> = Vec::new();
        for r in &mut rooms {
            let mut v = r.furn_value;
            walls.clear();
            for y in r.y1..=r.y2 {
                for x in r.x1..=r.x2 {
                    let p = Pos::new(x, y, r.z);
                    if !r.contains(p) {
                        continue;
                    }
                    let Some(i) = self.world.idx(p) else { continue };
                    v += self.room_tile_value(i);
                    for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                        let q = Pos::new(x + dx, y + dy, r.z);
                        if r.contains(q) {
                            continue;
                        }
                        if let Some(j) = self.world.idx(q) {
                            if matches!(self.world.shape(q), Shape::Wall | Shape::Fortification) {
                                walls.push(j as u32);
                            }
                        }
                    }
                }
            }
            walls.sort_unstable();
            walls.dedup();
            for &j in &walls {
                v += self.room_tile_value(j as usize);
            }
            r.value = v;
        }
        self.social.rooms = rooms;
    }

    /// Value of one floor or wall tile for room value (wiki "Engraving": origin multiplier x
    /// material value + engraving bonus).
    fn room_tile_value(&self, i: usize) -> i32 {
        let t = self.world.tiles[i];
        let info = self.world.tiletypes.get(t.tt);
        let built = info.material == TtMaterial::Construction;
        let smooth = info.special == TtSpecial::Smooth || info.special == TtSpecial::Track;
        let mult = match info.shape {
            Shape::Floor => {
                if built {
                    7
                } else if smooth {
                    4
                } else {
                    1
                }
            }
            Shape::Wall | Shape::Fortification => {
                if built {
                    9
                } else if smooth {
                    5
                } else {
                    1
                }
            }
            _ => return 0,
        };
        let mv = self.world.material(t.mat).map(|m| m.value).unwrap_or(0).max(0);
        let engr = self.engraving_at(self.world.pos_of(i)).map(|e| self.engraving_value(e)).unwrap_or(0);
        mult * mv + engr
    }

    /// Value of a civzone as the social simulation sees it (furniture, tiles, engravings), or
    /// `None` if the zone is not a room kind (bedroom, dining hall, meeting area, ...).
    pub fn room_value(&self, zone_id: i32) -> Option<i32> {
        let b = self.world.buildings.iter().find(|b| b.id == zone_id && b.kind == "Civzone")?;
        self.social.rooms.iter().find(|r| r.x1 == b.x1 && r.y1 == b.y1 && r.x2 == b.x2 && r.y2 == b.y2 && r.z == b.z).map(|r| r.value)
    }

    /// Make sure every unit slot has social state (new units, first call).
    fn social_ensure(&mut self) {
        if self.social.psy.len() < self.ai.len() {
            self.social.psy.resize(self.ai.len(), Psy::default());
        }
        for s in 0..self.ai.len() {
            if self.social.psy[s].ready {
                continue;
            }
            let u = &mut self.world.units[self.ai[s].uidx];
            let f = Facets::of(&u.facets);
            if u.flags.citizen && u.needs.is_empty() && !u.flags.animal {
                u.needs = default_needs(&f, u.id);
            }
            let p = &mut self.social.psy[s];
            p.facets = f;
            p.ready = true;
            p.next_admire = (s as u64 % 7) * DAY as u64;
            p.focus = focus_of(&u.needs);
        }
    }

    // ------------------------------------------------------------------ thoughts

    /// A unit experiences a thought: pick an emotion from its personality, remember it and
    /// change its stress.
    pub(crate) fn feel(&mut self, s: usize, thought: &'static str, class: Class, strength: i32, sub: i32, severity: i32, now: u64) {
        if self.social.off {
            return;
        }
        self.social_ensure();
        let mut r = Rng::for_unit(self.seed, s, now, salt(thought));
        let f = self.social.psy[s].facets;
        // DF reacts to the same circumstance with the same emotion: reuse the unit's own last
        // emotion about this thought (from the export or earlier), `ANYTHING` = no reaction
        // (e.g. dwarves hardened to corpses). Otherwise the personality picks one.
        let known = self.world.units[self.ai[s].uidx]
            .emotions
            .iter()
            .rev()
            .find(|e| e.thought == thought)
            .and_then(|e| known_emotion(&e.emotion));
        let emotion = match known {
            Some(e) => e,
            None => match pick_emotion(class, &f, &mut r) {
                Some(e) => e,
                None => "ANYTHING",
            },
        };
        let div = divider(emotion);
        *self.social.counts.entry(thought).or_default() += 1;
        let (year, year_tick) = (self.world.calendar.year, self.world.calendar.year_tick);
        let u = &mut self.world.units[self.ai[s].uidx];
        let n = u.emotions.len();
        let recent = u.emotions[n.saturating_sub(12)..]
            .iter_mut()
            .rev()
            .find(|e| e.thought == thought && e.subthought == sub && e.emotion == emotion);
        match recent {
            Some(e) => {
                e.strength = e.strength.max(strength);
                e.severity = severity;
                e.year = year;
                e.year_tick = year_tick;
            }
            None => {
                if n >= MAX_EMOTIONS {
                    u.emotions.drain(..n + 1 - MAX_EMOTIONS);
                }
                u.emotions.push(Emotion {
                    emotion: emotion.to_string(),
                    thought: thought.to_string(),
                    subthought: sub,
                    strength,
                    severity,
                    year,
                    year_tick,
                    flags: Vec::new(),
                });
            }
        }
        if div == 0 {
            return;
        }
        let cap = 100_000 / div.abs();
        let mut delta = strength * STRESS_K / div.abs();
        let st = u.stress;
        let p = &mut self.social.psy[s];
        if div > 0 {
            // BRAVERY: how fast stress builds up (wiki "Stress"), x0.5 .. x1.5.
            delta = delta * (150 - f.bravery) / 100;
            if st < cap {
                u.stress = (st + delta).min(cap);
            }
            p.without_distress = 0;
        } else {
            if st > -cap {
                u.stress = (st - delta).max(-cap);
            }
            p.without_eustress = 0;
        }
        u.stress = u.stress.clamp(ST_MIN, ST_MAX);
    }

    fn satisfy(&mut self, s: usize, need: &str) -> i32 {
        let u = &mut self.world.units[self.ai[s].uidx];
        let mut before = i32::MAX;
        for n in u.needs.iter_mut().filter(|n| n.id == need) {
            before = before.min(n.focus_level);
            n.focus_level = FOCUS_MAX;
        }
        before
    }

    fn psy(&self, s: usize) -> Option<&Psy> {
        self.social.psy.get(s)
    }

    // ------------------------------------------------------------------ event hooks

    /// [hook] A unit finished drinking item `id` (-1 = water).
    pub(crate) fn social_drank(&mut self, s: usize, id: i32, now: u64) {
        if !self.is_citizen_slot(s) {
            return;
        }
        self.social_ensure();
        if id < 0 {
            let here = self.world.pos_of(self.ai[s].pos as usize);
            let at_well = self.social.wells.iter().any(|w| w.z == here.z && w.dist(here) <= 1);
            if !at_well {
                self.feel(s, "LackWell", Class::NegMild, 80, -1, 0, now);
            }
            return;
        }
        let Some(&i) = self.item_idx.get(&id) else { return };
        let (mat, q) = {
            let it = &self.world.items[i as usize];
            (it.mat, it.quality.min(6) as i32)
        };
        self.satisfy(s, "DrinkAlcohol");
        let p = &mut self.social.psy[s];
        let same = p.drinks[0] == mat && p.drinks[1] == mat;
        p.drinks = [p.drinks[1], mat];
        if q >= 1 {
            let class = if q >= 4 { Class::Pos } else { Class::PosMild };
            self.feel(s, "GoodDrink", class, 60 + 20 * q, -1, q, now);
        } else if same {
            self.feel(s, "SameBooze", Class::NegMild, 50, -1, 0, now);
        }
    }

    /// [hook] A unit finished eating item `id` (before it is consumed).
    pub(crate) fn social_ate(&mut self, s: usize, id: i32, now: u64) {
        let seat = self.social.psy.get(s).map(|p| p.seat).unwrap_or(NONE);
        self.social_release_seat(s);
        if !self.is_citizen_slot(s) {
            return;
        }
        self.social_ensure();
        if let Some(&i) = self.item_idx.get(&id) {
            let (mat, q, meal, rotten, value) = {
                let it = &self.world.items[i as usize];
                (it.mat, it.quality.min(6) as i32, it.kind == "FOOD", it.flags.rotten, it.value / it.stack.max(1))
            };
            let p = &mut self.social.psy[s];
            let same = p.meals[0] == mat && p.meals[1] == mat;
            p.meals = [p.meals[1], mat];
            if rotten {
                self.feel(s, "AteRotten", Class::Neg, 120, -1, 0, now);
            } else if meal {
                let class = match q {
                    0..=1 => Class::PosMild,
                    2..=4 => Class::Pos,
                    _ => Class::PosStrong,
                };
                self.feel(s, "GoodMeal", class, 80 + 20 * q, -1, q, now);
                if q >= 1 || value >= 20 {
                    self.satisfy(s, "EatGoodMeal");
                }
            } else if same {
                self.feel(s, "SameFood", Class::NegMild, 50, -1, 0, now);
            }
        }
        if seat != NONE {
            self.social.meals_at_table += 1;
            let room = self.social.seats.get(seat as usize).map(|st| st.room).unwrap_or(NONE);
            if let Some(r) = self.social.rooms.get(room as usize) {
                let tier = r.tier();
                match tier {
                    0..=1 => self.feel(s, "DiningQuality", Class::NegMild, 60, -1, tier - 2, now),
                    2 => {}
                    3..=4 => self.feel(s, "DiningQuality", Class::PosMild, 60 + 20 * tier, -1, tier - 2, now),
                    5..=6 => self.feel(s, "DiningQuality", Class::Pos, 60 + 20 * tier, -1, tier - 2, now),
                    _ => self.feel(s, "DiningQuality", Class::PosStrong, 200, -1, tier - 2, now),
                }
            }
        } else {
            self.social.meals_without_table += 1;
            if self.social.has_dining {
                self.feel(s, "LackTables", Class::NegMild, 100, -1, 0, now);
            } else {
                self.feel(s, "NoDining", Class::NegMild, 100, -1, 0, now);
            }
        }
    }

    /// [hook] At the food: walk to a free dining seat instead of eating on the spot. Returns
    /// true if the unit is now on its way to a table (task `GoDine`).
    pub(crate) fn social_go_dine(&mut self, s: usize, id: i32, now: u64) -> bool {
        if self.social.off || self.social.seats.is_empty() || !self.is_citizen_slot(s) {
            return false;
        }
        self.social_ensure();
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, NONE);
        for (k, st) in self.social.seats.iter().enumerate() {
            if st.occupant != NONE && self.social.psy.get(st.occupant as usize).map(|p| p.seat) == Some(k as u32) {
                continue;
            }
            if !self.nav.walkable(st.tile as usize) || self.nav.comp_root(st.tile as usize) != c {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(st.tile as usize));
            if d < best.0 {
                best = (d, k as u32);
            }
        }
        if best.1 == NONE {
            return false;
        }
        let k = best.1;
        self.social.seats[k as usize].occupant = s as u32;
        self.social.psy[s].seat = k;
        let tile = self.social.seats[k as usize].tile;
        self.ai[s].task = Task::GoDine(id, k);
        self.request_path(s, Goals::one(tile), now);
        true
    }

    /// [hook] Arrived at the dining seat: eat.
    pub(crate) fn social_arrive_dine(&mut self, s: usize, id: i32, now: u64) {
        let ai = &mut self.ai[s];
        ai.task = Task::Eat(id);
        ai.act_until = now + needs::EAT_TICKS;
        ai.wake = now + needs::EAT_TICKS;
    }

    pub(crate) fn social_release_seat(&mut self, s: usize) {
        if let Some(p) = self.social.psy.get_mut(s) {
            if p.seat != NONE {
                if let Some(st) = self.social.seats.get_mut(p.seat as usize) {
                    if st.occupant == s as u32 {
                        st.occupant = NONE;
                    }
                }
                p.seat = NONE;
            }
        }
    }

    /// [hook] A unit wakes up (task is still `Sleep(bed)`).
    pub(crate) fn social_woke(&mut self, s: usize, now: u64) {
        if !self.is_citizen_slot(s) {
            return;
        }
        let Task::Sleep(b) = self.ai[s].task else { return };
        let uid = self.ai[s].id;
        if b >= 0 {
            let tile = self.beds[b as usize].tile;
            let p = self.world.pos_of(tile as usize);
            let room = self.social.rooms.iter().find(|r| matches!(r.kind, RoomKind::Bedroom | RoomKind::Dormitory) && r.contains(p));
            match room {
                Some(r) if r.kind == RoomKind::Bedroom && r.owner == uid => {
                    let tier = r.tier();
                    match tier {
                        0..=1 => self.feel(s, "BedroomQuality", Class::NegMild, 60, -1, tier - 2, now),
                        2 => {}
                        3..=4 => self.feel(s, "BedroomQuality", Class::PosMild, 60 + 20 * tier, -1, tier - 2, now),
                        5..=6 => self.feel(s, "BedroomQuality", Class::Pos, 60 + 20 * tier, -1, tier - 2, now),
                        _ => self.feel(s, "BedroomQuality", Class::PosStrong, 200, -1, tier - 2, now),
                    }
                }
                Some(_) => {}
                None => self.feel(s, "LackBedroom", Class::NegMild, 80, -1, 0, now),
            }
            return;
        }
        let here = self.ai[s].pos as usize;
        let info = self.world.tiletypes.get(self.world.tiles[here].tt);
        let thought = match (info.shape, info.material) {
            (Shape::Boulder | Shape::Pebbles, _) => "SleptRocks",
            (_, m) if m.is_grass() => "SleptGrass",
            (_, TtMaterial::Soil) => "SleptDirt",
            (_, TtMaterial::FrozenLiquid) => "SleptIce",
            (_, TtMaterial::Construction) => "SleptFloor",
            _ if info.special == TtSpecial::Smooth => "SleptFloor",
            (_, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::Feature | TtMaterial::LavaStone)
                if self.world.tiles[here].flags & rf_core::tile_flags::SUBTERRANEAN != 0 =>
            {
                "SleptRoughFloor"
            }
            _ => "SleptFloor",
        };
        self.feel(s, thought, Class::Neg, 120, -1, 0, now);
    }

    /// [hook] Skill use at work (from `add_xp`).
    pub(crate) fn social_work(&mut self, s: usize, skill: &str, leveled: bool) {
        if !self.is_citizen_slot(s) {
            return;
        }
        let now = self.ticks;
        self.social_ensure();
        self.satisfy(s, "StayOccupied");
        self.satisfy(s, "PracticeSkill");
        if matches!(skill, "CARPENTRY" | "MASONRY" | "STONECRAFT" | "WOODCRAFT" | "BONECARVE" | "GEM_SETTING" | "ENGRAVE_STONE") {
            self.satisfy(s, "CraftObject");
            self.satisfy(s, "BeCreative");
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0x5a71);
        let rating = self.world.units[self.ai[s].uidx].skill(skill);
        // "Satisfied at work": more likely the better the dwarf is at it (estimate).
        if r.chance(2 + rating.clamp(0, 15) as u32, 60) {
            self.feel(s, "SatisfiedAtWork", Class::PosMild, 60, -1, 0, now);
        }
        if leveled {
            self.satisfy(s, "LearnSomething");
            self.feel(s, "ImproveSkill", Class::PosMild, 80, -1, rating, now);
        }
    }

    /// [hook] Unit in slot `s` just died: grief for relations, horror for witnesses.
    pub(crate) fn social_death(&mut self, s: usize, now: u64) {
        self.social_ensure();
        self.social_release_seat(s);
        let (dead_id, hf, pos, animal) = {
            let u = &self.world.units[self.ai[s].uidx];
            (u.id, u.hist_fig_id, u.pos, u.flags.animal)
        };
        for t in 0..self.ai.len() {
            if t == s || !self.is_citizen_slot(t) {
                continue;
            }
            let u = &self.world.units[self.ai[t].uidx];
            let rel = u
                .relationships
                .iter()
                .filter(|r| r.unit_id == dead_id || (hf >= 0 && r.hist_fig_id == hf))
                .map(|r| r.kind.clone())
                .next();
            let near = u.pos.z == pos.z && u.pos.dist(pos) <= 8;
            if let Some(kind) = rel {
                let (thought, strength) = match kind.as_str() {
                    "Spouse" | "Lover" => ("Death", 600),
                    "Mother" | "Father" | "Child" | "Sibling" | "Brother" | "Sister" => ("Death", 450),
                    "Friend" | "CloseFriend" => ("Death", 300),
                    "KindredSpirit" => ("Death", 200),
                    "PassingAcquaintance" | "Deity" | "DeceasedSpouse" => {
                        if near && !animal {
                            self.feel(t, "WitnessDeath", Class::Horror, 150, dead_id, 0, now);
                        }
                        continue;
                    }
                    "Pet" | "PetOwner" => ("LostPet", 250),
                    "Grudge" | "Enemy" => continue,
                    _ => ("Death", 150),
                };
                let thought = if animal { "LostPet" } else { thought };
                self.feel(t, thought, Class::Grief, strength, dead_id, 0, now);
            } else if near && !animal {
                self.feel(t, "WitnessDeath", Class::Horror, 150, dead_id, 0, now);
            }
        }
    }

    // ------------------------------------------------------------------ periodic pass

    /// [hook] Called every `SOCIAL_PERIOD` ticks: each citizen thinks once a day.
    pub(crate) fn social_pass(&mut self, now: u64) {
        if self.social.off {
            return;
        }
        if self.world.buildings.len() != self.social.known_buildings {
            self.social_rebuild_rooms();
        } else if self.detail_epoch() != self.social.known_detail {
            self.social_revalue_rooms();
        }
        self.social_ensure();
        let phase = (now / SOCIAL_PERIOD) % WHEEL;
        if phase == 0 {
            self.social_month(now);
            self.social_corpses();
        }
        for s in 0..self.ai.len() {
            if s as u64 % WHEEL == phase && self.is_citizen_slot(s) {
                self.social_day(s, now);
            }
        }
    }

    fn is_citizen_slot(&self, s: usize) -> bool {
        let ai = &self.ai[s];
        let u = &self.world.units[ai.uidx];
        u.flags.citizen && !u.flags.dead && !u.flags.animal && (ai.kind == Kind::Worker || u.flags.caged)
    }

    fn social_corpses(&mut self) {
        self.social.corpses.clear();
        for (i, it) in self.world.items.iter().enumerate() {
            if it.kind != "CORPSE" || !it.flags.on_ground || it.container >= 0 || it.building >= 0 || self.imeta[i].dead {
                continue;
            }
            // Only bodies of people ("Urist's corpse", "dwarf corpse"), not animals.
            let d = it.desc.to_lowercase();
            if d.contains("'s ") || d.contains("dwarf") || d.contains("human") || d.contains("elf") || d.contains("goblin") {
                self.social.corpses.push((it.id, it.pos));
            }
        }
    }

    fn social_month(&mut self, _now: u64) {
        let c = self.world.calendar;
        let key = c.year * 12 + c.month();
        if key == self.social.last_month {
            return;
        }
        self.social.last_month = key;
        let mut m = MonthStress { year: c.year, month: c.month(), ..Default::default() };
        let (mut sum, mut lt, mut fo, mut n) = (0i64, 0i64, 0i64, 0i64);
        for s in 0..self.ai.len() {
            if !self.is_citizen_slot(s) {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            let lvl = crate::StressLevel::from_stress(u.stress) as usize;
            m.counts[lvl] += 1;
            sum += u.stress as i64;
            lt += u.longterm_stress as i64;
            fo += self.social.psy[s].focus as i64;
            n += 1;
        }
        if n > 0 {
            m.mean_stress = (sum / n) as i32;
            m.mean_longterm = (lt / n) as i32;
            m.mean_focus = (fo / n) as i32;
        }
        self.social.months.push(m);
    }

    /// One day of thinking for citizen slot `s`.
    fn social_day(&mut self, s: usize, now: u64) {
        let uidx = self.ai[s].uidx;
        let f = self.social.psy[s].facets;
        // --- needs decay and overall focus
        {
            let u = &mut self.world.units[uidx];
            for n in &mut u.needs {
                let lvl = n.need_level.clamp(0, 10);
                n.focus_level = (n.focus_level - lvl * (DAY / 100)).max(FOCUS_MIN);
            }
            let fc = focus_of(&u.needs);
            self.social.psy[s].focus = fc;
        }
        // --- stress dynamics
        {
            let p = &mut self.social.psy[s];
            p.without_distress = (p.without_distress + DAY).min(806_400);
            p.without_eustress = (p.without_eustress + DAY).min(806_400);
            let (wd, we) = (p.without_distress, p.without_eustress);
            let u = &mut self.world.units[uidx];
            let st = u.stress;
            if st > 0 {
                // ANXIETY_PROPENSITY: how fast stress dissipates (wiki), x1.5 .. x0.5.
                let rate = (500 * (150 - f.anxiety) / 100) as i64 * (806_400 + 2 * wd as i64) / 806_400; // per 100 000
                u.stress = (st - (st as i64 * rate / 100_000) as i32 - 1).max(0);
            } else if st < 0 {
                let rate = 500 * (806_400 + 2 * we as i64) / 806_400;
                u.stress = (st - (st as i64 * rate as i64 / 100_000) as i32 + 1).min(0);
            }
            let lt = u.longterm_stress;
            let step = ((u.stress - lt) / 8).clamp(-130, 60);
            u.longterm_stress = (lt + step).clamp(LT_MIN, LT_MAX);
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0x50c1a1);
        // --- body
        let (th, hu, sl) = self.needs_now(&self.ai[s]);
        if th >= needs::DEHYDRATED {
            self.feel(s, "Dehydrated", Class::NegStrong, 150, -1, 0, now);
        } else if th >= needs::THIRSTY {
            self.feel(s, "Thirsty", Class::NegMild, 80, -1, 0, now);
        }
        if hu >= needs::STARVING {
            self.feel(s, "Starving", Class::NegStrong, 150, -1, 0, now);
        } else if hu >= needs::HUNGRY {
            self.feel(s, "Hungry", Class::NegMild, 80, -1, 0, now);
        }
        if sl >= needs::VERY_DROWSY {
            self.feel(s, "VeryDrowsy", Class::Neg, 120, -1, 0, now);
        } else if sl >= needs::DROWSY && !self.ai[s].sleeping {
            self.feel(s, "Drowsy", Class::NegMild, 60, -1, 0, now);
        }
        let (pain, caged, pos, uid) = {
            let u = &self.world.units[uidx];
            (u.wounds.iter().map(|w| w.pain.max(0)).sum::<i32>(), u.flags.caged, u.pos, u.id)
        };
        if pain > 0 && now >= self.social.psy[s].next_injury {
            let major = pain >= 50;
            self.feel(s, if major { "MajorInjuries" } else { "MinorInjuries" }, if major { Class::Neg } else { Class::NegMild }, 100, -1, 0, now);
            self.social.psy[s].next_injury = now + 7 * DAY as u64;
        }
        if caged {
            self.feel(s, "Jailed", Class::Neg, 100, -1, 0, now);
            return;
        }
        // --- environment: corpses, fine furniture, animals
        let corpse = self.social.corpses.iter().find(|(id, p)| p.z == pos.z && p.dist(pos) <= 4 && !self.social.psy[s].seen.contains(id)).copied();
        if let Some((id, _)) = corpse {
            let seen = &mut self.social.psy[s].seen;
            if seen.len() >= 16 {
                seen.remove(0);
            }
            seen.push(id);
            self.feel(s, "SawDeadBody", Class::Horror, 120, id, 0, now);
        }
        if now >= self.social.psy[s].next_admire {
            let min_q = if f.art >= 70 { 2 } else { 3 };
            let best = self
                .social
                .furn
                .iter()
                .filter(|fu| fu.quality >= min_q && fu.pos.z == pos.z && fu.pos.dist(pos) <= 2)
                .max_by_key(|fu| (fu.quality, fu.bid))
                .copied();
            if let Some(fu) = best {
                if r.chance(1, 2) {
                    let own = fu.room != NONE && self.social.rooms[fu.room as usize].owner == uid;
                    let q = fu.quality.min(6) as i32;
                    let class = match q {
                        0..=4 => Class::PosMild,
                        5 => Class::Pos,
                        _ => Class::PosStrong,
                    };
                    self.feel(s, if own { "AdmireOwnBuilding" } else { "AdmireBuilding" }, class, 50 + 25 * q, fu.bid, q, now);
                    self.satisfy(s, "AdmireArt");
                    self.social.psy[s].next_admire = now + 3 * DAY as u64;
                }
            }
        }
        if self.world.units[uidx].needs.iter().any(|n| n.id == "SeeAnimal" && n.focus_level < 0) {
            let seen_animal = self.world.units.iter().any(|a| a.flags.animal && a.flags.tame && !a.flags.dead && a.pos.z == pos.z && a.pos.dist(pos) <= 6);
            if seen_animal {
                self.satisfy(s, "SeeAnimal");
            }
        }
        // --- idle: socialize, pray, rest
        let idle = matches!(self.ai[s].task, Task::Idle | Task::Wander);
        if idle && self.social.psy[s].breakdown == Breakdown::None {
            self.satisfy(s, "TakeItEasy");
            let room = self.social.rooms.iter().position(|rm| !matches!(rm.kind, RoomKind::Bedroom | RoomKind::Dormitory | RoomKind::Other) && rm.contains(pos));
            if let Some(ri) = room {
                let kind = self.social.rooms[ri].kind;
                let prays = self.world.units[uidx].needs.iter().any(|n| n.id == "PrayOrMeditate");
                if kind == RoomKind::Temple && prays {
                    let before = self.satisfy(s, "PrayOrMeditate");
                    if before < 0 {
                        self.social.prayers += 1;
                        let (class, strength) = match before {
                            i32::MIN..=-10_000 => (Class::Bliss, 200),
                            -9_999..=-1_000 => (Class::PosStrong, 150),
                            _ => (Class::Pos, 100),
                        };
                        let deity = self.world.units[uidx].needs.iter().find(|n| n.id == "PrayOrMeditate").map(|n| n.deity_id).unwrap_or(-1);
                        self.feel(s, "Prayer", class, strength, deity, 0, now);
                        self.locations_prayed(s, pos, deity, now); // [society2] dedicated temples
                    }
                } else {
                    self.socialize(s, ri, kind, now, &mut r);
                }
            }
        }
        // --- unmet needs
        if now >= self.social.psy[s].next_need_thought {
            let worst = self.world.units[uidx]
                .needs
                .iter()
                .filter(|n| n.focus_level < -10_000)
                .min_by_key(|n| (n.focus_level, n.id.clone()))
                .map(|n| (need_index(&n.id).map(|i| i as i32).unwrap_or(-1), n.focus_level));
            if let Some((ni, fl)) = worst {
                let class = if fl < -100_000 { Class::Neg } else { Class::NegMild };
                self.feel(s, "NeedsUnfulfilled", class, 50 + (-fl / 2000).min(100), ni, 0, now);
                self.social.psy[s].next_need_thought = now + 14 * DAY as u64;
            }
        }
        self.social_recall(s, now, &mut r); // [skills9] strong memories come back
        self.social_breakdown(s, now, &mut r);
    }

    /// [skills9] Dwarves relive strong bad memories (wiki v50 "Thought": memories of strong
    /// events are kept and remembered, which brings the emotion back). Each day with chance
    /// 1 in `RECALL_IN` the strongest grief or horror memory of the last two years (death of a
    /// relation or friend, witnessed death, a dead body, a lost pet) is felt again at
    /// 1/`RECALL_DIV` of its strength, half that in the second year (estimate, fitted so that the losses of Gordon's fort
    /// between r4 and r5 leave about as many stressed dwarves as DF had).
    fn social_recall(&mut self, s: usize, now: u64, r: &mut Rng) {
        if !r.chance(1, RECALL_IN) {
            return;
        }
        let f = self.social.psy[s].facets;
        let c = self.world.calendar;
        let t_now = c.year as i64 * rf_core::TICKS_PER_YEAR as i64 + c.year_tick as i64;
        let u = &mut self.world.units[self.ai[s].uidx];
        let best = u
            .emotions
            .iter()
            .filter(|e| matches!(e.thought.as_str(), "Death" | "WitnessDeath" | "SawDeadBody" | "LostPet"))
            .filter_map(|e| {
                let age = t_now - (e.year as i64 * rf_core::TICKS_PER_YEAR as i64 + e.year_tick as i64);
                // Memories of the last two years; the second year at half strength.
                let k = if age <= rf_core::TICKS_PER_YEAR as i64 { 1 } else if age <= 2 * rf_core::TICKS_PER_YEAR as i64 { 2 } else { return None };
                Some((e.strength / k, divider(&e.emotion)))
            })
            .filter(|&(_, d)| d > 0)
            .max();
        let Some((strength, div)) = best else { return };
        let cap = 100_000 / div;
        let delta = strength * STRESS_K / div / RECALL_DIV * (150 - f.bravery) / 100;
        if u.stress < cap && delta > 0 {
            u.stress = (u.stress + delta).min(cap).clamp(ST_MIN, ST_MAX);
            self.social.psy[s].without_distress = 0;
            self.skills.recalls += 1;
        }
        let _ = now;
    }

    fn socialize(&mut self, s: usize, ri: usize, kind: RoomKind, now: u64, r: &mut Rng) {
        let room = &self.social.rooms[ri];
        let me = &self.world.units[self.ai[s].uidx];
        let mut other = None;
        for t in 0..self.ai.len() {
            if t == s || self.ai[t].kind != Kind::Worker {
                continue;
            }
            let u = &self.world.units[self.ai[t].uidx];
            if !u.flags.citizen || !room.contains(u.pos) {
                continue;
            }
            let rel = me.relationships.iter().find(|x| x.unit_id == u.id || (u.hist_fig_id >= 0 && x.hist_fig_id == u.hist_fig_id));
            let code = match rel.map(|x| x.kind.as_str()) {
                Some("Spouse" | "Lover") => 3,
                Some("Mother" | "Father" | "Child" | "Sibling" | "Brother" | "Sister") => 2,
                Some("Friend" | "CloseFriend" | "KindredSpirit") => 1,
                Some("Grudge" | "Enemy") => -2,
                _ => 0,
            };
            if other.map(|(c, _)| code > c).unwrap_or(true) {
                other = Some((code, u.id));
            }
        }
        let Some((code, other_id)) = other else { return };
        self.satisfy(s, "Socialize");
        match code {
            3 => {
                self.satisfy(s, "MakeRomance");
                self.satisfy(s, "BeWithFamily");
            }
            2 => {
                self.satisfy(s, "BeWithFamily");
            }
            1 => {
                self.satisfy(s, "BeWithFriends");
            }
            _ => {}
        }
        if kind == RoomKind::Tavern {
            self.satisfy(s, "MakeMerry");
        }
        if kind == RoomKind::Library {
            self.satisfy(s, "ThinkAbstractly");
        }
        if r.chance(1, 4) {
            self.social.conversations += 1;
            let partner_stress = self.world.unit(other_id).map(|u| u.stress).unwrap_or(0); // [skills9]
            self.skills_talked(s, code, partner_stress, now); // [skills9] conversation skills
            if code == -2 {
                self.feel(s, "Argument", Class::NegMild, 80, -1, 0, now);
            } else {
                let class = if code >= 1 { Class::Pos } else { Class::PosMild };
                self.feel(s, "Talked", class, 60 + 20 * code, code, 0, now);
            }
        }
    }

    // ------------------------------------------------------------------ breakdowns

    fn social_breakdown(&mut self, s: usize, now: u64, r: &mut Rng) {
        let uidx = self.ai[s].uidx;
        let (st, lt, name, mood) = {
            let u = &self.world.units[uidx];
            (u.stress, u.longterm_stress, u.name.clone(), u.mood.clone())
        };
        if matches!(mood.as_str(), "Melancholy" | "Raving" | "Berserk") {
            return;
        }
        let f = self.social.psy[s].facets;
        let state = self.social.psy[s].breakdown;
        if state != Breakdown::None {
            if now >= self.social.psy[s].until {
                self.social.psy[s].breakdown = Breakdown::None;
                let p = &mut self.social.psy[s];
                // Insanity: repeated breakdowns while harrowed (estimate).
                if p.breakdowns >= 3 && lt >= 100_000 && st >= 75_000 && r.chance(1, 4) {
                    let (m, text) = if f.anger >= f.depression && f.anger >= f.anxiety {
                        ("Berserk", "has gone berserk!")
                    } else if f.depression >= f.anxiety {
                        ("Melancholy", "has been overtaken by melancholy.")
                    } else {
                        ("Raving", "has gone stark raving mad!")
                    };
                    self.social.insane += 1;
                    self.world.units[uidx].mood = m.into();
                    let pos = self.world.units[uidx].pos;
                    self.announce(now, format!("{name} {text}"), Some(pos), AnnouncementKind::Warning);
                    return;
                }
                let pos = self.world.units[uidx].pos;
                self.announce(now, format!("{name} has calmed down."), Some(pos), AnnouncementKind::Info);
            } else if state == Breakdown::Tantrum {
                self.tantrum_act(s, now, r);
            }
            return;
        }
        // Both short-term and long-term stress must be high (wiki); STRESS_VULNERABILITY moves
        // the threshold. Prone dwarves snap about once a month.
        let thr = 50_000 + (50 - f.vulnerability) * 500;
        if st < thr || lt < 25_000 || !r.chance(1, 28) {
            return;
        }
        let kind = if f.anger >= f.depression && f.anger >= f.anxiety {
            Breakdown::Tantrum
        } else if f.depression >= f.anxiety {
            Breakdown::Depressed
        } else {
            Breakdown::Oblivious
        };
        let days = match kind {
            Breakdown::Tantrum => 2 + r.below(3),
            Breakdown::Depressed => 3 + r.below(5),
            _ => 2 + r.below(4),
        } as u64;
        let p = &mut self.social.psy[s];
        p.breakdown = kind;
        p.until = now + days * DAY as u64;
        p.breakdowns += 1;
        let text = match kind {
            Breakdown::Tantrum => {
                self.social.tantrums += 1;
                "is throwing a tantrum!"
            }
            Breakdown::Depressed => {
                self.social.depressions += 1;
                "has slipped into depression..."
            }
            _ => {
                self.social.oblivious += 1;
                "is stumbling around obliviously!"
            }
        };
        let pos = self.world.units[uidx].pos;
        self.announce(now, format!("{name} {text}"), Some(pos), AnnouncementKind::Warning);
        if matches!(self.ai[s].task, Task::Job(_) | Task::GoJob(_) | Task::Fetch(_) | Task::GoSite | Task::Craft | Task::GoPickup(..) | Task::GoStore(..)) {
            self.cancel_task(s);
            self.ai[s].wake = now + 1;
        }
    }

    /// What a tantruming dwarf does once a day: throw something, topple furniture, or start
    /// a fist fight (wiki "Tantrum").
    fn tantrum_act(&mut self, s: usize, now: u64, r: &mut Rng) {
        let uidx = self.ai[s].uidx;
        let pos = self.world.units[uidx].pos;
        let name = self.world.units[uidx].name.clone();
        match r.below(4) {
            0 => {
                // Topple a nearby piece of furniture (never artifacts or beds in use).
                let target = self
                    .social
                    .furn
                    .iter()
                    .filter(|fu| fu.pos.z == pos.z && fu.pos.dist(pos) <= 3 && fu.quality < 6 && matches!(fu.kind, "chair" | "table" | "cabinet" | "chest" | "statue"))
                    .map(|fu| fu.bid)
                    .min();
                if let Some(bid) = target {
                    let kind = self.world.building(bid).map(|b| b.kind.to_lowercase()).unwrap_or_default();
                    self.remove_building(bid);
                    self.announce(now, format!("{name} has toppled a {kind}."), Some(pos), AnnouncementKind::Warning);
                    self.feel(s, "ToppledStuff", Class::PosMild, 60, -1, 0, now);
                    self.justice_tantrum(s, -1, now); // [society2] crime: BuildingDestruction
                }
            }
            1 => {
                // Start a fist fight with a citizen nearby.
                let victim = (0..self.ai.len()).find(|&t| {
                    t != s && self.ai[t].kind == Kind::Worker && {
                        let u = &self.world.units[self.ai[t].uidx];
                        u.pos.z == pos.z && u.pos.dist(pos) <= 3
                    }
                });
                if let Some(t) = victim {
                    let vname = self.world.units[self.ai[t].uidx].name.clone();
                    self.announce(now, format!("{name} punches {vname}!"), Some(pos), AnnouncementKind::Warning);
                    self.feel(s, "FistFight", Class::PosMild, 60, -1, 0, now);
                    let me = self.ai[s].id;
                    self.feel(t, "Attacked", Class::Neg, 120, me, 0, now);
                    let vid = self.ai[t].id;
                    self.justice_tantrum(s, vid, now); // [society2] crime: DisorderlyBehavior
                }
            }
            2 => self.feel(s, "ThrownStuff", Class::PosMild, 50, -1, 0, now),
            _ => {}
        }
    }

    /// [hook] Units in a breakdown (or insane) do not work.
    pub(crate) fn social_no_work(&self, s: usize) -> bool {
        if let Some(p) = self.psy(s) {
            if p.breakdown != Breakdown::None {
                return true;
            }
        }
        let u = &self.world.units[self.ai[s].uidx];
        matches!(u.mood.as_str(), "Melancholy" | "Raving" | "Berserk")
    }

    /// [hook] Effective skill rating with the unit's focus (+-50 %, wiki "Need").
    pub(crate) fn social_rating(&self, s: usize, rating: i32) -> i32 {
        let f = self.psy(s).map(|p| p.focus).unwrap_or(100);
        (rating * f + 50) / 100
    }

    /// [hook] Which meeting zone (index into `zones`) an idle unit walks to: temples for
    /// prayer, taverns/meeting halls for company; otherwise a random one.
    pub(crate) fn social_pick_zone(&mut self, s: usize, r: &mut Rng) -> usize {
        let n = self.zones.len();
        let fallback = r.below(n as u32) as usize;
        let key = (n, self.world.buildings.len());
        if self.social.zone_key != key {
            self.social.zone_key = key;
            let rooms = &self.social.rooms;
            self.social.zone_kind = self
                .zones
                .iter()
                .map(|z| {
                    rooms
                        .iter()
                        .find(|rm| rm.x1 == z.x1 && rm.y1 == z.y1 && rm.x2 == z.x2 && rm.y2 == z.y2 && rm.z == z.z)
                        .map(|rm| rm.kind)
                        .unwrap_or(RoomKind::Meeting)
                })
                .collect();
        }
        if s >= self.ai.len() || !self.is_citizen_slot(s) {
            return fallback;
        }
        let u = &self.world.units[self.ai[s].uidx];
        let lvl = |id: &str| u.needs.iter().filter(|x| x.id == id).map(|x| x.focus_level).min().unwrap_or(i32::MAX);
        let pray = lvl("PrayOrMeditate");
        let social = lvl("Socialize").min(lvl("BeWithFriends")).min(lvl("MakeMerry")).min(lvl("DrinkAlcohol"));
        let want: &[RoomKind] = if pray < 0 && pray <= social {
            &[RoomKind::Temple]
        } else if social < 0 {
            &[RoomKind::Tavern, RoomKind::Meeting]
        } else {
            return fallback;
        };
        let cands: Vec<usize> = (0..n).filter(|&k| want.contains(&self.social.zone_kind[k])).collect();
        if cands.is_empty() || r.chance(1, 4) {
            return fallback;
        }
        cands[r.below(cands.len() as u32) as usize]
    }

    pub(crate) fn social_hash(&self, mix: &mut dyn FnMut(u64)) {
        for ai in &self.ai {
            let u = &self.world.units[ai.uidx];
            if u.flags.citizen {
                mix(((u.stress as u32 as u64) << 32) | u.longterm_stress as u32 as u64);
                mix(u.emotions.len() as u64);
            }
        }
    }

    // ------------------------------------------------------------------ public API

    /// Switch the social simulation on/off (default on). Off = v0.2 behaviour: no thoughts,
    /// no stress changes, no dining detours (for comparisons and benchmarks).
    pub fn set_social_enabled(&mut self, on: bool) {
        self.social.off = !on;
    }

    /// Long-term stress of a unit (DF v50 `longterm_stress`).
    pub fn longterm_stress(&self, unit_id: i32) -> i32 {
        self.world.unit(unit_id).map(|u| u.longterm_stress).unwrap_or(0)
    }

    /// Overall focus of a unit in percent (100 = undistracted; DF: 60 and lower = badly
    /// distracted, 140 and higher = very focused).
    pub fn focus_percent(&self, unit_id: i32) -> i32 {
        match self.unit_slot.get(&unit_id) {
            Some(&s) => self.psy(s).map(|p| p.focus).unwrap_or(100),
            None => 100,
        }
    }

    /// The unit's needs: (DF need name, need level 1..10, focus level, DF focus word).
    pub fn need_report(&self, unit_id: i32) -> Vec<(String, i32, i32, &'static str)> {
        self.world
            .unit(unit_id)
            .map(|u| u.needs.iter().map(|n| (n.id.clone(), n.need_level, n.focus_level, focus_state(n.focus_level))).collect())
            .unwrap_or_default()
    }

    /// Current mental breakdown of a unit, if any.
    pub fn breakdown(&self, unit_id: i32) -> Breakdown {
        match self.unit_slot.get(&unit_id) {
            Some(&s) => self.psy(s).map(|p| p.breakdown).unwrap_or_default(),
            None => Breakdown::None,
        }
    }

    /// The unit's most recent thoughts as DF-style sentences, newest first.
    pub fn recent_thoughts(&self, unit_id: i32, n: usize) -> Vec<String> {
        self.world
            .unit(unit_id)
            .map(|u| u.emotions.iter().rev().filter(|e| e.thought != "None").take(n).map(|e| self.describe(e)).collect())
            .unwrap_or_default()
    }

    pub fn social_report(&self) -> SocialReport {
        let mut thoughts: Vec<(String, u64)> = self.social.counts.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        thoughts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        SocialReport {
            months: self.social.months.clone(),
            thoughts,
            tantrums: self.social.tantrums,
            depressions: self.social.depressions,
            oblivious: self.social.oblivious,
            insane: self.social.insane,
            meals_at_table: self.social.meals_at_table,
            meals_without_table: self.social.meals_without_table,
            prayers: self.social.prayers,
            conversations: self.social.conversations,
            seats: self.social.seats.len(),
            bedrooms: self.social.rooms.iter().filter(|r| r.kind == RoomKind::Bedroom && r.owner >= 0).count(),
        }
    }

    /// DF-style sentence for a remembered emotion (see `describe_emotion`).
    pub(crate) fn describe(&self, e: &Emotion) -> String {
        let caption = captions::CAPTIONS.iter().find(|(n, _)| *n == e.thought).map(|(_, c)| *c);
        let word = if e.emotion == "ANYTHING" { "nothing".to_string() } else { emotion_word(&e.emotion) };
        let Some(cap) = caption else {
            return format!("felt {word} ({})", e.thought);
        };
        let q = e.severity.clamp(0, 7) as usize;
        let room_word = TIER_WORD[(e.severity + 2).clamp(0, 7) as usize];
        let somebody = || -> String {
            if let Some(u) = self.world.unit(e.subthought) {
                return u.name.clone();
            }
            if let Some(it) = self.world.item(e.subthought) {
                if let Some((n, _)) = it.desc.split_once("'s ") {
                    return n.to_string();
                }
            }
            "somebody".to_string()
        };
        let text = match e.thought.as_str() {
            "Death" => format!("at the death of {}", somebody()),
            "WitnessDeath" => match self.world.unit(e.subthought) {
                Some(u) => format!("after seeing {} die", u.name),
                None => cap.to_string(),
            },
            "SawDeadBody" => format!("after seeing {}'s dead body", somebody()),
            "NeedsUnfulfilled" => {
                format!("after {}", NEED_UNMET.get(e.subthought.max(0) as usize).copied().unwrap_or("unmet needs"))
            }
            "Talked" => {
                let rel = match e.subthought {
                    3 => "spouse",
                    2 => "family member",
                    1 => "friend",
                    _ => "acquaintance",
                };
                format!("talking with {}", article(rel))
            }
            "Prayer" => "after communing with a deity".to_string(),
            "GoodMeal" => {
                let w = FOOD_WORD[q.min(6)];
                if w.is_empty() {
                    "after eating a prepared meal".to_string()
                } else {
                    format!("after eating a {w} meal")
                }
            }
            "GoodDrink" => format!("after having a {} drink", QUALITY_WORD[q.min(6)].replace("crafted", "made")),
            "DiningQuality" => format!("dining in {} dining room", article(room_word)),
            "BedroomQuality" => format!("after sleeping in {} bedroom", article(room_word)),
            "AdmireBuilding" | "AdmireOwnBuilding" => {
                let kind = self.world.building(e.subthought).and_then(|b| furniture_kind(&b.kind)).unwrap_or("building");
                let own = if e.thought == "AdmireOwnBuilding" { "their own " } else { "" };
                let w = QUALITY_WORD[q.min(6)];
                if own.is_empty() {
                    format!("near {} {kind}", article(w))
                } else {
                    format!("near {own}{w} {kind}")
                }
            }
            "ImproveSkill" => "upon improving a skill".to_string(),
            "Attacked" => match self.world.unit(e.subthought) {
                Some(u) => format!("after being attacked by {}", u.name),
                None => cap.to_string(),
            },
            _ => cap
                .replace("[somebody]", &somebody())
                .replace("[his]", "their")
                .replace("[he]", "they")
                .replace("[a quality]", "a")
                .replace("[quality] ", "")
                .replace('[', "")
                .replace(']', ""),
        };
        format!("felt {word} {text}")
    }
}

fn article(w: &str) -> String {
    if w.starts_with(|c: char| "aeiou".contains(c)) {
        format!("an {w}")
    } else {
        format!("a {w}")
    }
}

/// Overall focus in percent from the unit's needs (DF: `current_focus / undistracted_focus`,
/// every need counts the same).
fn focus_of(needs: &[UnitNeed]) -> i32 {
    let (mut num, mut den) = (0i64, 0i64);
    for n in needs {
        num += focus_points(n.focus_level) as i64;
        den += 40;
    }
    if den == 0 {
        100
    } else {
        (num * 100 / den) as i32
    }
}

/// Needs for a unit the export gave none (estimate from facets, see module docs).
fn default_needs(f: &Facets, id: i32) -> Vec<UnitNeed> {
    let mut v = Vec::new();
    let mut add = |id: &str, lvl: i32| {
        if lvl > 0 {
            v.push(UnitNeed { id: id.into(), deity_id: -1, focus_level: 0, need_level: lvl.clamp(1, 10) });
        }
    };
    add("DrinkAlcohol", 1 + f.immoderation / 20);
    add("PrayOrMeditate", 1 + (id.rem_euclid(5)));
    if f.gregarious >= 40 {
        add("Socialize", (f.gregarious - 30) / 10);
    }
    if f.activity >= 60 {
        add("StayOccupied", (f.activity - 50) / 5);
    }
    if f.activity <= 40 {
        add("TakeItEasy", (50 - f.activity) / 5);
    }
    if f.friendliness >= 60 {
        add("BeWithFriends", (f.friendliness - 50) / 5);
    }
    add("CraftObject", 2);
    if f.immoderation >= 60 {
        add("EatGoodMeal", (f.immoderation - 50) / 5);
    }
    if f.art >= 60 {
        add("AdmireArt", (f.art - 50) / 5);
    }
    if f.curious >= 60 {
        add("LearnSomething", (f.curious - 50) / 5);
    }
    v
}
