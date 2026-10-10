//! [society2] Justice (DF v50): crimes, investigation, conviction and punishment.
//!
//! # DF rules implemented (wiki v50: Justice, Crime, Sheriff, Captain of the guard, Jail,
//! Hammerer, Mandate)
//! - **Crimes** are recorded in `World::crimes` with DF's `crime_type` names:
//!   tantrums that topple furniture are `BuildingDestruction`, tantrum fist fights
//!   `DisorderlyBehavior`; visiting thieves commit `Theft`; an unmet production mandate is a
//!   `ProductionOrderViolation` and a broken export ban an `ExportViolation` (the noble names a
//!   culprit, as in DF). Citizens near the crime are its witnesses.
//! - **Investigation**: only a fort with a sheriff or captain of the guard investigates. The
//!   law enforcer interviews the witnesses; a witnessed crime is solved when the first witness
//!   is interviewed (the culprit becomes `Crime::criminal`, status `Accused`). Crimes nobody saw
//!   are solved only by interrogation. Without a law enforcer crimes stay unsolved.
//! - **Conviction** is the player's decision (`Sim::convict`, DF's justice screen); headless runs
//!   can let the sheriff convict accused criminals (`Sim::set_auto_convict`). Convicting someone
//!   other than the culprit is a wrongful conviction (the victim gets no justice).
//! - **Punishment**: jail time in a jail (a Dungeon zone with restraints, i.e. a chain or a
//!   built cage); the prisoner is restrained there, cannot work, is brought food and water and
//!   thinks "Jailed" daily, "JailReleased" when the time is up. Without a free restraint a jail
//!   sentence becomes a beating by the law enforcer ("GotBeaten" / "GaveBeating"). With a
//!   hammerer, crimes punished by hammerstrikes are struck by the hammerer with a hammer
//!   ("GotHammered" / "GaveHammering"; without a hammer "NoHammer" and a beating instead);
//!   hammerstrikes bruise and can break bones (then `health` treats the patient). The victim of a
//!   punished crime thinks "ConvictionJusticeSelf".
//!
//! # Estimates (not on the wiki)
//! - Sentences: DisorderlyBehavior 7 jail days, BuildingDestruction / Vandalism 14,
//!   ProductionOrderViolation / ExportViolation 14 (or 5 hammerstrikes when a hammerer exists),
//!   Theft 28, AttemptedMurder 112 (10 strikes), Murder 336 (20 strikes).
//! - One witness interview per day; interrogation of an unwitnessed crime 1 in 56 per day;
//!   a hammerstrike breaks a bone 1 in 15; witnesses within 6 tiles on the same level.

use crate::rng::Rng;
use crate::social::Class;
use crate::{needs, AnnouncementKind, Kind, Sim, Task};
use rf_core::{Crime, Wound, TICKS_PER_DAY};

const DAY: u64 = TICKS_PER_DAY as u64;

