//! [mega13] Megabeasts, semi-megabeasts and titans (DF v50; wiki Megabeast, Semi-megabeast,
//! Titan, Dragon, Hydra, Roc, Bronze colossus, Building destroyer, Difficulty).
//!
//! # DF rules implemented
//! - **Who comes**: only living megabeasts of the world. If the export lists historical
//!   figures of megabeast races (creature raws flagged `MEGABEAST` / `SEMIMEGABEAST` / `TITAN`,
//!   or the vanilla tokens below), those alive that have not come yet are the pool, and each
//!   comes at most once. Without such a list a world pool is generated: vanilla megabeasts
//!   (dragon, hydra, roc, bronze colossus), semi-megabeasts (giant, ettin, cyclops, minotaur)
//!   and procedurally generated titans (body and material like forgotten beasts, `caverns`).
//!   Creature raws of the save with those flags are preferred over the built-in table.
//! - **When**: checked at each season start once the fort reaches the third enemy tier (the
//!   save's `ENEMY_POP_TRIGGER` / `ENEMY_PROD_TRIGGER` / `ENEMY_TRADE_TRIGGER`, Normal: 80
//!   citizens and 100 000 created and 10 000 exported wealth, all at once; titans ignore the
//!   exported wealth, wiki Megabeast), at most one at a time, `MEGABEAST_INTERVAL` seasons apart, never when
//!   the save's difficulty flag `FLAG_MEGABEASTS=true` (megabeasts off) or invasions are off.
//! - **Arrival** at a map edge connected to the fort ("The dragon X has come! ..."); they hunt
//!   the fort like forgotten beasts (no retreat; they fight until slain).
//! - **Building destroyers**: vanilla megabeasts and semi-megabeasts except the roc are
//!   `BUILDINGDESTROYER:2` (see `defense::DESTROYERS`); titans are too (hook in `defense::race_info`). Wrecked
//!   buildings count in `MegabeastReport::buildings_destroyed`.
//! - **Dragonfire**: dragons (and some titans) breathe fire at a target a few tiles away; the
//!   target and everybody next to it catch fire. Fire-immune creatures are not hurt.
//! - **Slaying**: the death is announced with the slayer, kept in the fort's megabeast history
//!   (`Sim::megabeasts`), the slayer gets a strong positive thought, the other citizens a mild
//!   one.
//!
//! # Estimates (not on the wiki)
//! - `MEGABEAST_INTERVAL` is read as seasons; for an imported fort the counter starts at the
//!   import. Chance per eligible season 1/2. Kind odds: megabeast 35 %, semi-megabeast 40 %,
//!   titan 25 %. No megabeast while a siege is on the map (sieges and megabeasts do not stack,
//!   and a megabeast is never counted as a siege in `World::fort_history`).
//! - Body sizes without raws are the vanilla adult `BODY_SIZE`s (dragon 25 M, hydra 8 M, roc
//!   20 M, bronze colossus 20 M, giant 9 M, ettin/cyclops 8 M, minotaur 0.22 M cm³); titans 2-8 M. Natural skills 6
//!   (like forgotten beasts), strength/toughness 1 500-2 500.
//! - There is no temperature or fire model yet: fire is a wound model. A burning creature
//!   gets a burn (severity 2 500, pain 200) every 50 ticks for 200-400 ticks and dies when the
//!   burns reach `3 + body size / 100 000` (dwarves: 3). Breath range 2-10 tiles with a line
//!   of fire, one breath every 300-600 ticks.
//! - Thoughts: `SlewMegabeast` (slayer, strong positive) and `MegabeastSlain` (others, mild).

use crate::combat::{wf, Role, WoundS};
use crate::raids::RaidKind;
use crate::rng::Rng;
use crate::round13_api::{MegabeastInfo, MegabeastReport, Threat, ThreatKind, UnitCurse};
use crate::social::Class;
use crate::{AnnouncementKind, Kind, Sim, NONE};
use rf_core::{Skill, Unit, UnitFlags, NO_MAT};
use serde::{Deserialize, Serialize};

/// `megabeasts_day` runs when `now % TICKS_PER_DAY == MEGA_DAY_PHASE` (owner may change it).
pub(crate) const MEGA_DAY_PHASE: u64 = 733;
/// Burning creatures take a burn every `BURN_PERIOD` ticks (hook in `tick_inner`).
pub(crate) const BURN_PERIOD: u64 = 50;
pub(crate) const BURN_PHASE: u64 = 17;
/// Marker civ id of megabeasts (they hunt the fort like invaders do; not a siege).
pub(crate) const MEGA_CIV: i32 = 999_002;

