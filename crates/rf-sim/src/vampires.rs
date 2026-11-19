//! [vampire13] Vampires and werebeasts (DF v50; wiki Vampire, Werebeast, Night creature,
//! Justice, Calendar).
//!
//! # Rules implemented
//! - **Vampire migrants**: each seasonal migrant wave may hide one vampire among its adults
//!   (`VAMP_WAVE_CHANCE`). It lives under a false identity: `Unit::name` is the assumed name,
//!   `Unit::true_name` the real one (shown once unmasked). Vampires do not eat, drink, sleep or
//!   age and are sterile; they carry more and higher skills (wiki Vampire).
//! - **Feeding**: about once a month (`FEED_DAYS_*`) the vampire drinks from a sleeping citizen
//!   in its walk area, preferring one nobody awake is near. The victim usually dies ("drained
//!   of blood", `DRAIN_KILL`), otherwise it loses blood and remembers nothing. An awake citizen
//!   close by may see it: the vampire is unmasked at once and stands accused. A death without
//!   a witness is an unsolved Murder for `justice` (interrogation may still name the vampire,
//!   which makes it a suspect).
//! - **Unmasking**: caught in the act, or `Sim::accuse_vampire` on a real vampire. The unit
//!   takes back its true name; open crimes of the vampire become accusations.
//! - **Werebeasts**: the curse (`Unit::curse == "WEREBEAST"`, `curse_race` the were-form)
//!   makes a unit turn into its were-form on the full-moon day (moon phase 14 of 0..=27) and
//!   back the day after. In were-form it drops its gear and attacks everyone (combat runs it as
//!   a hostile). Werebeasts from the wilds may attack at a full moon (`WERE_ATTACK_CHANCE`, only with
//!   invasions on and at least `WERE_ATTACK_MIN_POP` citizens); after
//!   the full moon they leave the map. Visitors may carry the curse hidden
//!   (`WERE_VISITOR_CHANCE`). A creature wounded by a were-form's attack that survives is
//!   infected and turns at the next full moon.
//!
//! # Estimates (not on the wiki; see docs/FIDELITY.md section `[vampire13]`)
//! The chances and intervals below, the moon calendar (one 28-day lunar cycle per DF month,
//! full moon on the 15th), the one-day transformation, the were-form attributes and the
//! direct walk to the victim (not simulated: the vampire appears beside the sleeper).

