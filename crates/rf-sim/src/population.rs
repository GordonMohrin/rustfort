//! Population (round 4): migrants, pregnancies and births, aging, old age, burial, ghosts and
//! nobles (elections, mandates).
//!
//! # DF rules implemented (wiki v50: Migrant, Population cap, Pregnancy, Birth, Child, Age,
//! Death, Burial, Coffin, Tomb, Ghost, Memorial slab, Noble, Mayor, Mandate)
//! - Population caps from `World.settings` (d_init `POPULATION_CAP`, `STRICT_POPULATION_CAP`,
//!   `BABY_CHILD_CAP`); without exported settings DF's defaults 200 / 220 / 100:1000 apply.
//!   Migrants stop at the population cap, the strict cap also stops births (d_init text), the
//!   baby cap (lower of the absolute number and the percentage of adults) stops new pregnancies.
//! - Migrant waves are decided when a season starts and arrive later in that season at a map
//!   edge connected to the fort ("Some migrants have arrived."). The new dwarves are citizens of
//!   the fort's civilization from the start; attributes follow the vanilla DWARF attribute
//!   ranges, personality and values are drawn around the existing citizens (the civ's culture),
//!   names are recombined from the citizens' name parts (no language data in the export).
//! - Married couples (both alive on the map) can conceive; the pregnancy lasts until birth,
//!   babies are carried by their mother, become children at 1 (raws `BABY:1`) and adults at 13
//!   (raws say `CHILD:12`, but Gordon's fort shows dwarves aged 12.1-12.9 still as children, so
//!   the observed threshold is used). Grown-ups become `PEASANT` with the fort's default labors.
//! - Units age by 1/336 year per day; dwarves die of old age at their MAXAGE (raws `150:170`,
//!   fixed per unit from its id).
//! - Burial (DF "Bury" job, labor HAUL_BODY): the corpse of a dead citizen is carried to a free
//!   built coffin inside a Tomb zone (zone unassigned or assigned to that dwarf) and interred.
//! - Ghosts: a dead citizen that is neither buried nor memorialized can rise as a ghost and haunts
//!   the living ("GhostHaunt", "GhostNightmare" thoughts) until buried or until an engraved
//!   memorial slab is dedicated to it.
//! - Nobles: the expedition leader is replaced by an elected mayor once the fort has 50 citizens;
//!   the mayor is re-elected every year; a vacant expedition leader or mayor is filled again.
//!   Appointed positions (manager, broker, ...) stay vacant until the player appoints someone.
//!   Nobles issue production mandates; an unmet mandate gives the noble a bad thought at the
//!   deadline ("MandateIgnored"), a met one a good one. Punishments are not simulated.
//!
//! # Estimates (not on the wiki)
//! - Wave chance 3 in 4 per season below the cap; arrival 1-27 days into the season; wave size
//!   2 + rand(min(28, 4 + fort wealth / 25 000)), capped by the room under both caps.
//! - Conception chance 1 in 12 per month per couple, at least one year after the last birth;
//!   pregnancy 9 months; twins 1 in 50.
//! - Ghosts rise with a daily chance of 1 in 7 once the body lay unburied for 14 days.
//! - Mandates: one per noble at most, issued with a chance of 1 in 3 per season, deadline one
//!   season; election score = social skills + 2 per friend (incumbent x1.5).