/// Built-in vanilla pool (used when the save's raws do not list megabeasts):
/// token, name, kind, adult body size (vanilla raws), quadruped, fire breath, outer material.
const VANILLA: &[(&str, &str, ThreatKind, i32, bool, bool, &str)] = &[
    ("DRAGON", "dragon", ThreatKind::Megabeast, 25_000_000, true, true, ""),
    ("HYDRA", "hydra", ThreatKind::Megabeast, 8_000_000, true, false, ""),
    ("BIRD_ROC", "roc", ThreatKind::Megabeast, 20_000_000, true, false, ""),
    ("COLOSSUS_BRONZE", "bronze colossus", ThreatKind::Megabeast, 20_000_000, false, false, "BRONZE"),
    ("GIANT", "giant", ThreatKind::SemiMegabeast, 9_000_000, false, false, ""),
    ("ETTIN", "ettin", ThreatKind::SemiMegabeast, 8_000_000, false, false, ""),
    ("CYCLOPS", "cyclops", ThreatKind::SemiMegabeast, 8_000_000, false, false, ""),
    ("MINOTAUR", "minotaur", ThreatKind::SemiMegabeast, 220_000, false, false, ""),
];
/// Vanilla megabeasts with `NOPAIN` (used when the save's raws do not list them).
const NOPAIN: &[&str] = &["HYDRA", "COLOSSUS_BRONZE"];

/// A creature that is on fire.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Burn {
    pub slot: u32,
    /// Slot of the creature whose fire it is.
    pub src: u32,
    pub until: u64,
    pub next: u64,
    pub burns: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Mega {
    /// Switched off by the player.
    pub off: bool,
    /// Every megabeast that came (alive and slain).
    pub beasts: Vec<MegabeastInfo>,
    pub rep: MegabeastReport,
    /// Season of the last check (None before the first one).
    pub last_season: Option<i32>,
    /// Season starts since the last attack (or since the import).
    pub since_attack: i32,
    /// Slots of living megabeasts (ascending), parallel info in `beasts` by unit id.
    pub slots: Vec<u32>,
    /// Fire breathers: (slot, next breath tick).
    pub fire: Vec<(u32, u64)>,
    /// Last creature that hit a megabeast: (beast slot, attacker slot).
    pub last_hit: Vec<(u32, u32)>,
    pub burning: Vec<Burn>,
    /// Historical figures that already came.
    pub came_hf: Vec<i32>,
    /// Titans generated so far (race token numbering).
    pub titans: u32,
    /// Fort history lines (tick, text).
    pub history: Vec<(u64, String)>,
}

/// One candidate of the world's pool.
struct Cand {
    race: String,
    name: String,
    kind: ThreatKind,
    size: i32,
    quad: bool,
    fire: bool,
    mat: String,
    hf: i32,
    hf_name: String,
}

impl Sim {
    /// Hook from `tick_inner` (once a day).
    pub(crate) fn megabeasts_day(&mut self, now: u64) {
        if !self.mega.slots.is_empty() || self.mega.beasts.iter().any(|b| b.alive) {
            self.mega_check_dead(now);
        }
        let season = self.world.calendar.month() / 3;
        match self.mega.last_season {
            None => {
                self.mega.last_season = Some(season);
                return;
            }
            Some(s) if s == season => return,
            _ => {}
        }
        self.mega.last_season = Some(season);
        self.mega.since_attack = self.mega.since_attack.saturating_add(1);
        if !self.mega_allowed() {
            return;
        }
        let interval = self.war_setting("MEGABEAST_INTERVAL", 0, 10).max(1) as i32;
        if self.mega.since_attack < interval || !self.mega.slots.is_empty() {
            return;
        }
        let (full, titan_ok) = self.mega_triggers();
        if !full && !titan_ok {
            return;
        }
        // Sieges and megabeasts do not stack.
        if self.combat.inv.sieges.iter().any(|g| g.kind == RaidKind::Siege && !g.fleeing) {
            return;
        }
        let mut r = Rng::for_unit(self.seed, 13, now, 0x3E6A);
        if !r.chance(1, 2) {
            return;
        }
        self.mega_spawn_with(&mut r, if full { "" } else { "TITAN?" }, now);
    }

    /// wiki Megabeast: population 80 and created wealth 100 000 and exported wealth 10 000
    /// (Normal), i.e. all three third-tier triggers of the save at once.
    #[cfg(test)]
    pub(crate) fn mega_trigger_met(&self) -> bool {
        self.mega_triggers().0
    }

    /// (all three triggers met, titan triggers met). wiki Megabeast: "Titans ignore exported
    /// wealth".
    pub(crate) fn mega_triggers(&self) -> (bool, bool) {
        let (p, c, e) = self.war_triggers();
        let titan = self.world.citizens().count() >= p[2] && self.created_wealth() >= c[2];
        (titan && self.exported_wealth() >= e[2], titan)
    }