use crate::combat::Role;
use crate::rng::Rng;
use crate::round13_api::{Threat, ThreatKind, UnitCurse, VampireReport};
use crate::{AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{Skill, Unit, UnitFlags, TICKS_PER_DAY};
use serde::{Deserialize, Serialize};

/// `vampires_pass` runs when `now % VAMP_PERIOD == VAMP_PHASE` (owner may change both).
pub(crate) const VAMP_PERIOD: u64 = 100;
pub(crate) const VAMP_PHASE: u64 = 83;

const DAY: u64 = TICKS_PER_DAY as u64;
/// Estimate: chance that a migrant wave brings a vampire (wiki: "many forts see one within
/// the first few years"; about 3 waves a year).
pub(crate) const VAMP_WAVE_CHANCE: (u32, u32) = (1, 12);
/// Estimate: days between feedings (wiki leaves "how often" open).
pub(crate) const FEED_DAYS_MIN: u64 = 28;
pub(crate) const FEED_DAYS_RAND: u32 = 28;
/// Estimate: a feeding kills the victim (wiki: "usually killing them").
pub(crate) const DRAIN_KILL: (u32, u32) = (2, 3);
/// Estimate: each awake citizen within `WITNESS_RANGE` tiles (same level) sees the feeding.
pub(crate) const WITNESS_SEE: (u32, u32) = (1, 2);
pub(crate) const WITNESS_RANGE: i32 = 6;
/// Estimate: a werebeast from the wilds attacks at a full moon.
pub(crate) const WERE_ATTACK_CHANCE: (u32, u32) = (1, 36);
/// Estimate: wild attacks need invasions on and the first invasion tier's population (20,
/// wiki Difficulty "Hamlet"); a tiny new fort is not hunted by night creatures.
pub(crate) const WERE_ATTACK_MIN_POP: usize = 20;
/// Estimate: a new visitor secretly carries the werebeast curse.
pub(crate) const WERE_VISITOR_CHANCE: (u32, u32) = (1, 60);
/// Moon phase of the full moon (contract: 0..=27, 14 = full).
pub(crate) const FULL_MOON: u8 = 14;

/// Animal bases of the generated were-forms (DF makes them per world; estimate list).
const WERE_BASES: [&str; 10] = ["WOLF", "BEAR", "HYENA", "JACKAL", "LION", "TIGER", "BADGER", "LIZARD", "ELK", "CROCODILE"];
/// Social skills a vampire collected over its long life (wiki Vampire: "4-5+ social skills").
const VAMP_SOCIAL: [&str; 8] = ["PERSUASION", "NEGOTIATION", "JUDGING_INTENT", "CONVERSATION", "FLATTERY", "LYING", "INTIMIDATION", "CONSOLE"];

/// A vampire on the map.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct VampRec {
    pub id: i32,
    /// The assumed name it arrived under.
    pub assumed: String,
    /// Apparent age (frozen: vampires do not age).
    pub age: f32,
    pub next_feed: u64,
    pub suspected: bool,
    pub unmasked: bool,
    pub feedings: u32,
    pub kills: u32,
    pub arrived: u64,
}

/// A unit carrying the werebeast curse.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WereRec {
    pub id: i32,
    pub transformed: bool,
    /// Came from the wilds to attack (leaves after the full moon).
    pub attacker: bool,
    pub since: u64,
    /// Tick of the last transformation (0 = never).
    pub changed: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Vamp {
    /// Switched off by the player.
    pub vampires_off: bool,
    pub werebeasts_off: bool,
    pub rep: VampireReport,
    pub vamps: Vec<VampRec>,
    pub weres: Vec<WereRec>,
    /// Units wounded by a were-form since the last pass: (victim id, were-form race).
    pub bitten: Vec<(i32, String)>,
    /// Bookkeeping (not hashed): migrant waves and units seen, last moon day handled.
    pub init: bool,
    pub seen_waves: usize,
    pub units_seen: usize,
    pub moon_day: i64,
}

impl Sim {
    /// Hook from `tick_inner` (every `VAMP_PERIOD` ticks).
    pub(crate) fn vampires_pass(&mut self, now: u64) {
        if !self.vamp.init {
            self.vamp.init = true;
            self.vamp.seen_waves = self.pop.rep.waves.len();
            self.vamp.units_seen = self.world.units.len();
            self.vamp.moon_day = self.abs_day();
        }
        self.vamp_new_units(now);
        if !self.vamp.vamps.is_empty() {
            self.vamp_upkeep(now);
        }
        if !self.vamp.bitten.is_empty() {
            self.were_infect(now);
        }
        let day = self.abs_day();
        if day != self.vamp.moon_day {
            self.vamp.moon_day = day;
            self.moon_day_start(now);
        }
    }

    fn abs_day(&self) -> i64 {
        let c = self.world.calendar;
        c.year as i64 * 336 + (c.year_tick / TICKS_PER_DAY) as i64
    }

