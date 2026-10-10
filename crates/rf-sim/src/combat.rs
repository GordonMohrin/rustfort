//! Combat (round 3): bodies, attacks, layered damage through armor and tissues, wounds,
//! bleeding, pain, unconsciousness, death, healing and the combat report log.
//!
//! # DF rules implemented (wiki: Combat, Attack, Material science, Wound, Blood, Pain, Weapon, Armor)
//! - **Bodies** are DF's expanded caste bodies from the export (`creature_raws`: parts with
//!   relsize, flags and tissue layers). Without raws a synthetic humanoid body (DF `HUMANOID` +
//!   neck, brain, heart, lungs, spine, guts; relsizes from the vanilla body raws) or a
//!   synthetic quadruped is used, with skin/fat/muscle/bone layers (DF tissue thickness 1/1/3/2).
//! - **Attack momentum** (wiki Material science, formula 3):
//!   `M = Size * Str * Vel / (10^6 * (1 + i_Size / (w_density * w_size)))`, with the weapon's
//!   material density and size, or for body-part attacks the attacking part's tissue density
//!   and volume. `Vel` = the attack's velocity multiplier.
//! - **Edged attacks** cut a layer if `M >= (rSY + (A+1)*rSF) * (10 + 2*Qa) / (S * Qw)`
//!   (rSY/rSF = layer/weapon shear yield/fracture, S = sharpness = MAX_EDGE/10000, so 1 for
//!   metals and 0.1 for teeth and claws, Qa = armor quality level, Qw = weapon quality factor).
//!   A cut reduces momentum by 5 %. A failed cut multiplies momentum by
//!   SHEAR_STRAIN_AT_YIELD/50000 and continues as a blunt attack.
//! - **Blunt attacks** are deflected entirely by armor if `2 * Sw * IYw < A * Da`; they break a
//!   layer if `M >= (2*IF - IY) * (2 + 0.4*Qa) * A` (MPa), momentum -5 %; otherwise momentum is
//!   multiplied by IMPACT_STRAIN_AT_YIELD/50000 (soft tissue, 50000, passes the force on and
//!   is bruised; rigid metal absorbs almost all of it).
//! - Contact area is the minimum of the attack's and the body part's contact area.
//! - **Wounds** per body part and tissue layer with DF flags (CUT, BRUISED, BROKEN, SEVERED,
//!   TORN, BLEEDING); severed parts take their children with them and drop a body part item.
//! - **Death**: brain (THOUGHT) cut, torn or fully damaged; head severed; heart (CIRCULATION)
//!   torn bleeds out within moments; no blood left. Below 50 % blood a creature is unconscious
//!   (wiki Blood: "Faint" < 50 %).
//! - **Bleeding** from VASCULAR tissues (DF tissue VASCULAR values: skin 1, fat 3, muscle 5 with
//!   ARTERIES, bone 3, heart 10, lung 8); **pain** from PAIN_RECEPTORS (skin/fat/muscle 5,
//!   bone 50); NOPAIN creatures feel none.
//! - **Defense**: dodging (DODGING, agility), blocking with a shield (SHIELD), parrying with a
//!   weapon (weapon skill); attackers and defenders gain skill experience.
//! - **Healing**: wounds heal over time, much faster when resting in a bed or a hospital zone;
//!   broken bones only heal while resting; severed parts never come back. Blood regenerates
//!   when nothing bleeds.
//!
//! # Estimates (not on the wiki; marked here and in PROGRESS.md)
//! - Body-part contact area = volume^(2/3) * 50 (mm², volume in cm³); the formulas use
//!   A = contact/1000.
//! - Penetration depth: tissue layer depth = part radius split by relative thickness; edged
//!   attacks stop when the raw penetration/100 (mm) is used up. Internal organs are reached
//!   when all layers of the outer part are defeated (organ chosen by relsize).
//! - Blunt attacks on armor use A >= 1 (a rigid plate spreads the blow; without this every
//!   whip and mace would break iron armor).
//! - Edged attacks see at least A = 2 on an armor layer; force absorbed by rigid armor is
//!   spread over the whole body part beneath.
//! - An edged attack never cuts an armor layer whose shear yield is at least twice the weapon's
//!   (DF lore: copper cannot cut iron armor).
//! - Severing: all layers cut, the attack's contact area is at least half the part's, 50 %.
//! - Hit/dodge/block/parry chances, attack intervals (prepare + recover + 4 ticks, scaled by
//!   agility), the momentum roll (60-100 % plus 3 % per skill level), bleeding volumes, clotting,
//!   pain thresholds (250 + WILLPOWER/10), healing and blood regeneration rates.
//! - Blood volume when not exported: body size / 10.

use crate::path::Goals;
use crate::rng::Rng;
use crate::{Kind, Sim, Task, NONE};
use rf_core::{AttackRaw, CasteRaw, Item, ItemFlags, MaterialInfo, Pos, TissueLayerRaw, Wound, NO_MAT};
use std::collections::{HashMap, VecDeque};
mod saves10; // [saves10] save/load of raid groups and per-unit combat state

/// Body part flag bits (from DF body part flags).
pub(crate) mod bp {
    pub const VITAL: u16 = 1;
    pub const THOUGHT: u16 = 2;
    pub const NERVOUS: u16 = 4;
    pub const CIRC: u16 = 8;
    pub const GRASP: u16 = 16;
    pub const STANCE: u16 = 32;
    pub const HEAD: u16 = 64;
    pub const UPPER: u16 = 128;
    pub const LOWER: u16 = 256;
    pub const INTERNAL: u16 = 512;
    pub const BREATHE: u16 = 1024;
    pub const LIMB: u16 = 2048;
    pub const EMBEDDED: u16 = 4096;
    pub const THROAT: u16 = 8192;
}

/// Wound flag bits.
pub(crate) mod wf {
    pub const CUT: u16 = 1;
    pub const BRUISED: u16 = 2;
    pub const BROKEN: u16 = 4;
    pub const SEVERED: u16 = 8;
    pub const TORN: u16 = 16;
    pub const BLEEDING: u16 = 32;
    pub const ARTERY: u16 = 64;
    pub const NAMES: [(u16, &str); 7] =
        [(CUT, "CUT"), (BRUISED, "BRUISED"), (BROKEN, "BROKEN"), (SEVERED, "SEVERED"), (TORN, "TORN"), (BLEEDING, "BLEEDING"), (ARTERY, "ARTERY")];
}

/// Armor zones a body part belongs to.
pub(crate) mod zone {
    pub const UB: u16 = 1;
    pub const LB: u16 = 2;
    pub const HEAD: u16 = 4;
    pub const NECK: u16 = 8;
    pub const ARM_UP: u16 = 16;
    pub const ARM_LOW: u16 = 32;
    pub const HAND: u16 = 64;
    pub const LEG_UP: u16 = 128;
    pub const LEG_LOW: u16 = 256;
    pub const FOOT: u16 = 512;
}

/// Mechanical properties of a material (DF raw units: density kg/m³, yields/fractures kPa).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mech {
    pub dens: f32,
    pub iy: f32,
    pub ifr: f32,
    pub isay: f32,
    pub sy: f32,
    pub sf: f32,
    pub ssay: f32,
    pub edge: f32,
}

const fn mech(dens: f32, iy: f32, ifr: f32, isay: f32, sy: f32, sf: f32, ssay: f32, edge: f32) -> Mech {
    Mech { dens, iy, ifr, isay, sy, sf, ssay, edge }
}