    fn mega_allowed(&self) -> bool {
        if self.mega.off || !self.combat.inv.enabled {
            return false;
        }
        !self.world.settings.other.iter().any(|s| s == "FLAG_MEGABEASTS=true")
    }

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn megabeasts_hash(&self, mix: &mut dyn FnMut(u64)) {
        if self.mega.beasts.is_empty() && self.mega.burning.is_empty() {
            return;
        }
        mix(self.mega.beasts.len() as u64);
        for b in &self.mega.beasts {
            mix(b.unit_id as u64 ^ (b.alive as u64) << 40 ^ (b.caught as u64) << 41 ^ (b.kills as u64) << 48);
        }
        let r = &self.mega.rep;
        mix(r.attacks ^ r.slain << 16 ^ r.fire_breaths << 32 ^ r.buildings_destroyed << 48);
        mix(self.mega.burning.len() as u64);
    }

    pub(crate) fn megabeast_threats(&self, out: &mut Vec<Threat>) {
        // Megabeasts, semi-megabeasts, titans.
        for b in self.mega.beasts.iter().filter(|b| b.alive && !b.caught) {
            let pos = self.unit_slot.get(&b.unit_id).map(|&s| self.upos(s));
            out.push(Threat { kind: b.kind, name: b.name.clone(), race: b.race.clone(), units: vec![b.unit_id], pos, since: b.arrived, entity_id: -1 });
        }
        // Forgotten beasts (caverns).
        for b in self.caverns.beasts.iter().filter(|b| b.killed.is_none()) {
            let Some(&s) = self.unit_slot.get(&b.unit_id) else { continue };
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let race = self.world.units[self.ai[s].uidx].race.clone();
            out.push(Threat { kind: ThreatKind::ForgottenBeast, name: b.name.clone(), race, units: vec![b.unit_id], pos: Some(self.upos(s)), since: b.arrived, entity_id: -1 });
        }
        // Regular sieges (revealed sieges and ambushes still on the map).
        let civ = self.war_enemy_civ();
        for g in &self.combat.inv.sieges {
            if !matches!(g.kind, RaidKind::Siege | RaidKind::Ambush) || !g.revealed {
                continue;
            }
            let units: Vec<i32> = g.members.iter().filter(|&&m| self.ai[m as usize].kind != Kind::Dead).map(|&m| self.world.units[self.ai[m as usize].uidx].id).collect();
            if units.is_empty() {
                continue;
            }
            let lead = if g.leader != NONE && self.alive(g.leader) { g.leader as usize } else { *g.members.iter().find(|&&m| self.ai[m as usize].kind != Kind::Dead).unwrap() as usize };
            let name = if g.kind == RaidKind::Siege { format!("{} siege", g.race.to_ascii_lowercase()) } else { format!("{} ambush", g.race.to_ascii_lowercase()) };
            out.push(Threat { kind: ThreatKind::Siege, name, race: g.race.clone(), units, pos: Some(self.upos(lead)), since: g.start, entity_id: civ });
        }
    }

    pub(crate) fn megabeast_unit_flags(&self, unit_id: i32, f: &mut UnitCurse) {
        if self.mega.beasts.iter().any(|b| b.unit_id == unit_id) {
            f.megabeast = true;
        }
    }

    pub(crate) fn megabeasts_impl(&self) -> Vec<MegabeastInfo> {
        self.mega.beasts.clone()
    }

    pub(crate) fn spawn_megabeast_impl(&mut self, race: &str) -> Option<i32> {
        let now = self.ticks;
        let mut r = Rng::for_unit(self.seed, 14, now, 0x3E6B ^ self.mega.beasts.len() as u64);
        self.mega_spawn_with(&mut r, race, now)
    }

    pub(crate) fn set_megabeasts_enabled_impl(&mut self, on: bool) {
        self.mega.off = !on;
    }

    pub(crate) fn megabeast_report_impl(&self) -> MegabeastReport {
        self.mega.rep
    }

    /// [mega13] Fort history of megabeast attacks and slayings: (tick, text).
    pub fn megabeast_history(&self) -> Vec<(u64, String)> {
        self.mega.history.clone()
    }

    // ------------------------------------------------------------------ pool and arrival

    /// Megabeast kind of a creature raw (flags of the creature or any caste), or the vanilla table.
    fn mega_kind_of_raw(c: &rf_core::CreatureRaw) -> Option<ThreatKind> {
        let has = |f: &str| c.flags.iter().any(|x| x == f) || c.castes.iter().any(|k| k.flags.iter().any(|x| x == f));
        if has("TITAN") {
            None // generated per world; we make our own titans
        } else if has("MEGABEAST") {
            Some(ThreatKind::Megabeast)
        } else if has("SEMIMEGABEAST") {
            Some(ThreatKind::SemiMegabeast)
        } else {
            VANILLA.iter().find(|v| v.0 == c.token).map(|v| v.2)
        }
    }