    /// New migrant waves (vampires) and new visitors (hidden werebeasts).
    fn vamp_new_units(&mut self, now: u64) {
        let n = self.world.units.len();
        if n < self.vamp.units_seen {
            self.vamp.units_seen = n;
        }
        let waves = self.pop.rep.waves.len();
        if waves > self.vamp.seen_waves {
            let fresh = self.pop.rep.waves[waves - 1].0 + DAY >= now;
            self.vamp.seen_waves = waves;
            if fresh && !self.vamp.vampires_off {
                let mut r = Rng::for_unit(self.seed, 0, now, 0x7A3B_0001);
                if r.chance(VAMP_WAVE_CHANCE.0, VAMP_WAVE_CHANCE.1) {
                    let cands: Vec<usize> = (self.vamp.units_seen..n).filter(|&i| Self::vamp_candidate(&self.world.units[i])).collect();
                    if !cands.is_empty() {
                        let uidx = cands[r.below(cands.len() as u32) as usize];
                        self.make_vampire(uidx, now);
                    }
                }
            }
        }
        if !self.vamp.werebeasts_off {
            for i in self.vamp.units_seen..n {
                let u = &self.world.units[i];
                if !u.flags.visitor || u.flags.animal || u.flags.dead || !u.curse.is_empty() || u.undead {
                    continue;
                }
                let id = u.id;
                let mut r = Rng::for_unit(self.seed, 0, now, 0x7A3B_0002 ^ (id as u64) << 20);
                if r.chance(WERE_VISITOR_CHANCE.0, WERE_VISITOR_CHANCE.1) {
                    let race = format!("WERE{}", WERE_BASES[r.below(WERE_BASES.len() as u32) as usize]);
                    self.curse_were(i, &race, false, now);
                }
            }
        }
        self.vamp.units_seen = n;
    }

    fn vamp_candidate(u: &Unit) -> bool {
        u.flags.citizen && !u.flags.dead && !u.flags.animal && u.race == "DWARF" && u.age >= 18.0 && u.profession != "CHILD" && u.profession != "BABY" && u.curse.is_empty() && !u.undead
    }

    /// Turn the unit at `uidx` into a vampire under a false identity.
    fn make_vampire(&mut self, uidx: usize, now: u64) -> i32 {
        let mut r = Rng::for_unit(self.seed, uidx, now, 0x7A3B_0003);
        let (id, pos, sex, age, assumed) = {
            let u = &self.world.units[uidx];
            (u.id, u.pos, u.sex, u.age, u.name.clone())
        };
        // The real name: a fresh dwarf name of the civilization (not the assumed one).
        let mut true_name = String::new();
        for _ in 0..4 {
            true_name = self.gen_dwarf(&mut r, id, age, sex, pos, None).name;
            if true_name != assumed {
                break;
            }
        }
        let u = &mut self.world.units[uidx];
        u.curse = "VAMPIRE".into();
        u.true_name = true_name;
        u.pregnancy = 0;
        // Centuries of practice: more and higher skills (wiki Vampire; amounts are estimates).
        for s in &mut u.skills {
            s.rating = (s.rating + 2 + r.below(4) as i32).min(15);
        }
        let k = 4 + r.below(3) as usize;
        let start = r.below(VAMP_SOCIAL.len() as u32) as usize;
        for j in 0..k {
            let sk = VAMP_SOCIAL[(start + j) % VAMP_SOCIAL.len()];
            if !u.skills.iter().any(|s| s.id == sk) {
                u.skills.push(Skill { id: sk.into(), rating: 2 + r.below(6) as i32, xp: 0 });
            }
        }
        let feed = now + FEED_DAYS_MIN * DAY + r.below(FEED_DAYS_RAND) as u64 * DAY;
        self.vamp.vamps.push(VampRec { id, assumed, age, next_feed: feed, arrived: now, ..Default::default() });
        self.vamp.rep.vampires_arrived += 1;
        id
    }