use crate::rng::{splitmix, Rng};
use crate::social::Class;
use crate::{labor, AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{Pos, Relationship, Skill, Unit, UnitFlags, ValueStrength, TICKS_PER_DAY, TICKS_PER_MONTH};

/// The population pass runs when `tick % POP_PERIOD == POP_PHASE`.
pub const POP_PERIOD: u64 = 100;
pub const POP_PHASE: u64 = 1;
const DAY: u64 = TICKS_PER_DAY as u64;
const MONTH: u64 = TICKS_PER_MONTH as u64;
/// DF: 336 days per year.
const DAYS_PER_YEAR: f32 = 336.0;
/// Vanilla DWARF raws: `BABY:1`; adults from 13 (observed, see module docs).
pub const BABY_AGE: f32 = 1.0;
pub const ADULT_AGE: f32 = 13.0;
/// Vanilla DWARF raws: `MAXAGE:150:170`.
pub const MAXAGE_MIN: i32 = 150;
pub const MAXAGE_MAX: i32 = 170;
/// d_init defaults (DF 53.16 `d_init_default.txt`).
pub const DEFAULT_POP_CAP: i32 = 200;
pub const DEFAULT_STRICT_CAP: i32 = 220;
pub const DEFAULT_BABY_CAP: i32 = 100;
pub const DEFAULT_BABY_PCT: i32 = 1000;
/// Population needed for the mayor (wiki "Mayor").
pub const MAYOR_POPULATION: usize = 50;
/// Pregnancy length (estimate: 9 months).
pub const PREGNANCY_TICKS: u64 = 9 * MONTH;

#[derive(Clone, Debug)]
pub(crate) struct Pregnancy {
    pub mother: i32,
    pub father: i32,
    pub due: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct Dead {
    pub unit_id: i32,
    pub name: String,
    pub died: u64,
    pub corpse: i32,
    pub buried: bool,
    pub memorial: bool,
    pub ghost: bool,
    /// Died before the sim started (corpse from the export): never rises.
    pub old: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct BuryJob {
    pub corpse: i32,
    pub coffin: i32,
    pub tile: u32,
    pub claimed: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Mandate {
    pub noble: String,
    pub text: String,
    pub kind: String,
    pub count: i32,
    pub baseline: i32,
    pub deadline: u64,
}

/// What happened to the population (for the UI, the bench and tests).
#[derive(Clone, Debug, Default)]
pub struct PopulationReport {
    /// Migrant waves: (tick, number of migrants).
    pub waves: Vec<(u64, usize)>,
    /// Births: (tick, baby name).
    pub births: Vec<(u64, String)>,
    pub pregnancies_started: usize,
    pub pregnant_now: usize,
    pub grown_up: Vec<(u64, String)>,
    pub old_age_deaths: Vec<(u64, String)>,
    pub burials: Vec<(u64, String)>,
    pub ghosts_risen: Vec<(u64, String)>,
    pub ghosts_laid: Vec<(u64, String)>,
    pub ghosts_active: usize,
    pub unburied: usize,
    pub free_coffins: usize,
    pub elections: Vec<(u64, String)>,
    pub mandates_issued: usize,
    pub mandates_met: usize,
    pub mandates_missed: usize,
    pub mandates_active: Vec<String>,
    /// Citizens at each month start: (year, month, citizens, children incl. babies).
    pub months: Vec<(i32, i32, usize, usize)>,
}

#[derive(Default)]
pub(crate) struct Pop {
    pub ready: bool,
    pub last_season: i32,
    pub last_month: i32,
    pub last_year: i32,
    pub last_day: u64,
    /// Scheduled migrant wave: (arrival tick, size).
    pub wave_due: Option<(u64, usize)>,
    pub preg: Vec<Pregnancy>,
    /// Last birth tick per mother unit id.
    pub last_birth: Vec<(i32, u64)>,
    pub dead_seen: Vec<bool>,
    pub dead: Vec<Dead>,
    pub bury: Vec<BuryJob>,
    pub mandates: Vec<Mandate>,
    pub rep: PopulationReport,
    /// Disable migrants/births (tests, comparisons).
    pub off: bool,
    /// The whole population/trade simulation off (speed comparisons).
    pub disabled: bool,
}

/// One vanilla DWARF attribute range: 7 points (min .. median .. max).
type Range7 = [i32; 7];
const R_HIGH: Range7 = [450, 950, 1150, 1250, 1350, 1550, 2250];
const R_LOW: Range7 = [150, 600, 700, 800, 900, 1100, 1500];
const R_DEF: Range7 = [200, 700, 900, 1000, 1100, 1300, 2000];
const R_VHIGH: Range7 = [700, 1200, 1400, 1500, 1600, 1800, 2500];
/// Approximately the vanilla DWARF attribute ranges.
const PHYS: &[(&str, Range7)] = &[
    ("STRENGTH", R_HIGH),
    ("AGILITY", R_LOW),
    ("TOUGHNESS", R_HIGH),
    ("ENDURANCE", R_HIGH),
    ("RECUPERATION", R_DEF),
    ("DISEASE_RESISTANCE", R_DEF),
];
const MENT: &[(&str, Range7)] = &[
    ("ANALYTICAL_ABILITY", R_HIGH),
    ("FOCUS", R_VHIGH),
    ("WILLPOWER", R_HIGH),
    ("CREATIVITY", R_HIGH),
    ("INTUITION", R_DEF),
    ("PATIENCE", R_HIGH),
    ("MEMORY", R_HIGH),
    ("LINGUISTIC_ABILITY", R_DEF),
    ("SPATIAL_SENSE", R_VHIGH),
    ("MUSICALITY", R_DEF),
    ("KINESTHETIC_SENSE", R_DEF),
    ("EMPATHY", R_DEF),
    ("SOCIAL_AWARENESS", R_DEF),
];

/// Migrant professions: (profession, skill, labor).
const MIGRANT_JOBS: &[(&str, &str, &str)] = &[
    ("MINER", "MINING", "MINE"),
    ("WOODCUTTER", "WOODCUTTING", "CUTWOOD"),
    ("CARPENTER", "CARPENTRY", "CARPENTER"),
    ("MASON", "MASONRY", "MASON"),
    ("STONEWORKER", "STONECRAFT", "STONE_CRAFT"),
    ("BREWER", "BREWING", "BREWER"),
    ("COOK", "COOK", "COOK"),
    ("PLANTER", "PLANT", "PLANT"),
    ("HERBALIST", "HERBALISM", "HERBALIST"),
    ("FISHERMAN", "FISH", "FISH"),
    ("MECHANIC", "MECHANICS", "MECHANIC"),
    ("WEAPONSMITH", "FORGE_WEAPON", "FORGE_WEAPON"),
    ("ARMORSMITH", "FORGE_ARMOR", "FORGE_ARMOR"),
    ("FURNACE_OPERATOR", "SMELT", "SMELT"),
    ("WEAVER", "WEAVING", "WEAVER"),
    ("CLOTHIER", "CLOTHESMAKING", "CLOTHESMAKER"),
    ("TANNER", "TANNER", "TANNER"),
    ("LEATHERWORKER", "LEATHERWORK", "LEATHER"),
    ("ENGRAVER", "ENGRAVE_STONE", "ENGRAVER"),
    ("BUTCHER", "BUTCHER", "BUTCHER"),
    ("PEASANT", "", ""),
    ("PEASANT", "", ""),
];

const MANDATE_ITEMS: &[(&str, &str)] = &[
    ("mugs", "GOBLET"),
    ("figurines", "FIGURINE"),
    ("rings", "RING"),
    ("bracelets", "BRACELET"),
    ("crowns", "CROWN"),
    ("chairs", "CHAIR"),
    ("tables", "TABLE"),
    ("doors", "DOOR"),
    ("cabinets", "CABINET"),
    ("statues", "STATUE"),
];

fn attr_roll(r: &mut Rng, p: &Range7) -> i32 {
    let k = r.below(6) as usize;
    p[k] + r.below((p[k + 1] - p[k]).max(1) as u32) as i32
}

fn is_noble_with_mandates(token: &str) -> bool {
    matches!(token, "MAYOR" | "BARON" | "BARONESS" | "COUNT" | "COUNTESS" | "DUKE" | "DUCHESS" | "MONARCH" | "KING" | "QUEEN")
}

impl Sim {
    // ------------------------------------------------------------------ settings

    /// (population cap, strict cap, baby cap, baby cap % of adults), DF defaults if the export
    /// carries no settings.
    pub fn population_caps(&self) -> (i32, i32, i32, i32) {
        let s = &self.world.settings;
        let pct = s
            .other
            .iter()
            .find_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (k.eq_ignore_ascii_case("BABY_CHILD_CAP_PERCENT") || k.eq_ignore_ascii_case("BABY_CAP_PERCENT") || k.eq_ignore_ascii_case("BABY_CHILD_CAP_PCT"))
                    .then(|| v.trim().parse().ok())
                    .flatten()
            })
            .unwrap_or(DEFAULT_BABY_PCT);
        if s.population_cap <= 0 && s.strict_population_cap <= 0 && s.baby_cap <= 0 {
            return (DEFAULT_POP_CAP, DEFAULT_STRICT_CAP, DEFAULT_BABY_CAP, pct);
        }
        let pop = if s.population_cap > 0 { s.population_cap } else { DEFAULT_POP_CAP };
        let strict = if s.strict_population_cap > 0 { s.strict_population_cap } else { pop.max(DEFAULT_STRICT_CAP) };
        (pop, strict, s.baby_cap.max(0), pct)
    }

    /// Switch migrants and pregnancies off (or on again). Default on, like DF.
    pub fn set_population_growth(&mut self, on: bool) {
        self.pop.off = !on;
    }

    /// Switch the whole round-4 population/trade/noble simulation off (speed comparisons).
    pub fn set_society_enabled(&mut self, on: bool) {
        self.pop.disabled = !on;
    }

    /// Migrants and caravans need the fort's civilization: an exported `Own` entity, or a
    /// world imported from DF (synthetic test worlds have neither).
    pub(crate) fn has_civilization(&self) -> bool {
        self.world.entities.iter().any(|e| e.relation == "Own") || self.world.meta.source.starts_with("export")
    }

    pub fn population_report(&self) -> PopulationReport {
        let mut r = self.pop.rep.clone();
        r.pregnant_now = self.pop.preg.len();
        r.ghosts_active = self.pop.dead.iter().filter(|d| d.ghost).count();
        r.unburied = self.pop.dead.iter().filter(|d| !d.buried && !d.memorial).count();
        r.free_coffins = self.free_coffins().len();
        r.mandates_active = self.pop.mandates.iter().map(|m| format!("{}: {}", m.noble, m.text)).collect();
        r
    }

    // ------------------------------------------------------------------ helpers

    fn pop_citizen_slot(&self, s: usize) -> bool {
        let ai = &self.ai[s];
        let u = &self.world.units[ai.uidx];
        ai.kind == Kind::Worker && u.flags.citizen && !u.flags.dead && !u.flags.animal
    }

    fn is_child(u: &Unit) -> bool {
        u.profession == "CHILD" || u.profession == "BABY" || (u.age < ADULT_AGE && u.race == "DWARF" && u.age > 0.0 && u.labors.is_empty() && u.profession.is_empty())
    }

    pub(crate) fn pop_counts(&self) -> (usize, usize) {
        let mut cit = 0;
        let mut kids = 0;
        for u in self.world.citizens() {
            cit += 1;
            if Self::is_child(u) {
                kids += 1;
            }
        }
        (cit, kids)
    }

    fn max_age(&self, id: i32) -> f32 {
        let h = splitmix(0x0A6E_u64 ^ (id as u64).wrapping_mul(0x9E37_79B9));
        (MAXAGE_MIN + (h % (MAXAGE_MAX - MAXAGE_MIN + 1) as u64) as i32) as f32
    }

    /// Labors that most adult civilians of the fort have enabled (DF v50 default work details).
    fn default_labors(&self) -> Vec<String> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        let mut n = 0usize;
        for u in self.world.citizens() {
            if Self::is_child(u) || u.labors.is_empty() {
                continue;
            }
            n += 1;
            for l in &u.labors {
                match counts.iter_mut().find(|(k, _)| k == l) {
                    Some(e) => e.1 += 1,
                    None => counts.push((l.clone(), 1)),
                }
            }
        }
        let mut out: Vec<String> = counts.into_iter().filter(|(_, c)| *c * 2 > n).map(|(l, _)| l).collect();
        for h in ["HAUL_STONE", "HAUL_WOOD", "HAUL_BODY", "HAUL_FOOD", "HAUL_REFUSE", "HAUL_ITEM", "HAUL_FURNITURE", "HAUL_ANIMALS", "HAUL_TRADE"] {
            if !out.iter().any(|l| l == h) {
                out.push(h.to_string());
            }
        }
        out
    }

    /// Walkable map-edge tiles in walk component `c` (outside tiles preferred). `c == 0`: the
    /// component with the most citizens.
    pub(crate) fn society_edge_tiles(&self, c: u32) -> Vec<u32> {
        let c = if c != 0 {
            c
        } else {
            let mut counts: Vec<(u32, u32)> = Vec::new();
            for ai in &self.ai {
                if ai.kind != Kind::Worker {
                    continue;
                }
                let k = self.nav.comp_near(ai.pos as usize);
                if k == 0 {
                    continue;
                }
                match counts.iter_mut().find(|e| e.0 == k) {
                    Some(e) => e.1 += 1,
                    None => counts.push((k, 1)),
                }
            }
            match counts.iter().max_by_key(|e| (e.1, std::cmp::Reverse(e.0))) {
                Some(&(k, _)) => k,
                None => return Vec::new(),
            }
        };
        let (dx, dy, dz) = (self.nav.dx, self.nav.dy, self.nav.dz);
        let mut outside = Vec::new();
        let mut any = Vec::new();
        let mut visit = |x: usize, y: usize, z: usize| {
            let i = self.nav.idx(x, y, z);
            if !self.nav.walkable(i) || self.nav.comp_root(i) != c {
                return;
            }
            if self.world.tiles[i].flags & rf_core::tile_flags::OUTSIDE != 0 {
                outside.push(i as u32);
            } else {
                any.push(i as u32);
            }
        };
        for z in 0..dz {
            for x in 0..dx {
                visit(x, 0, z);
                if dy > 1 {
                    visit(x, dy - 1, z);
                }
            }
            for y in 1..dy.saturating_sub(1) {
                visit(0, y, z);
                if dx > 1 {
                    visit(dx - 1, y, z);
                }
            }
        }
        if outside.is_empty() {
            any
        } else {
            outside
        }
    }

    /// Name parts of the citizens: (first names, last names, native first, native last).
    fn name_pool(&self) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
        let (mut f, mut l, mut nf, mut nl) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for u in &self.world.units {
            if !u.flags.citizen || u.race != "DWARF" {
                continue;
            }
            let mut it = u.name.split_whitespace();
            if let (Some(a), Some(b)) = (it.next(), it.last()) {
                f.push(a.to_string());
                l.push(b.to_string());
            }
            let mut it = u.name_native.split_whitespace();
            if let (Some(a), Some(b)) = (it.next(), it.last()) {
                nf.push(a.to_string());
                nl.push(b.to_string());
            }
        }
        for v in [&mut f, &mut l, &mut nf, &mut nl] {
            v.sort();
            v.dedup();
            if v.is_empty() {
                v.extend(["Urist", "Kadol", "Doren", "Zon"].iter().map(|s| s.to_string()));
            }
        }
        (f, l, nf, nl)
    }

    /// A new dwarf of the fort's civilization (migrant or baby).
    pub(crate) fn gen_dwarf(&self, r: &mut Rng, id: i32, age: f32, sex: i8, pos: Pos, last_name: Option<(String, String)>) -> Unit {
        let (f, l, nf, nl) = self.name_pool();
        let pick = |r: &mut Rng, v: &Vec<String>| v[r.below(v.len() as u32) as usize].clone();
        let first = pick(r, &f);
        let nfirst = pick(r, &nf);
        let (last, nlast) = match last_name {
            Some(x) => x,
            None => (pick(r, &l), pick(r, &nl)),
        };
        let caste_tok = if sex == 0 { "FEMALE" } else { "MALE" };
        let raw = self.world.creature_raws.iter().find(|c| c.token == "DWARF");
        let caste = raw.and_then(|c| c.castes.iter().find(|k| k.token == caste_tok).or(c.castes.first()));
        let body = caste.map(|c| c.body_size).filter(|&b| b > 0).unwrap_or(60_000);
        let walk = caste.map(|c| c.walk_speed).filter(|&w| w > 0).unwrap_or(900);
        let phys = PHYS.iter().map(|(n, p)| (n.to_string(), attr_roll(r, p))).collect();
        let ment = MENT.iter().map(|(n, p)| (n.to_string(), attr_roll(r, p))).collect();
        // Personality and values around the fort's citizens (shared culture of the civ).
        let models: Vec<&Unit> = self.world.units.iter().filter(|u| u.flags.citizen && u.race == "DWARF" && !u.facets.is_empty()).collect();
        let (facets, values) = if models.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            let m = models[r.below(models.len() as u32) as usize];
            let m2 = models[r.below(models.len() as u32) as usize];
            let facets = m
                .facets
                .iter()
                .map(|(k, v)| {
                    let other = m2.facets.iter().find(|(k2, _)| k2 == k).map(|x| x.1).unwrap_or(*v);
                    (k.clone(), ((v + other) / 2 + r.below(21) as i32 - 10).clamp(0, 100))
                })
                .collect();
            let values = m.values.iter().map(|v| ValueStrength { id: v.id.clone(), strength: (v.strength + r.below(11) as i32 - 5).clamp(-50, 50) }).collect();
            (facets, values)
        };
        let civ = if self.world.meta.civ_id >= 0 { self.world.meta.civ_id } else { -1 };
        let scale = if age < ADULT_AGE { (0.1 + 0.9 * age / ADULT_AGE).clamp(0.05, 1.0) } else { 1.0 };
        Unit {
            id,
            race: "DWARF".into(),
            caste: caste_tok.into(),
            name: format!("{first} {last}"),
            name_native: format!("{nfirst} {nlast}"),
            profession: "PEASANT".into(),
            pos,
            flags: UnitFlags { citizen: true, resident: false, ..Default::default() },
            civ_id: civ,
            hist_fig_id: -1,
            sex,
            age,
            phys_attrs: phys,
            ment_attrs: ment,
            facets,
            values,
            squad_id: -1,
            squad_position: -1,
            body_size: (body as f32 * scale) as i32,
            walk_speed: walk,
            invader_civ: -1,
            ..Default::default()
        }
    }

    // ------------------------------------------------------------------ tick

    /// [hook] Population pass, every `POP_PERIOD` ticks.
    pub(crate) fn population_tick(&mut self, now: u64) {
        if self.pop.disabled {
            return;
        }
        if !self.pop.ready {
            self.pop_init(now);
        }
        self.pop_carry_babies(now);
        self.pop_check_deaths(now);
        self.pop_check_claims();
        if let Some((due, n)) = self.pop.wave_due {
            if now >= due {
                self.pop.wave_due = None;
                self.spawn_migrants(n);
            }
        }
        let day = now / DAY;
        if day != self.pop.last_day {
            self.pop.last_day = day;
            self.pop_daily(now);
        }
        self.trade_tick(now);
    }

    fn pop_init(&mut self, now: u64) {
        self.pop.ready = true;
        let c = self.world.calendar;
        self.pop.last_season = c.year * 4 + c.month() / 3;
        self.pop.last_month = c.year * 12 + c.month();
        self.pop.last_year = c.year;
        self.pop.last_day = now / DAY;
        // Unburied corpses of dwarves from before the sim started.
        let mut old = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if it.kind != "CORPSE" || self.imeta[i].dead || it.flags.in_building || it.building >= 0 {
                continue;
            }
            let dwarf = it.subtype == "DWARF" || it.desc.contains("dwarf");
            if dwarf && it.desc.contains("'s ") {
                let name = it.desc.split("'s ").next().unwrap_or("").to_string();
                old.push(Dead { unit_id: -1, name, died: 0, corpse: it.id, buried: false, memorial: false, ghost: false, old: true });
            }
        }
        self.pop.dead.extend(old);
        self.pop.dead_seen = vec![false; self.ai.len()];
        for s in 0..self.ai.len() {
            if self.world.units[self.ai[s].uidx].flags.dead {
                self.pop.dead_seen[s] = true;
            }
        }
        for (i, n) in self.world.nobles.iter().enumerate() {
            for (k, text) in n.mandates.iter().enumerate() {
                let (count, kind) = parse_mandate(text);
                let baseline = if kind.is_empty() { 0 } else { self.count_kind(&kind) };
                self.pop.mandates.push(Mandate {
                    noble: n.token.clone(),
                    text: text.clone(),
                    kind,
                    count,
                    baseline,
                    deadline: now + 3 * MONTH + (i * 7 + k) as u64 * DAY,
                });
            }
        }
    }

    /// Babies ride with their mother and are nursed (no food/drink of their own).
    fn pop_carry_babies(&mut self, now: u64) {
        for s in 0..self.ai.len() {
            let uidx = self.ai[s].uidx;
            if self.ai[s].kind != Kind::Worker || self.world.units[uidx].profession != "BABY" {
                continue;
            }
            let mother = self.world.units[uidx].relationships.iter().find(|r| r.kind == "Mother").map(|r| r.unit_id).unwrap_or(-1);
            let Some(&ms) = self.unit_slot.get(&mother) else { continue };
            if self.ai[ms].kind == Kind::Dead {
                continue;
            }
            let mpos = self.ai[ms].pos;
            let p = self.world.pos_of(mpos as usize);
            if self.ai[s].task != Task::Idle {
                self.cancel_task(s);
            }
            let ai = &mut self.ai[s];
            ai.pos = mpos;
            ai.thirst = 0;
            ai.hunger = 0;
            ai.sleep = 0;
            ai.sleeping = false;
            ai.last = now;
            ai.wake = now + POP_PERIOD + 1;
            self.world.units[uidx].pos = p;
        }
    }

    /// New deaths among citizens: remember them and their corpse for burial and ghosts.
    fn pop_check_deaths(&mut self, now: u64) {
        if self.pop.dead_seen.len() < self.ai.len() {
            self.pop.dead_seen.resize(self.ai.len(), false);
        }
        for s in 0..self.ai.len() {
            if self.pop.dead_seen[s] || self.ai[s].kind != Kind::Dead {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            if !u.flags.dead {
                continue; // left the map
            }
            self.pop.dead_seen[s] = true;
            if !u.flags.citizen || u.flags.animal {
                continue;
            }
            let (id, name) = (u.id, u.name.clone());
            let want = format!("{name}'s corpse");
            let corpse = self
                .world
                .items
                .iter()
                .enumerate()
                .rev()
                .take(4000)
                .find(|(i, it)| it.kind == "CORPSE" && !self.imeta[*i].dead && it.desc == want)
                .map(|(_, it)| it.id)
                .unwrap_or(-1);
            self.pop.dead.push(Dead { unit_id: id, name, died: now, corpse, buried: false, memorial: false, ghost: false, old: false });
            // Babies lose their carrier: they stay where they are.
            // Nobles: free the position.
            for n in &mut self.world.nobles {
                if n.holder == id {
                    n.holder = -1;
                }
            }
        }
    }

    /// Release burial/depot jobs whose hauler gave up.
    fn pop_check_claims(&mut self) {
        for k in 0..self.pop.bury.len() {
            let j = &self.pop.bury[k];
            if j.claimed == NONE {
                continue;
            }
            let s = j.claimed as usize;
            let busy = match self.ai[s].task {
                Task::GoPickup(id, _) | Task::GoStore(id, _) => id == j.corpse,
                _ => false,
            };
            if !busy || self.ai[s].kind == Kind::Dead {
                self.pop.bury[k].claimed = NONE;
            }
        }
    }

    fn pop_daily(&mut self, now: u64) {
        let c = self.world.calendar;
        // Aging, growing up, old age.
        let per_day = 1.0 / DAYS_PER_YEAR;
        let mut grown = Vec::new();
        let mut old_age = Vec::new();
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let uidx = self.ai[s].uidx;
            let max_age = self.max_age(self.world.units[uidx].id);
            let u = &mut self.world.units[uidx];
            if u.age <= 0.0 && !u.flags.citizen {
                continue;
            }
            let before = u.age;
            u.age += per_day;
            if !u.flags.citizen || u.race != "DWARF" {
                continue;
            }
            if u.profession == "BABY" && u.age >= BABY_AGE {
                u.profession = "CHILD".into();
                self.ai[s].wake = now + 1;
            }
            if u.profession == "CHILD" && u.age >= ADULT_AGE && before < u.age.floor().max(ADULT_AGE) {
                grown.push(s);
            }
            if u.age >= max_age {
                old_age.push(s);
            }
        }
        if !grown.is_empty() {
            let labors = self.default_labors();
            for s in grown {
                let uidx = self.ai[s].uidx;
                let u = &mut self.world.units[uidx];
                u.profession = "PEASANT".into();
                u.labors = labors.clone();
                let name = u.name.clone();
                let pos = u.pos;
                self.ai[s].labors = labor::bits(&labors);
                self.ai[s].seen_jobs = 0;
                self.ai[s].seen_haul = 0;
                self.ai[s].seen_prod = 0;
                self.announce(now, format!("{name} has grown up and is now a Peasant."), Some(pos), AnnouncementKind::Info);
                self.pop.rep.grown_up.push((now, name));
            }
        }
        for s in old_age {
            let name = self.world.units[self.ai[s].uidx].name.clone();
            self.kill(s, now, "has died of old age");
            self.pop.rep.old_age_deaths.push((now, name));
        }
        self.pop_births(now);
        self.pop_burials(now);
        self.pop_ghosts(now);
        // Month / season / year starts.
        let mkey = c.year * 12 + c.month();
        if mkey != self.pop.last_month {
            self.pop.last_month = mkey;
            let (cit, kids) = self.pop_counts();
            self.pop.rep.months.push((c.year, c.month(), cit, kids));
            self.pop_monthly(now);
        }
        let skey = c.year * 4 + c.month() / 3;
        if skey != self.pop.last_season {
            self.pop.last_season = skey;
            self.pop_season(now);
        }
        if c.year != self.pop.last_year {
            self.pop.last_year = c.year;
            self.nobles_yearly(now);
        }
        self.nobles_daily(now);
    }

    // ------------------------------------------------------------------ migrants

    fn pop_season(&mut self, now: u64) {
        self.trade_season(now);
        self.mandates_season(now);
        if self.pop.off || self.pop.wave_due.is_some() || !self.has_civilization() {
            return;
        }
        let (cap, strict, _, _) = self.population_caps();
        let (cit, _) = self.pop_counts();
        let room = (cap - cit as i32).min(strict - cit as i32);
        if room <= 0 {
            return;
        }
        let mut r = Rng::for_unit(self.seed, 2, now, 0x316_A7E);
        if !r.chance(3, 4) {
            return;
        }
        let wealth: i64 = self.fort_wealth();
        let max = (4 + wealth / 25_000).clamp(1, 28) as u32;
        let n = (2 + r.below(max) as i32).min(room) as usize;
        let delay = (1 + r.below(27)) as u64 * DAY;
        self.pop.wave_due = Some((now + delay, n));
    }

    /// Sum of the base values of the fort's items (an approximation of DF's created wealth).
    pub(crate) fn fort_wealth(&self) -> i64 {
        let mut w = 0i64;
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].dead || it.flags.forbid {
                continue;
            }
            w += it.value.max(0) as i64;
        }
        w
    }

    /// Spawn a migrant wave of `n` dwarves at a map edge (debug/UI and the seasonal wave).
    /// Returns the number that arrived.
    pub fn spawn_migrants(&mut self, n: usize) -> usize {
        let now = self.ticks;
        let tiles = self.society_edge_tiles(0);
        if tiles.is_empty() || n == 0 {
            return 0;
        }
        let mut r = Rng::for_unit(self.seed, 3, now, 0x316);
        let start = tiles[r.below(tiles.len() as u32) as usize];
        let sp = self.nav.xyz(start as usize);
        let mut near = tiles.clone();
        near.sort_by_key(|&t| (crate::manhattan(sp, self.nav.xyz(t as usize)), t));
        let labors = self.default_labors();
        let mut next_id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        for k in 0..n {
            let tile = near[k % near.len()];
            let p = self.world.pos_of(tile as usize);
            let child = r.chance(1, 10);
            let age = if child { 2.0 + r.below(10) as f32 + r.below(100) as f32 / 100.0 } else { 18.0 + r.below(110) as f32 + r.below(100) as f32 / 100.0 };
            let sex = r.below(2) as i8;
            let mut u = self.gen_dwarf(&mut r, next_id, age, sex, p, None);
            if child {
                u.profession = "CHILD".into();
            } else {
                let (prof, skill, lab) = MIGRANT_JOBS[r.below(MIGRANT_JOBS.len() as u32) as usize];
                u.profession = prof.into();
                u.labors = labors.clone();
                if !skill.is_empty() {
                    let rating = 1 + r.below(6) as i32;
                    u.skills.push(Skill { id: skill.into(), rating, xp: 0 });
                    if !u.labors.iter().any(|l| l == lab) {
                        u.labors.push(lab.into());
                    }
                }
                // A couple of dabbling side skills.
                for _ in 0..r.below(3) {
                    let (_, sk, _) = MIGRANT_JOBS[r.below(MIGRANT_JOBS.len() as u32) as usize];
                    if !sk.is_empty() && !u.skills.iter().any(|s| s.id == sk) {
                        u.skills.push(Skill { id: sk.into(), rating: r.below(2) as i32, xp: 0 });
                    }
                }
            }
            self.world.units.push(u);
            next_id += 1;
        }
        self.init_units();
        self.pop.rep.waves.push((now, n));
        let p = self.world.pos_of(start as usize);
        self.announce(now, "Some migrants have arrived.".into(), Some(p), AnnouncementKind::Info);
        n
    }

    // ------------------------------------------------------------------ births

    fn pop_monthly(&mut self, now: u64) {
        if self.pop.off {
            return;
        }
        let (_, strict, baby_cap, pct) = self.population_caps();
        let (cit, kids) = self.pop_counts();
        let adults = cit.saturating_sub(kids) as i32;
        let cap = baby_cap.min(adults * pct / 100);
        if cap <= 0 || kids as i32 + self.pop.preg.len() as i32 >= cap || cit as i32 >= strict {
            return;
        }
        // Married couples on the map.
        let mut couples = Vec::new();
        for s in 0..self.ai.len() {
            if !self.pop_citizen_slot(s) {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            if u.sex != 0 || u.age < ADULT_AGE || Self::is_child(u) {
                continue;
            }
            let Some(sp) = u.relationships.iter().find(|r| r.kind == "Spouse" && r.unit_id >= 0) else { continue };
            let Some(&ts) = self.unit_slot.get(&sp.unit_id) else { continue };
            if !self.pop_citizen_slot(ts) || self.world.units[self.ai[ts].uidx].sex != 1 {
                continue;
            }
            if self.pop.preg.iter().any(|p| p.mother == u.id) {
                continue;
            }
            if self.pop.last_birth.iter().any(|&(m, t)| m == u.id && now < t + 12 * MONTH) {
                continue;
            }
            couples.push((u.id, sp.unit_id));
        }
        for (k, (m, f)) in couples.into_iter().enumerate() {
            let mut r = Rng::for_unit(self.seed, k, now, 0xBAB1);
            if r.chance(1, 12) {
                self.pop.preg.push(Pregnancy { mother: m, father: f, due: now + PREGNANCY_TICKS });
                self.pop.rep.pregnancies_started += 1;
            }
        }
    }

    fn pop_births(&mut self, now: u64) {
        let mut k = 0;
        while k < self.pop.preg.len() {
            if self.pop.preg[k].due > now {
                k += 1;
                continue;
            }
            let p = self.pop.preg.remove(k);
            let Some(&ms) = self.unit_slot.get(&p.mother) else { continue };
            if self.ai[ms].kind != Kind::Worker {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, ms, now, 0xB127);
            let n = if r.chance(1, 50) { 2 } else { 1 };
            for _ in 0..n {
                self.give_birth(ms, p.father, &mut r, now);
            }
            self.pop.last_birth.retain(|&(m, _)| m != p.mother);
            self.pop.last_birth.push((p.mother, now));
        }
    }

    fn give_birth(&mut self, ms: usize, father: i32, r: &mut Rng, now: u64) -> i32 {
        let muidx = self.ai[ms].uidx;
        let (mid, mname, mpos, mhf) = {
            let m = &self.world.units[muidx];
            (m.id, m.name.clone(), m.pos, m.hist_fig_id)
        };
        let fpart = self.unit_slot.get(&father).map(|&fs| {
            let f = &self.world.units[self.ai[fs].uidx];
            (f.name.split_whitespace().last().unwrap_or("").to_string(), f.name_native.split_whitespace().last().unwrap_or("").to_string(), f.hist_fig_id)
        });
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let sex = r.below(2) as i8;
        let last = fpart.as_ref().filter(|(a, b, _)| !a.is_empty() && !b.is_empty()).map(|(a, b, _)| (a.clone(), b.clone()));
        let mut baby = self.gen_dwarf(r, id, 0.0, sex, mpos, last);
        baby.profession = "BABY".into();
        baby.flags.resident = false;
        baby.relationships.push(Relationship { kind: "Mother".into(), hist_fig_id: mhf, unit_id: mid });
        let fhf = fpart.as_ref().map(|x| x.2).unwrap_or(-1);
        baby.relationships.push(Relationship { kind: "Father".into(), hist_fig_id: fhf, unit_id: father });
        let bname = baby.name.clone();
        self.world.units.push(baby);
        self.init_units();
        self.world.units[muidx].relationships.push(Relationship { kind: "Child".into(), hist_fig_id: -1, unit_id: id });
        if let Some(&fs) = self.unit_slot.get(&father) {
            let fu = self.ai[fs].uidx;
            self.world.units[fu].relationships.push(Relationship { kind: "Child".into(), hist_fig_id: -1, unit_id: id });
            if self.ai[fs].kind == Kind::Worker {
                self.feel(fs, "SpouseGaveBirth", Class::PosStrong, 300, id, 0, now);
            }
        }
        self.feel(ms, "GaveBirth", Class::PosStrong, 300, id, 0, now);
        let word = if sex == 0 { "girl" } else { "boy" };
        self.announce(now, format!("{mname} has given birth to a {word}."), Some(mpos), AnnouncementKind::Info);
        self.pop.rep.births.push((now, bname));
        id
    }

    // ------------------------------------------------------------------ burial

    /// Built coffins in tomb zones without a body: (coffin building id, tile, tomb owner).
    pub(crate) fn free_coffins(&self) -> Vec<(i32, u32, i32)> {
        let mut out = Vec::new();
        for b in &self.world.buildings {
            if b.kind != "Coffin" || !b.built {
                continue;
            }
            let occupied = b.items.iter().any(|id| self.item_idx.get(id).map(|&i| self.world.items[i as usize].kind == "CORPSE" || self.world.items[i as usize].kind == "CORPSEPIECE").unwrap_or(false));
            if occupied || self.pop.bury.iter().any(|j| j.coffin == b.id) {
                continue;
            }
            let tomb = self.world.buildings.iter().find(|z| z.kind == "Civzone" && z.subtype == "Tomb" && z.contains(b.center));
            let Some(tomb) = tomb else { continue };
            let Some(t) = self.world.idx(b.center) else { continue };
            out.push((b.id, t as u32, tomb.owner));
        }
        out
    }

    fn pop_burials(&mut self, now: u64) {
        // Finished: corpse gone (destroyed) or interred.
        let pending: Vec<usize> = (0..self.pop.dead.len()).filter(|&k| !self.pop.dead[k].buried && self.pop.dead[k].corpse >= 0).collect();
        if pending.is_empty() {
            return;
        }
        let mut coffins = self.free_coffins();
        for k in pending {
            let (cid, uid) = (self.pop.dead[k].corpse, self.pop.dead[k].unit_id);
            if self.pop.bury.iter().any(|j| j.corpse == cid) {
                continue;
            }
            let Some(&ii) = self.item_idx.get(&cid) else {
                self.pop.dead[k].corpse = -1;
                continue;
            };
            let ii = ii as usize;
            if self.imeta[ii].dead {
                self.pop.dead[k].corpse = -1;
                continue;
            }
            let it = &self.world.items[ii];
            if it.flags.forbid || it.flags.in_building || it.building >= 0 || it.container >= 0 || it.holder >= 0 {
                continue;
            }
            // A tomb assigned to this dwarf first, else an unassigned one; living owners keep theirs.
            let pick = coffins.iter().position(|c| uid >= 0 && c.2 == uid).or_else(|| coffins.iter().position(|c| c.2 < 0 || !self.unit_alive(c.2)));
            let Some(pi) = pick else { break };
            let (bid, tile, _) = coffins.remove(pi);
            self.pop.bury.push(BuryJob { corpse: cid, coffin: bid, tile, claimed: NONE });
        }
        let _ = now;
    }

    fn unit_alive(&self, id: i32) -> bool {
        self.unit_slot.get(&id).map(|&s| self.ai[s].kind != Kind::Dead).unwrap_or(false)
    }

    /// [hook] Decide: burial and depot hauling, broker duty. True if the unit got a task.
    pub(crate) fn society_decide(&mut self, s: usize, now: u64) -> bool {
        if self.pop.bury.is_empty() && self.trade.pending.is_empty() && !self.trade.broker_called {
            return false;
        }
        if self.trade.broker_called && self.trade_broker_decide(s, now) {
            return true;
        }
        let labors = self.ai[s].labors;
        if !self.pop.bury.is_empty() && labors & labor::HAUL_BODY != 0 {
            let pos = self.ai[s].pos as usize;
            let c = self.nav.comp_near(pos);
            for k in 0..self.pop.bury.len() {
                let j = &self.pop.bury[k];
                if j.claimed != NONE {
                    continue;
                }
                let (cid, tile) = (j.corpse, j.tile);
                let Some(&ii) = self.item_idx.get(&cid) else { continue };
                let ii = ii as usize;
                let m = &self.imeta[ii];
                if m.dead || m.hauling || m.reserved > 0 || m.blocked_until > now {
                    continue;
                }
                let Some(ct) = self.world.idx(self.world.items[ii].pos) else { continue };
                if c == 0 || self.nav.comp_near(ct) != c || self.nav.comp_near(tile as usize) != c {
                    continue;
                }
                self.set_hauling(cid, true);
                self.haul_remove(cid);
                self.pop.bury[k].claimed = s as u32;
                self.ai[s].task = Task::GoPickup(cid, tile);
                let g = self.item_goals(cid);
                self.request_path(s, g, now);
                return true;
            }
        }
        self.trade_haul_decide(s, now)
    }

    /// [hook] A hauler put item `id` down at `tile` (end of a GoStore).
    pub(crate) fn society_stored(&mut self, id: i32, tile: u32) {
        if let Some(k) = self.pop.bury.iter().position(|j| j.corpse == id && j.tile == tile) {
            let j = self.pop.bury.remove(k);
            self.inter(id, j.coffin, tile);
            return;
        }
        self.trade_stored(id, tile);
    }

    /// Put a corpse into a coffin building.
    fn inter(&mut self, id: i32, coffin: i32, tile: u32) {
        let now = self.ticks;
        let Some(&ii) = self.item_idx.get(&id) else { return };
        let ii = ii as usize;
        self.haul_remove(id);
        let was_loose = self.world.items[ii].flags.on_ground && self.world.items[ii].container < 0;
        {
            let it = &mut self.world.items[ii];
            it.flags.on_ground = false;
            it.flags.in_building = true;
            it.building = coffin;
        }
        if was_loose {
            self.pile_occ_add(tile, -1);
        }
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == coffin) {
            b.items.push(id);
        }
        if let Some(d) = self.pop.dead.iter_mut().find(|d| d.corpse == id) {
            d.buried = true;
            let (name, ghost) = (d.name.clone(), d.ghost);
            d.ghost = false;
            self.pop.rep.burials.push((now, name.clone()));
            if ghost {
                self.pop.rep.ghosts_laid.push((now, name.clone()));
                self.announce(now, format!("{name} has been laid to rest."), None, AnnouncementKind::Info);
            }
        }
    }

    // ------------------------------------------------------------------ ghosts

    fn pop_ghosts(&mut self, now: u64) {
        // Engraved memorial slabs put the oldest ghost (or unburied dead) to rest.
        let ghost_idx = (0..self.pop.dead.len()).find(|&k| self.pop.dead[k].ghost);
        if let Some(k) = ghost_idx {
            let slab = self.world.items.iter().enumerate().find(|(i, it)| it.kind == "SLAB" && it.name == "memorial" && !self.imeta[*i].dead).map(|(i, _)| i);
            if let Some(si) = slab {
                let name = self.pop.dead[k].name.clone();
                self.world.items[si].name = format!("memorial to {name}");
                self.world.items[si].desc = format!("memorial slab to {name}");
                let d = &mut self.pop.dead[k];
                d.memorial = true;
                d.ghost = false;
                self.pop.rep.ghosts_laid.push((now, name.clone()));
                self.announce(now, format!("{name} has been laid to rest."), None, AnnouncementKind::Info);
            }
        }
        // Rising.
        for k in 0..self.pop.dead.len() {
            let d = &self.pop.dead[k];
            if d.old || d.ghost || d.buried || d.memorial || now < d.died + 14 * DAY {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, k, now, 0x6405);
            if r.chance(1, 7) {
                let name = d.name.clone();
                self.pop.dead[k].ghost = true;
                self.pop.rep.ghosts_risen.push((now, name.clone()));
                self.announce(now, format!("The spirit of {name} has risen and is haunting the fortress!"), None, AnnouncementKind::Warning);
            }
        }
        // Haunting: each ghost torments one citizen a day.
        let ghosts: Vec<i32> = self.pop.dead.iter().filter(|d| d.ghost).map(|d| d.unit_id).collect();
        if ghosts.is_empty() {
            return;
        }
        let living: Vec<usize> = (0..self.ai.len()).filter(|&s| self.pop_citizen_slot(s) && self.world.units[self.ai[s].uidx].profession != "BABY").collect();
        if living.is_empty() {
            return;
        }
        for (k, gid) in ghosts.into_iter().enumerate() {
            let mut r = Rng::for_unit(self.seed, k, now, 0x4A07);
            let t = living[r.below(living.len() as u32) as usize];
            let thought = if self.ai[t].sleeping { "GhostNightmare" } else { "GhostHaunt" };
            self.feel(t, thought, Class::Horror, 200, gid, 0, now);
        }
    }

    // ------------------------------------------------------------------ nobles

    fn election_score(&self, s: usize) -> i64 {
        let u = &self.world.units[self.ai[s].uidx];
        let social: i32 = ["PERSUASION", "NEGOTIATION", "JUDGING_INTENT", "CONVERSATION", "LEADERSHIP", "ORGANIZATION", "COMEDY", "CONSOLE", "PACIFY"]
            .iter()
            .map(|k| u.skill(k).clamp(0, 20))
            .sum();
        let friends = u.relationships.iter().filter(|r| matches!(r.kind.as_str(), "Friend" | "CloseFriend" | "KindredSpirit")).count() as i64;
        social as i64 * 10 + friends * 2 * 10
    }

    /// Hold an election for `token` (MAYOR or EXPEDITION_LEADER) and install the winner.
    fn elect(&mut self, ni: usize, now: u64) {
        let incumbent = self.world.nobles[ni].holder;
        let mut best = (-1i64, -1i32, usize::MAX);
        for s in 0..self.ai.len() {
            if !self.pop_citizen_slot(s) {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            if Self::is_child(u) {
                continue;
            }
            let mut sc = self.election_score(s);
            if u.id == incumbent {
                sc = sc * 3 / 2;
            }
            if (sc, -u.id) > (best.0, -best.1) {
                best = (sc, u.id, s);
            }
        }
        if best.2 == usize::MAX {
            return;
        }
        let (id, s) = (best.1, best.2);
        let n = &mut self.world.nobles[ni];
        let changed = n.holder != id;
        n.holder = id;
        let title = if n.name.is_empty() { n.token.to_ascii_lowercase().replace('_', " ") } else { n.name.clone() };
        if changed {
            let name = self.world.units[self.ai[s].uidx].name.clone();
            let text = if n.token == "MAYOR" { format!("{name} has been elected {title}.") } else { format!("{name} is now the {title}.") };
            self.announce(now, text.clone(), None, AnnouncementKind::Info);
            self.pop.rep.elections.push((now, text));
            self.feel(s, "BecomeNoble", Class::Pos, 200, -1, 0, now);
        }
    }

    fn nobles_daily(&mut self, now: u64) {
        if self.world.nobles.is_empty() {
            return;
        }
        // Holders who died or left.
        for k in 0..self.world.nobles.len() {
            let h = self.world.nobles[k].holder;
            if h >= 0 && !self.unit_alive(h) && self.unit_slot.contains_key(&h) {
                self.world.nobles[k].holder = -1;
            }
        }
        let (cit, _) = self.pop_counts();
        let has_mayor = self.world.nobles.iter().any(|n| n.token == "MAYOR");
        if cit >= MAYOR_POPULATION && !has_mayor {
            if let Some(k) = self.world.nobles.iter().position(|n| n.token == "EXPEDITION_LEADER") {
                let n = &mut self.world.nobles[k];
                n.token = "MAYOR".into();
                n.name = "mayor".into();
                // The position is new: announce even if the expedition leader wins.
                n.holder = -1;
                self.elect(k, now);
            }
        }
        for k in 0..self.world.nobles.len() {
            let n = &self.world.nobles[k];
            if n.holder < 0 && matches!(n.token.as_str(), "MAYOR" | "EXPEDITION_LEADER") {
                self.elect(k, now);
            }
        }
        // Mandates.
        let mut k = 0;
        while k < self.pop.mandates.len() {
            let m = &self.pop.mandates[k];
            let ban = m.kind.starts_with("BAN:");
            let late = now >= m.deadline;
            // An export ban that ran out without being broken counts as kept.
            let done = if ban { late } else { !m.kind.is_empty() && self.count_kind(&m.kind) - m.baseline >= m.count };
            if !done && !late {
                k += 1;
                continue;
            }
            let m = self.pop.mandates.remove(k);
            let holder = self.world.nobles.iter().find(|n| n.token == m.noble).map(|n| n.holder).unwrap_or(-1);
            let slot = self.unit_slot.get(&holder).copied().filter(|&s| self.ai[s].kind == Kind::Worker);
            if done {
                self.pop.rep.mandates_met += 1;
                if let Some(s) = slot {
                    self.feel(s, "MandateDeadlineMet", Class::Pos, 150, -1, 0, now);
                }
            } else {
                self.pop.rep.mandates_missed += 1;
                if let Some(s) = slot {
                    self.feel(s, "MandateIgnored", Class::Neg, 200, -1, 0, now);
                }
                self.justice_mandate_missed(&m.noble, false, now); // [society2] crime: ProductionOrderViolation
                self.announce(now, format!("The {} is upset: the mandate \"{}\" was not fulfilled.", m.noble.to_ascii_lowercase().replace('_', " "), m.text), None, AnnouncementKind::Warning);
            }
        }
    }

    /// Trading away goods under an export ban angers the noble (punishment not simulated).
    /// Returns the bans broken by offering items with these descriptions/subtypes.
    pub(crate) fn check_export_bans(&mut self, items: &[(String, String)], now: u64) -> usize {
        let mut broken = 0;
        let mut k = 0;
        while k < self.pop.mandates.len() {
            let m = &self.pop.mandates[k];
            let Some(word) = m.kind.strip_prefix("BAN:") else {
                k += 1;
                continue;
            };
            let w = word.to_ascii_lowercase();
            let hit = items.iter().any(|(desc, sub)| desc.to_ascii_lowercase().contains(&w) || sub.to_ascii_lowercase().contains(&w));
            if !hit {
                k += 1;
                continue;
            }
            let m = self.pop.mandates.remove(k);
            broken += 1;
            self.pop.rep.mandates_missed += 1;
            let holder = self.world.nobles.iter().find(|n| n.token == m.noble).map(|n| n.holder).unwrap_or(-1);
            if let Some(s) = self.unit_slot.get(&holder).copied().filter(|&s| self.ai[s].kind == Kind::Worker) {
                self.feel(s, "MandateIgnored", Class::Neg, 200, -1, 0, now);
            }
            self.justice_mandate_missed(&m.noble, true, now); // [society2] crime: ExportViolation
            self.announce(now, format!("The {} is upset: the export ban \"{}\" was broken.", m.noble.to_ascii_lowercase().replace('_', " "), m.text), None, AnnouncementKind::Warning);
        }
        broken
    }

    fn nobles_yearly(&mut self, now: u64) {
        if let Some(k) = self.world.nobles.iter().position(|n| n.token == "MAYOR") {
            self.elect(k, now);
        }
    }

    fn mandates_season(&mut self, now: u64) {
        for k in 0..self.world.nobles.len() {
            let (tok, holder) = (self.world.nobles[k].token.clone(), self.world.nobles[k].holder);
            if !is_noble_with_mandates(&tok) || holder < 0 || !self.unit_alive(holder) {
                continue;
            }
            if self.pop.mandates.iter().any(|m| m.noble == tok) {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, k, now, 0x3A4D);
            if !r.chance(1, 3) {
                continue;
            }
            let (word, kind) = MANDATE_ITEMS[r.below(MANDATE_ITEMS.len() as u32) as usize];
            let count = 1 + r.below(5) as i32;
            let text = format!("Make {count} {word}");
            let baseline = self.count_kind(kind);
            self.world.nobles[k].mandates.push(text.clone());
            self.pop.mandates.push(Mandate { noble: tok.clone(), text: text.clone(), kind: kind.into(), count, baseline, deadline: now + 3 * MONTH });
            self.pop.rep.mandates_issued += 1;
            let title = tok.to_ascii_lowercase().replace('_', " ");
            self.announce(now, format!("The {title} has mandated: {text}."), None, AnnouncementKind::Info);
        }
        // Drop texts of finished mandates from the positions.
        let active: Vec<(String, String)> = self.pop.mandates.iter().map(|m| (m.noble.clone(), m.text.clone())).collect();
        for n in &mut self.world.nobles {
            let tok = n.token.clone();
            n.mandates.retain(|t| active.iter().any(|(a, b)| *a == tok && b == t));
        }
    }

    // ------------------------------------------------------------------ round-4 API bodies

    pub(crate) fn appoint_impl(&mut self, token: &str, unit_id: i32) {
        let now = self.ticks;
        let k = match self.world.nobles.iter().position(|n| n.token == token) {
            Some(k) => k,
            None => {
                // Position not in the export (e.g. a round-3 export): create it for the fort.
                self.world.nobles.push(rf_core::NoblePosition {
                    token: token.into(),
                    name: token.to_ascii_lowercase().replace('_', " "),
                    holder: -1,
                    entity_id: self.world.meta.group_id,
                    mandates: Vec::new(),
                });
                self.world.nobles.len() - 1
            }
        };
        let changed = self.world.nobles[k].holder != unit_id;
        self.world.nobles[k].holder = unit_id;
        if !changed || unit_id < 0 {
            return;
        }
        if let Some(&s) = self.unit_slot.get(&unit_id) {
            if self.ai[s].kind == Kind::Worker {
                self.feel(s, "BecomeNoble", Class::Pos, 200, -1, 0, now);
                let name = self.world.units[self.ai[s].uidx].name.clone();
                let title = self.world.nobles[k].name.clone();
                self.announce(now, format!("{name} has been appointed {title}."), None, AnnouncementKind::Info);
            }
        }
    }

    pub(crate) fn population_info_impl(&self) -> crate::PopulationInfo {
        let (cit, kids) = self.pop_counts();
        let (cap, _, _, _) = self.population_caps();
        let now = self.ticks;
        let next_wave_in = match self.pop.wave_due {
            Some((t, _)) => Some(t.saturating_sub(now)),
            None if (cit as i32) < cap && !self.pop.off => {
                let c = self.world.calendar;
                let into = (c.year_tick % (3 * TICKS_PER_MONTH)) as u64;
                Some(3 * MONTH - into)
            }
            None => None,
        };
        crate::PopulationInfo { citizens: cit, children: kids, population_cap: cap, next_wave_in, caravan_expected: self.caravan_expected() }
    }

    pub(crate) fn set_population_cap_impl(&mut self, cap: i32) {
        let s = &mut self.world.settings;
        if s.population_cap <= 0 && s.strict_population_cap <= 0 && s.baby_cap <= 0 {
            s.strict_population_cap = DEFAULT_STRICT_CAP;
            s.baby_cap = DEFAULT_BABY_CAP;
        }
        s.population_cap = cap;
        if s.strict_population_cap < cap {
            s.strict_population_cap = cap;
        }
    }

    fn count_kind(&self, kind: &str) -> i32 {
        self.world.items.iter().enumerate().filter(|(i, it)| it.kind == kind && !self.imeta[*i].dead).count() as i32
    }
}