    /// Candidate races (no titans): the save's raws with megabeast flags, else the table.
    fn mega_races(&self) -> Vec<Cand> {
        let mut v = Vec::new();
        for c in &self.world.creature_raws {
            let Some(kind) = Self::mega_kind_of_raw(c) else { continue };
            let k0 = c.castes.first();
            let fire = c.castes.iter().any(|k| k.flags.iter().any(|f| f.contains("FIRE") && f.contains("BREATH"))) || c.token == "DRAGON";
            let quad = k0.map(|k| !k.body_parts.iter().any(|p| p.flags.iter().any(|f| f == "GRASP"))).unwrap_or(true);
            let size = k0.map(|k| k.body_size).filter(|&s| s > 0).or_else(|| VANILLA.iter().find(|x| x.0 == c.token).map(|x| x.3)).unwrap_or(5_000_000);
            let name = if c.name.is_empty() { c.token.to_ascii_lowercase().replace('_', " ") } else { c.name.clone() };
            let mat = VANILLA.iter().find(|x| x.0 == c.token).map(|x| x.6).unwrap_or("").to_string();
            v.push(Cand { race: c.token.clone(), name, kind, size, quad, fire, mat, hf: -1, hf_name: String::new() });
        }
        if v.is_empty() {
            for x in VANILLA {
                v.push(Cand { race: x.0.into(), name: x.1.into(), kind: x.2, size: x.3, quad: x.4, fire: x.5, mat: x.6.into(), hf: -1, hf_name: String::new() });
            }
        }
        v
    }

    /// The world's pool: living historical figures of megabeast races that did not come yet;
    /// if the export lists none, the generated pool (races + a titan). Third: living titans of
    /// the world (historical figures of `TITAN` races) that did not come yet. // [mega13] verify
    fn mega_pool(&mut self) -> (Vec<Cand>, bool, Vec<(i32, String)>) {
        let races = self.mega_races();
        let mut hfs = Vec::new();
        let titan_race = |race: &str| self.world.creature_raws.iter().any(|c| c.token == race && (c.flags.iter().any(|f| f == "TITAN") || c.castes.iter().any(|k| k.flags.iter().any(|f| f == "TITAN"))));
        let mut titan_hfs = Vec::new();
        let mut titans_listed = false;
        for h in &self.world.hist_figs {
            let titan = titan_race(&h.race);
            titans_listed |= titan;
            // Dead, already came, or already on the map (e.g. caged in the fort).
            if !h.alive || h.unit_id >= 0 || self.mega.came_hf.contains(&h.id) {
                continue;
            }
            if titan {
                titan_hfs.push((h.id, h.name.clone()));
                continue;
            }
            if let Some(c) = races.iter().find(|c| c.race == h.race) {
                hfs.push(Cand { race: c.race.clone(), name: c.name.clone(), kind: c.kind, size: c.size, quad: c.quad, fire: c.fire, mat: c.mat.clone(), hf: h.id, hf_name: h.name.clone() });
            }
        }
        let listed = titans_listed || self.world.hist_figs.iter().any(|h| races.iter().any(|c| c.race == h.race));
        if listed {
            (hfs, false, titan_hfs)
        } else {
            (races, true, titan_hfs)
        }
    }

    /// Pick and spawn one megabeast (`race` "" = by the odds, `TITAN` = a titan, `TITAN?` = a
    /// titan of the world if one is left).
    fn mega_spawn_with(&mut self, r: &mut Rng, race: &str, now: u64) -> Option<i32> {
        let (pool, titans, titan_hfs) = self.mega_pool();
        let world_titan = titans || !titan_hfs.is_empty();
        if race == "TITAN?" && !world_titan {
            return None;
        }
        let want_titan = race.eq_ignore_ascii_case("TITAN") || race == "TITAN?" || (race.is_empty() && world_titan && (pool.is_empty() || r.below(100) < 25));
        let pick: Option<Cand> = if want_titan {
            None
        } else if !race.is_empty() {
            let found = self.mega_races().into_iter().find(|c| c.race == race).or_else(|| {
                VANILLA.iter().find(|x| x.0 == race).map(|x| Cand { race: x.0.into(), name: x.1.into(), kind: x.2, size: x.3, quad: x.4, fire: x.5, mat: x.6.into(), hf: -1, hf_name: String::new() })
            });
            Some(found?)
        } else {
            if pool.is_empty() {
                return None;
            }
            let semi = r.below(75) < 40;
            let want = if semi { ThreatKind::SemiMegabeast } else { ThreatKind::Megabeast };
            let of: Vec<usize> = (0..pool.len()).filter(|&i| pool[i].kind == want).collect();
            let i = if of.is_empty() { r.below(pool.len() as u32) as usize } else { of[r.below(of.len() as u32) as usize] };
            pool.into_iter().nth(i)
        };
        let edges = self.edge_tiles();
        if edges.is_empty() {
            return None;
        }
        let tile = edges[r.below(edges.len() as u32) as usize];
        match pick {
            Some(c) => self.mega_spawn_race(r, c, tile, now),
            None => {
                // A titan of the world's list keeps its name and comes only once.
                let hf = if titans { None } else { titan_hfs.first().cloned() };
                self.mega_spawn_titan(r, tile, now, hf)
            }
        }
    }