    /// No needs, no aging, suspicion from justice, feeding.
    fn vamp_upkeep(&mut self, now: u64) {
        self.vamp.vamps.retain(|v| self.unit_slot.get(&v.id).map(|&s| self.ai[s].kind != Kind::Dead).unwrap_or(false));
        for k in 0..self.vamp.vamps.len() {
            let (id, age) = (self.vamp.vamps[k].id, self.vamp.vamps[k].age);
            let Some(&s) = self.unit_slot.get(&id) else { continue };
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            // Vampires do not eat, drink or sleep (wiki Vampire).
            let ai = &mut self.ai[s];
            ai.thirst = 0;
            ai.hunger = 0;
            ai.sleep = 0;
            ai.last = now;
            let u = &mut self.world.units[ai.uidx];
            u.age = age;
            u.pregnancy = 0;
            if !self.vamp.vamps[k].suspected {
                let accused = self.world.crimes.iter().any(|c| c.criminal == id && c.kind == "Murder");
                if accused {
                    self.vamp.vamps[k].suspected = true;
                }
            }
            if !self.vamp.vampires_off && now >= self.vamp.vamps[k].next_feed {
                self.vamp_feed(k, s, now);
            }
        }
    }

    /// The vampire in slot `s` (record `k`) drinks from a sleeping citizen if there is one.
    fn vamp_feed(&mut self, k: usize, s: usize, now: u64) {
        if self.ai[s].kind != Kind::Worker || self.combat_owns(s) || self.justice_is_prisoner(self.ai[s].id) {
            return;
        }
        let vpos = self.ai[s].pos as usize;
        let comp = self.nav.comp_near(vpos);
        if comp == 0 {
            return;
        }
        let vp = self.world.pos_of(vpos);
        // Sleeping citizens it can reach; the least observed first, then the nearest.
        let mut best: Option<(u32, i32, usize)> = None;
        for t in 0..self.ai.len() {
            let ai = &self.ai[t];
            if t == s || ai.kind != Kind::Worker || !ai.sleeping || self.nav.comp_near(ai.pos as usize) != comp {
                continue;
            }
            if self.vamp.vamps.iter().any(|v| v.id == ai.id) {
                continue;
            }
            let tp = self.world.pos_of(ai.pos as usize);
            let watchers = self.vamp_watchers(tp, s, t).len() as u32;
            let d = (tp.x as i32 - vp.x as i32).abs() + (tp.y as i32 - vp.y as i32).abs() + 3 * (tp.z as i32 - vp.z as i32).abs();
            let key = (watchers, d, t);
            if best.map(|b| key < b).unwrap_or(true) {
                best = Some(key);
            }
        }
        let Some((_, _, t)) = best else { return };
        let mut r = Rng::for_unit(self.seed, s, now, 0x7A3B_0004);
        let tile = self.ai[t].pos;
        let tp = self.world.pos_of(tile as usize);
        // The vampire slips over to the sleeper (the walk itself is not simulated).
        self.cancel_task(s);
        self.move_to(s, tile, now);
        self.ai[s].task = Task::Idle;
        self.ai[s].wake = now + 10;
        let seen: Vec<i32> = self.vamp_watchers(tp, s, t).into_iter().filter(|_| r.chance(WITNESS_SEE.0, WITNESS_SEE.1)).collect();
        let victim = self.ai[t].id;
        self.vamp.rep.feedings += 1;
        self.vamp.vamps[k].feedings += 1;
        self.vamp.vamps[k].next_feed = now + FEED_DAYS_MIN * DAY + r.below(FEED_DAYS_RAND) as u64 * DAY;
        let kill = r.chance(DRAIN_KILL.0, DRAIN_KILL.1);
        if !seen.is_empty() {
            self.unmask_vampire(k, now);
        }
        if kill {
            self.combat_sync_units();
            // The corpse is found bloodless (wiki Vampire: victims are "drained of blood").
            self.combat.f[t].blood = 0;
            self.combat.f[t].bleed = 0;
            self.combat_kill(t, now, "has been drained of blood");
            self.vamp.rep.drained += 1;
            self.vamp.vamps[k].kills += 1;
        } else {
            self.combat_sync_units();
            let f = &mut self.combat.f[t];
            if f.blood_max > 0 {
                f.blood = (f.blood - f.blood_max * 3 / 10).max(f.blood_max / 3);
            }
        }
        if kill || !seen.is_empty() {
            let kind = if kill { "Murder" } else { "AttemptedMurder" };
            let cid = self.justice_crime(kind, s, victim, !seen.is_empty(), now);
            if let Some(c) = self.world.crimes.iter_mut().find(|c| c.id == cid) {
                c.witnesses = seen;
            }
        }
    }