/// Justice statistics for reports and the benchmark.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct JusticeReport {
    /// Crimes by DF crime type.
    pub crimes: Vec<(String, u64)>,
    pub identified: u64,
    pub convicted: u64,
    pub wrongful: u64,
    pub jailed: u64,
    pub jail_days: u64,
    pub released: u64,
    pub beatings: u64,
    pub hammerstrikes: u64,
    pub broken_bones: u64,
    /// Jail sentences turned into beatings for lack of a free restraint.
    pub no_jail: u64,
    pub thefts: u64,
    pub stolen_value: i64,
    /// Crimes left unsolved because the fort has no sheriff or captain of the guard.
    pub no_enforcer: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct CrimeRt {
    pub id: i32,
    /// The real culprit (unit id), known to the sim, not to the fort.
    pub culprit: i32,
    pub interviewed: u16,
    pub no_enforcer_counted: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct Prisoner {
    pub slot: u32,
    pub crime: i32,
    pub until: u64,
    /// Restraint tile.
    pub tile: u32,
    pub next_day: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Justice {
    pub rt: Vec<CrimeRt>,
    pub prisoners: Vec<Prisoner>,
    pub auto_convict: bool,
    pub counts: std::collections::BTreeMap<String, u64>,
    pub rep: JusticeReport,
}

/// Sentence for a crime type: (jail days, hammerstrikes if a hammerer exists).
pub fn sentence(kind: &str) -> (i32, i32) {
    match kind {
        "DisorderlyBehavior" => (7, 0),
        "BuildingDestruction" | "Vandalism" => (14, 0),
        "ProductionOrderViolation" | "ExportViolation" | "JobOrderViolation" => (14, 5),
        "Theft" | "AttemptedTheft" => (28, 0),
        "AttemptedMurder" => (112, 10),
        "Murder" => (336, 20),
        _ => (7, 0),
    }
}

impl Sim {
    fn crime_idx(&self, id: i32) -> Option<usize> {
        self.world.crimes.iter().position(|c| c.id == id)
    }

    /// Law enforcer: sheriff, else captain of the guard (wiki "Justice").
    fn law_enforcer(&self) -> Option<usize> {
        self.s2_noble_slot("SHERIFF").or_else(|| self.s2_noble_slot("CAPTAIN_OF_THE_GUARD"))
    }

    /// Record a crime by the unit in slot `culprit`. `accused` = the culprit is named at once
    /// (mandates: the noble names him). Returns the crime id.
    pub(crate) fn justice_crime(&mut self, kind: &str, culprit: usize, victim: i32, accused: bool, now: u64) -> i32 {
        if self.locs.off {
            return -1;
        }
        let cu = &self.world.units[self.ai[culprit].uidx];
        let (cid, pos) = (cu.id, cu.pos);
        let mut witnesses = Vec::new();
        for t in 0..self.ai.len() {
            if t == culprit || !self.s2_citizen(t) {
                continue;
            }
            let u = &self.world.units[self.ai[t].uidx];
            if u.pos.z == pos.z && u.pos.dist(pos) <= 6 {
                witnesses.push(u.id);
            }
        }
        let id = self.world.crimes.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let c = self.world.calendar;
        self.world.crimes.push(Crime {
            id,
            kind: kind.into(),
            criminal: if accused { cid } else { -1 },
            victim,
            site_tick: c.year_tick,
            year: c.year,
            convicted: -1,
            punishment: String::new(),
            witnesses,
            status: if accused { "Accused".into() } else { "Unsolved".into() },
            sentence_days: 0,
            criminal_name: if accused { self.world.unit(cid).map(|u| u.name.clone()).unwrap_or_default() } else { String::new() },
            victim_name: self.world.unit(victim).map(|u| u.name.clone()).unwrap_or_default(),
        });
        self.justice.rt.push(CrimeRt { id, culprit: cid, interviewed: 0, no_enforcer_counted: false });
        *self.justice.counts.entry(kind.to_string()).or_default() += 1;
        if accused {
            self.justice.rep.identified += 1;
        }
        let text = match kind {
            "BuildingDestruction" => "A building has been destroyed: a crime has been committed.",
            "DisorderlyBehavior" => "Disorderly behavior: a crime has been committed.",
            "Theft" => "Something has been stolen: a crime has been committed.",
            "ProductionOrderViolation" => "A production order has been violated: a crime has been committed.",
            "ExportViolation" => "An export ban has been violated: a crime has been committed.",
            _ => "A crime has been committed.",
        };
        self.announce(now, text.into(), Some(pos), AnnouncementKind::Warning);
        id
    }

    /// [hook] From `social::tantrum_act`: toppled furniture (`victim` -1) or a fist fight.
    pub(crate) fn justice_tantrum(&mut self, s: usize, fight_victim: i32, now: u64) {
        let kind = if fight_victim >= 0 { "DisorderlyBehavior" } else { "BuildingDestruction" };
        self.justice_crime(kind, s, fight_victim, false, now);
    }

    /// [hook] From `population`: a noble's mandate (or export ban) was not kept. The noble
    /// names a culprit (DF: a random citizen) who stands accused.
    pub(crate) fn justice_mandate_missed(&mut self, noble: &str, export_ban: bool, now: u64) {
        let Some(ns) = self.s2_noble_slot(noble) else { return };
        let cands: Vec<usize> = (0..self.ai.len()).filter(|&t| t != ns && self.s2_citizen(t)).collect();
        if cands.is_empty() {
            return;
        }
        let mut r = Rng::for_unit(self.seed, ns, now, 0x3A40);
        let culprit = cands[r.below(cands.len() as u32) as usize];
        let nid = self.ai[ns].id;
        let kind = if export_ban { "ExportViolation" } else { "ProductionOrderViolation" };
        let id = self.justice_crime(kind, culprit, nid, true, now);
        if let Some(ci) = self.crime_idx(id) {
            self.world.crimes[ci].witnesses = vec![nid];
        }
    }

    /// [hook] A visiting thief leaves: steal the most valuable free item near it.
    pub(crate) fn thief_strike(&mut self, vi: usize, now: u64) {
        let (s, thief) = {
            let v = &self.locs.visitors[vi];
            (v.slot as usize, v.thief)
        };
        if !thief || self.locs.off {
            return;
        }
        let pos = self.world.units[self.ai[s].uidx].pos;
        let mut best: Option<(i32, i32)> = None;
        for (i, it) in self.world.items.iter().enumerate() {
            if it.pos.z != pos.z || it.pos.dist(pos) > 10 || it.flags.artifact && it.value <= 0 {
                continue;
            }
            if !matches!(it.kind.as_str(), "GOBLET" | "INSTRUMENT" | "TOY" | "AMULET" | "BRACELET" | "CROWN" | "EARRING" | "FIGURINE" | "RING" | "SCEPTER" | "TOTEM" | "GEM" | "SMALLGEM" | "COIN" | "BAR" | "TOOL" | "WEAPON") {
                continue;
            }
            if !self.item_free(i, now) || it.container >= 0 {
                continue;
            }
            if best.map(|b| (it.value, -it.id) > (b.0, -b.1)).unwrap_or(true) {
                best = Some((it.value, it.id));
            }
        }
        let Some((value, id)) = best else { return };
        self.s2_take_item(s, id);
        self.locs.visitors[vi].loot = id;
        self.justice.rep.thefts += 1;
        self.justice.rep.stolen_value += value as i64;
        let cid = self.justice_crime("Theft", s, -1, false, now);
        // A thief seen in the act is identified at once (DF: "X is a thief!").
        if let Some(ci) = self.crime_idx(cid) {
            if !self.world.crimes[ci].witnesses.is_empty() {
                let uid = self.ai[s].id;
                self.world.crimes[ci].criminal = uid;
                self.world.crimes[ci].status = "Accused".into();
                self.justice.rep.identified += 1;
                let name = self.world.units[self.ai[s].uidx].name.clone();
                self.announce(now, format!("{name} is a thief! Stop the thief!"), Some(pos), AnnouncementKind::Warning);
            }
        }
    }

    /// Daily investigation and every pass the prisoners (from the society2 pass).
    pub(crate) fn justice_tick(&mut self, now: u64, phase: u64) {
        self.prisoners_tick(now);
        if phase != 0 || self.world.crimes.is_empty() {
            return;
        }
        let enforcer = self.law_enforcer();
        for k in 0..self.justice.rt.len() {
            let (id, culprit) = (self.justice.rt[k].id, self.justice.rt[k].culprit);
            let Some(ci) = self.crime_idx(id) else { continue };
            let status = self.world.crimes[ci].status.clone();
            // Culprit gone (left the map or dead): the case is closed.
            let gone = self.unit_slot.get(&culprit).map(|&s| self.ai[s].kind == Kind::Dead).unwrap_or(true);
            if gone && matches!(status.as_str(), "Unsolved" | "Accused" | "") {
                self.world.crimes[ci].status = "Closed".into();
                continue;
            }
            if !matches!(status.as_str(), "Unsolved" | "") {
                continue;
            }
            let Some(es) = enforcer else {
                if !self.justice.rt[k].no_enforcer_counted {
                    self.justice.rt[k].no_enforcer_counted = true;
                    self.justice.rep.no_enforcer += 1;
                }
                continue;
            };
            let n_wit = self.world.crimes[ci].witnesses.len();
            let solved = if n_wit > 0 {
                self.justice.rt[k].interviewed += 1;
                true
            } else {
                let mut r = Rng::for_unit(self.seed, es, now, 0x1A7E + id as u64);
                r.chance(1, 56)
            };
            if solved {
                self.world.crimes[ci].criminal = culprit;
                self.world.crimes[ci].status = "Accused".into();
                self.justice.rep.identified += 1;
                let name = self.world.unit(culprit).map(|u| u.name.clone()).unwrap_or_default();
                let kind = self.world.crimes[ci].kind.clone();
                self.announce(now, format!("{name} has been accused of {kind}."), None, AnnouncementKind::Info);
            }
        }
        if self.justice.auto_convict && enforcer.is_some() {
            let accused: Vec<(i32, i32)> = self.world.crimes.iter().filter(|c| c.status == "Accused" && c.criminal >= 0).map(|c| (c.id, c.criminal)).collect();
            for (id, who) in accused {
                self.convict_impl(id, who);
            }
        }
    }

    /// Convict `unit_id` of crime `crime_id` and sentence him (wiki "Justice").
    pub(crate) fn convict_impl(&mut self, crime_id: i32, unit_id: i32) {
        let now = self.ticks;
        let Some(ci) = self.crime_idx(crime_id) else { return };
        if matches!(self.world.crimes[ci].status.as_str(), "Convicted" | "Serving" | "Punished") {
            return;
        }
        self.world.crimes[ci].convicted = unit_id;
        self.world.crimes[ci].status = "Convicted".into();
        self.justice.rep.convicted += 1;
        let culprit = self.justice.rt.iter().find(|r| r.id == crime_id).map(|r| r.culprit).unwrap_or(unit_id);
        let wrongful = culprit != unit_id;
        if wrongful {
            self.justice.rep.wrongful += 1;
        }
        let Some(&s) = self.unit_slot.get(&unit_id) else {
            self.world.crimes[ci].status = "Closed".into();
            return;
        };
        if self.ai[s].kind == Kind::Dead {
            self.world.crimes[ci].status = "Closed".into();
            return;
        }
        let kind = self.world.crimes[ci].kind.clone();
        let (days, strikes) = sentence(&kind);
        let hammerer = self.s2_noble_slot("HAMMERER");
        let name = self.world.units[self.ai[s].uidx].name.clone();
        let punished = if strikes > 0 && hammerer.is_some() {
            self.world.crimes[ci].punishment = format!("Hammerstrikes {strikes}");
            self.hammerstrikes(s, hammerer.unwrap(), strikes, now)
        } else if days > 0 && self.jail_prisoner(s, crime_id, days, now) {
            self.world.crimes[ci].punishment = format!("Jail {days} days");
            self.world.crimes[ci].status = "Serving".into();
            self.world.crimes[ci].sentence_days = days;
            self.announce(now, format!("{name} has been sentenced to {days} days in jail."), None, AnnouncementKind::Info);
            false
        } else {
            if days > 0 {
                self.justice.rep.no_jail += 1;
            }
            self.world.crimes[ci].punishment = "Beating".into();
            self.beating(s, now)
        };
        if punished {
            self.world.crimes[ci].status = "Punished".into();
        }
        let victim = self.world.crimes[ci].victim;
        if !wrongful {
            if let Some(&vs) = self.unit_slot.get(&victim) {
                if self.s2_citizen(vs) && vs != s {
                    self.feel(vs, "ConvictionJusticeSelf", Class::Pos, 120, -1, 0, now);
                }
            }
        }
    }

    /// Beating by the law enforcer. False if there is none (punishment not given).
    fn beating(&mut self, s: usize, now: u64) -> bool {
        let Some(es) = self.law_enforcer() else { return false };
        let w = Wound { body_part: "UB".into(), layer: "SKIN".into(), flags: vec!["BRUISED".into()], severity: 400, pain: 15, bleeding: 0, age: 0 };
        self.world.units[self.ai[s].uidx].wounds.push(w);
        self.feel(s, "GotBeaten", Class::Neg, 150, -1, 0, now);
        if es != s {
            self.feel(es, "GaveBeating", Class::PosMild, 60, -1, 0, now);
        }
        self.justice.rep.beatings += 1;
        let (a, b) = (self.world.units[self.ai[es].uidx].name.clone(), self.world.units[self.ai[s].uidx].name.clone());
        let p = self.world.units[self.ai[s].uidx].pos;
        self.announce(now, format!("{a} beats {b} as punishment."), Some(p), AnnouncementKind::Info);
        true
    }

    /// Hammerstrikes by the hammerer (wiki "Hammerer").
    fn hammerstrikes(&mut self, s: usize, hs: usize, n: i32, now: u64) -> bool {
        let hammer = self.world.items.iter().enumerate().any(|(i, it)| it.kind == "WEAPON" && it.subtype.contains("HAMMER") && !self.imeta[i].dead);
        if !hammer {
            self.feel(hs, "NoHammer", Class::NegMild, 80, -1, 0, now);
            return self.beating(s, now);
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0x4A33);
        const PARTS: [&str; 6] = ["UB", "LB", "RUA", "LUA", "RUL", "LUL"];
        for _ in 0..n {
            let part = PARTS[r.below(PARTS.len() as u32) as usize];
            let broken = r.chance(1, 15);
            let w = if broken {
                self.justice.rep.broken_bones += 1;
                Wound { body_part: part.into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 3000, pain: 40, bleeding: 0, age: 0 }
            } else {
                Wound { body_part: part.into(), layer: "MUSCLE".into(), flags: vec!["BRUISED".into()], severity: 600, pain: 10, bleeding: 0, age: 0 }
            };
            self.world.units[self.ai[s].uidx].wounds.push(w);
        }
        self.justice.rep.hammerstrikes += n as u64;
        self.feel(s, "GotHammered", Class::NegStrong, 200, -1, 0, now);
        self.feel(hs, "GaveHammering", Class::PosMild, 60, -1, 0, now);
        let (a, b) = (self.world.units[self.ai[hs].uidx].name.clone(), self.world.units[self.ai[s].uidx].name.clone());
        let p = self.world.units[self.ai[s].uidx].pos;
        self.announce(now, format!("{a} strikes {b} with a hammer {n} times as punishment."), Some(p), AnnouncementKind::Warning);
        true
    }

    /// Free restraint tiles in jails: chains or built cages inside Dungeon zones.
    fn free_restraints(&self) -> Vec<u32> {
        let jails: Vec<&rf_core::Building> = self.world.buildings.iter().filter(|b| b.kind == "Civzone" && matches!(b.subtype.as_str(), "Dungeon" | "Jail")).collect();
        let mut out = Vec::new();
        for b in &self.world.buildings {
            if !matches!(b.kind.as_str(), "Chain" | "Cage") || !b.built {
                continue;
            }
            if !jails.iter().any(|j| j.contains(b.center)) {
                continue;
            }
            let Some(t) = self.world.idx(b.center) else { continue };
            // Stand on the restraint, or next to it.
            let tile = if self.nav.walkable(t) {
                Some(t as u32)
            } else {
                let (x, y, z) = self.nav.xyz(t);
                crate::nav::DIRS8
                    .iter()
                    .filter_map(|&(dx, dy)| {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        (nx >= 0 && ny >= 0 && (nx as usize) < self.nav.dx && (ny as usize) < self.nav.dy).then(|| self.nav.idx(nx as usize, ny as usize, z))
                    })
                    .find(|&q| self.nav.walkable(q))
                    .map(|q| q as u32)
            };
            if let Some(tile) = tile {
                if !self.justice.prisoners.iter().any(|p| p.tile == tile) {
                    out.push(tile);
                }
            }
        }
        out
    }

    fn jail_prisoner(&mut self, s: usize, crime: i32, days: i32, now: u64) -> bool {
        let free = self.free_restraints();
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let Some(&tile) = free.iter().find(|&&t| self.nav.comp_root(t as usize) == c) else { return false };
        self.cancel_task(s);
        self.justice.prisoners.push(crate::justice::Prisoner { slot: s as u32, crime, until: now + days as u64 * DAY, tile, next_day: now + DAY });
        self.justice.rep.jailed += 1;
        self.ai[s].wake = now + 1;
        true
    }

    fn prisoners_tick(&mut self, now: u64) {
        let mut k = 0;
        while k < self.justice.prisoners.len() {
            let p = self.justice.prisoners[k].clone();
            let s = p.slot as usize;
            if self.ai[s].kind == Kind::Dead {
                self.justice.prisoners.remove(k);
                continue;
            }
            if now >= p.until {
                self.justice.prisoners.remove(k);
                self.justice.rep.released += 1;
                if let Some(ci) = self.crime_idx(p.crime) {
                    self.world.crimes[ci].status = "Punished".into();
                    self.world.crimes[ci].sentence_days = 0;
                }
                self.feel(s, "JailReleased", Class::PosMild, 100, -1, 0, now);
                let name = self.world.units[self.ai[s].uidx].name.clone();
                self.announce(now, format!("{name} has been released from jail."), None, AnnouncementKind::Info);
                if matches!(self.ai[s].task, Task::Idle | Task::Wander) {
                    self.ai[s].wake = now + 1;
                }
                continue;
            }
            if now >= p.next_day {
                self.justice.prisoners[k].next_day = now + DAY;
                self.justice.rep.jail_days += 1;
                if let Some(ci) = self.crime_idx(p.crime) {
                    let c = &mut self.world.crimes[ci];
                    c.sentence_days = (c.sentence_days - 1).max(0);
                }
                self.feel(s, "Jailed", Class::Neg, 100, -1, 0, now);
            }
            k += 1;
        }
    }

    /// [hook] Start of `decide`: prisoners stay restrained in the jail; jailers bring them food
    /// and water. True if the unit is a prisoner (decision made).
    pub(crate) fn justice_confined(&mut self, s: usize, now: u64) -> bool {
        let Some(k) = self.justice.prisoners.iter().position(|p| p.slot == s as u32) else { return false };
        let tile = self.justice.prisoners[k].tile;
        let (th, hu, sl) = (self.ai[s].thirst, self.ai[s].hunger, self.ai[s].sleep);
        if th >= needs::THIRST_SEEK {
            if let Some(id) = self.s2_take_consumable(true) {
                self.social_drank(s, id, now);
                self.consume(id);
            }
            self.ai[s].thirst = (th - needs::DRINK_SATISFIES).max(0);
        }
        if hu >= needs::HUNGER_SEEK {
            if let Some(id) = self.s2_take_consumable(false) {
                self.ai[s].hunger = (hu - needs::EAT_SATISFIES).max(0);
                self.consume(id);
            }
        }
        if self.ai[s].pos != tile {
            let c = self.nav.comp_near(self.ai[s].pos as usize);
            if c != 0 && self.nav.comp_root(tile as usize) == c {
                self.ai[s].task = Task::Wander;
                self.request_path(s, crate::path::Goals::one(tile), now);
            } else {
                // No way to the jail: the guards carry him there (estimate).
                let p = self.world.pos_of(tile as usize);
                self.ai[s].pos = tile;
                self.world.units[self.ai[s].uidx].pos = p;
                self.ai[s].wake = now + 1;
            }
            return true;
        }
        if sl >= needs::SLEEP_SEEK {
            self.start_sleep_here(s, now);
            return true;
        }
        self.ai[s].wake = now + 100;
        true
    }

    pub(crate) fn justice_report_impl(&self) -> JusticeReport {
        let mut r = self.justice.rep.clone();
        r.crimes = self.justice.counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        r
    }

    pub(crate) fn justice_is_prisoner(&self, unit_id: i32) -> bool {
        self.unit_slot.get(&unit_id).map(|&s| self.justice.prisoners.iter().any(|p| p.slot == s as u32)).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