    fn mega_spawn_race(&mut self, r: &mut Rng, c: Cand, tile: u32, now: u64) -> Option<i32> {
        let pname = if c.hf_name.is_empty() { mega_name(r) } else { c.hf_name.clone() };
        let cap_race = crate::combat::cap_pub(&c.name);
        let walk = 900 + r.below(600) as i32;
        let id = self.mega_unit(r, &c.race, format!("{pname}, the {cap_race}"), c.size, c.quad, walk, tile)?;
        let s = *self.unit_slot.get(&id)?;
        if !c.mat.is_empty() {
            let mat = self.world.materials.iter().position(|m| m.token.ends_with(&format!(":{}", c.mat)) || m.token == c.mat).map(|i| i as u16).unwrap_or(NO_MAT);
            self.mega_skin(s, mat);
        }
        if c.fire {
            self.mega.fire.push((s as u32, 0));
        }
        let raw_nopain = self.world.creature_raws.iter().find(|x| x.token == c.race).map(|x| x.flags.iter().chain(x.castes.iter().flat_map(|k| k.flags.iter())).any(|f| f == "NOPAIN"));
        if raw_nopain.unwrap_or_else(|| NOPAIN.contains(&c.race.as_str())) {
            let b = self.combat.f[s].body as usize;
            if let Some(body) = self.combat.bodies.get_mut(b) {
                body.nopain = true; // body template of this race only
            }
        }
        if c.hf >= 0 {
            self.mega.came_hf.push(c.hf);
        }
        let desc = match c.race.as_str() {
            "DRAGON" => "A huge winged reptile. Beware its fiery breath!".to_string(),
            "HYDRA" => "A huge many-headed reptile.".to_string(),
            "BIRD_ROC" => "A gigantic bird of prey.".to_string(),
            "COLOSSUS_BRONZE" => "A towering statue of bronze come to life.".to_string(),
            _ if c.kind == ThreatKind::SemiMegabeast => format!("A towering {}.", c.name),
            _ => format!("A huge {}.{}", c.name, if c.fire { " Beware its fiery breath!" } else { "" }),
        };
        let text = format!("The {} {pname} has come! {desc}", c.name);
        self.mega_arrived(s, id, pname, c.race, c.kind, text, now);
        Some(id)
    }

    /// A generated titan (like a forgotten beast, on the surface).
    fn mega_spawn_titan(&mut self, r: &mut Rng, tile: u32, now: u64, hf: Option<(i32, String)>) -> Option<i32> {
        self.mega.titans += 1;
        let race = format!("TITAN_{}", self.mega.titans);
        let pname = mega_name(r);
        let pname = match hf {
            Some((id, name)) => {
                self.mega.came_hf.push(id);
                if name.is_empty() { pname } else { name }
            }
            None => pname,
        };
        let quad = r.chance(1, 2);
        let size = 2_000_000 + r.below(6_000_001) as i32;
        let solid: Vec<u16> = self
            .world
            .materials
            .iter()
            .enumerate()
            .filter(|(_, m)| (m.is_stone || m.is_gem || m.is_metal) && !m.is_ore && !m.name.is_empty() && m.token.starts_with("INORGANIC:"))
            .map(|(i, _)| i as u16)
            .collect();
        let mat = if !solid.is_empty() && r.chance(1, 2) { solid[r.below(solid.len() as u32) as usize] } else { NO_MAT };
        let walk = 900 + r.below(600) as i32;
        let id = self.mega_unit(r, &race, format!("{pname}, the Titan"), size, quad, walk, tile)?;
        let s = *self.unit_slot.get(&id)?;
        self.mega_skin(s, mat);
        // Special attack: a third breathe fire, the rest roll a forgotten beast's power.
        let mut rp = Rng::for_unit(self.seed, 15, now, 0x7174 + self.mega.titans as u64);
        let fire = rp.chance(1, 3);
        let mut beware = String::new();
        if fire {
            self.mega.fire.push((s as u32, 0));
            beware = " Beware its fiery breath!".into();
        } else if let Some(p) = crate::syndrome::roll_power(&mut rp, quad) {
            beware = format!(" {}", Sim::beware_text(&p));
            self.caverns.powers.push((s as u32, p));
        }
        let adj = ["towering", "colossal", "gigantic", "enormous"][r.below(4) as usize];
        let shape = if quad { ["four-legged beast", "serpent", "lizard", "great cat"][r.below(4) as usize] } else { ["humanoid", "giant ape", "hulking brute"][r.below(3) as usize] };
        let made = if mat == NO_MAT { String::new() } else { format!(" made of {}", self.world.materials[mat as usize].name) };
        let text = format!("The Titan {pname} has come! A {adj} {shape}{made}.{beware}");
        self.mega_arrived(s, id, pname, race, ThreatKind::Titan, text, now);
        Some(id)
    }