    /// Awake citizens (not the vampire `s`, not the victim `t`) near `p` on the same level.
    fn vamp_watchers(&self, p: rf_core::Pos, s: usize, t: usize) -> Vec<i32> {
        let mut v = Vec::new();
        for (w, ai) in self.ai.iter().enumerate() {
            if w == s || w == t || ai.kind != Kind::Worker || ai.sleeping {
                continue;
            }
            let q = self.world.pos_of(ai.pos as usize);
            if q.z == p.z && (q.x as i32 - p.x as i32).abs().max((q.y as i32 - p.y as i32).abs()) <= WITNESS_RANGE {
                v.push(ai.id);
            }
        }
        v
    }

    /// The vampire of record `k` is revealed: true name back, open crimes become accusations.
    fn unmask_vampire(&mut self, k: usize, now: u64) {
        if self.vamp.vamps[k].unmasked {
            return;
        }
        let id = self.vamp.vamps[k].id;
        self.vamp.vamps[k].unmasked = true;
        self.vamp.vamps[k].suspected = true;
        self.vamp.rep.unmasked += 1;
        let Some(u) = self.world.units.iter_mut().find(|u| u.id == id) else { return };
        let assumed = u.name.clone();
        if !u.true_name.is_empty() {
            u.name = u.true_name.clone();
        }
        let (name, pos) = (u.name.clone(), u.pos);
        self.announce(now, format!("{assumed} is a vampire! Their true name is {name}."), Some(pos), AnnouncementKind::Warning);
        let open: Vec<i32> = self.justice.rt.iter().filter(|c| c.culprit == id).map(|c| c.id).collect();
        for cid in open {
            if let Some(c) = self.world.crimes.iter_mut().find(|c| c.id == cid) {
                if matches!(c.status.as_str(), "Unsolved" | "") {
                    c.status = "Accused".into();
                    c.criminal = id;
                    c.criminal_name = name.clone();
                    self.justice.rep.identified += 1;
                }
            }
        }
    }

    // ------------------------------------------------------------------ werebeasts

    /// Moon phase from the calendar: one 28-day cycle per 28-day month (estimate).
    fn moon_now(&self) -> u8 {
        ((self.world.calendar.year_tick / TICKS_PER_DAY) % 28) as u8
    }

    /// A new day: transformations at the full moon, back the day after, wild attacks.
    fn moon_day_start(&mut self, now: u64) {
        if self.vamp.werebeasts_off {
            if self.vamp.weres.iter().any(|w| w.transformed) {
                self.were_revert_all(now);
            }
            return;
        }
        let full = self.moon_now() == FULL_MOON;
        if full {
            let mut r = Rng::for_unit(self.seed, 0, self.abs_day() as u64, 0x7A3B_0005);
            if r.chance(WERE_ATTACK_CHANCE.0, WERE_ATTACK_CHANCE.1) && self.invasions_enabled() && self.world.citizens().count() >= WERE_ATTACK_MIN_POP {
                self.spawn_were_attacker(now);
            }
            for k in 0..self.vamp.weres.len() {
                if !self.vamp.weres[k].transformed {
                    self.were_transform(k, now);
                }
            }
        } else if self.vamp.weres.iter().any(|w| w.transformed) {
            self.were_revert_all(now);
        }
        let (slots, ai) = (&self.unit_slot, &self.ai);
        self.vamp.weres.retain(|w| (!w.attacker || w.transformed) && slots.get(&w.id).map(|&s| ai[s].kind != Kind::Dead).unwrap_or(false));
    }