/// Vanilla DF material values (data/vanilla raws: inorganic_metal, material_template_default).
const DEFAULT_MECH: &[(&str, Mech)] = &[
    ("ADAMANTINE", mech(200.0, 5e6, 5e6, 0.0, 5e6, 5e6, 0.0, 100_000.0)),
    ("PIG_IRON", mech(7850.0, 455_000.0, 700_000.0, 268.0, 130_000.0, 200_000.0, 159.0, 10_000.0)),
    ("STEEL", mech(7850.0, 1_505_000.0, 2_520_000.0, 940.0, 430_000.0, 720_000.0, 556.0, 10_000.0)),
    ("IRON", mech(7850.0, 542_500.0, 1_085_000.0, 319.0, 155_000.0, 310_000.0, 189.0, 10_000.0)),
    ("BRONZE", mech(8250.0, 602_000.0, 843_500.0, 523.0, 172_000.0, 241_000.0, 420.0, 10_000.0)),
    ("COPPER", mech(8930.0, 245_000.0, 770_000.0, 175.0, 70_000.0, 220_000.0, 145.0, 10_000.0)),
    ("SILVER", mech(10490.0, 350_000.0, 595_000.0, 350.0, 100_000.0, 170_000.0, 333.0, 10_000.0)),
    ("GOLD", mech(19320.0, 175_000.0, 350_000.0, 97.0, 50_000.0, 100_000.0, 185.0, 10_000.0)),
    ("SKIN", mech(1000.0, 10_000.0, 10_000.0, 50_000.0, 20_000.0, 20_000.0, 50_000.0, 0.0)),
    ("FAT", mech(900.0, 10_000.0, 10_000.0, 50_000.0, 10_000.0, 10_000.0, 50_000.0, 0.0)),
    ("MUSCLE", mech(1060.0, 10_000.0, 10_000.0, 50_000.0, 20_000.0, 20_000.0, 50_000.0, 0.0)),
    ("BONE", mech(500.0, 200_000.0, 200_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("NAIL", mech(500.0, 200_000.0, 200_000.0, 5000.0, 115_000.0, 130_000.0, 5000.0, 1000.0)),
    ("CLAW", mech(500.0, 200_000.0, 200_000.0, 5000.0, 115_000.0, 130_000.0, 5000.0, 1000.0)),
    ("TALON", mech(500.0, 200_000.0, 200_000.0, 5000.0, 115_000.0, 130_000.0, 5000.0, 1000.0)),
    ("TOOTH", mech(500.0, 200_000.0, 200_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("HORN", mech(500.0, 200_000.0, 200_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("HOOF", mech(500.0, 200_000.0, 200_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("IVORY", mech(500.0, 200_000.0, 200_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("CHITIN", mech(500.0, 10_000.0, 10_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("SHELL", mech(500.0, 200_000.0, 200_000.0, 100.0, 115_000.0, 130_000.0, 100.0, 1000.0)),
    ("HAIR", mech(500.0, 10_000.0, 10_000.0, 100_000.0, 60_000.0, 120_000.0, 100_000.0, 0.0)),
    ("FEATHER", mech(500.0, 10_000.0, 10_000.0, 100_000.0, 60_000.0, 120_000.0, 100_000.0, 0.0)),
    ("SCALE", mech(500.0, 10_000.0, 10_000.0, 100_000.0, 60_000.0, 120_000.0, 100_000.0, 0.0)),
    ("CARTILAGE", mech(500.0, 10_000.0, 10_000.0, 25_000.0, 30_000.0, 30_000.0, 25_000.0, 0.0)),
    ("LEATHER", mech(500.0, 10_000.0, 10_000.0, 50_000.0, 25_000.0, 25_000.0, 50_000.0, 0.0)),
    ("WOOD", mech(500.0, 10_000.0, 10_000.0, 1000.0, 40_000.0, 40_000.0, 1000.0, 1000.0)),
    ("STONE", mech(2670.0, 120_000.0, 120_000.0, 100.0, 15_000.0, 15_000.0, 100.0, 1000.0)),
    ("METAL", mech(7850.0, 1_080_000.0, 1_080_000.0, 1000.0, 520_000.0, 860_000.0, 1000.0, 10_000.0)),
];
/// Organs, brain, nerves, cloth and anything unknown: soft tissue (DF organ templates).
const SOFT: Mech = mech(500.0, 10_000.0, 10_000.0, 50_000.0, 20_000.0, 20_000.0, 50_000.0, 0.0);

/// Default mechanics for a material token or tissue name (upper case, any position).
pub fn default_mech(name: &str) -> Mech {
    let up = name.to_ascii_uppercase();
    for (k, m) in DEFAULT_MECH {
        if up.contains(k) {
            return *m;
        }
    }
    if up.contains("TEETH") {
        return default_mech("TOOTH");
    }
    SOFT
}

/// DF tissue templates: (vascular, pain receptors, relative thickness, has major arteries).
fn tissue_info(name: &str) -> (i32, i32, i32, bool) {
    let up = name.to_ascii_uppercase();
    let t: &[(&str, (i32, i32, i32, bool))] = &[
        ("SKIN", (1, 5, 1, false)),
        ("FAT", (3, 5, 1, false)),
        ("MUSCLE", (5, 5, 3, true)),
        ("BONE", (3, 50, 2, false)),
        ("BRAIN", (3, 0, 1, false)),
        ("HEART", (10, 5, 1, false)),
        ("LUNG", (8, 5, 1, false)),
        ("LIVER", (8, 5, 1, false)),
        ("KIDNEY", (8, 5, 1, false)),
        ("SPLEEN", (5, 5, 1, false)),
        ("NERVE", (3, 0, 1, false)),
        ("EYE", (3, 5, 1, false)),
        ("CARTILAGE", (0, 0, 2, false)),
        ("HAIR", (0, 0, 1, false)),
        ("FEATHER", (0, 0, 1, false)),
        ("CHITIN", (1, 5, 1, false)),
        ("SCALE", (1, 5, 1, false)),
        ("NAIL", (0, 0, 2, false)),
        ("CLAW", (0, 0, 2, false)),
        ("TOOTH", (0, 0, 2, false)),
        ("HORN", (0, 0, 2, false)),
        ("HOOF", (0, 0, 2, false)),
        ("GUT", (3, 5, 1, false)),
        ("STOMACH", (3, 5, 1, false)),
    ];
    for (k, v) in t {
        if up.contains(k) {
            return *v;
        }
    }
    (1, 5, 1, false)
}

#[derive(Clone, Debug)]
pub(crate) struct LayerT {
    pub tissue: String,
    pub mech: Mech,
    /// Depth in mm (for penetration).
    pub depth: f32,
    pub vascular: i32,
    pub pain: i32,
    pub arteries: bool,
    pub rigid: bool,
    pub hair: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PartT {
    pub token: String,
    pub name: String,
    pub parent: i32,
    pub rel: i32,
    pub flags: u16,
    pub zone: u16,
    /// Volume in cm³ for an adult of the caste.
    pub vol: f32,
    /// Contact area in mm² (estimate: volume^(2/3) * 50).
    pub contact: f32,
    /// First layer in the body's flat layer list, and the count.
    pub l0: u16,
    pub nl: u16,
    pub internal_children: Vec<u16>,
    pub children: Vec<u16>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct AtkT {
    pub verb2: String,
    pub verb3: String,
    /// Attacking body part (body-part attacks), -1 for weapon attacks.
    pub part: i32,
    pub contact: f32,
    pub pen: f32,
    pub vel: f32,
    pub prep: i32,
    pub rec: i32,
    pub edge: bool,
    pub weight: u32,
    pub skill: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct BodyT {
    pub race: String,
    pub creature: String,
    pub size: i32,
    pub parts: Vec<PartT>,
    pub layers: Vec<LayerT>,
    pub attacks: Vec<AtkT>,
    /// Non-internal parts with cumulative relsize weights (hit location table).
    pub targets: Vec<(u16, u32)>,
    pub nopain: bool,
}

/// [ranged] A blow that landed: the attack, the weapon or projectile, the names for the log.
pub(crate) struct Blow {
    pub atk: AtkT,
    /// Struck with an object (weapon, projectile), not a body part.
    pub armed: bool,
    pub wmech: Mech,
    pub wsize: f32,
    pub wquality: f32,
    pub wdesc: Option<String>,
    pub skill: i32,
    pub an: String,
    pub dn: String,
    /// Projectile momentum: replaces the attacker's strength-based momentum.
    pub momentum: Option<f32>,
}

#[derive(Clone, Debug)]
pub(crate) struct WeapT {
    pub name: String,
    pub skill: String,
    pub size: f32,
    pub attacks: Vec<AtkT>,
}

/// One wound on one layer of one body part (sim side; mirrored into `Unit::wounds`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WoundS {
    pub part: u16,
    /// Flat layer index, or u16::MAX for a part-level wound (severed).
    pub layer: u16,
    pub flags: u16,
    pub sev: i32,
    pub bleed: i32,
    pub pain: i32,
    pub age: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Role {
    #[default]
    None,
    Hostile,
    Soldier,
    Civilian,
    Fighter,
    Patient,
    Leave,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sub {
    #[default]
    None,
    Equip(i32),
    Rest(i32),
    Resting(i32),
}

/// Per-unit combat state.
#[derive(Clone, Debug)]
pub(crate) struct Fighter {
    pub role: Role,
    pub sub: Sub,
    pub body: u16,
    pub target: u32,
    pub next_atk: u64,
    pub uncon_until: u64,
    pub paralyzed: bool,
    pub repath_at: u64,
    pub threat: u32,
    pub calm_since: u64,
    /// [chaos10] The alert burrow is unreachable for this civilian until then: behave normally.
    pub alert_skip_until: u64,
    pub equip_at: u64,
    pub train_at: u64,
    pub siege: u16,
    pub home: u32,
    pub duty: crate::military::Duty,
    pub squad: i32,
    pub wounds: Vec<WoundS>,
    /// Per body part: 1 = severed/lost, 2 = broken bone, 4 = function lost.
    pub part_state: Vec<u8>,
    pub bleed: i32,
    pub blood: i32,
    pub blood_max: i32,
    pub dormant_until: u64,
    /// Healed exported wounds (scars), kept for `Unit::wounds`.
    pub scars: Vec<Wound>,
    pub listed_bleed: bool,
    pub listed_wound: bool,
    /// [audit-combat] Helpless from a syndrome (paralysis, unconsciousness, webs) until.
    pub syn_until: u64,
    /// [military2] Sneaking raider not spotted yet (ambushers, thieves, snatchers).
    pub hidden: bool,
    /// [military2] Raid kind code (`raids::RaidKind::code`), 0 = none.
    pub raid: u8,
    /// [military2] Thieves: item id going for / item id stolen (-1 none).
    pub want: i32,
    pub loot: i32,
    /// [military2] Slot of the unit this one carries (snatched child, rescued patient).
    pub carry: u32,
    /// [military2] Slot of the unit carrying this one.
    pub carried_by: u32,
    /// [military2] Patrol: index of the next waypoint.
    pub patrol_i: u16,
    /// [military2] Training spot inside the barracks (tile), NONE = none.
    pub spot: u32,
    /// [military2] Cannot stand (broken leg): crawls at a fraction of the walking speed.
    pub crawl: bool,
    /// [war9] Tick the unit started leaving the map (0 = not leaving).
    pub leave_since: u64,
    /// [war9] Civilian on a drink/eat break despite a threat, until this tick.
    pub brk_until: u64,
    /// [war9] Building destroyers: building id being wrecked (-1 none), done at, next search.
    pub wreck: i32,
    pub wreck_until: u64,
    pub wreck_at: u64,
}

impl Default for Fighter {
    fn default() -> Self {
        Fighter {
            role: Role::None,
            sub: Sub::None,
            body: u16::MAX,
            target: NONE,
            next_atk: 0,
            uncon_until: 0,
            paralyzed: false,
            repath_at: 0,
            threat: NONE,
            calm_since: 0,
            alert_skip_until: 0, // [chaos10]
            equip_at: 0,
            train_at: 0,
            siege: 0,
            home: NONE,
            duty: crate::military::Duty::None,
            squad: -1,
            wounds: Vec::new(),
            part_state: Vec::new(),
            bleed: 0,
            blood: 0,
            blood_max: 0,
            dormant_until: 0,
            scars: Vec::new(),
            listed_bleed: false,
            listed_wound: false,
            syn_until: 0,
            hidden: false,
            raid: 0,
            want: -1,
            loot: -1,
            carry: NONE,
            carried_by: NONE,
            patrol_i: 0,
            spot: NONE,
            crawl: false,
            leave_since: 0,
            brk_until: 0,
            wreck: -1,
            wreck_until: 0,
            wreck_at: 0,
        }
    }
}

/// Counters for reports and tests.
#[derive(Clone, Debug, Default)]
pub struct CombatStats {
    pub attacks: u64,
    pub hits: u64,
    pub misses: u64,
    pub dodges: u64,
    pub blocks: u64,
    pub parries: u64,
    pub deflected: u64,
    pub severed: u64,
    pub kills: u64,
    pub bled_out: u64,
    pub unconscious: u64,
    pub wounds: u64,
    pub sieges: u64,
    pub invaders_spawned: u64,
    pub invaders_killed: u64,
    pub invaders_left: u64,
    pub fort_deaths: u64,
    /// [audit-combat] Times a wounded soldier on duty went to bed instead of training.
    pub soldiers_rested: u64,
    /// [audit-combat] Lurking (dormant) hostiles spotted and engaged by soldiers on duty.
    pub lurkers_engaged: u64,
    /// [audit-combat] Weapon-trap kills that jammed the trap (wiki: 50 %).
    pub traps_jammed: u64,
    /// [audit-combat] Syndromes (venom, breath) that took hold.
    pub syndromes: u64,
    /// [military2] Ambushes, thieves and snatchers that arrived; raids spotted.
    pub ambushes: u64,
    pub thieves: u64,
    pub snatchers: u64,
    pub raids_spotted: u64,
    /// [military2] Items grabbed by thieves / carried off the map.
    pub items_stolen: u64,
    pub items_lost: u64,
    /// [military2] Children carried off the map by snatchers.
    pub children_abducted: u64,
    /// [military2] Siege monsters (trolls, beak dogs), sieges that camped, leaders killed.
    pub monsters_spawned: u64,
    pub siege_camps: u64,
    pub leaders_killed: u64,
    /// [military2] Patients carried to a bed (Recover wounded); units that had to crawl.
    pub rescues: u64,
    pub crawlers: u64,
    /// [military2] Barracks: sparring bouts, sparring injuries, demonstrations watched,
    /// nights slept in barracks beds, patrol waypoints reached.
    pub sparring: u64,
    pub sparring_injuries: u64,
    pub demonstrations: u64,
    pub barracks_sleeps: u64,
    pub patrol_points: u64,
    /// [war9] Mounts that came with sieges, ambush squads sent alongside sieges.
    pub mounts_spawned: u64,
    pub siege_ambush_squads: u64,
    /// [war9] Buildings (workshops, furniture, doors...) wrecked by building destroyers.
    pub buildings_wrecked: u64,
    /// [war9] Leaving raiders that could not get off the map in time (fought on / slipped away
    /// / left lying helpless).
    pub leave_gave_up: u64,
    /// [war9] Civilians who broke off fleeing to drink or eat.
    pub flee_breaks: u64,
}

/// Combat, military and invasion state of the simulation.
pub(crate) struct Combat {
    pub f: Vec<Fighter>,
    pub bodies: Vec<BodyT>,
    pub body_of: HashMap<(String, String), u16>,
    pub weapons: Vec<WeapT>,
    pub weap_of: HashMap<String, u16>,
    /// Slots of living hostile units on the map, ascending.
    pub hostiles: Vec<u32>,
    pub bleeders: Vec<u32>,
    pub wounded: Vec<u32>,
    /// Fort units (citizens and tame animals) for target and threat searches, ascending.
    pub fort: Vec<u32>,
    pub log: VecDeque<String>,
    pub log_total: u64,
    pub stats: CombatStats,
    pub mil: crate::military::Military,
    pub inv: crate::invasion::Invasions,
    /// Hospital zones (x1, y1, x2, y2, z).
    pub hospitals: Vec<(i16, i16, i16, i16, i16)>,
    /// [military2] Units carrying another unit right now (cheap check in `move_to`).
    pub carriers: u32,
}

impl Default for Combat {
    fn default() -> Self {
        Combat {
            f: Vec::new(),
            bodies: Vec::new(),
            body_of: HashMap::new(),
            weapons: Vec::new(),
            weap_of: HashMap::new(),
            hostiles: Vec::new(),
            bleeders: Vec::new(),
            wounded: Vec::new(),
            fort: Vec::new(),
            log: VecDeque::new(),
            log_total: 0,
            stats: CombatStats::default(),
            mil: Default::default(),
            inv: crate::invasion::Invasions::new(),
            hospitals: Vec::new(),
            carriers: 0,
        }
    }
}

const LOG_MAX: usize = 2000;
/// Body-part contact area = volume^(2/3) * CONTACT_K mm² (estimate).
const CONTACT_K: f32 = 50.0;
/// Sight radius of wild hostiles and monsters (estimate) and of fleeing civilians.
pub(crate) const SIGHT: i32 = 20;

/// [audit-combat] `cap` for sibling modules.
pub(crate) fn cap_pub(s: &str) -> String {
    cap(s)
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn verb3(v: &str) -> String {
    if v.ends_with("ch") || v.ends_with("sh") || v.ends_with('s') || v.ends_with('x') {
        format!("{v}es")
    } else {
        format!("{v}s")
    }
}

/// Default DF weapon definitions (vanilla item_weapon.txt) for worlds without exported raws.
fn default_weapons() -> Vec<rf_core::WeaponRaw> {
    let a = |verb: &str, edge: bool, contact: i32, pen: i32, vel: i32| AttackRaw {
        name: verb.into(),
        verb: verb.into(),
        contact_area: contact,
        penetration: pen,
        velocity_multiplier: vel,
        prepare_ticks: 3,
        recover_ticks: 3,
        main: false,
        flags: if edge { vec!["EDGE".into()] } else { vec![] },
        ..Default::default()
    };
    let w = |token: &str, name: &str, skill: &str, size: i32, attacks: Vec<AttackRaw>| rf_core::WeaponRaw {
        token: token.into(),
        name: name.into(),
        skill: skill.into(),
        size,
        attacks,
        ..Default::default()
    };
    vec![
        w("ITEM_WEAPON_AXE_BATTLE", "battle axe", "AXE", 800, vec![a("hack", true, 40000, 6000, 1250), a("slap", false, 40000, 6000, 1250), a("strike", false, 100, 1000, 1000)]),
        w("ITEM_WEAPON_SWORD_SHORT", "short sword", "SWORD", 300, vec![a("slash", true, 20000, 4000, 1250), a("stab", true, 50, 2000, 1000), a("strike", false, 100, 1000, 1000)]),
        w("ITEM_WEAPON_SPEAR", "spear", "SPEAR", 400, vec![a("stab", true, 20, 10000, 1000), a("bash", false, 10000, 6000, 1250)]),
        w("ITEM_WEAPON_MACE", "mace", "MACE", 800, vec![a("bash", false, 20, 200, 2000)]),
        w("ITEM_WEAPON_HAMMER_WAR", "war hammer", "HAMMER", 400, vec![a("bash", false, 10, 200, 2000)]),
        w("ITEM_WEAPON_WHIP", "whip", "WHIP", 100, vec![a("lash", false, 1, 10, 5000)]),
        w("ITEM_WEAPON_PICK", "pick", "MINING", 500, vec![a("strike", true, 100, 4000, 2000)]),
        w("ITEM_WEAPON_PIKE", "pike", "PIKE", 800, vec![a("stab", true, 20, 12000, 1000), a("bash", false, 10000, 6000, 1250)]),
        w("ITEM_WEAPON_HALBERD", "halberd", "AXE", 1200, vec![a("slash", true, 20000, 8000, 1250), a("stab", true, 50, 2000, 1000)]),
        w("ITEM_WEAPON_SWORD_2H", "two-handed sword", "SWORD", 900, vec![a("slash", true, 100000, 8000, 1250), a("stab", true, 50, 4000, 1000)]),
        w("ITEM_WEAPON_CROSSBOW", "crossbow", "HAMMER", 400, vec![a("bash", false, 10000, 4000, 1250)]),
        w("ITEM_WEAPON_BOW", "bow", "SWORD", 300, vec![a("bash", false, 10000, 4000, 1250)]),
    ]
}

/// DF armor definitions used when the export has no armor raws: (token, kind, layer, coverage, layer size).
pub(crate) const DEFAULT_ARMOR: &[(&str, &str, &str, i32, i32)] = &[
    ("ITEM_ARMOR_BREASTPLATE", "ARMOR", "ARMOR", 100, 20),
    ("ITEM_ARMOR_MAIL_SHIRT", "ARMOR", "OVER", 100, 15),
    ("ITEM_ARMOR_LEATHER", "ARMOR", "ARMOR", 100, 20),
    ("ITEM_ARMOR_COAT", "ARMOR", "OVER", 100, 20),
    ("ITEM_ARMOR_SHIRT", "ARMOR", "UNDER", 100, 10),
    ("ITEM_ARMOR_CLOAK", "ARMOR", "COVER", 100, 15),
    ("ITEM_ARMOR_TUNIC", "ARMOR", "UNDER", 100, 10),
    ("ITEM_ARMOR_ROBE", "ARMOR", "OVER", 100, 20),
    ("ITEM_ARMOR_VEST", "ARMOR", "OVER", 50, 10),
    ("ITEM_HELM_HELM", "HELM", "ARMOR", 100, 30),
    ("ITEM_HELM_CAP", "HELM", "OVER", 50, 10),
    ("ITEM_HELM_HOOD", "HELM", "COVER", 100, 10),
    ("ITEM_PANTS_PANTS", "PANTS", "OVER", 100, 15),
    ("ITEM_PANTS_GREAVES", "PANTS", "ARMOR", 100, 15),
    ("ITEM_PANTS_LEGGINGS", "PANTS", "ARMOR", 100, 15),
    ("ITEM_GLOVES_GAUNTLETS", "GLOVES", "ARMOR", 100, 15),
    ("ITEM_GLOVES_GLOVES", "GLOVES", "UNDER", 100, 10),
    ("ITEM_SHOES_BOOTS", "SHOES", "ARMOR", 100, 15),
    ("ITEM_SHOES_BOOTS_LOW", "SHOES", "ARMOR", 100, 15),
    ("ITEM_SHOES_SHOES", "SHOES", "OVER", 100, 10),
    ("ITEM_SHOES_SOCKS", "SHOES", "UNDER", 100, 10),
];

pub(crate) fn is_armor_kind(kind: &str) -> bool {
    matches!(kind, "ARMOR" | "HELM" | "PANTS" | "GLOVES" | "SHOES")
}

/// One synthetic body part: (token, name, parent token, relsize, flags, layers).
type SynPart = (&'static str, &'static str, &'static str, i32, u16, &'static [&'static str]);
const FLESH: &[&str] = &["SKIN", "FAT", "MUSCLE", "BONE"];
const ORGAN_BRAIN: &[&str] = &["BRAIN"];
const ORGAN_HEART: &[&str] = &["HEART"];
const ORGAN_LUNG: &[&str] = &["LUNG"];
const ORGAN_GUT: &[&str] = &["GUT"];
const ORGAN_SPINE: &[&str] = &["BONE", "NERVE"];
const NECK_THROAT: &[&str] = &["SKIN", "MUSCLE"];

/// DF HUMANOID_NECK body + organs (vanilla relsizes).
const SYN_HUMANOID: &[SynPart] = &[
    ("UB", "upper body", "", 1000, bp::UPPER, FLESH),
    ("LB", "lower body", "UB", 1000, bp::LOWER, FLESH),
    ("NK", "neck", "UB", 150, 0, FLESH),
    ("HD", "head", "NK", 300, bp::HEAD, FLESH),
    ("RUA", "right upper arm", "UB", 200, bp::LIMB, FLESH),
    ("LUA", "left upper arm", "UB", 200, bp::LIMB, FLESH),
    ("RLA", "right lower arm", "RUA", 200, bp::LIMB, FLESH),
    ("LLA", "left lower arm", "LUA", 200, bp::LIMB, FLESH),
    ("RH", "right hand", "RLA", 80, bp::GRASP, FLESH),
    ("LH", "left hand", "LLA", 80, bp::GRASP, FLESH),
    ("RUL", "right upper leg", "LB", 500, bp::LIMB, FLESH),
    ("LUL", "left upper leg", "LB", 500, bp::LIMB, FLESH),
    ("RLL", "right lower leg", "RUL", 400, bp::LIMB, FLESH),
    ("LLL", "left lower leg", "LUL", 400, bp::LIMB, FLESH),
    ("RF", "right foot", "RLL", 120, bp::STANCE, FLESH),
    ("LF", "left foot", "LLL", 120, bp::STANCE, FLESH),
    ("BRAIN", "brain", "HD", 200, bp::THOUGHT | bp::NERVOUS | bp::INTERNAL, ORGAN_BRAIN),
    ("HEART", "heart", "UB", 100, bp::CIRC | bp::VITAL | bp::INTERNAL, ORGAN_HEART),
    ("RLUNG", "right lung", "UB", 200, bp::BREATHE | bp::INTERNAL, ORGAN_LUNG),
    ("LLUNG", "left lung", "UB", 200, bp::BREATHE | bp::INTERNAL, ORGAN_LUNG),
    ("UPPERSPINE", "middle spine", "UB", 150, bp::NERVOUS | bp::INTERNAL, ORGAN_SPINE),
    ("GUTS", "guts", "LB", 600, bp::INTERNAL, ORGAN_GUT),
    ("THROAT", "throat", "NK", 40, bp::THROAT | bp::EMBEDDED, NECK_THROAT),
];

/// DF QUADRUPED_NECK body + organs (vanilla relsizes, simplified).
const SYN_QUADRUPED: &[SynPart] = &[
    ("UB", "body", "", 1000, bp::UPPER | bp::LOWER, FLESH),
    ("NK", "neck", "UB", 150, 0, FLESH),
    ("HD", "head", "NK", 300, bp::HEAD, FLESH),
    ("RFL", "right front leg", "UB", 300, bp::LIMB | bp::STANCE, FLESH),
    ("LFL", "left front leg", "UB", 300, bp::LIMB | bp::STANCE, FLESH),
    ("RRL", "right rear leg", "UB", 300, bp::LIMB | bp::STANCE, FLESH),
    ("LRL", "left rear leg", "UB", 300, bp::LIMB | bp::STANCE, FLESH),
    ("TAIL", "tail", "UB", 50, 0, FLESH),
    ("BRAIN", "brain", "HD", 200, bp::THOUGHT | bp::NERVOUS | bp::INTERNAL, ORGAN_BRAIN),
    ("HEART", "heart", "UB", 100, bp::CIRC | bp::VITAL | bp::INTERNAL, ORGAN_HEART),
    ("RLUNG", "right lung", "UB", 200, bp::BREATHE | bp::INTERNAL, ORGAN_LUNG),
    ("LLUNG", "left lung", "UB", 200, bp::BREATHE | bp::INTERNAL, ORGAN_LUNG),
    ("GUTS", "guts", "UB", 600, bp::INTERNAL, ORGAN_GUT),
];

fn flags_of(names: &[String]) -> u16 {
    let mut f = 0;
    for n in names {
        f |= match n.as_str() {
            "VITAL" => bp::VITAL,
            "THOUGHT" => bp::THOUGHT,
            "NERVOUS" => bp::NERVOUS,
            "CIRCULATION" => bp::CIRC,
            "GRASP" => bp::GRASP,
            "STANCE" => bp::STANCE,
            "HEAD" => bp::HEAD,
            "UPPERBODY" => bp::UPPER,
            "LOWERBODY" => bp::LOWER,
            "INTERNAL" => bp::INTERNAL,
            "BREATHE" => bp::BREATHE,
            "LIMB" => bp::LIMB,
            "EMBEDDED" => bp::EMBEDDED,
            "THROAT" => bp::THROAT,
            _ => 0,
        };
    }
    f
}

impl Combat {
    pub(crate) fn log_line(&mut self, line: String) {
        self.log.push_back(line);
        self.log_total += 1;
        if self.log.len() > LOG_MAX {
            self.log.pop_front();
        }
    }
}

/// Finish a body template: volumes, contact areas, zones, children, hit table.
fn finish_body(race: &str, creature: &str, size: i32, mut parts: Vec<PartT>, layers: Vec<LayerT>, attacks: Vec<AtkT>, nopain: bool) -> BodyT {
    let total: i64 = parts.iter().map(|p| p.rel.max(0) as i64).sum::<i64>().max(1);
    let n = parts.len();
    for i in 0..n {
        let vol = (size.max(1) as f64 * parts[i].rel.max(1) as f64 / total as f64) as f32;
        parts[i].vol = vol.max(0.5);
        parts[i].contact = parts[i].vol.powf(2.0 / 3.0) * CONTACT_K;
        let par = parts[i].parent;
        if par >= 0 && (par as usize) < n {
            let me = i as u16;
            if parts[i].flags & bp::INTERNAL != 0 {
                parts[par as usize].internal_children.push(me);
            } else {
                parts[par as usize].children.push(me);
            }
        }
    }
    // Armor zones.
    for i in 0..n {
        let f = parts[i].flags;
        let cat = parts[i].token.to_ascii_uppercase();
        let parent_zone = |parts: &Vec<PartT>, i: usize| -> u16 {
            let p = parts[i].parent;
            if p >= 0 && (p as usize) < parts.len() {
                parts[p as usize].zone
            } else {
                0
            }
        };
        let z = if f & bp::UPPER != 0 {
            zone::UB | if f & bp::LOWER != 0 { zone::LB } else { 0 }
        } else if f & bp::LOWER != 0 {
            zone::LB
        } else if f & bp::HEAD != 0 {
            zone::HEAD
        } else if f & bp::GRASP != 0 {
            zone::HAND
        } else if f & bp::STANCE != 0 {
            zone::FOOT
        } else if cat.contains("NK") || cat.contains("NECK") {
            zone::NECK
        } else {
            let pz = parent_zone(&parts, i);
            if f & bp::LIMB != 0 {
                if pz & zone::UB != 0 && pz & zone::LB == 0 {
                    zone::ARM_UP
                } else if pz & zone::ARM_UP != 0 {
                    zone::ARM_LOW
                } else if pz & zone::LB != 0 {
                    zone::LEG_UP
                } else if pz & zone::LEG_UP != 0 {
                    zone::LEG_LOW
                } else {
                    pz
                }
            } else {
                pz
            }
        };
        parts[i].zone = z;
    }
    let mut targets = Vec::new();
    let mut cum = 0u32;
    for (i, p) in parts.iter().enumerate() {
        if p.flags & bp::INTERNAL == 0 && p.nl > 0 {
            cum += p.rel.max(1) as u32;
            targets.push((i as u16, cum));
        }
    }
    BodyT { race: race.into(), creature: creature.into(), size, parts, layers, attacks, targets, nopain }
}

fn layer_from(tissue: &str, mech: Mech) -> LayerT {
    let (vascular, pain, _, arteries) = tissue_info(tissue);
    let up = tissue.to_ascii_uppercase();
    LayerT {
        tissue: tissue.to_ascii_lowercase(),
        mech,
        depth: 1.0,
        vascular,
        pain,
        arteries,
        rigid: mech.isay < 1000.0,
        hair: ["HAIR", "FEATHER", "EYEBROW", "EYELASH", "WHISKER", "MOUSTACHE", "SIDEBURNS", "BEARD"].iter().any(|h| up.contains(h)),
    }
}

/// Set layer depths of part `pi`: the part radius split by relative thickness.
fn set_depths(parts: &[PartT], layers: &mut [LayerT], thick: &[i32]) {
    for p in parts {
        let total: i32 = (p.l0..p.l0 + p.nl).map(|l| thick[l as usize].max(1)).sum::<i32>().max(1);
        let radius_mm = p.vol.cbrt() * 10.0 / 2.0;
        for l in p.l0..p.l0 + p.nl {
            layers[l as usize].depth = radius_mm * thick[l as usize].max(1) as f32 / total as f32;
        }
    }
}

impl Sim {
    // ------------------------------------------------------------------ setup

    /// Mechanics of a palette material (exported values, else DF defaults by token).
    pub(crate) fn mech_of_mat(&self, mat: u16) -> Mech {
        match self.world.material(mat) {
            Some(m) if m.shear_yield > 0 || m.impact_yield > 0 => Mech {
                dens: if m.density > 0 { m.density as f32 } else { default_mech(&m.token).dens },
                iy: m.impact_yield as f32,
                ifr: m.impact_fracture.max(m.impact_yield) as f32,
                isay: m.impact_strain_at_yield as f32,
                sy: m.shear_yield as f32,
                sf: m.shear_fracture.max(m.shear_yield) as f32,
                ssay: m.shear_strain_at_yield as f32,
                edge: m.max_edge as f32,
            },
            Some(m) => {
                if m.is_wood {
                    default_mech("WOOD")
                } else if m.is_stone || m.is_gem {
                    default_mech("STONE")
                } else if m.is_metal {
                    let d = default_mech(&m.token);
                    if d == SOFT {
                        default_mech("METAL")
                    } else {
                        d
                    }
                } else {
                    default_mech(&m.token)
                }
            }
            None => SOFT,
        }
    }

    fn build_body_from_caste(&self, race: &str, creature: &str, c: &CasteRaw) -> BodyT {
        let mut parts = Vec::new();
        let mut layers = Vec::new();
        let mut thick = Vec::new();
        for p in &c.body_parts {
            let l0 = layers.len() as u16;
            let mut ls: Vec<&TissueLayerRaw> = p.layers.iter().collect();
            // Outermost first: DF lists layers from the outside in; reverse if skin comes last.
            let outer = |t: &TissueLayerRaw| {
                let u = t.tissue.to_ascii_uppercase();
                u.contains("SKIN") || u.contains("HAIR") || u.contains("SCALE") || u.contains("CHITIN") || u.contains("FEATHER")
            };
            if ls.len() > 1 && outer(ls[ls.len() - 1]) && !outer(ls[0]) {
                ls.reverse();
            }
            for t in ls {
                let m = if t.mat != NO_MAT && (t.mat as usize) < self.world.materials.len() {
                    let mm = self.mech_of_mat(t.mat);
                    if mm == SOFT {
                        default_mech(&format!("{} {}", t.material, t.tissue))
                    } else {
                        mm
                    }
                } else {
                    default_mech(&format!("{} {}", t.material, t.tissue))
                };
                layers.push(layer_from(&t.tissue, m));
                thick.push(if t.thickness > 0 { t.thickness } else { tissue_info(&t.tissue).2 });
            }
            parts.push(PartT {
                token: p.token.clone(),
                name: if p.name.is_empty() { p.token.to_ascii_lowercase() } else { p.name.clone() },
                parent: p.parent,
                rel: p.relsize.max(1),
                flags: flags_of(&p.flags),
                zone: 0,
                vol: 0.0,
                contact: 0.0,
                l0,
                nl: layers.len() as u16 - l0,
                internal_children: Vec::new(),
                children: Vec::new(),
            });
        }
        let size = if c.body_size > 0 { c.body_size } else { 60_000 };
        let mut attacks = Vec::new();
        for a in &c.attacks {
            let part = parts.iter().position(|p| p.token == a.body_part).map(|p| p as i32).unwrap_or(-1);
            if part < 0 {
                continue;
            }
            attacks.push(self.attack_t(a, part, true));
        }
        let nopain = c.flags.iter().any(|f| f == "NOPAIN");
        let mut b = finish_body(race, creature, size, parts, layers, attacks, nopain);
        set_depths(&b.parts, &mut b.layers, &thick);
        // Body-part attacks: contact/penetration given as a percentage of the part.
        for a in &mut b.attacks {
            if a.part >= 0 {
                let p = &b.parts[a.part as usize];
                if a.contact <= 100.0 {
                    a.contact = p.contact * if a.contact > 0.0 { a.contact / 100.0 } else { 1.0 };
                }
                if a.pen <= 100.0 {
                    a.pen = p.vol.cbrt() * 10.0 * 100.0 * a.pen / 100.0;
                }
            }
        }
        b
    }

    fn attack_t(&self, a: &AttackRaw, part: i32, body: bool) -> AtkT {
        let edge = a.flags.iter().any(|f| f == "EDGE" || f == "ATTACK_FLAG_EDGE");
        let verb = if a.verb.is_empty() { a.name.to_ascii_lowercase() } else { a.verb.clone() };
        let skill = if body {
            let v = verb.as_str();
            match v {
                "bite" => "BITE",
                "kick" | "scratch" | "stomp" => "STANCE_STRIKE",
                _ => "GRASP_STRIKE",
            }
        } else {
            ""
        };
        AtkT {
            verb3: verb3(&verb),
            verb2: verb,
            part,
            contact: a.contact_area as f32,
            pen: a.penetration as f32,
            vel: if a.velocity_multiplier > 0 { a.velocity_multiplier as f32 / 1000.0 } else { 1.0 },
            prep: if a.prepare_ticks > 0 { a.prepare_ticks } else { 3 },
            rec: if a.recover_ticks > 0 { a.recover_ticks } else { 3 },
            edge,
            weight: if a.main { 4 } else { 1 },
            skill: skill.into(),
        }
    }

    fn synthetic_body(&self, race: &str, creature: &str, size: i32, quadruped: bool) -> BodyT {
        let def = if quadruped { SYN_QUADRUPED } else { SYN_HUMANOID };
        let mut parts = Vec::new();
        let mut layers = Vec::new();
        let mut thick = Vec::new();
        for (tok, name, parent, rel, flags, ls) in def {
            let l0 = layers.len() as u16;
            for t in ls.iter() {
                layers.push(layer_from(t, default_mech(t)));
                thick.push(tissue_info(t).2);
            }
            let parent = def.iter().position(|d| d.0 == *parent).map(|p| p as i32).unwrap_or(-1);
            parts.push(PartT {
                token: (*tok).into(),
                name: (*name).into(),
                parent,
                rel: *rel,
                flags: *flags,
                zone: 0,
                vol: 0.0,
                contact: 0.0,
                l0,
                nl: ls.len() as u16,
                internal_children: Vec::new(),
                children: Vec::new(),
            });
        }
        let mut b = finish_body(race, creature, size, parts, layers, Vec::new(), false);
        set_depths(&b.parts, &mut b.layers, &thick);
        // Punch/kick/bite (DF creature_standard: punch with the hand, kick with the foot,
        // bite with the head; contact 100 % of the part; bite is edged, penetration 100 %).
        let find = |b: &BodyT, f: u16| b.parts.iter().position(|p| p.flags & f != 0).map(|p| p as i32);
        let mut atk = Vec::new();
        if !quadruped {
            if let Some(h) = find(&b, bp::GRASP) {
                atk.push(("punch", h, false, 4));
            }
            if let Some(f) = find(&b, bp::STANCE) {
                atk.push(("kick", f, false, 1));
            }
            if let Some(h) = find(&b, bp::HEAD) {
                atk.push(("bite", h, true, 1));
            }
        } else {
            if let Some(h) = find(&b, bp::HEAD) {
                atk.push(("bite", h, true, 4));
            }
            if let Some(f) = find(&b, bp::STANCE) {
                atk.push(("scratch", f, true, 2));
            }
        }
        for (verb, part, edge, weight) in atk {
            let p = &b.parts[part as usize];
            // Teeth/claws are small: 10 % of the part's contact area (estimate).
            let contact = if edge { p.contact * 0.1 } else { p.contact };
            let pen = if edge { p.vol.cbrt() * 10.0 * 100.0 } else { 0.0 };
            b.attacks.push(AtkT {
                verb2: verb.into(),
                verb3: verb3(verb),
                part,
                contact,
                pen,
                vel: 1.0,
                prep: 3,
                rec: 3,
                edge,
                weight,
                skill: match verb {
                    "bite" => "BITE",
                    "kick" | "scratch" => "STANCE_STRIKE",
                    _ => "GRASP_STRIKE",
                }
                .into(),
            });
        }
        b
    }

    /// Body template of a unit (created on first use).
    pub(crate) fn body_of(&mut self, s: usize) -> u16 {
        let uidx = self.ai[s].uidx;
        let (race, caste, animal, usize_) = {
            let u = &self.world.units[uidx];
            (u.race.clone(), u.caste.clone(), u.flags.animal, u.body_size)
        };
        let key = (race.clone(), caste.clone());
        if let Some(&b) = self.combat.body_of.get(&key) {
            return b;
        }
        let raw = self.world.creature_raws.iter().find(|c| c.token == race);
        let creature = raw.map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| race.to_ascii_lowercase().replace('_', " "));
        let body = match raw.and_then(|c| c.castes.iter().find(|k| k.token == caste).or_else(|| c.castes.first())) {
            Some(c) if !c.body_parts.is_empty() => self.build_body_from_caste(&race, &creature, c),
            _ => {
                let quad = animal && race != "DWARF" && race != "HUMAN" && race != "ELF" && race != "GOBLIN";
                let size = if usize_ > 0 {
                    usize_
                } else {
                    match race.as_str() {
                        "DWARF" => 60_000,
                        "GOBLIN" | "HUMAN" | "ELF" => 60_000,
                        "CAT" => 5_000,
                        "DOG" => 30_000,
                        _ => {
                            if quad {
                                20_000
                            } else {
                                60_000
                            }
                        }
                    }
                };
                self.synthetic_body(&race, &creature, size, quad)
            }
        };
        let id = self.combat.bodies.len() as u16;
        self.combat.bodies.push(body);
        self.combat.body_of.insert(key, id);
        id
    }

    pub(crate) fn weapon_t(&mut self, token: &str) -> Option<u16> {
        if let Some(&w) = self.combat.weap_of.get(token) {
            return Some(w);
        }
        let raw = self.world.weapon_raws.iter().find(|w| w.token == token).cloned().or_else(|| default_weapons().into_iter().find(|w| w.token == token))?;
        let mut attacks: Vec<AtkT> = raw.attacks.iter().map(|a| self.attack_t(a, -1, false)).collect();
        if attacks.is_empty() {
            return None;
        }
        // DF soldiers mostly use the weapon's main attack (first in the raws).
        attacks[0].weight = 6;
        for a in &mut attacks {
            a.skill = raw.skill.clone();
        }
        let id = self.combat.weapons.len() as u16;
        self.combat.weapons.push(WeapT {
            name: if raw.name.is_empty() { token.to_ascii_lowercase() } else { raw.name.clone() },
            skill: raw.skill.clone(),
            size: raw.size.max(50) as f32,
            attacks,
        });
        self.combat.weap_of.insert(token.to_string(), id);
        Some(id)
    }

    /// Make sure the fighter state exists for all unit slots (new units, spawned invaders).
    pub(crate) fn combat_sync_units(&mut self) {
        while self.combat.f.len() < self.ai.len() {
            let s = self.combat.f.len();
            self.combat.f.push(Fighter::default());
            self.combat_init_unit(s);
        }
    }

    fn combat_init_unit(&mut self, s: usize) {
        let uidx = self.ai[s].uidx;
        let u = &self.world.units[uidx];
        let alive = self.ai[s].kind != Kind::Dead;
        let hostile = u.flags.hostile && alive;
        let has_wounds = !u.wounds.is_empty();
        let (blood, blood_max, bsize) = (u.blood, u.blood_max, u.body_size);
        if hostile {
            self.combat.f[s].role = Role::Hostile;
            self.combat.hostiles.push(s as u32);
        }
        if alive && (has_wounds || hostile || blood_max > 0) {
            self.ensure_body(s);
        }
        let f = &mut self.combat.f[s];
        if blood_max > 0 {
            f.blood_max = blood_max;
            f.blood = if blood > 0 { blood.min(blood_max) } else { blood_max };
        } else if bsize > 0 {
            f.blood_max = bsize / 10;
            f.blood = f.blood_max;
        }
        if alive && has_wounds {
            self.load_wounds(s);
        }
    }

    /// Body, part states and blood of a unit (lazy).
    pub(crate) fn ensure_body(&mut self, s: usize) {
        if self.combat.f[s].body != u16::MAX {
            return;
        }
        let b = self.body_of(s);
        let (n, size) = {
            let bt = &self.combat.bodies[b as usize];
            (bt.parts.len(), bt.size)
        };
        let uidx = self.ai[s].uidx;
        let usz = self.world.units[uidx].body_size;
        let f = &mut self.combat.f[s];
        f.body = b;
        f.part_state = vec![0; n];
        if f.blood_max <= 0 {
            f.blood_max = (if usz > 0 { usz } else { size }) / 10;
            f.blood = f.blood_max;
        }
    }

    /// Import exported DF wounds (`Unit::wounds`) into the sim's wound list.
    fn load_wounds(&mut self, s: usize) {
        self.ensure_body(s);
        let uidx = self.ai[s].uidx;
        let ws = self.world.units[uidx].wounds.clone();
        let b = self.combat.f[s].body as usize;
        let mut out = Vec::new();
        let mut scars = Vec::new();
        for w in &ws {
            // Scars and long-healed wounds (DF keeps them) are not active wounds.
            let scar = w.flags.iter().any(|f| f.starts_with("SCAR")) || (w.bleeding <= 0 && w.pain <= 0 && w.age > rf_core::TICKS_PER_MONTH);
            if scar {
                scars.push(w.clone());
                continue;
            }
            let body = &self.combat.bodies[b];
            let Some(pi) = body.parts.iter().position(|p| p.token == w.body_part) else { continue };
            let p = &body.parts[pi];
            let layer = (p.l0..p.l0 + p.nl).find(|&l| body.layers[l as usize].tissue.eq_ignore_ascii_case(&w.layer)).unwrap_or(u16::MAX);
            let mut flags = 0;
            for (bit, name) in wf::NAMES {
                if w.flags.iter().any(|f| f == name) {
                    flags |= bit;
                }
            }
            out.push(WoundS { part: pi as u16, layer, flags, sev: w.severity.clamp(0, 10_000), bleed: w.bleeding.max(0), pain: w.pain.max(0), age: w.age });
        }
        let f = &mut self.combat.f[s];
        for w in &out {
            if w.flags & wf::SEVERED != 0 {
                f.part_state[w.part as usize] |= 1;
            }
            if w.flags & wf::BROKEN != 0 {
                f.part_state[w.part as usize] |= 2;
            }
        }
        f.bleed = out.iter().map(|w| w.bleed).sum();
        let active = !out.is_empty();
        f.wounds = out;
        f.scars = scars;
        if active {
            self.list_wounded(s);
            if self.combat.f[s].bleed > 0 {
                self.list_bleeder(s);
            }
        }
    }

    pub(crate) fn init_combat(&mut self) {
        for b in &self.world.buildings {
            if b.kind == "Civzone" && b.subtype.contains("Hospital") {
                self.combat.hospitals.push((b.x1, b.y1, b.x2, b.y2, b.z));
            }
        }
        self.combat_sync_units();
        self.refresh_military();
        self.rebuild_fort_list();
    }

    pub(crate) fn rebuild_fort_list(&mut self) {
        self.combat.fort.clear();
        for (s, ai) in self.ai.iter().enumerate() {
            let fort = match ai.kind {
                Kind::Worker => true,
                Kind::Animal => {
                    let u = &self.world.units[ai.uidx];
                    u.flags.tame && !u.flags.hostile
                }
                _ => false,
            };
            if fort {
                self.combat.fort.push(s as u32);
            }
        }
    }

    // ------------------------------------------------------------------ tick

    /// Per-tick combat bookkeeping (cheap when nothing fights).
    pub(crate) fn combat_tick(&mut self, now: u64) {
        if self.combat.f.len() < self.ai.len() {
            self.combat_sync_units();
            self.rebuild_fort_list();
        }
        if now % 10 == 0 {
            if !self.combat.inv.hidden.is_empty() {
                self.raid_detect(now); // [military2] spotting hidden raiders
            }
            if !self.combat.hostiles.is_empty() {
                self.threat_scan(now);
            }
            if !self.combat.bleeders.is_empty() {
                self.bleed_pass(now);
            }
        }
        if now % 100 == 0 && !self.combat.wounded.is_empty() {
            self.heal_pass(now);
        }
        if now % 600 == 0 {
            self.rebuild_fort_list();
        }
        if now % rf_core::TICKS_PER_DAY as u64 == 0 {
            self.invasion_daily(now);
        }
    }

    pub(crate) fn alive(&self, s: u32) -> bool {
        s != NONE && (s as usize) < self.ai.len() && self.ai[s as usize].kind != Kind::Dead
    }

    pub(crate) fn upos(&self, s: usize) -> Pos {
        self.world.pos_of(self.ai[s].pos as usize)
    }

    /// Display name for reports: the unit's name, else "the goblin" (with profession for invaders).
    pub(crate) fn uname(&self, s: usize) -> String {
        let u = &self.world.units[self.ai[s].uidx];
        if !u.name.is_empty() {
            return u.name.clone();
        }
        let creature = self
            .world
            .creature_raws
            .iter()
            .find(|c| c.token == u.race)
            .map(|c| c.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| u.race.to_ascii_lowercase().replace('_', " "));
        if !u.custom_profession.is_empty() {
            format!("the {creature} {}", u.custom_profession)
        } else {
            format!("the {creature}")
        }
    }

    fn pronoun(&self, s: usize) -> &'static str {
        match self.world.units[self.ai[s].uidx].sex {
            0 => "her",
            1 => "his",
            _ => "its",
        }
    }

    fn attr(&self, s: usize, name: &str, default: i32) -> i32 {
        let u = &self.world.units[self.ai[s].uidx];
        u.phys_attrs.iter().chain(u.ment_attrs.iter()).find(|(n, _)| n == name).map(|(_, v)| *v).unwrap_or(default)
    }

    pub(crate) fn can_fight(&self, s: usize, now: u64) -> bool {
        let f = &self.combat.f[s];
        self.ai[s].kind != Kind::Dead && f.uncon_until <= now && !f.paralyzed
    }

    /// Wielded weapon: (item index, weapon template).
    pub(crate) fn wielded(&mut self, s: usize) -> Option<(usize, u16)> {
        if s == self.def.trap_unit {
            return self.def.trap_weapon; // [defense] weapon traps, falling stones, upright spikes
        }
        let uidx = self.ai[s].uidx;
        let inv = self.world.units[uidx].inventory.clone();
        for id in inv {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if self.imeta[i].dead || self.world.items[i].kind != "WEAPON" {
                continue;
            }
            let sub = self.world.items[i].subtype.clone();
            if let Some(w) = self.weapon_t(&sub) {
                return Some((i, w));
            }
        }
        None
    }

    fn shield_of(&self, s: usize) -> Option<usize> {
        let uidx = self.ai[s].uidx;
        for &id in &self.world.units[uidx].inventory {
            if let Some(&i) = self.item_idx.get(&id) {
                if !self.imeta[i as usize].dead && self.world.items[i as usize].kind == "SHIELD" {
                    return Some(i as usize);
                }
            }
        }
        None
    }

    /// Armor (item index, layer order, coverage, layer size) worn over zone `z`, outermost first.
    fn armor_over(&self, s: usize, z: u16, out: &mut Vec<(usize, u8, i32)>) {
        out.clear();
        let uidx = self.ai[s].uidx;
        for &id in &self.world.units[uidx].inventory {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let it = &self.world.items[i as usize];
            if self.imeta[i as usize].dead || !is_armor_kind(&it.kind) {
                continue;
            }
            let (layer, coverage) = match self.world.armor_raws.iter().find(|a| a.token == it.subtype) {
                Some(a) => (a.layer.clone(), a.coverage),
                None => match DEFAULT_ARMOR.iter().find(|a| a.0 == it.subtype) {
                    Some(a) => (a.2.to_string(), a.3),
                    None => ("OVER".to_string(), 100),
                },
            };
            let sub = it.subtype.as_str();
            let covers = match it.kind.as_str() {
                "ARMOR" => {
                    let arms = sub.contains("MAIL") || sub.contains("COAT") || sub.contains("ROBE") || sub.contains("SHIRT") || sub.contains("LEATHER") || sub.contains("CLOAK");
                    // UBSTEP 1 (mail shirt, coat, ...) also covers the neck and upper arms.
                    zone::UB | zone::LB | if arms { zone::ARM_UP | zone::NECK } else { 0 } | if sub.contains("COAT") || sub.contains("ROBE") { zone::ARM_LOW | zone::LEG_UP } else { 0 }
                }
                "HELM" => zone::HEAD,
                "PANTS" => zone::LB | zone::LEG_UP | zone::LEG_LOW,
                "GLOVES" => zone::HAND,
                "SHOES" => zone::FOOT,
                _ => 0,
            };
            if covers & z == 0 {
                continue;
            }
            let order = match layer.as_str() {
                "COVER" => 0,
                "OVER" => 1,
                "ARMOR" => 2,
                _ => 3,
            };
            out.push((i as usize, order, coverage.max(1)));
        }
        out.sort_by_key(|&(i, o, _)| (o, i));
    }

    // ------------------------------------------------------------------ attack

    /// One attack of slot `a` on slot `d` (adjacent). Returns true if `d` died.
    pub(crate) fn attack(&mut self, a: usize, d: usize, now: u64) -> bool {
        if self.combat.f[a].hidden {
            self.raid_reveal(a, now); // [military2] an ambusher strikes: revealed
        }
        self.ensure_body(a);
        self.ensure_body(d);
        self.combat.stats.attacks += 1;
        let mut r = Rng::for_unit(self.seed, a, now, 0xC0B4);
        // --- choose the attack
        let weapon = if self.grasp_ok(a) { self.wielded(a) } else { None };
        let (atk, wmech, wsize, wquality, wdesc) = match weapon {
            Some((wi, w)) => {
                let wt = &self.combat.weapons[w as usize];
                let total: u32 = wt.attacks.iter().map(|x| x.weight).sum::<u32>().max(1);
                let mut roll = r.below(total);
                let mut pick = 0;
                for (k, x) in wt.attacks.iter().enumerate() {
                    if roll < x.weight {
                        pick = k;
                        break;
                    }
                    roll -= x.weight;
                }
                let atk = wt.attacks[pick].clone();
                let size = wt.size;
                let wname = wt.name.clone();
                let it = &self.world.items[wi];
                let mat = it.mat;
                let q = it.quality.min(5) as f32;
                let matname = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default();
                let desc = if matname.is_empty() { wname } else { format!("{matname} {wname}") };
                (atk, self.mech_of_mat(mat), size, q, Some(desc))
            }
            None => {
                let b = &self.combat.bodies[self.combat.f[a].body as usize];
                let ps = &self.combat.f[a].part_state;
                let usable: Vec<&AtkT> = b.attacks.iter().filter(|x| x.part < 0 || ps.get(x.part as usize).copied().unwrap_or(0) & 3 == 0).collect();
                if usable.is_empty() {
                    return false;
                }
                let total: u32 = usable.iter().map(|x| x.weight).sum::<u32>().max(1);
                let mut roll = r.below(total);
                let mut pick = 0;
                for (k, x) in usable.iter().enumerate() {
                    if roll < x.weight {
                        pick = k;
                        break;
                    }
                    roll -= x.weight;
                }
                let atk = usable[pick].clone();
                let p = &b.parts[atk.part as usize];
                // The attacking part acts as the weapon: its hardest tissue, its volume.
                let mut m = SOFT;
                let mut best = 0.0f32;
                for l in p.l0..p.l0 + p.nl {
                    let lm = b.layers[l as usize].mech;
                    if lm.sy > best {
                        best = lm.sy;
                        m = lm;
                    }
                }
                if atk.edge && m.edge <= 0.0 {
                    m = default_mech("TOOTH");
                }
                (atk, m, p.vol, 0.0, None)
            }
        };
        let a_skill_name = if atk.skill.is_empty() { "WRESTLING".to_string() } else { atk.skill.clone() };
        let a_skill = self.world.units[self.ai[a].uidx].skill(&a_skill_name);
        let an = self.uname(a);
        let dn = self.uname(d);
        // --- hit, dodge, block, parry (estimates)
        let d_ok = self.can_fight(d, now);
        let miss = (15 - a_skill).clamp(2, 15) as u32;
        if r.below(100) < miss {
            self.combat.stats.misses += 1;
            self.add_xp(a, &a_skill_name, 5);
            self.combat.log_line(format!("{} {} at {} but misses!", cap(&an), atk.verb3, dn));
            self.after_attacked(d, a, now);
            return false;
        }
        if d_ok {
            let du = &self.world.units[self.ai[d].uidx];
            let dodge = du.skill("DODGING");
            let agi = self.attr(d, "AGILITY", 1000);
            let pd = (15 + 3 * (dodge - a_skill / 2) + (agi - 1000) / 100).clamp(0, 60) as u32;
            if r.below(100) < pd {
                self.combat.stats.dodges += 1;
                self.add_xp(d, "DODGING", 10);
                self.add_xp(a, &a_skill_name, 5);
                self.combat.log_line(format!("{} {} at {}, but {} jumps away!", cap(&an), atk.verb3, dn, dn));
                self.after_attacked(d, a, now);
                return false;
            }
            if let Some(si) = self.shield_of(d) {
                let sk = self.world.units[self.ai[d].uidx].skill("SHIELD");
                let pb = (20 + 4 * (sk - a_skill / 2)).clamp(5, 60) as u32;
                if r.below(100) < pb {
                    self.combat.stats.blocks += 1;
                    self.add_xp(d, "SHIELD", 10);
                    let sname = self.world.items[si].desc.clone();
                    let sname = if sname.is_empty() { "shield".into() } else { sname };
                    self.combat.log_line(format!("{} blocks {}'s attack with {} {}!", cap(&dn), an, self.pronoun(d), sname));
                    self.after_attacked(d, a, now);
                    return false;
                }
            }
            if let Some((wi, w)) = self.wielded(d) {
                let wskill = self.combat.weapons[w as usize].skill.clone();
                let sk = self.world.units[self.ai[d].uidx].skill(&wskill);
                let pp = (10 + 3 * (sk - a_skill / 2)).clamp(0, 40) as u32;
                if r.below(100) < pp {
                    self.combat.stats.parries += 1;
                    self.add_xp(d, &wskill, 10);
                    let wname = self.world.items[wi].desc.clone();
                    self.combat.log_line(format!("{} parries {}'s attack with {} {}!", cap(&dn), an, self.pronoun(d), if wname.is_empty() { "weapon".into() } else { wname }));
                    self.after_attacked(d, a, now);
                    return false;
                }
            }
        }
        self.combat.stats.hits += 1;
        self.add_xp(a, &a_skill_name, 10);
        self.add_xp(a, "MELEE_COMBAT", 3);
        self.strike(a, d, now, &mut r, Blow { atk, armed: weapon.is_some(), wmech, wsize, wquality, wdesc, skill: a_skill, an, dn, momentum: None }) // [ranged]
    }

    /// [ranged] Resolve a blow that landed (melee or projectile): hit location, momentum, armor
    /// and tissue layers, wounds and death. Returns true if `d` died.
    pub(crate) fn strike(&mut self, a: usize, d: usize, now: u64, r: &mut Rng, b: Blow) -> bool {
        let Blow { atk, armed, wmech, wsize, wquality, wdesc, skill: a_skill, an, dn, momentum } = b; // [ranged]
        // --- hit location
        let db = self.combat.f[d].body as usize;
        let mut part = u16::MAX;
        for _ in 0..6 {
            let t = &self.combat.bodies[db].targets;
            if t.is_empty() {
                break;
            }
            let total = t.last().unwrap().1;
            let roll = r.below(total.max(1));
            let k = t.partition_point(|&(_, c)| c <= roll);
            let p = t[k.min(t.len() - 1)].0;
            if self.combat.f[d].part_state[p as usize] & 1 == 0 {
                part = p;
                break;
            }
        }
        if part == u16::MAX {
            return false;
        }
        // --- momentum (wiki formula 3)
        let ab = &self.combat.bodies[self.combat.f[a].body as usize];
        let isize_ = {
            let u = &self.world.units[self.ai[a].uidx];
            if u.body_size > 0 {
                u.body_size as f32
            } else {
                ab.size as f32
            }
        };
        let str_ = self.attr(a, "STRENGTH", 1000) as f32;
        let wmass = (wmech.dens.max(100.0) * wsize.max(1.0)).max(1.0);
        let mut m = isize_ * str_ * atk.vel / (1e6 * (1.0 + isize_ / wmass));
        m *= (60 + r.below(41)) as f32 / 100.0 * (1.0 + 0.03 * a_skill.clamp(0, 20) as f32);
        if let Some(pm) = momentum {
            m = pm; // [ranged] projectile momentum (launcher force, ammo weight)
        }
        let qw = 1.0 + 0.2 * wquality;
        let sharp = (wmech.edge / 10_000.0).max(0.05);
        // --- armor layers
        let pz = self.combat.bodies[db].parts[part as usize].zone;
        let pcontact = self.combat.bodies[db].parts[part as usize].contact;
        let contact = atk.contact.max(1.0).min(pcontact);
        let mut edge = atk.edge;
        let mut armor = Vec::new();
        self.armor_over(d, pz, &mut armor);
        let mut stopped_by: Option<usize> = None;
        let mut armor_hit = false;
        // Set when rigid armor absorbed a blow: the rest of the force is spread over the part.
        let mut spread = false;
        for &(ai_, _, cov) in &armor {
            if r.below(100) >= cov as u32 {
                continue;
            }
            armor_hit = true;
            let it = &self.world.items[ai_];
            let am = self.mech_of_mat(it.mat);
            // Quality multiplier: ordinary 1, +0.2 per level (estimate).
            let qa = 1.0 + 0.2 * it.quality.min(5) as f32;
            let aa = contact / 1000.0;
            if edge {
                let rsy = am.sy / wmech.sy.max(1.0);
                let rsf = am.sf / wmech.sf.max(1.0);
                let cost = (rsy + (aa + 1.0) * rsf) * (10.0 + 2.0 * qa) / (sharp * qw);
                // An armor plate presents at least A = 2 to a point or edge (estimate).
                let cost = cost.max((rsy + 3.0 * rsf) * (10.0 + 2.0 * qa) / (sharp * qw));
                if rsy < 2.0 && m >= cost {
                    m *= 0.95;
                    continue;
                }
                m *= (am.ssay / 50_000.0).min(1.0);
                edge = false;
                spread = true;
            } else {
                // Blunt deflection: 2 * Sw * IYw < A * Da (MPa, g/cm³).
                if armed && 2.0 * wsize * (wmech.iy / 1000.0) < aa * (am.dens / 1000.0) {
                    m = 0.0;
                } else {
                    // Rigid armor spreads a blow over at least ~1000 mm² (estimate, A >= 1).
                    let cost = (2.0 * am.ifr - am.iy) / 1000.0 * (2.0 + 0.4 * qa) * aa.max(1.0);
                    if m >= cost {
                        m *= 0.95;
                        continue;
                    }
                    m *= (am.isay / 50_000.0).min(1.0);
                    spread = true;
                }
            }
            if m < 0.5 {
                stopped_by = Some(ai_);
                break;
            }
        }
        if armor_hit {
            self.add_xp(d, "ARMOR", 5);
        }
        let loc = self.combat.bodies[db].parts[part as usize].name.clone();
        let with = match &wdesc {
            Some(w) => format!(" with {} {}", self.pronoun(a), w),
            None => String::new(),
        };
        if let Some(si) = stopped_by.or(if m < 0.5 && armor_hit { armor.first().map(|x| x.0) } else { None }) {
            self.combat.stats.deflected += 1;
            let it = &self.world.items[si];
            let mat = self.world.material(it.mat).map(|x| x.name.clone()).unwrap_or_default();
            let what = if it.desc.is_empty() { format!("{mat} {}", it.subtype.trim_start_matches("ITEM_").to_ascii_lowercase()) } else { it.desc.clone() };
            self.combat.log_line(format!("{} {} {} in the {}{}, but the attack is deflected by {}'s {}!", cap(&an), atk.verb3, dn, loc, with, dn, what));
            self.after_attacked(d, a, now);
            return false;
        }
        // --- tissue layers (and internal organs)
        let mut pen_left = atk.pen / 100.0;
        let mut results: Vec<String> = Vec::new();
        let mut cur = part;
        let outer = part;
        let mut died: Option<&'static str> = None;
        let mut severed_part: Option<u16> = None;
        let mut depth = 0;
        let mut bruise_brain = false;
        let mut wounded = false; // [perf10]
        loop {
            depth += 1;
            let (l0, nl, pvol, pflags) = {
                let p = &self.combat.bodies[db].parts[cur as usize];
                (p.l0, p.nl, p.vol, p.flags)
            };
            let pc = self.combat.bodies[db].parts[cur as usize].contact;
            // Blunt force reaching an internal organ spreads over the whole organ (estimate).
            let cur_bone = nl > 0 && (l0..l0 + nl).all(|l| self.combat.bodies[db].layers[l as usize].rigid);
            let c = if (depth > 1 && !edge && !cur_bone) || spread { pc } else { atk.contact.max(1.0).min(pc) };
            let mut all_cut = nl > 0;
            let mut through = true;
            for l in l0..l0 + nl {
                if m < 0.05 {
                    through = false;
                    all_cut = false;
                    break;
                }
                let lt = self.combat.bodies[db].layers[l as usize].clone();
                let mut flags = 0u16;
                let mut sev = 0i32;
                if edge {
                    if pen_left <= 0.0 && depth > 1 {
                        through = false;
                        all_cut = false;
                        break;
                    }
                    let aa = c / 1000.0;
                    let rsy = lt.mech.sy / wmech.sy.max(1.0);
                    let rsf = lt.mech.sf / wmech.sf.max(1.0);
                    let cost = (rsy + (aa + 1.0) * rsf) * 10.0 / (sharp * qw);
                    if m >= cost && pen_left > 0.0 {
                        m *= 0.95;
                        pen_left -= lt.depth;
                        flags = wf::CUT;
                        sev = (10_000.0 * (c / pc).clamp(0.2, 1.0)) as i32;
                        if !lt.hair {
                            results.push(if lt.rigid { format!("cutting the {}", lt.tissue) } else if sev >= 10_000 { format!("~tearing apart the {}", lt.tissue) } else { format!("~tearing the {}", lt.tissue) });
                        }
                    } else {
                        all_cut = false;
                        m *= (lt.mech.ssay / 50_000.0).min(1.0);
                        edge = false;
                    }
                }
                if !edge && flags == 0 {
                    all_cut = false;
                    let ab_ = c / 1000.0;
                    let cost = (2.0 * lt.mech.ifr - lt.mech.iy) / 1000.0 * 2.0 * ab_;
                    if m >= cost {
                        m *= 0.95;
                        flags = if lt.rigid { wf::BROKEN } else { wf::TORN };
                        sev = 10_000;
                        if !lt.hair {
                            results.push(if lt.rigid { format!("fracturing the {}", lt.tissue) } else { format!("~tearing the {}", lt.tissue) });
                        }
                    } else {
                        let k = (lt.mech.isay / 50_000.0).min(1.0);
                        if k >= 0.5 {
                            flags = wf::BRUISED;
                            sev = ((m / cost.max(0.01)) * 3000.0).clamp(100.0, 4000.0) as i32;
                            if !lt.hair {
                                results.push(format!("~bruising the {}", lt.tissue));
                            }
                        } else {
                            flags = wf::BRUISED;
                            sev = ((m / cost.max(0.01)) * 2000.0).clamp(50.0, 2000.0) as i32;
                            if !lt.hair {
                                results.push(format!("~bruising the {}", lt.tissue));
                            }
                            through = false;
                        }
                        m *= k;
                    }
                }
                if flags != 0 && !lt.hair {
                    if pflags & bp::THOUGHT != 0 {
                        if flags & (wf::CUT | wf::TORN) != 0 {
                            died = Some("brain");
                        } else {
                            bruise_brain = true;
                        }
                    }
                    if pflags & bp::CIRC != 0 && flags & (wf::CUT | wf::TORN) != 0 {
                        died = died.or(Some("heart"));
                    }
                    self.add_wound(d, cur, l, flags, sev, pvol, &lt, now);
                    wounded = true; // [perf10]
                    if flags & wf::BROKEN != 0 {
                        self.combat.f[d].part_state[cur as usize] |= 2;
                    }
                    if pflags & bp::NERVOUS != 0 && pflags & bp::THOUGHT == 0 && flags & (wf::CUT | wf::TORN) != 0 {
                        self.combat.f[d].paralyzed = true;
                        results.push("and the nervous tissue is torn".into());
                    }
                }
                if !through {
                    break;
                }
            }
            // Severing (limbs, head, neck; not the torso).
            let p = &self.combat.bodies[db].parts[cur as usize];
            if depth == 1 && all_cut && pflags & (bp::UPPER | bp::LOWER | bp::INTERNAL) == 0 && atk.contact >= 0.5 * p.contact && r.chance(1, 2) {
                severed_part = Some(cur);
                break;
            }
            if !through || m < 0.05 || depth >= 3 {
                break;
            }
            // Into the inside of the outer part: first maybe a protecting bone (skull, ribs:
            // internal parts made of rigid tissue only), then an organ (weighted by relsize).
            let bodyp = &self.combat.bodies[db];
            let kids = &bodyp.parts[outer as usize].internal_children;
            if kids.is_empty() {
                break;
            }
            let is_bone = |k: u16| {
                let p = &bodyp.parts[k as usize];
                p.nl > 0 && p.flags & bp::NERVOUS == 0 && (p.l0..p.l0 + p.nl).all(|l| bodyp.layers[l as usize].rigid)
            };
            let prel = bodyp.parts[outer as usize].rel.max(1) as u32;
            let want_bone = depth == 1;
            let pick: Vec<u16> = kids.iter().copied().filter(|&k| is_bone(k) == want_bone && self.combat.f[d].part_state[k as usize] & 1 == 0).collect();
            let ks: u32 = pick.iter().map(|&k| bodyp.parts[k as usize].rel.max(1) as u32).sum();
            let mut next = u16::MAX;
            if ks > 0 {
                let roll = r.below(ks.max(prel));
                let mut acc = 0;
                for &k in &pick {
                    acc += bodyp.parts[k as usize].rel.max(1) as u32;
                    if roll < acc {
                        next = k;
                        break;
                    }
                }
            }
            if next == u16::MAX && want_bone {
                // Missed the bones: straight into an organ.
                let organs: Vec<u16> = kids.iter().copied().filter(|&k| !is_bone(k) && self.combat.f[d].part_state[k as usize] & 1 == 0).collect();
                let os: u32 = organs.iter().map(|&k| bodyp.parts[k as usize].rel.max(1) as u32).sum();
                if os > 0 {
                    let roll = r.below(os.max(prel));
                    let mut acc = 0;
                    for &k in &organs {
                        acc += bodyp.parts[k as usize].rel.max(1) as u32;
                        if roll < acc {
                            next = k;
                            depth += 1;
                            break;
                        }
                    }
                }
            }
            if next == u16::MAX {
                break;
            }
            let oname = self.combat.bodies[db].parts[next as usize].name.clone();
            results.push(format!("into the {oname}"));
            cur = next;
            if !edge {
                // Blunt force reaching an organ spreads out.
                m *= 0.5;
            }
        }
        if wounded {
            self.sync_wounds(d); // [perf10] once, after the last wound of this blow
        }
        let mut tail = String::new();
        if let Some(sp) = severed_part {
            self.sever(d, sp, now);
            tail = " and the severed part sails off in an arc!".into();
            let pf = self.combat.bodies[db].parts[sp as usize].flags;
            if pf & bp::HEAD != 0 || self.subtree_has(db, sp, bp::THOUGHT) {
                died = Some("head");
            }
        }
        // DF reports the deepest soft-tissue effect of each run, plus bones and organs.
        let mut shown: Vec<&str> = Vec::new();
        for (k, t) in results.iter().enumerate() {
            let soft_next = results.get(k + 1).map(|n| n.starts_with('~')).unwrap_or(false);
            if t.starts_with('~') && soft_next {
                continue;
            }
            shown.push(t.trim_start_matches('~'));
        }
        let res = if shown.is_empty() { "but it has no effect".to_string() } else { shown.join(" and ").replace("and into", "into").replace("and and", "and") };
        let end = if tail.is_empty() { "!".to_string() } else { tail };
        self.combat.log_line(format!("{} {} {} in the {}{}, {}{}", cap(&an), atk.verb3, dn, loc, with, res, if end == "!" { end } else { format!(",{end}") }.replace(",,", ",").replace(", and the severed", " and the severed")));
        if bruise_brain && died.is_none() {
            let until = now + 100 + r.below(200) as u64;
            self.knock_out(d, until, "has been knocked unconscious");
        }
        if let Some(why) = died {
            let text = match why {
                "brain" | "head" => "has been struck down",
                _ => "has been struck down",
            };
            self.combat_kill(d, now, text);
            self.combat.stats.kills += 1;
            return true;
        }
        self.check_pain(d, now);
        self.after_attacked(d, a, now);
        false
    }

    /// At least one grasping part whose limb chain is intact (no severed part, no broken bone).
    pub(crate) fn grasp_ok(&self, s: usize) -> bool {
        let f = &self.combat.f[s];
        if f.body == u16::MAX {
            return true;
        }
        let b = &self.combat.bodies[f.body as usize];
        let mut any = false;
        for (i, p) in b.parts.iter().enumerate() {
            if p.flags & bp::GRASP == 0 {
                continue;
            }
            any = true;
            let mut k = i as i32;
            let mut ok = true;
            while k >= 0 {
                let kp = &b.parts[k as usize];
                if f.part_state[k as usize] & 3 != 0 {
                    ok = false;
                    break;
                }
                if kp.flags & (bp::UPPER | bp::LOWER) != 0 {
                    break;
                }
                k = kp.parent;
            }
            if ok {
                return true;
            }
        }
        !any
    }

    fn subtree_has(&self, b: usize, p: u16, flag: u16) -> bool {
        let body = &self.combat.bodies[b];
        let part = &body.parts[p as usize];
        if part.flags & flag != 0 {
            return true;
        }
        part.children.iter().chain(part.internal_children.iter()).any(|&c| self.subtree_has(b, c, flag))
    }

    fn add_wound(&mut self, d: usize, part: u16, layer: u16, flags: u16, sev: i32, pvol: f32, lt: &LayerT, _now: u64) {
        self.combat.stats.wounds += 1;
        let mut bleed = 0;
        let mut fl = flags;
        if flags & (wf::CUT | wf::TORN) != 0 && lt.vascular > 0 {
            // Estimate: blood per 10 ticks = vascular * volume/2000 * damage fraction.
            bleed = (lt.vascular as f32 * pvol / 2000.0 * sev as f32 / 10_000.0).max(1.0) as i32;
            if lt.arteries {
                bleed *= 2;
                fl |= wf::ARTERY;
            }
            fl |= wf::BLEEDING;
        }
        let pain = lt.pain * sev / 10_000 * if flags & wf::BROKEN != 0 { 2 } else { 1 };
        let f = &mut self.combat.f[d];
        match f.wounds.iter_mut().find(|w| w.part == part && w.layer == layer) {
            Some(w) => {
                w.flags |= fl;
                w.sev = (w.sev + sev).min(10_000);
                w.bleed += bleed;
                w.pain = w.pain.max(pain);
                w.age = 0;
            }
            None => f.wounds.push(WoundS { part, layer, flags: fl, sev: sev.min(10_000), bleed, pain, age: 0 }),
        }
        f.bleed += bleed;
        self.list_wounded(d);
        if bleed > 0 {
            self.list_bleeder(d);
        }
        // [perf10] `strike` mirrors the wounds into `Unit::wounds` once after its last wound
        // (rebuilding that list per tissue layer was most of a big siege's think time).
    }

    fn sever(&mut self, d: usize, part: u16, now: u64) {
        self.combat.stats.severed += 1;
        let db = self.combat.f[d].body as usize;
        let mut stack = vec![part];
        while let Some(p) = stack.pop() {
            self.combat.f[d].part_state[p as usize] |= 1;
            let pt = &self.combat.bodies[db].parts[p as usize];
            stack.extend(pt.children.iter().copied());
            stack.extend(pt.internal_children.iter().copied());
        }
        let (vol, name) = {
            let p = &self.combat.bodies[db].parts[part as usize];
            (p.vol, p.name.clone())
        };
        let bleed = (vol / 40.0).max(3.0) as i32;
        let f = &mut self.combat.f[d];
        f.wounds.push(WoundS { part, layer: u16::MAX, flags: wf::SEVERED | wf::BLEEDING, sev: 10_000, bleed, pain: 50, age: 0 });
        f.bleed += bleed;
        self.list_wounded(d);
        self.list_bleeder(d);
        self.sync_wounds(d);
        // The severed part lies on the ground (DF CORPSEPIECE).
        let tile = self.ai[d].pos as usize;
        let who = self.uname(d);
        let mat = self.corpse_mat(d);
        let id = self.create_item("CORPSEPIECE", mat, tile, format!("{who}'s {name}"));
        let _ = (id, now);
    }

    pub(crate) fn corpse_mat(&self, s: usize) -> u16 {
        let race = &self.world.units[self.ai[s].uidx].race;
        let pre = format!("CREATURE:{race}:");
        self.world
            .materials
            .iter()
            .position(|m| m.token.starts_with(&pre))
            .or_else(|| self.world.materials.iter().position(|m| m.token.starts_with("CREATURE:")))
            .map(|m| m as u16)
            .unwrap_or(NO_MAT)
    }

    pub(crate) fn list_bleeder(&mut self, s: usize) {
        if !self.combat.f[s].listed_bleed {
            self.combat.f[s].listed_bleed = true;
            let v = &mut self.combat.bleeders;
            let p = v.partition_point(|&x| x < s as u32);
            v.insert(p, s as u32);
        }
    }

    pub(crate) fn list_wounded(&mut self, s: usize) {
        if !self.combat.f[s].listed_wound {
            self.combat.f[s].listed_wound = true;
            let v = &mut self.combat.wounded;
            let p = v.partition_point(|&x| x < s as u32);
            v.insert(p, s as u32);
        }
    }

    /// Mirror the sim's wounds into `Unit::wounds` / `blood` (for the UI, saves and exports).
    pub(crate) fn sync_wounds(&mut self, s: usize) {
        let f = &self.combat.f[s];
        let b = f.body;
        let mut out = f.scars.clone();
        if b != u16::MAX {
            let body = &self.combat.bodies[b as usize];
            for w in &f.wounds {
                let p = &body.parts[w.part as usize];
                out.push(Wound {
                    body_part: p.token.clone(),
                    layer: if w.layer == u16::MAX { String::new() } else { body.layers[w.layer as usize].tissue.to_ascii_uppercase() },
                    flags: wf::NAMES.iter().filter(|(bit, _)| w.flags & bit != 0).map(|(_, n)| n.to_string()).collect(),
                    severity: w.sev,
                    pain: w.pain,
                    bleeding: w.bleed,
                    age: w.age,
                });
            }
        }
        let (blood, bmax) = (f.blood, f.blood_max);
        let u = &mut self.world.units[self.ai[s].uidx];
        u.wounds = out;
        u.blood = blood;
        u.blood_max = bmax;
        self.health_sync_names(s); // [moods]
    }

    fn knock_out(&mut self, s: usize, until: u64, why: &str) {
        if self.ai[s].kind == Kind::Dead {
            return;
        }
        let was = self.combat.f[s].uncon_until;
        self.combat.f[s].uncon_until = was.max(until);
        if self.combat.f[s].carry != NONE {
            let now = self.ticks;
            self.carry_unlink(s, now); // [military2] a carrier that passes out drops its load
        }
        if was <= self.ticks {
            self.combat.stats.unconscious += 1;
            let n = self.uname(s);
            self.combat.log_line(format!("{} {why}!", cap(&n)));
        }
    }

    fn pain_of(&self, s: usize) -> i32 {
        self.combat.f[s].wounds.iter().map(|w| w.pain).sum()
    }

    pub(crate) fn check_pain(&mut self, s: usize, now: u64) {
        let b = self.combat.f[s].body;
        if b == u16::MAX || self.combat.bodies[b as usize].nopain {
            return;
        }
        let pain = self.pain_of(s);
        let thr = pain_threshold(self.attr(s, "WILLPOWER", 1000));
        if pain > thr {
            self.knock_out(s, now + 200, "gives in to pain");
        }
    }

    /// Reaction of a unit that was attacked (flee, fight back, wake up).
    pub(crate) fn after_attacked(&mut self, d: usize, a: usize, now: u64) {
        if self.ai[d].kind == Kind::Dead {
            return;
        }
        let f = &mut self.combat.f[d];
        match f.role {
            Role::Hostile | Role::Fighter | Role::Soldier => {
                if f.target == NONE || !matches!(f.role, Role::Soldier) {
                    if f.target == NONE {
                        f.target = a as u32;
                    }
                }
            }
            Role::Civilian => {
                f.threat = a as u32;
            }
            Role::Leave => {}
            Role::None | Role::Patient => {
                if self.ai[d].kind == Kind::Worker {
                    f.role = Role::Civilian;
                    f.threat = a as u32;
                    f.calm_since = now;
                } else {
                    f.role = Role::Fighter;
                    f.target = a as u32;
                }
            }
        }
        if self.ai[d].task != Task::Combat {
            self.cancel_task(d);
            self.ai[d].task = Task::Combat;
            self.ai[d].wake = now + 1;
        }
    }

    /// Kill a unit in combat: drop its gear, leave a corpse, report.
    pub(crate) fn combat_kill(&mut self, s: usize, now: u64, why: &str) {
        if self.ai[s].kind == Kind::Dead {
            return;
        }
        let name = self.uname(s);
        self.carry_unlink(s, now); // [military2] drop a carried unit / let go of the carrier
        self.drop_gear(s);
        self.release_rest(s);
        let fort = self.ai[s].kind == Kind::Worker;
        // Give the corpse the creature's own material.
        let uidx = self.ai[s].uidx;
        let display = self.world.units[uidx].name.clone();
        if display.is_empty() {
            self.world.units[uidx].name = cap(&name.trim_start_matches("the ").to_string());
        }
        self.kill(s, now, why);
        if display.is_empty() {
            self.world.units[uidx].name = display;
        }
        if let Some(c) = self.world.items.last_mut() {
            if c.kind == "CORPSE" {
                c.desc = format!("{}'s corpse", name.trim_start_matches("the "));
            }
        }
        let cm = self.corpse_mat(s);
        if let Some(c) = self.world.items.last_mut() {
            if c.kind == "CORPSE" && cm != NO_MAT {
                c.mat = cm;
            }
        }
        self.combat.log_line(format!("{} {why}.", cap(&name)));
        if fort {
            self.combat.stats.fort_deaths += 1;
        }
        let f = &mut self.combat.f[s];
        let siege = f.siege;
        f.role = Role::None;
        f.target = NONE;
        f.bleed = 0;
        if let Ok(p) = self.combat.hostiles.binary_search(&(s as u32)) {
            self.combat.hostiles.remove(p);
        }
        if siege > 0 {
            self.combat.stats.invaders_killed += 1;
        }
        self.sync_wounds(s);
    }

    /// Drop everything a unit carries or wears onto its tile.
    pub(crate) fn drop_gear(&mut self, s: usize) {
        let uidx = self.ai[s].uidx;
        let inv = std::mem::take(&mut self.world.units[uidx].inventory);
        let pos = self.world.units[uidx].pos;
        let tile = self.ai[s].pos;
        self.ai[s].carrying = -1;
        for id in inv {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if self.imeta[i].dead {
                continue;
            }
            let it = &mut self.world.items[i];
            it.holder = -1;
            it.flags.in_inventory = false;
            it.flags.on_ground = true;
            it.pos = pos;
            self.imeta[i].hauling = false;
            self.imeta[i].reserved = 0;
            self.pile_occ_add(tile, 1);
            if !self.is_stored(i) {
                self.haul_add(id);
            }
        }
    }

    // ------------------------------------------------------------------ bleeding and healing

    fn bleed_pass(&mut self, now: u64) {
        let list = std::mem::take(&mut self.combat.bleeders);
        let mut keep = Vec::with_capacity(list.len());
        for &s in &list {
            let s = s as usize;
            if self.ai[s].kind == Kind::Dead {
                self.combat.f[s].listed_bleed = false;
                continue;
            }
            let f = &mut self.combat.f[s];
            f.blood -= f.bleed;
            let (blood, bmax, bleed) = (f.blood, f.blood_max, f.bleed);
            if blood <= 0 {
                self.combat.f[s].listed_bleed = false;
                self.combat.stats.bled_out += 1;
                self.combat_kill(s, now, "has bled to death");
                continue;
            }
            if blood * 2 < bmax {
                // Wiki Blood: "Faint" below 50 %.
                self.knock_out(s, now + 20, "passes out from loss of blood");
            }
            if bleed <= 0 {
                self.combat.f[s].listed_bleed = false;
                continue;
            }
            keep.push(s as u32);
        }
        self.combat.bleeders = keep;
    }

    pub(crate) fn resting(&self, s: usize) -> bool {
        matches!(self.combat.f[s].sub, Sub::Resting(_)) || matches!(self.ai[s].task, Task::Sleep(_))
    }

    fn heal_pass(&mut self, now: u64) {
        let list = std::mem::take(&mut self.combat.wounded);
        let mut keep = Vec::with_capacity(list.len());
        for &s in &list {
            let s = s as usize;
            if self.ai[s].kind == Kind::Dead {
                self.combat.f[s].listed_wound = false;
                continue;
            }
            let rest = self.resting(s);
            let in_hosp = rest && self.in_hospital(s);
            let f = &mut self.combat.f[s];
            let mut bleed = 0;
            for w in &mut f.wounds {
                w.age += 100;
                // Clotting (estimate): -10 % per 100 ticks, arteries and stumps -5 %.
                if w.bleed > 0 {
                    let k = if w.flags & (wf::ARTERY | wf::SEVERED) != 0 { 20 } else { 10 };
                    w.bleed = w.bleed * (100 - k) / 100;
                    if w.bleed == 0 {
                        w.flags &= !wf::BLEEDING;
                    }
                }
                bleed += w.bleed;
                if w.flags & wf::SEVERED != 0 {
                    w.pain = (w.pain - 1).max(5);
                    continue;
                }
                // Healing (estimate): resting 60/100 ticks (hospital 100), else 15; bones only at rest.
                let rate = if in_hosp {
                    100
                } else if rest {
                    60
                } else if w.flags & wf::BROKEN != 0 {
                    0
                } else {
                    15
                };
                w.sev -= rate;
                w.pain = (w.pain * w.sev.max(0) / 10_000).max(if w.sev > 0 { 1 } else { 0 });
            }
            f.wounds.retain(|w| w.sev > 0 || w.flags & wf::SEVERED != 0);
            f.bleed = bleed;
            if bleed == 0 && f.blood < f.blood_max {
                f.blood = (f.blood + (f.blood_max / 100).max(1)).min(f.blood_max);
            }
            for p in f.part_state.iter_mut() {
                *p &= !2;
            }
            for w in &f.wounds {
                if w.flags & wf::BROKEN != 0 {
                    f.part_state[w.part as usize] |= 2;
                }
            }
            let done = f.wounds.iter().all(|w| w.flags & wf::SEVERED != 0) && f.blood >= f.blood_max;
            if bleed > 0 {
                self.list_bleeder(s);
            }
            // Pain-induced unconsciousness ends when the pain drops.
            let b = self.combat.f[s].body;
            if b != u16::MAX && self.combat.f[s].uncon_until > now {
                let thr = pain_threshold(self.attr(s, "WILLPOWER", 1000));
                if self.pain_of(s) <= thr && self.combat.f[s].blood * 2 >= self.combat.f[s].blood_max {
                    self.combat.f[s].uncon_until = now.max(self.combat.f[s].syn_until); // [audit-combat] syndromes
                    self.ai[s].wake = self.ai[s].wake.min(now + 1);
                } else {
                    self.combat.f[s].uncon_until = (now + 200).max(self.combat.f[s].syn_until);
                }
            }
            self.sync_wounds(s);
            self.health_gait(s); // [military2] crawl while unable to stand
            if done {
                self.combat.f[s].listed_wound = false;
            } else {
                keep.push(s as u32);
            }
        }
        // Units wounded during this pass were appended to the (now empty) list; merge.
        let extra = std::mem::take(&mut self.combat.wounded);
        keep.extend(extra);
        keep.sort_unstable();
        keep.dedup();
        self.combat.wounded = keep;
    }

    fn in_hospital(&self, s: usize) -> bool {
        let p = self.upos(s);
        self.combat.hospitals.iter().any(|&(x1, y1, x2, y2, z)| p.z == z && p.x >= x1 && p.x <= x2 && p.y >= y1 && p.y <= y2)
    }

    /// Needs bed rest: a broken bone in a limb, lots of damage or low blood (estimate).
    pub(crate) fn needs_rest(&self, s: usize) -> bool {
        let Some(f) = self.combat.f.get(s) else { return false };
        if f.wounds.is_empty() {
            return false;
        }
        let total: i32 = f.wounds.iter().filter(|w| w.flags & wf::SEVERED == 0).map(|w| w.sev).sum();
        f.wounds.iter().any(|w| w.flags & wf::BROKEN != 0) || total > 15_000 || f.blood * 10 < f.blood_max * 7 || self.health_wants_bed(s) // [moods]
    }

    /// [audit-combat] An able hostile within 3 tiles (same z +-1): too close to lie down.
    pub(crate) fn fight_nearby(&self, s: usize) -> bool {
        let p = self.upos(s);
        let now = self.ticks;
        self.combat.hostiles.iter().any(|&h| {
            let h = h as usize;
            if self.ai[h].kind == Kind::Dead || !self.can_fight(h, now) || self.combat.f[h].role == Role::Leave {
                return false;
            }
            let q = self.upos(h);
            (p.x as i32 - q.x as i32).abs().max((p.y as i32 - q.y as i32).abs()) <= 3 && (p.z as i32 - q.z as i32).abs() <= 1
        })
    }

    // ------------------------------------------------------------------ unit control

    /// Units whose AI is run by the combat module (hostiles, fighters, soldiers, fleeing, ...).
    pub(crate) fn combat_owns(&self, s: usize) -> bool {
        self.combat.f.get(s).map(|f| f.role != Role::None).unwrap_or(false)
    }

    /// Think step of a unit under combat control (`Task::Combat`).
    pub(crate) fn combat_think(&mut self, s: usize, now: u64) {
        if s >= self.combat.f.len() {
            self.combat_sync_units();
        }
        self.ai[s].task = Task::Combat;
        if self.ai[s].waiting {
            return;
        }
        let f = &self.combat.f[s];
        if f.carried_by != NONE {
            self.ai[s].wake = u64::MAX; // [military2] being carried: the carrier moves us
            return;
        }
        if f.uncon_until > now {
            self.ai[s].wake = f.uncon_until;
            return;
        }
        if f.paralyzed {
            self.ai[s].wake = now + 600;
            return;
        }
        match f.role {
            Role::Hostile | Role::Fighter => self.fighter_think(s, now),
            Role::Soldier => self.soldier_think(s, now),
            Role::Civilian => {
                if !self.war_flee_break(s, now) {
                    self.civilian_think(s, now);
                }
            }
            Role::Patient => self.patient_think(s, now),
            Role::Leave => self.leave_think(s, now),
            Role::None => self.release(s, now),
        }
    }

    /// Hand a unit back to the normal AI.
    pub(crate) fn release(&mut self, s: usize, now: u64) {
        let f = &mut self.combat.f[s];
        f.role = Role::None;
        f.target = NONE;
        f.threat = NONE;
        f.sub = Sub::None;
        let ai = &mut self.ai[s];
        ai.task = Task::Idle;
        ai.path.clear();
        ai.path_i = 0;
        if ai.kind == Kind::Worker {
            self.decide(s, now);
        } else {
            self.ai[s].wake = now + 1;
        }
    }

    /// Nearest enemy of a hostile in its walk component (invaders: anywhere; others: in sight).
    pub(crate) fn find_target(&self, s: usize, sight: Option<i32>) -> u32 {
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return NONE;
        }
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, NONE);
        for &t in &self.combat.fort {
            let ai = &self.ai[t as usize];
            if ai.kind == Kind::Dead {
                continue;
            }
            let tp = self.nav.xyz(ai.pos as usize);
            if let Some(r) = sight {
                let cheb = (up.0 as i32 - tp.0 as i32).abs().max((up.1 as i32 - tp.1 as i32).abs());
                if cheb > r || (up.2 as i32 - tp.2 as i32).abs() > 4 {
                    continue;
                }
            }
            let d = crate::manhattan(up, tp);
            if (d, t) < best && self.nav.comp_near(ai.pos as usize) == c {
                best = (d, t);
            }
        }
        best.1
    }

    /// Move towards target `t` and attack it when adjacent. False if the target is gone.
    pub(crate) fn pursue(&mut self, s: usize, now: u64) -> bool {
        if let Some(shot) = self.ranged_pursue(s, now) {
            return shot; // [ranged] shoot from a distance instead of closing in
        }
        let t = self.combat.f[s].target;
        if !self.alive(t) || self.world.units[self.ai[t as usize].uidx].flags.inactive {
            self.combat.f[s].target = NONE;
            return false;
        }
        let t = t as usize;
        let (ps, pt) = (self.upos(s), self.upos(t));
        if ps.dist(pt) <= 1 {
            let f = &self.combat.f[s];
            if now >= f.next_atk {
                let wounds0 = self.combat.stats.wounds; // [audit-combat]
                let killed = self.attack(s, t, now);
                if !self.caverns.powers.is_empty() {
                    self.syndrome_after_attack(s, t, wounds0, now); // [audit-combat] venom
                }
                let agi = self.attr(s, "AGILITY", 1000);
                // Estimate: prepare + recover + 4 ticks, faster with agility.
                let base = 3 + 3 + 4;
                let interval = ((base * 1000) / (500 + agi / 2)).max(4) as u64;
                self.combat.f[s].next_atk = now + interval;
                if killed {
                    self.combat.f[s].target = NONE;
                }
            }
            if self.ai[s].kind == Kind::Dead {
                return true;
            }
            self.ai[s].path.clear();
            self.ai[s].path_i = 0;
            self.ai[s].wake = self.combat.f[s].next_atk.max(now + 1);
            return true;
        }
        // Same walk component?
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 || self.nav.comp_near(self.ai[t].pos as usize) != c {
            self.combat.f[s].target = NONE;
            return false;
        }
        let ai = &self.ai[s];
        // [perf10] Far from the target, a path to where it stood a moment ago is good enough:
        // its end may be off by a quarter of the distance (re-planned on the way in, exact
        // within 12 tiles). Before, every step of a fleeing target re-planned every chaser.
        let d = ps.dist(pt);
        let tol = if d > 12 { d / 4 } else { 1 };
        let end_ok = ai.path_i < ai.path.len() && self.world.pos_of(*ai.path.last().unwrap() as usize).dist(pt) <= tol;
        if end_ok && (self.combat.f[s].repath_at > now || ps.dist(pt) > 12) {
            self.step(s, now);
        } else if !end_ok && self.combat.f[s].repath_at > now + 20 {
            // A recent path failure (see `combat_cancel`): wait before searching again.
            self.ai[s].wake = self.combat.f[s].repath_at - 20;
        } else {
            self.combat.f[s].repath_at = now + 20;
            let tt = self.ai[t].pos;
            self.request_path(s, Goals::one(tt), now);
        }
        true
    }

    /// Hostiles and retaliating creatures.
    fn fighter_think(&mut self, s: usize, now: u64) {
        if self.combat.f[s].raid != 0 && self.raid_think(s, now) {
            return; // [military2] siege camps, thieves, snatchers
        }
        if !self.caverns.powers.is_empty() && self.syndrome_breath(s, now) {
            return; // [audit-combat] forgotten beast breath and webs
        }
        if self.war_wreck_think(s, now) {
            return; // [war9] building destroyers wreck workshops, furniture and doors nearby
        }
        if self.pursue(s, now) {
            return;
        }
        let role = self.combat.f[s].role;
        if role == Role::Fighter {
            self.release(s, now);
            return;
        }
        // Hostile without target: look for one.
        let invader = self.combat.f[s].siege > 0 || self.world.units[self.ai[s].uidx].invader_civ >= 0;
        if self.defense_destroyer_think(s, now, invader) {
            return; // [defense] building destroyers smash locked doors in their way
        }
        if now < self.combat.f[s].dormant_until {
            self.ai[s].wake = self.combat.f[s].dormant_until;
            return;
        }
        let t = self.find_target(s, if invader { None } else { Some(SIGHT) });
        if t != NONE {
            self.combat.f[s].target = t;
            self.combat.f[s].dormant_until = 0;
            self.ai[s].wake = now + 1;
            return;
        }
        // Nothing to attack: dormant (costs nothing), re-checked now and then.
        let mut r = Rng::for_unit(self.seed, s, now, 0xD0);
        let wait = if invader { 50 } else { 400 } + r.below(100) as u64;
        self.combat.f[s].dormant_until = now + wait;
        if invader {
            let _ = self.random_step(s, now, &mut r, None);
        }
        self.ai[s].wake = now + wait;
    }

    /// Wake fort units near active hostiles: on-duty soldiers engage, civilians flee.
    fn threat_scan(&mut self, now: u64) {
        let hs = self.combat.hostiles.clone();
        for &h in &hs {
            let h = h as usize;
            // [defense] Helpless hostiles (paralyzed or knocked out, e.g. by a trap) are still
            // finished off by soldiers on duty; civilians ignore them.
            let helpless = !self.can_fight(h, now);
            if self.ai[h].kind == Kind::Dead || self.combat.f[h].role == Role::Leave && self.combat.f[h].raid < 3 || self.combat.f[h].hidden {
                continue; // [military2] hidden raiders are unseen; fleeing thieves are chased
            }
            // [audit-combat] A lurking (dormant) hostile is still seen by soldiers on duty (wiki
            // Military: they "will attack any hostiles they encounter"); checked every 100 ticks
            // to keep idle cave wildlife cheap. Civilians only react to active hostiles.
            let lurking = !helpless && self.combat.f[h].target == NONE && self.combat.f[h].dormant_until > now;
            if lurking && now % 100 != 0 {
                continue;
            }
            let hp = self.nav.xyz(self.ai[h].pos as usize);
            let hc = self.nav.comp_near(self.ai[h].pos as usize);
            if hc == 0 {
                continue;
            }
            for k in 0..self.combat.fort.len() {
                let t = self.combat.fort[k] as usize;
                if self.ai[t].kind != Kind::Worker {
                    continue;
                }
                let tp = self.nav.xyz(self.ai[t].pos as usize);
                let cheb = (hp.0 as i32 - tp.0 as i32).abs().max((hp.1 as i32 - tp.1 as i32).abs());
                let dz = (hp.2 as i32 - tp.2 as i32).abs();
                let f = &self.combat.f[t];
                let soldier = f.duty != crate::military::Duty::None;
                if (helpless || lurking) && !soldier {
                    continue; // [defense] [audit-combat]
                }
                if soldier && f.role == Role::Patient {
                    continue; // [audit-combat] wounded soldiers stay in bed
                }
                let radius = if soldier { SIGHT } else { 12 };
                if cheb > radius || dz > 2 {
                    continue;
                }
                if self.nav.comp_near(self.ai[t].pos as usize) != hc {
                    continue;
                }
                let f = &mut self.combat.f[t];
                if soldier {
                    if f.role != Role::Soldier || f.target == NONE {
                        f.role = Role::Soldier;
                        f.target = h as u32;
                        if lurking {
                            self.combat.stats.lurkers_engaged += 1; // [audit-combat]
                        }
                        self.engage_now(t, now);
                    }
                } else if f.brk_until > now && (cheb > 2 || dz > 1) {
                    // [war9] on a drink/eat break: only a hostile right next to them interrupts it
                } else if f.role == Role::None || f.role == Role::Patient || (f.role == Role::Civilian && f.threat == NONE) {
                    f.role = Role::Civilian;
                    f.threat = h as u32;
                    f.calm_since = now;
                    self.engage_now(t, now);
                } else if f.role == Role::Civilian {
                    f.calm_since = now;
                }
            }
        }
    }

    /// [war9] A fleeing civilian who has to drink or eat goes for it when the threat is not
    /// right next to them (more than 2 tiles away): DF dwarves cancel jobs on seeing a hostile
    /// but take the next drink once it is out of reach, while the r8 flee loop kept them running
    /// until they died of thirst. For two days afterwards only a hostile within 2 tiles makes
    /// them flee again (estimate). True if the civilian was released to its break.
    pub(crate) fn war_flee_break(&mut self, s: usize, now: u64) -> bool {
        let thr = self.combat.f[s].threat;
        if thr == NONE || !self.alive(thr) || !self.needs_break(s) {
            return false;
        }
        let (a, b) = (self.upos(s), self.upos(thr as usize));
        let cheb = (a.x as i32 - b.x as i32).abs().max((a.y as i32 - b.y as i32).abs());
        if cheb <= 2 && (a.z as i32 - b.z as i32).abs() <= 1 {
            return false;
        }
        self.combat.f[s].brk_until = now + 2 * rf_core::TICKS_PER_DAY as u64;
        self.combat.stats.flee_breaks += 1;
        self.release(s, now);
        true
    }

    /// Interrupt whatever a unit does and let the combat AI take over next tick.
    pub(crate) fn engage_now(&mut self, s: usize, now: u64) {
        if self.ai[s].task != Task::Combat {
            self.cancel_task(s);
            self.ai[s].task = Task::Combat;
        }
        if !self.ai[s].waiting {
            self.ai[s].wake = self.ai[s].wake.min(now + 1);
        }
    }

    // ------------------------------------------------------------------ recovery

    pub(crate) fn patient_think(&mut self, s: usize, now: u64) {
        if self.needs_break(s) && !self.health_keep_in_bed(s) { // [moods] bedridden patients are fed
            self.release_rest(s);
            self.combat.f[s].role = Role::None;
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        }
        if !self.needs_rest(s) {
            self.release_rest(s);
            self.release(s, now);
            return;
        }
        match self.combat.f[s].sub {
            Sub::Resting(_) => {
                self.ai[s].wake = now + 200;
            }
            Sub::Rest(b) => {
                if self.cannot_stand(s) && self.rescue_available(s) {
                    // [military2] Cannot walk: lie down and wait to be carried (Recover wounded).
                    self.release_rest(s);
                    self.combat.f[s].sub = Sub::Resting(-1);
                    self.ai[s].path.clear();
                    self.ai[s].path_i = 0;
                    self.ai[s].wake = now + 200;
                    return;
                }
                if self.ai[s].path_i < self.ai[s].path.len() {
                    self.step(s, now);
                } else {
                    let here = self.ai[s].pos;
                    let at_bed = b < 0 || self.beds[b as usize].tile == here;
                    if at_bed {
                        self.combat.f[s].sub = Sub::Resting(b);
                        self.ai[s].wake = now + 200;
                    } else {
                        let t = self.beds[b as usize].tile;
                        self.request_path(s, Goals::one(t), now);
                    }
                }
            }
            _ => {
                if self.cannot_stand(s) && self.rescue_available(s) {
                    // [military2] Wait on the floor for a dwarf with Recover wounded.
                    self.combat.f[s].sub = Sub::Resting(-1);
                    self.ai[s].wake = now + 200;
                    return;
                }
                // Hospital beds first, then the own bed, then any free bed (wiki Health care).
                let pos = self.ai[s].pos as usize;
                let c = self.nav.comp_near(pos);
                let up = self.nav.xyz(pos);
                let uid = self.ai[s].id;
                let mut best = (i32::MAX, -1i32);
                for (k, bed) in self.beds.iter().enumerate() {
                    if bed.occupant != NONE || (bed.owner >= 0 && bed.owner != uid) || c == 0 || self.nav.comp_near(bed.tile as usize) != c {
                        continue;
                    }
                    let bp_ = self.world.pos_of(bed.tile as usize);
                    let hosp = self.combat.hospitals.iter().any(|&(x1, y1, x2, y2, z)| bp_.z == z && bp_.x >= x1 && bp_.x <= x2 && bp_.y >= y1 && bp_.y <= y2);
                    let d = crate::manhattan(up, self.nav.xyz(bed.tile as usize)) - if hosp { 10_000 } else { 0 } - if bed.owner == uid { 5_000 } else { 0 };
                    if d < best.0 {
                        best = (d, k as i32);
                    }
                }
                if best.1 >= 0 {
                    let b = best.1;
                    self.beds[b as usize].occupant = s as u32;
                    self.combat.f[s].sub = Sub::Rest(b);
                    let t = self.beds[b as usize].tile;
                    self.request_path(s, Goals::one(t), now);
                } else {
                    self.combat.f[s].sub = Sub::Resting(-1);
                    self.ai[s].wake = now + 200;
                }
            }
        }
    }

    pub(crate) fn release_rest(&mut self, s: usize) {
        if let Some(f) = self.combat.f.get_mut(s) {
            if let Sub::Rest(b) | Sub::Resting(b) = f.sub {
                if b >= 0 && (b as usize) < self.beds.len() && self.beds[b as usize].occupant == s as u32 {
                    self.beds[b as usize].occupant = NONE;
                }
                f.sub = Sub::None;
            }
        }
    }

    /// A worker has to drink, eat or sleep now (soldiers and patients take a break).
    pub(crate) fn needs_break(&self, s: usize) -> bool {
        let ai = &self.ai[s];
        ai.kind == Kind::Worker
            && ((ai.thirst >= crate::needs::THIRST_SEEK && now_ok(ai.no_drink_until, self.ticks))
                || (ai.hunger >= crate::needs::HUNGER_SEEK && now_ok(ai.no_food_until, self.ticks))
                || ai.sleep >= crate::needs::SLEEP_SEEK)
    }

    /// Activity caption for the unit sheet.
    pub(crate) fn combat_caption(&self, s: usize) -> (&'static str, Option<Pos>) {
        let Some(f) = self.combat.f.get(s) else { return ("No Job", None) };
        if f.uncon_until > self.ticks {
            return ("Unconscious", None);
        }
        let tpos = if self.alive(f.target) { Some(self.upos(f.target as usize)) } else { None };
        match f.role {
            Role::Hostile | Role::Fighter => (if tpos.is_some() { "Attack" } else { "Lurk" }, tpos),
            Role::Soldier => match (tpos, f.duty) {
                (Some(_), _) => ("Fight", tpos),
                (None, crate::military::Duty::Train) => ("Train", None),
                (None, crate::military::Duty::Station(t)) => ("Station", Some(self.world.pos_of(t as usize))),
                (None, crate::military::Duty::Patrol) => ("Patrol", None), // [military2]
                (None, _) => ("Soldier", None),
            },
            Role::Civilian => (if f.threat != NONE { "Flee" } else { "Go to Burrow" }, None),
            Role::Patient => ("Rest", None),
            Role::Leave => ("Leave", None),
            Role::None => ("No Job", None),
        }
    }

    pub(crate) fn combat_hash(&self, mix: &mut dyn FnMut(u64)) {
        mix(self.combat.log_total);
        mix(self.combat.stats.hits);
        for f in &self.combat.f {
            mix(((f.blood as u64) << 32) ^ f.wounds.len() as u64 ^ ((f.role as u64) << 48));
        }
    }

    // ------------------------------------------------------------------ public API helpers

    /// Give a unit a new item into its inventory (worn/wielded), e.g. for tests, the arena or
    /// invader equipment. `mat_token` like `INORGANIC:IRON` is added to the palette if missing.
    pub fn give_item(&mut self, unit_id: i32, kind: &str, subtype: &str, mat_token: &str, quality: u8) -> Option<i32> {
        let &s = self.unit_slot.get(&unit_id)?;
        let mat = self.ensure_material(mat_token);
        let id = self.next_item_id;
        let pos = self.upos(s);
        let mname = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default();
        let sname = match kind {
            "WEAPON" => self.weapon_t(subtype).map(|w| self.combat.weapons[w as usize].name.clone()),
            _ => None,
        }
        .unwrap_or_else(|| {
            let t = subtype.split('_').skip(2).collect::<Vec<_>>().join(" ").to_ascii_lowercase();
            if t.is_empty() {
                kind.to_ascii_lowercase()
            } else {
                t
            }
        });
        self.push_item(Item {
            id,
            kind: kind.into(),
            subtype: subtype.into(),
            mat,
            pos,
            stack: 1,
            quality,
            container: -1,
            holder: unit_id,
            building: -1,
            desc: format!("{mname} {sname}").trim().to_string(),
            flags: ItemFlags { in_inventory: true, ..Default::default() },
            ..Default::default()
        });
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(id);
        Some(id)
    }

    /// Palette index of a material token; metals/leather are added with DF's values if missing.
    pub fn ensure_material(&mut self, token: &str) -> u16 {
        if let Some(i) = self.world.materials.iter().position(|m| m.token == token) {
            return i as u16;
        }
        let m = default_mech(token);
        let name = token.rsplit(':').next().unwrap_or(token).to_ascii_lowercase().replace('_', " ");
        let metal = token.starts_with("INORGANIC:");
        let rgb = match name.as_str() {
            "iron" => [140, 140, 150],
            "copper" => [200, 110, 60],
            "bronze" => [190, 140, 70],
            "steel" => [170, 180, 190],
            "silver" => [200, 200, 210],
            _ => [150, 150, 150],
        };
        self.world.materials.push(MaterialInfo {
            mat_type: -1,
            mat_index: -1,
            token: token.into(),
            name,
            rgb,
            is_metal: metal,
            density: m.dens as i32,
            shear_yield: m.sy as i32,
            shear_fracture: m.sf as i32,
            shear_strain_at_yield: m.ssay as i32,
            impact_yield: m.iy as i32,
            impact_fracture: m.ifr as i32,
            impact_strain_at_yield: m.isay as i32,
            max_edge: m.edge as i32,
            ..Default::default()
        });
        (self.world.materials.len() - 1) as u16
    }

    /// Combat counters (attacks, hits, kills, sieges, ...).
    pub fn combat_stats(&self) -> CombatStats {
        self.combat.stats.clone()
    }

    /// Blood and blood max of a unit, plus whether it is unconscious right now.
    pub fn unit_condition(&self, unit_id: i32) -> Option<(i32, i32, bool)> {
        let &s = self.unit_slot.get(&unit_id)?;
        let f = self.combat.f.get(s)?;
        Some((f.blood, f.blood_max, f.uncon_until > self.ticks))
    }

    /// Mark a unit hostile (e.g. a wild animal turning on the fort, or for an arena fight).
    pub fn make_hostile(&mut self, unit_id: i32) {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return };
        self.combat_sync_units();
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].flags.hostile = true;
        self.world.units[uidx].flags.tame = false;
        if self.ai[s].kind == Kind::Dead {
            return;
        }
        let f = &mut self.combat.f[s];
        f.role = Role::Hostile;
        if let Err(p) = self.combat.hostiles.binary_search(&(s as u32)) {
            self.combat.hostiles.insert(p, s as u32);
        }
        self.rebuild_fort_list();
        let now = self.ticks;
        self.engage_now(s, now);
    }

}

/// Pain at which a creature passes out (estimate: 250 + WILLPOWER/10).
fn pain_threshold(willpower: i32) -> i32 {
    250 + willpower / 10
}

#[inline]
fn now_ok(until: u64, now: u64) -> bool {
    now >= until
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mech_defaults_match_df_raws() {
        assert_eq!(default_mech("INORGANIC:IRON").sy, 155_000.0);
        assert_eq!(default_mech("INORGANIC:PIG_IRON").sy, 130_000.0);
        assert_eq!(default_mech("CREATURE:DWARF:SKIN").sy, 20_000.0);
        assert_eq!(default_mech("BONE").isay, 100.0);
        assert_eq!(verb3("punch"), "punches");
        assert_eq!(verb3("hack"), "hacks");
    }

    #[test]
    fn dwarf_momentum_matches_wiki_example() {
        // Wiki: dwarf (size 60000), strength 1250, battle axe (size 800, velocity 1.25):
        // M = 0.06 * Str * Vel / (1 + i_Size / (w_density * w_size)).
        let (size, str_, vel) = (60_000.0f32, 1250.0f32, 1.25f32);
        let iron = default_mech("IRON");
        let m = size * str_ * vel / (1e6 * (1.0 + size / (iron.dens * 800.0)));
        let wiki = 0.06 * str_ * vel / (1.0 + size / (iron.dens * 800.0));
        assert!((m - wiki).abs() < 1e-3, "{m} vs {wiki}");
        assert!(m > 90.0 && m < 95.0);
    }

    /// [audit-combat] Surface arena: dwarf 1 and the hostile cat 100 only.
    fn surface_arena(dwarf: (i16, i16), cat: (i16, i16)) -> (Sim, usize, usize, i16) {
        let mut w = rf_core::World::synthetic(64, 64, 20);
        let sz = (20i16 * 2 / 3).max(2);
        w.units.retain(|u| u.id == 1 || u.id == 100);
        for u in &mut w.units {
            u.pos = if u.id == 1 { Pos::new(dwarf.0, dwarf.1, sz) } else { Pos::new(cat.0, cat.1, sz) };
            u.invader_civ = -1;
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        let mut sim = Sim::new(w, 3);
        sim.set_invasions(false);
        let (d, c) = (sim.unit_slot[&1], sim.unit_slot[&100]);
        (sim, d, c, sz)
    }

    #[test]
    fn soldiers_spot_a_lurking_hostile() {
        // Before round 6 a dormant hostile (no prey in its own sight) was invisible to soldiers
        // until it woke up by itself (400-500 ticks later); now soldiers on duty see it.
        let (mut sim, d, c, sz) = surface_arena((5, 32), (50, 32));
        sim.make_hostile(100);
        sim.run_ticks(30);
        assert!(sim.combat.f[c].dormant_until > sim.ticks, "the cat lurks");
        let q = sim.world.idx(Pos::new(37, 32, sz)).unwrap() as u32;
        sim.move_to(d, q, sim.ticks);
        let sq = sim.create_squad("Watch");
        sim.assign_squad_member(sq, 0, 1);
        sim.order_squad(sq, crate::SquadOrder::Train);
        sim.run_ticks(150);
        assert!(sim.combat.f[c].dormant_until > sim.ticks, "the cat is still dormant");
        assert_eq!(sim.combat.f[d].target, c as u32, "the soldier goes for the lurker");
        assert!(sim.combat.stats.lurkers_engaged >= 1);
    }

    #[test]
    fn wounded_soldier_rests_instead_of_training() {
        let (mut sim, d, _c, _sz) = surface_arena((20, 32), (60, 60));
        let sq = sim.create_squad("Guard");
        sim.assign_squad_member(sq, 0, 1);
        sim.order_squad(sq, crate::SquadOrder::Train);
        sim.run_ticks(100);
        assert_eq!(sim.combat.f[d].role, Role::Soldier);
        // A broken bone (as after a fight): DF sends the dwarf to bed.
        sim.ensure_body(d);
        let part = 0u16;
        sim.combat.f[d].wounds.push(WoundS { part, layer: u16::MAX, flags: wf::BROKEN, sev: 5_000, bleed: 0, pain: 10, age: 0 });
        sim.list_wounded(d);
        sim.run_ticks(200);
        assert_eq!(sim.combat.f[d].role, Role::Patient, "wounded soldier rests");
        assert!(matches!(sim.combat.f[d].sub, Sub::Rest(_) | Sub::Resting(_)));
        assert!(sim.combat.stats.soldiers_rested >= 1);
        // Healed: back on duty.
        sim.combat.f[d].wounds.clear();
        sim.run_ticks(400);
        assert_eq!(sim.combat.f[d].role, Role::Soldier, "back on duty after healing");
    }
}