    /// Create the unit, its body and hostile fighter state.
    #[allow(clippy::too_many_arguments)]
    fn mega_unit(&mut self, r: &mut Rng, race: &str, name: String, size: i32, quad: bool, walk: i32, tile: u32) -> Option<i32> {
        let attr = |r: &mut Rng| 1500 + r.below(1001) as i32;
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let six = |s: &str| Skill { id: s.into(), rating: 6, xp: 0 };
        let unit = Unit {
            id,
            race: race.to_string(),
            caste: "NEUTER".into(),
            name,
            profession: "STANDARD".into(),
            pos: self.world.pos_of(tile as usize),
            flags: UnitFlags { hostile: true, animal: quad, ..Default::default() },
            civ_id: -1,
            hist_fig_id: -1,
            sex: -1,
            age: 500.0,
            skills: ["WRESTLING", "BITE", "GRASP_STRIKE", "STANCE_STRIKE", "MELEE_COMBAT", "DODGING", "SITUATIONAL_AWARENESS"].iter().map(|s| six(s)).collect(),
            phys_attrs: vec![("STRENGTH".into(), attr(r)), ("AGILITY".into(), 800 + r.below(500) as i32), ("TOUGHNESS".into(), attr(r))],
            ment_attrs: vec![("WILLPOWER".into(), attr(r))],
            squad_id: -1,
            squad_position: -1,
            body_size: size,
            walk_speed: walk,
            invader_civ: MEGA_CIV,
            ..Default::default()
        };
        self.world.units.push(unit);
        self.init_units();
        self.combat_sync_units();
        let s = *self.unit_slot.get(&id)?;
        self.ensure_body(s);
        Some(id)
    }