    /// Give the unit at `uidx` the werebeast curse.
    fn curse_were(&mut self, uidx: usize, race: &str, attacker: bool, now: u64) {
        let u = &mut self.world.units[uidx];
        u.curse = "WEREBEAST".into();
        u.curse_race = race.into();
        let id = u.id;
        if !self.vamp.weres.iter().any(|w| w.id == id) {
            self.vamp.weres.push(WereRec { id, attacker, since: now, ..Default::default() });
        }
    }

    fn were_transform(&mut self, k: usize, now: u64) {
        let id = self.vamp.weres[k].id;
        let Some(&s) = self.unit_slot.get(&id) else { return };
        let uidx = self.ai[s].uidx;
        let u = &self.world.units[uidx];
        if self.ai[s].kind == Kind::Dead || u.flags.caged || u.flags.inactive || self.justice_is_prisoner(id) {
            return;
        }
        let (name, pos, race) = (u.name.clone(), u.pos, u.curse_race.clone());
        self.vamp.weres[k].transformed = true;
        self.vamp.weres[k].changed = now;
        self.vamp.rep.transformations += 1;
        self.combat_sync_units();
        self.drop_gear(s);
        if self.ai[s].kind == Kind::Worker {
            self.cancel_task(s);
            self.ai[s].kind = Kind::Other;
        }
        self.make_hostile(id);
        let form = race.to_ascii_lowercase();
        self.announce(now, format!("{name} has transformed into a {form}!"), Some(pos), AnnouncementKind::Warning);
    }

    /// After the full moon: citizens and visitors turn back, attackers leave the map.
    fn were_revert_all(&mut self, now: u64) {
        for k in 0..self.vamp.weres.len() {
            if !self.vamp.weres[k].transformed {
                continue;
            }
            self.vamp.weres[k].transformed = false;
            self.vamp.weres[k].changed = now;
            let id = self.vamp.weres[k].id;
            let Some(&s) = self.unit_slot.get(&id) else { continue };
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            self.combat_sync_units();
            let su = s as u32;
            for f in self.combat.f.iter_mut() {
                if f.target == su {
                    f.target = NONE;
                }
            }
            if self.vamp.weres[k].attacker {
                let f = &mut self.combat.f[s];
                f.role = Role::Leave;
                f.target = NONE;
                self.engage_now(s, now);
                continue;
            }
            let uidx = self.ai[s].uidx;
            self.world.units[uidx].flags.hostile = false;
            let f = &mut self.combat.f[s];
            f.role = Role::None;
            f.target = NONE;
            if let Ok(p) = self.combat.hostiles.binary_search(&su) {
                self.combat.hostiles.remove(p);
            }
            let ai = &mut self.ai[s];
            ai.task = Task::Idle;
            ai.path.clear();
            ai.path_i = 0;
            ai.last = now;
            ai.wake = now + 1;
            if self.world.units[uidx].is_worker() {
                self.ai[s].kind = Kind::Worker;
            }
            self.rebuild_fort_list();
            let (name, pos) = (self.world.units[uidx].name.clone(), self.world.units[uidx].pos);
            self.announce(now, format!("{name} has returned to their normal form."), Some(pos), AnnouncementKind::Info);
        }
    }