/// "Make 3 goblets (iron)" -> (3, "GOBLET"); "Ban export of spears" -> (0, "BAN:spear");
/// unknown -> (0, "").
fn parse_mandate(text: &str) -> (i32, String) {
    let t = text.to_ascii_lowercase();
    if let Some(rest) = t.strip_prefix("ban export of ") {
        let w = rest.split(" (").next().unwrap_or(rest).trim();
        let stem = w.strip_suffix("es").filter(|s| s.ends_with("ss") || s.ends_with("sh") || s.ends_with("ch")).or_else(|| w.strip_suffix('s')).unwrap_or(w);
        return (0, format!("BAN:{stem}"));
    }
    let mut words = t.split_whitespace();
    if words.next() != Some("make") {
        return (0, String::new());
    }
    let n: i32 = words.next().and_then(|w| w.parse().ok()).unwrap_or(1);
    let rest: String = words.collect::<Vec<_>>().join(" ");
    for (w, k) in MANDATE_ITEMS {
        let stem = w.trim_end_matches('s');
        if rest.contains(stem) || (k == &"GOBLET" && rest.contains("goblet")) {
            return (n, k.to_string());
        }
    }
    (0, String::new())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rf_core::{Building, World};

    /// Synthetic fort plus a built coffin in a tomb zone at the hall's corner.
    pub(crate) fn world_with_tomb() -> World {
        let mut w = World::synthetic(48, 48, 20);
        let hz = (20i16 * 2 / 3) - 4;
        for u in &mut w.units {
            if u.flags.citizen {
                u.labors.push("HAUL_BODY".into());
            }
        }
        let p = Pos::new(19, 27, hz);
        w.buildings.push(Building { id: 500, kind: "Coffin".into(), x1: p.x, y1: p.y, x2: p.x, y2: p.y, z: p.z, center: p, built: true, owner: -1, ..Default::default() });
        w.buildings.push(Building { id: 501, kind: "Civzone".into(), subtype: "Tomb".into(), x1: p.x, y1: p.y, x2: p.x, y2: p.y, z: p.z, center: p, built: true, owner: -1, ..Default::default() });
        w
    }

    fn slot_of(sim: &Sim, id: i32) -> usize {
        sim.unit_slot[&id]
    }

    pub(crate) fn marry(w: &mut World, a: usize, b: usize) {
        let (ida, idb) = (w.units[a].id, w.units[b].id);
        w.units[a].sex = 0;
        w.units[b].sex = 1;
        w.units[a].relationships.push(Relationship { kind: "Spouse".into(), hist_fig_id: -1, unit_id: idb });
        w.units[b].relationships.push(Relationship { kind: "Spouse".into(), hist_fig_id: -1, unit_id: ida });
    }

    #[test]
    fn caps_default_and_settings() {
        let mut sim = Sim::new(World::synthetic(48, 48, 20), 1);
        assert_eq!(sim.population_caps(), (200, 220, 100, 1000));
        sim.world.settings.population_cap = 75;
        sim.world.settings.strict_population_cap = 100;
        sim.world.settings.baby_cap = 266;
        assert_eq!(sim.population_caps(), (75, 100, 266, 1000));
        let mut sim = Sim::new(World::synthetic(48, 48, 20), 1);
        sim.set_population_cap(10);
        assert_eq!(sim.population_caps().0, 10);
        assert_eq!(sim.population_info().population_cap, 10);
    }

    #[test]
    fn migrants_arrive_as_citizens_at_the_edge() {
        let mut sim = Sim::new(World::synthetic(48, 48, 20), 3);
        let before = sim.world.citizens().count();
        let n = sim.spawn_migrants(6);
        assert_eq!(n, 6);
        assert_eq!(sim.world.citizens().count(), before + 6);
        let new: Vec<Unit> = sim.world.units.iter().rev().take(6).cloned().collect();
        for u in &new {
            assert_eq!(u.race, "DWARF");
            assert!(u.flags.citizen);
            let p = u.pos;
            assert!(p.x == 0 || p.y == 0 || p.x == 47 || p.y == 47, "arrives at the map edge: {p:?}");
            assert!(u.phys_attrs.iter().any(|(k, v)| k == "STRENGTH" && *v >= 450 && *v <= 2250));
            assert!(u.name.contains(' '));
        }
        assert!(sim.announcements.iter().any(|a| a.text == "Some migrants have arrived."));
        sim.run_ticks(2000);
        let s = slot_of(&sim, new[0].id);
        assert_eq!(sim.ai[s].kind, Kind::Worker);
    }

    #[test]
    fn no_migrant_waves_at_the_population_cap() {
        let mut w = World::synthetic(48, 48, 20);
        let mut sim = Sim::new(w.clone(), 4);
        sim.set_population_cap(60);
        sim.pop_season(7);
        assert!(sim.pop.wave_due.is_none(), "synthetic world: no civilization, no migrants");
        w.meta.source = "export:test".into();
        let mut sim = Sim::new(w, 4);
        sim.set_population_cap(7);
        for k in 0..12u64 {
            sim.pop_season(k * 3 * MONTH);
        }
        assert!(sim.pop.wave_due.is_none());
        sim.set_population_cap(60);
        let mut scheduled = 0;
        for k in 0..12u64 {
            sim.pop.wave_due = None;
            sim.pop_season(k * 3 * MONTH + 7);
            if let Some((_, n)) = sim.pop.wave_due {
                assert!((2..=53).contains(&n));
                scheduled += 1;
            }
        }
        assert!(scheduled >= 5, "about 3 in 4 seasons bring a wave, got {scheduled}");
    }

    #[test]
    fn married_couples_conceive_unless_capped() {
        let mut w = World::synthetic(48, 48, 20);
        marry(&mut w, 1, 0);
        let mut sim = Sim::new(w.clone(), 5);
        for m in 0..48u64 {
            sim.pop_monthly(m * MONTH + 11);
        }
        assert_eq!(sim.pop.preg.len(), 1, "one pregnancy at a time");
        // Baby cap 0 disallows pregnancies (d_init).
        let mut w2 = w.clone();
        w2.settings.population_cap = 200;
        w2.settings.strict_population_cap = 220;
        w2.settings.baby_cap = 0;
        let mut sim = Sim::new(w2, 5);
        for m in 0..48u64 {
            sim.pop_monthly(m * MONTH + 11);
        }
        assert!(sim.pop.preg.is_empty());
        // The strict cap stops births too.
        let mut w3 = w;
        w3.settings.population_cap = 5;
        w3.settings.strict_population_cap = 7;
        w3.settings.baby_cap = 100;
        let mut sim = Sim::new(w3, 5);
        for m in 0..48u64 {
            sim.pop_monthly(m * MONTH + 11);
        }
        assert!(sim.pop.preg.is_empty());
    }

    #[test]
    fn birth_baby_is_carried_and_grows_up() {
        let mut w = World::synthetic(48, 48, 20);
        marry(&mut w, 1, 0);
        let (mother, father) = (w.units[1].id, w.units[0].id);
        let mut sim = Sim::new(w, 6);
        sim.run_ticks(10);
        sim.pop.preg.push(Pregnancy { mother, father, due: sim.ticks + 5 });
        sim.run_ticks(2 * DAY as u32);
        let r = sim.population_report();
        assert_eq!(r.births.len(), 1);
        let baby = sim.world.units.iter().find(|u| u.profession == "BABY").expect("baby").clone();
        assert!(baby.relationships.iter().any(|r| r.kind == "Mother" && r.unit_id == mother));
        assert!(sim.world.unit(mother).unwrap().relationships.iter().any(|r| r.kind == "Child" && r.unit_id == baby.id));
        assert!(sim.announcements.iter().any(|a| a.text.contains("has given birth to a")));
        sim.run_ticks(3000);
        let mpos = sim.world.unit(mother).unwrap().pos;
        let bpos = sim.world.unit(baby.id).unwrap().pos;
        assert!(mpos.dist(bpos) <= 3, "baby rides with the mother: {mpos:?} vs {bpos:?}");
        // One year later: child; 13 years: adult peasant with labors.
        let bs = slot_of(&sim, baby.id);
        let ui = sim.ai[bs].uidx;
        sim.world.units[ui].age = BABY_AGE - 0.001;
        sim.run_ticks(DAY as u32 + 1);
        assert_eq!(sim.world.units[ui].profession, "CHILD");
        sim.world.units[ui].age = ADULT_AGE - 0.001;
        sim.run_ticks(DAY as u32 + 1);
        assert_eq!(sim.world.units[ui].profession, "PEASANT");
        assert!(!sim.world.units[ui].labors.is_empty());
        assert!(sim.ai[bs].labors != 0);
    }

    #[test]
    fn imported_child_past_twelve_waits_for_the_birthday() {
        let mut w = World::synthetic(48, 48, 20);
        w.units[2].profession = "CHILD".into();
        w.units[2].age = 12.5;
        w.units[2].labors.clear();
        let mut sim = Sim::new(w, 7);
        sim.run_ticks(3 * DAY as u32);
        assert_eq!(sim.world.units[2].profession, "CHILD");
        sim.world.units[2].age = 12.999;
        sim.run_ticks(DAY as u32 + 1);
        assert_eq!(sim.world.units[2].profession, "PEASANT");
    }

    #[test]
    fn old_dwarves_die_of_old_age() {
        let mut w = World::synthetic(48, 48, 20);
        w.units[3].age = MAXAGE_MAX as f32 + 0.5;
        let id = w.units[3].id;
        let mut sim = Sim::new(w, 8);
        sim.run_ticks(DAY as u32 + 50);
        assert!(sim.world.unit(id).unwrap().flags.dead);
        assert_eq!(sim.population_report().old_age_deaths.len(), 1);
        assert!(sim.announcements.iter().any(|a| a.text.ends_with("has died of old age.")));
        for k in 0..50 {
            let a = sim.max_age(k);
            assert!((150.0..=170.0).contains(&a));
        }
    }

    #[test]
    fn dead_citizens_are_buried_in_a_tomb_coffin() {
        let w = world_with_tomb();
        let id = w.units[4].id;
        let mut sim = Sim::new(w, 9);
        sim.run_ticks(20);
        let s = slot_of(&sim, id);
        sim.kill(s, sim.ticks, "has died");
        sim.run_ticks(6 * DAY as u32);
        let r = sim.population_report();
        assert_eq!(r.burials.len(), 1, "report {r:?}");
        let corpse = sim.world.items.iter().find(|i| i.kind == "CORPSE").unwrap();
        assert_eq!(corpse.building, 500);
        assert!(corpse.flags.in_building && !corpse.flags.on_ground);
        assert!(sim.world.building(500).unwrap().items.contains(&corpse.id));
        assert_eq!(r.free_coffins, 0);
        assert_eq!(r.ghosts_risen.len(), 0);
    }

    #[test]
    fn unburied_dead_rise_as_ghosts_until_memorialized() {
        let w = World::synthetic(48, 48, 20);
        let id = w.units[4].id;
        let mut sim = Sim::new(w, 10);
        sim.run_ticks(20);
        let s = slot_of(&sim, id);
        sim.kill(s, sim.ticks, "has died");
        sim.run_ticks(60 * DAY as u32);
        let r = sim.population_report();
        assert_eq!(r.ghosts_risen.len(), 1);
        assert_eq!(r.ghosts_active, 1);
        let haunted = sim.world.units.iter().filter(|u| u.emotions.iter().any(|e| e.thought == "GhostHaunt" || e.thought == "GhostNightmare")).count();
        assert!(haunted > 0);
        // An engraved memorial slab puts the ghost to rest.
        let t = sim.world.idx(sim.world.units[0].pos).unwrap();
        let sid = sim.create_item("SLAB", 0, t, "engraved granite slab".into());
        let si = sim.item_idx[&sid] as usize;
        sim.world.items[si].name = "memorial".into();
        sim.run_ticks(2 * DAY as u32);
        let r = sim.population_report();
        assert_eq!(r.ghosts_active, 0);
        assert_eq!(r.ghosts_laid.len(), 1);
        assert!(sim.world.items.iter().any(|i| i.name.starts_with("memorial to ")));
    }

    #[test]
    fn mayor_is_elected_at_fifty_citizens() {
        let mut w = World::synthetic(48, 48, 20);
        w.nobles.push(rf_core::NoblePosition { token: "EXPEDITION_LEADER".into(), name: "expedition leader".into(), holder: 1, entity_id: 0, mandates: vec![] });
        let mut sim = Sim::new(w, 11);
        sim.run_ticks(2 * DAY as u32);
        assert!(sim.world.nobles.iter().all(|n| n.token != "MAYOR"));
        sim.spawn_migrants(45);
        sim.run_ticks(2 * DAY as u32);
        let mayor = sim.world.nobles.iter().find(|n| n.token == "MAYOR").expect("mayor").clone();
        assert!(mayor.holder >= 0);
        assert!(sim.population_report().elections.iter().any(|(_, t)| t.contains("elected mayor")));
        // A vacant mayor is replaced.
        let h = mayor.holder;
        let s = slot_of(&sim, h);
        sim.kill(s, sim.ticks, "has died");
        sim.run_ticks(2 * DAY as u32);
        let mayor = sim.world.nobles.iter().find(|n| n.token == "MAYOR").unwrap();
        assert!(mayor.holder >= 0 && mayor.holder != h);
    }

    #[test]
    fn mandates_are_met_or_missed() {
        let mut w = World::synthetic(48, 48, 20);
        w.nobles.push(rf_core::NoblePosition { token: "MAYOR".into(), name: "mayor".into(), holder: 1, entity_id: 0, mandates: vec!["Make 2 chairs (granite)".into(), "Make 1 crowns".into()] });
        let mut sim = Sim::new(w, 12);
        sim.run_ticks(DAY as u32 + 50);
        assert_eq!(sim.pop.mandates.len(), 2);
        let t = sim.world.idx(sim.world.units[0].pos).unwrap();
        sim.create_item("CHAIR", 0, t, "granite chair".into());
        sim.create_item("CHAIR", 0, t, "granite chair".into());
        sim.run_ticks(DAY as u32 + 10);
        assert_eq!(sim.population_report().mandates_met, 1);
        // Deadline of the crown mandate (normally one season) comes now.
        let t = sim.ticks + 10;
        for m in &mut sim.pop.mandates {
            m.deadline = t;
        }
        sim.run_ticks(DAY as u32 + 20);
        assert_eq!(sim.population_report().mandates_missed, 1);
        let mayor = sim.world.unit(1).unwrap();
        assert!(mayor.emotions.iter().any(|e| e.thought == "MandateIgnored"));
        assert!(mayor.emotions.iter().any(|e| e.thought == "MandateDeadlineMet"));
    }

    #[test]
    fn appoint_creates_missing_position() {
        let mut sim = Sim::new(World::synthetic(48, 48, 20), 13);
        sim.appoint("BROKER", 3);
        assert_eq!(sim.world.nobles.iter().find(|n| n.token == "BROKER").unwrap().holder, 3);
        assert_eq!(sim.broker_unit(), Some(3));
    }

    #[test]
    fn population_is_deterministic_across_threads() {
        let run = |par: usize| {
            let mut w = world_with_tomb();
            marry(&mut w, 1, 0);
            let mut sim = Sim::new(w, 21);
            sim.set_parallel_threshold(par);
            sim.spawn_migrants(10);
            sim.run_ticks(20_000);
            sim.state_hash()
        };
        let a = run(1);
        let b = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap().install(|| run(1));
        let c = run(1000);
        assert_eq!(a, b);
        assert_eq!(a, c);
    }
}