    /// The outermost tissue of every body part is `mat` (no bleeding or pain there).
    fn mega_skin(&mut self, s: usize, mat: u16) {
        if mat == NO_MAT {
            return;
        }
        let mech = self.mech_of_mat(mat);
        let b = self.combat.f[s].body as usize;
        if let Some(body) = self.combat.bodies.get_mut(b) {
            let firsts: Vec<usize> = body.parts.iter().filter(|p| p.nl > 0).map(|p| p.l0 as usize).collect();
            for l0 in firsts {
                if let Some(l) = body.layers.get_mut(l0) {
                    l.mech = mech;
                    l.vascular = 0;
                    l.pain = 0;
                    l.arteries = false;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mega_arrived(&mut self, s: usize, id: i32, name: String, race: String, kind: ThreatKind, text: String, now: u64) {
        let tile = self.ai[s].pos;
        let f = &mut self.combat.f[s];
        f.role = Role::Hostile;
        f.home = tile;
        if let Err(p) = self.combat.hostiles.binary_search(&(s as u32)) {
            self.combat.hostiles.insert(p, s as u32);
        }
        if let Err(p) = self.mega.slots.binary_search(&(s as u32)) {
            self.mega.slots.insert(p, s as u32);
        }
        self.rebuild_fort_list();
        self.engage_now(s, now);
        let p = self.world.pos_of(tile as usize);
        self.announce(now, text.clone(), Some(p), AnnouncementKind::Warning);
        self.combat.log_line(text.clone());
        self.mega.history.push((now, text));
        self.mega.beasts.push(MegabeastInfo { unit_id: id, name, race, kind, arrived: now, alive: true, kills: 0, slain_by: -1, caught: false });
        self.mega.rep.attacks += 1;
        self.mega.since_attack = 0;
    }

    // ------------------------------------------------------------------ death and slaying

    fn mega_check_dead(&mut self, now: u64) {
        for k in 0..self.mega.beasts.len() {
            if !self.mega.beasts[k].alive || self.mega.beasts[k].caught {
                continue;
            }
            let id = self.mega.beasts[k].unit_id;
            let Some(&s) = self.unit_slot.get(&id) else { continue };
            if self.ai[s].kind != Kind::Dead {
                continue;
            }
            // A caged megabeast (cage trap) is caught, not slain (it lives on in its cage).
            if self.world.units[self.ai[s].uidx].flags.caged {
                self.mega.beasts[k].caught = true;
                let (race, kind, name) = (self.mega.beasts[k].race.clone(), self.mega.beasts[k].kind, self.mega.beasts[k].name.clone());
                let text = format!("The {} {name} has been captured.", self.mega_what(&race, kind));
                self.combat.log_line(text.clone());
                self.mega.history.push((now, text));
                continue;
            }
            let killer = self.mega.last_hit.iter().find(|x| x.0 == s as u32).map(|x| x.1);
            self.mega_slain(k, s, killer, now);
        }
        let ai = &self.ai;
        let alive = |s: u32| (s as usize) < ai.len() && ai[s as usize].kind != Kind::Dead;
        self.mega.slots.retain(|&s| alive(s));
        self.mega.fire.retain(|x| alive(x.0));
        self.mega.last_hit.retain(|x| alive(x.0));
    }

    fn mega_slain(&mut self, k: usize, _s: usize, killer: Option<u32>, now: u64) {
        let b = &mut self.mega.beasts[k];
        b.alive = false;
        let killer = killer.filter(|&a| (a as usize) < self.ai.len());
        b.slain_by = killer.map(|a| self.world.units[self.ai[a as usize].uidx].id).unwrap_or(-1);
        let (name, race, kind) = (b.name.clone(), b.race.clone(), b.kind);
        self.mega.rep.slain += 1;
        let what = self.mega_what(&race, kind);
        let text = match killer {
            Some(a) => format!("The {what} {name} has been slain by {}!", self.uname(a as usize)),
            None => format!("The {what} {name} has been slain."),
        };
        self.announce(now, text.clone(), None, AnnouncementKind::Info);
        self.combat.log_line(text.clone());
        self.mega.history.push((now, text));
        // Thoughts: the slayer (a citizen) is proud, the others are glad.
        let slayer = killer.map(|a| a as usize).filter(|&a| self.ai[a].kind == Kind::Worker);
        for s in 0..self.ai.len() {
            if self.ai[s].kind != Kind::Worker || !self.world.units[self.ai[s].uidx].flags.citizen {
                continue;
            }
            if Some(s) == slayer {
                self.feel(s, "SlewMegabeast", Class::PosStrong, 300, -1, 0, now);
            } else {
                self.feel(s, "MegabeastSlain", Class::PosMild, 80, -1, 0, now);
            }
        }
    }

    /// "Titan" or the creature name of a megabeast race.
    fn mega_what(&self, race: &str, kind: ThreatKind) -> String {
        match kind {
            ThreatKind::Titan => "Titan".to_string(),
            _ => self.world.creature_raws.iter().find(|c| c.token == race).map(|c| c.name.clone()).filter(|n| !n.is_empty()).or_else(|| VANILLA.iter().find(|v| v.0 == race).map(|v| v.1.to_string())).unwrap_or_else(|| race.to_ascii_lowercase()),
        }
    }

    /// [combat hook] A blow of `a` on `d` (megabeasts on the map only): remember who last hit
    /// a megabeast; `killed`: count a megabeast's kill.
    pub(crate) fn mega_blow(&mut self, a: usize, d: usize, killed: bool) {
        if self.mega.slots.binary_search(&(d as u32)).is_ok() {
            match self.mega.last_hit.iter_mut().find(|x| x.0 == d as u32) {
                Some(x) => x.1 = a as u32,
                None => self.mega.last_hit.push((d as u32, a as u32)),
            }
        }
        if killed && self.mega.slots.binary_search(&(a as u32)).is_ok() {
            self.mega_count_kill(a);
        }
        // A killing blow on a megabeast: announce the slaying at once, not at the daily check.
        if killed && self.mega.slots.binary_search(&(d as u32)).is_ok() {
            let now = self.ticks;
            self.mega_check_dead(now);
        }
    }

    fn mega_count_kill(&mut self, a: usize) {
        let id = self.world.units[self.ai[a].uidx].id;
        if let Some(b) = self.mega.beasts.iter_mut().find(|b| b.unit_id == id) {
            b.kills += 1;
        }
    }

    /// [defense hook] A building was wrecked by `s`.
    pub(crate) fn mega_wrecked(&mut self, s: usize) {
        if self.mega.slots.binary_search(&(s as u32)).is_ok() {
            self.mega.rep.buildings_destroyed += 1;
        }
    }

    /// [defense hook] `BUILDINGDESTROYER` level of generated titans.
    pub(crate) fn mega_destroyer_level(race: &str) -> u8 {
        if race.starts_with("TITAN_") {
            2
        } else {
            0
        }
    }

    // ------------------------------------------------------------------ fire

    fn fireproof(&self, s: usize) -> bool {
        let race = &self.world.units[self.ai[s].uidx].race;
        if self.mega.fire.iter().any(|x| x.0 == s as u32) {
            return true;
        }
        self.world
            .creature_raws
            .iter()
            .find(|c| &c.token == race)
            .map(|c| c.flags.iter().chain(c.castes.iter().flat_map(|k| k.flags.iter())).any(|f| f == "FIREIMMUNE" || f == "FIREIMMUNE_SUPER"))
            .unwrap_or(false)
    }

    /// [combat hook] A fire breather breathes at its target. True if it did (turn over).
    pub(crate) fn mega_breath(&mut self, s: usize, now: u64) -> bool {
        let Some(k) = self.mega.fire.iter().position(|x| x.0 == s as u32) else { return false };
        if now < self.mega.fire[k].1 {
            return false;
        }
        let t = self.combat.f[s].target;
        if !self.alive(t) || !self.can_fight(s, now) {
            return false;
        }
        let t = t as usize;
        let (a, b) = (self.upos(s), self.upos(t));
        let cheb = (a.x as i32 - b.x as i32).abs().max((a.y as i32 - b.y as i32).abs());
        if !(2..=10).contains(&cheb) || (a.z as i32 - b.z as i32).abs() > 1 || !self.line_of_fire(a, b, 20) {
            return false;
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0xF14E);
        self.mega.fire[k].1 = now + 300 + r.below(301) as u64;
        self.mega.rep.fire_breaths += 1;
        let line = format!("{} breathes fire at {}!", crate::combat::cap_pub(&self.uname(s)), self.uname(t));
        self.combat.log_line(line);
        let mut hit = vec![t];
        for &u in &self.combat.fort {
            let u = u as usize;
            if u == t || self.ai[u].kind == Kind::Dead {
                continue;
            }
            let q = self.upos(u);
            if q.z == b.z && (q.x as i32 - b.x as i32).abs() <= 1 && (q.y as i32 - b.y as i32).abs() <= 1 {
                hit.push(u);
            }
        }
        for u in hit {
            self.mega_ignite(u, s, now, &mut r);
        }
        self.after_attacked(t, s, now);
        self.ai[s].wake = now + 20;
        true
    }

    fn mega_ignite(&mut self, u: usize, src: usize, now: u64, r: &mut Rng) {
        if self.ai[u].kind == Kind::Dead || u == src || self.fireproof(u) {
            return;
        }
        let until = now + 200 + r.below(201) as u64;
        match self.mega.burning.iter_mut().find(|b| b.slot == u as u32) {
            Some(b) => {
                b.until = b.until.max(until);
                b.src = src as u32;
            }
            None => {
                self.mega.burning.push(Burn { slot: u as u32, src: src as u32, until, next: now, burns: 0 });
                let line = format!("{} catches fire!", crate::combat::cap_pub(&self.uname(u)));
                self.combat.log_line(line);
            }
        }
        self.mega_burn_once(u, src, now);
    }

    /// Hook from `tick_inner` every `BURN_PERIOD` ticks while something burns.
    pub(crate) fn mega_burn_tick(&mut self, now: u64) {
        let mut k = 0;
        while k < self.mega.burning.len() {
            let b = self.mega.burning[k].clone();
            let s = b.slot as usize;
            if s >= self.ai.len() || self.ai[s].kind == Kind::Dead {
                self.mega.burning.swap_remove(k);
                continue;
            }
            if now >= b.until {
                self.mega.burning.swap_remove(k);
                let line = format!("{} is no longer on fire.", crate::combat::cap_pub(&self.uname(s)));
                self.combat.log_line(line);
                continue;
            }
            if now >= b.next {
                self.mega_burn_once(s, b.src as usize, now);
            }
            k += 1;
        }
    }

    /// One burn on a random outer body part; kills at the limit.
    fn mega_burn_once(&mut self, d: usize, src: usize, now: u64) {
        let Some(k) = self.mega.burning.iter().position(|b| b.slot == d as u32) else { return };
        self.mega.burning[k].next = now + BURN_PERIOD;
        self.mega.burning[k].burns += 1;
        let burns = self.mega.burning[k].burns;
        self.ensure_body(d);
        let b = self.combat.f[d].body;
        if b != u16::MAX {
            let mut r = Rng::for_unit(self.seed, d, now, 0xB0B5);
            let body = &self.combat.bodies[b as usize];
            let parts: Vec<u16> = body.targets.iter().map(|t| t.0).filter(|&p| self.combat.f[d].part_state.get(p as usize).map(|s| s & 1 == 0).unwrap_or(true)).collect();
            if !parts.is_empty() {
                let part = parts[r.below(parts.len() as u32) as usize];
                let layer = body.parts[part as usize].l0;
                let f = &mut self.combat.f[d];
                f.wounds.push(WoundS { part, layer, flags: wf::BRUISED, sev: 2_500, bleed: 0, pain: 200, age: 0, mx: 0 /* [health15] */ });
                self.list_wounded(d);
                self.sync_wounds(d);
                self.check_pain(d, now);
            }
        }
        let size = self.world.units[self.ai[d].uidx].body_size.max(1);
        let limit = (3 + size / 100_000).clamp(3, 40);
        if burns >= limit && self.ai[d].kind != Kind::Dead {
            self.combat_kill(d, now, "has burned to death");
            self.combat.stats.kills += 1;
            if src < self.ai.len() && self.mega.slots.binary_search(&(src as u32)).is_ok() {
                self.mega_count_kill(src);
            }
            if let Some(k) = self.mega.burning.iter().position(|b| b.slot == d as u32) {
                self.mega.burning.swap_remove(k);
            }
        }
    }
}

/// A generated two-word name (own syllables; DF uses its language files).
fn mega_name(r: &mut Rng) -> String {
    const A: &[&str] = &["ag", "ro", "thal", "ix", "um", "kel", "zar", "ond", "bri", "gul", "esh", "nar", "vok", "tis", "ama", "drun", "sel", "orb", "kha", "lim"];
    let mut word = |n: u32| -> String {
        let mut w = String::new();
        for _ in 0..n {
            w.push_str(A[r.below(A.len() as u32) as usize]);
        }
        let mut c = w.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => w,
        }
    };
    let a = word(2);
    let b = word(2 + (a.len() as u32 % 2));
    format!("{a} {b}")
}

#[cfg(test)]
mod tests;