    /// A werebeast from the wilds comes in its were-form. Its unit id.
    fn spawn_were_attacker(&mut self, now: u64) -> Option<i32> {
        let tiles = self.edge_tiles();
        if tiles.is_empty() {
            return None;
        }
        let mut r = Rng::for_unit(self.seed, 1, now, 0x7A3B_0006);
        let tile = tiles[r.below(tiles.len() as u32) as usize];
        let base = WERE_BASES[r.below(WERE_BASES.len() as u32) as usize];
        let race = format!("WERE{base}");
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let pos = self.world.pos_of(tile as usize);
        // Were-forms are big and strong (estimates; DF generates them per world).
        let attrs = [("STRENGTH", 2250), ("AGILITY", 1500), ("TOUGHNESS", 2000), ("ENDURANCE", 2000), ("RECUPERATION", 2000)];
        let u = Unit {
            id,
            race: race.clone(),
            caste: if r.chance(1, 2) { "FEMALE".into() } else { "MALE".into() },
            name: String::new(),
            profession: "STANDARD".into(),
            pos,
            flags: UnitFlags { hostile: true, ..Default::default() },
            civ_id: -1,
            hist_fig_id: -1,
            sex: -1,
            age: 20.0 + r.below(200) as f32,
            phys_attrs: attrs.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
            skills: vec![Skill { id: "BITE".into(), rating: 6, xp: 0 }, Skill { id: "WRESTLING".into(), rating: 5, xp: 0 }],
            squad_id: -1,
            squad_position: -1,
            body_size: 80_000,
            walk_speed: 700,
            invader_civ: -1,
            pet_owner: -1,
            pasture: -1,
            curse: "WEREBEAST".into(),
            curse_race: race.clone(),
            ..Default::default()
        };
        self.world.units.push(u);
        self.init_units();
        self.vamp.units_seen = self.world.units.len();
        let s = self.ai.len() - 1;
        self.make_hostile(id);
        self.combat.f[s].home = tile;
        self.vamp.weres.push(WereRec { id, transformed: true, attacker: true, since: now, changed: now });
        self.vamp.rep.were_attacks += 1;
        self.vamp.rep.transformations += 1;
        let form = race.to_ascii_lowercase();
        self.announce(now, format!("A {form} has come! It is attacking the fortress!"), Some(pos), AnnouncementKind::Warning);
        Some(id)
    }

    /// [hook] From `combat::pursue` after an attack (only while werebeasts exist): a wounding
    /// attack of a were-form passes the curse on (checked again at the next pass).
    pub(crate) fn were_after_attack(&mut self, a: usize, d: usize, wounds0: u64) {
        if self.combat.stats.wounds <= wounds0 {
            return;
        }
        let aid = self.ai[a].id;
        let Some(w) = self.vamp.weres.iter().find(|w| w.id == aid) else { return };
        if !w.transformed {
            return;
        }
        let du = &self.world.units[self.ai[d].uidx];
        if !du.curse.is_empty() || du.undead || du.flags.dead || du.flags.animal {
            return;
        }
        let did = du.id;
        if self.vamp.bitten.iter().any(|b| b.0 == did) {
            return;
        }
        let race = self.world.units[self.ai[a].uidx].curse_race.clone();
        self.vamp.bitten.push((did, race));
    }

    /// Survivors of were-form wounds carry the curse from now on.
    fn were_infect(&mut self, now: u64) {
        let bitten = std::mem::take(&mut self.vamp.bitten);
        for (id, race) in bitten {
            let Some(&s) = self.unit_slot.get(&id) else { continue };
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let uidx = self.ai[s].uidx;
            if !self.world.units[uidx].curse.is_empty() {
                continue;
            }
            self.curse_were(uidx, &race, false, now);
            self.vamp.rep.infected += 1;
        }
    }

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn vampires_hash(&self, mix: &mut dyn FnMut(u64)) {
        let v = &self.vamp;
        if v.vamps.is_empty() && v.weres.is_empty() && v.bitten.is_empty() && v.rep.vampires_arrived == 0 && v.rep.were_attacks == 0 && v.rep.infected == 0 {
            return;
        }
        let r = &v.rep;
        mix(r.vampires_arrived ^ r.feedings << 8 ^ r.drained << 20 ^ r.unmasked << 32 ^ r.were_attacks << 40 ^ r.transformations << 48 ^ r.infected << 56);
        for x in &v.vamps {
            mix((x.id as u32 as u64) << 32 ^ x.next_feed ^ (x.suspected as u64) << 62 ^ (x.unmasked as u64) << 63);
        }
        for w in &v.weres {
            mix((w.id as u32 as u64) << 32 ^ w.changed ^ (w.transformed as u64) << 62 ^ (w.attacker as u64) << 63);
        }
        for b in &v.bitten {
            mix(b.0 as u32 as u64 ^ 0xB17E << 32);
        }
    }

    pub(crate) fn vampire_threats(&self, out: &mut Vec<Threat>) {
        for v in &self.vamp.vamps {
            if !v.unmasked || self.justice_is_prisoner(v.id) {
                continue;
            }
            let Some(u) = self.world.unit(v.id) else { continue };
            if u.flags.dead {
                continue;
            }
            out.push(Threat { kind: ThreatKind::Vampire, name: u.name.clone(), race: u.race.clone(), units: vec![v.id], pos: Some(u.pos), since: v.arrived, entity_id: -1 });
        }
        for w in &self.vamp.weres {
            if !w.transformed {
                continue;
            }
            let Some(u) = self.world.unit(w.id) else { continue };
            if u.flags.dead || u.flags.inactive {
                continue;
            }
            let name = if u.name.is_empty() { u.curse_race.to_ascii_lowercase() } else { u.name.clone() };
            out.push(Threat { kind: ThreatKind::Werebeast, name, race: u.curse_race.clone(), units: vec![w.id], pos: Some(u.pos), since: w.changed, entity_id: -1 });
        }
    }

    pub(crate) fn vampire_unit_flags(&self, unit_id: i32, f: &mut UnitCurse) {
        if let Some(v) = self.vamp.vamps.iter().find(|v| v.id == unit_id) {
            f.vampire = true;
            f.vampire_suspected = v.suspected;
            f.vampire_unmasked = v.unmasked;
        }
        if let Some(w) = self.vamp.weres.iter().find(|w| w.id == unit_id) {
            f.werebeast = true;
            f.transformed = w.transformed;
        }
    }

    pub(crate) fn set_vampires_enabled_impl(&mut self, on: bool) {
        self.vamp.vampires_off = !on;
    }

    pub(crate) fn set_werebeasts_enabled_impl(&mut self, on: bool) {
        self.vamp.werebeasts_off = !on;
    }

    pub(crate) fn spawn_vampire_migrant_impl(&mut self) -> Option<i32> {
        let n0 = self.world.units.len();
        let got = self.spawn_migrants(1);
        self.vamp.seen_waves = self.pop.rep.waves.len();
        self.vamp.units_seen = self.world.units.len();
        if got == 0 || self.world.units.len() <= n0 {
            return None;
        }
        let uidx = self.world.units.len() - 1;
        let u = &mut self.world.units[uidx];
        if u.profession == "CHILD" || u.age < 18.0 {
            // Children are never vampires (wiki): the debug migrant comes as an adult.
            u.profession = "PEASANT".into();
            u.age = 30.0;
        }
        let now = self.ticks;
        Some(self.make_vampire(uidx, now))
    }

    pub(crate) fn spawn_werebeast_attack_impl(&mut self) -> Option<i32> {
        let now = self.ticks;
        self.vamp.init = true;
        self.spawn_were_attacker(now)
    }

    pub(crate) fn moon_phase_impl(&self) -> u8 {
        self.moon_now()
    }

    pub(crate) fn accuse_vampire_impl(&mut self, unit_id: i32) -> bool {
        let Some(k) = self.vamp.vamps.iter().position(|v| v.id == unit_id) else { return false };
        let alive = self.world.unit(unit_id).map(|u| !u.flags.dead).unwrap_or(false);
        if !alive {
            return false;
        }
        let now = self.ticks;
        self.unmask_vampire(k, now);
        true
    }

    pub(crate) fn vampire_report_impl(&self) -> VampireReport {
        self.vamp.rep
    }
}

#[cfg(test)]
#[path = "vampires_tests.rs"]
mod tests;
