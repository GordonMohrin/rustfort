//! [undead13] Undead and necromancy (DF v50; wiki Undead, Reanimation, Necromancer, Husk,
//! Thrall, Evil weather, Surroundings).
//!
//! # DF rules implemented
//! - **Reanimation** (w:Surroundings, w:Undead): in a reanimating region (`World::climate.
//!   reanimating`, or the player/test toggle [`Sim::set_reanimation_enabled`]) dead things rise
//!   again: whole corpses as hostile undead of their race, severed body parts as small undead
//!   parts. Corpses inside buildings (coffins), containers or held by a unit stay down. A
//!   destroyed undead leaves a corpse that can rise again, as in DF.
//! - **Undead combat traits** (creature tags of the vanilla reanimation interaction:
//!   `NOT_LIVING`, `OPPOSED_TO_LIFE`, `NOPAIN`, `NOSTUN`, `NOFEAR`, `NO_EAT`, `NO_DRINK`,
//!   `NO_SLEEP`, `NOTHOUGHT`, `PARALYZEIMMUNE`): undead never pass out (pain, blood loss, brain
//!   bruises), do not bleed, are not paralyzed by nerve wounds and do not die from brain or
//!   heart wounds. They are destroyed by cutting off the head or by mangling the torso
//!   (`undead_blow`). As Kind::Other hostiles they have no needs and never flee.
//! - **Necromancers** raise corpses near them (DF `RAISE_DEAD`-style interaction).
//! - **Necromancer sieges** (w:Necromancer, w:Siege): a necromancer with an army of undead,
//!   through the regular siege machinery (`raids::spawn_raid_ex`: map edge, camp, charge,
//!   leader). The necromancer leads; when the siege breaks the undead do not retreat but stay
//!   on the map as a horde. Natural sieges come only when a necromancer tower entity exists in
//!   the world (an `EntityInfo` whose kind or name names a necromancer or tower) or the player
//!   enables them ([`Sim::set_necromancer_sieges`]).
//! - **Husks and thralls** (w:Evil weather): units outside under an evil cloud or rain whose
//!   effect is `HUSK` or `THRALL` (from [`Sim::evil_weather`], owned by weather) turn into
//!   hostile husks / thralls.
//!
//! # Estimates (see docs/FIDELITY.md, section `[undead13]`)
//! - Rising chance 1 in `RISE_ODDS` per pass (100 ticks) per corpse; parts are 1/10 body size.
//! - Torso destruction: total wound severity >= `MANGLE_SEV` on upper/lower body.
//! - Necromancer raising: 1 corpse per pass within `RAISE_RANGE` tiles, chance 1 in 2.
//! - Natural necromancer siege: 1 in 4 per season start once the fort meets the siege tier;
//!   undead count half the regular siege size, 10..=60; every 10th undead is intelligent.
//! - Exposure to a husk/thrall cloud: 1 in 2 per pass for each exposed unit.

use crate::combat::{bp, Role};
use crate::rng::Rng;
use crate::round13_api::{EvilWeather, Threat, ThreatKind, UndeadReport, UnitCurse};
use crate::{AnnouncementKind, Kind, Sim, NONE};
use rf_core::{tile_flags, Pos, Unit, UnitFlags};
use serde::{Deserialize, Serialize};

/// `undead_pass` runs when `now % UNDEAD_PERIOD == UNDEAD_PHASE` (owner may change both).
pub(crate) const UNDEAD_PERIOD: u64 = 100;
pub(crate) const UNDEAD_PHASE: u64 = 59;

/// Chance per pass that a corpse in a reanimating region rises: 1 in this (estimate).
pub(crate) const RISE_ODDS: u32 = 8;
/// Necromancers raise corpses within this distance (estimate).
pub(crate) const RAISE_RANGE: i32 = 12;
/// Total severity (10 000 per fully damaged layer) on the upper/lower body that destroys an
/// undead (estimate).
pub(crate) const MANGLE_SEV: i32 = 40_000;

const SALT_RISE: u64 = 0x0DEA_D001;
const SALT_RAISE: u64 = 0x0DEA_D002;
const SALT_EVIL: u64 = 0x0DEA_D003;
const SALT_SIEGE: u64 = 0x0DEA_D004;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Undead {
    /// Player toggle; None = follow `World::climate.reanimating`.
    pub reanimation: Option<bool>,
    pub rep: UndeadReport,
    /// Remains that may rise, ascending by item id: (item id, unit id of the dead, severed part).
    pub remains: Vec<(i32, i32, bool)>,
    /// Living necromancers on the map (unit ids, ascending).
    pub necros: Vec<i32>,
    /// Necromancer sieges: (raid id, necromancer unit id, arrival tick).
    pub sieges: Vec<(u16, i32, u64)>,
    /// Intelligent undead (unit ids, ascending).
    pub intelligent: Vec<i32>,
    /// Undead, husks and thralls made by this module: (unit id, tick).
    pub risen: Vec<(i32, u64)>,
    /// Natural necromancer sieges: None = only with a necromancer tower entity.
    pub necro_sieges: Option<bool>,
    /// Season seen last + 1 (0 = not yet) for natural necromancer sieges.
    pub last_season: i32,
}

impl Undead {
    fn idle(&self) -> bool {
        self.remains.is_empty() && self.necros.is_empty() && self.sieges.is_empty() && self.risen.is_empty() && self.intelligent.is_empty() && self.last_season == 0 && {
            let r = &self.rep;
            r.reanimated == 0 && r.destroyed == 0 && r.necro_sieges == 0 && r.husks == 0 && r.thralls == 0
        }
    }
}

impl Sim {
    /// Hook from `tick_inner` (every `UNDEAD_PERIOD` ticks).
    pub(crate) fn undead_pass(&mut self, now: u64) {
        let rean = self.reanimation_enabled();
        let ew = self.evil_weather();
        let sieges_ok = self.necro_sieges_possible();
        if !rean && ew.is_none() && !sieges_ok && self.undead.necros.is_empty() && self.undead.sieges.is_empty() && self.undead.remains.is_empty() {
            return;
        }
        if sieges_ok {
            self.undead_season(now);
        }
        self.undead_siege_upkeep(now);
        self.undead_prune_remains();
        if !self.undead.necros.is_empty() {
            self.undead_necro_raise(now);
        }
        if rean {
            self.undead_reanimate(now);
        }
        if let Some(ew) = ew {
            self.undead_evil_exposure(&ew, now);
        }
    }

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn undead_hash(&self, mix: &mut dyn FnMut(u64)) {
        let u = &self.undead;
        if u.idle() {
            return;
        }
        let r = &u.rep;
        for v in [r.reanimated, r.destroyed, r.necro_sieges, r.husks, r.thralls, u.last_season as u64] {
            mix(v);
        }
        mix(u.remains.len() as u64);
        for &(a, b, p) in &u.remains {
            mix(((a as u32 as u64) << 32) | (b as u32 as u64) ^ p as u64);
        }
        for &n in &u.necros {
            mix(n as u32 as u64);
        }
        for &(sid, n, t) in &u.sieges {
            mix(((sid as u64) << 32) | n as u32 as u64);
            mix(t);
        }
        for &(id, t) in &u.risen {
            mix(id as u32 as u64);
            mix(t);
        }
        for &i in &u.intelligent {
            mix(i as u32 as u64);
        }
    }

    // ------------------------------------------------------------------ combat traits

    /// NOSTUN / NOPAIN: undead, husks and thralls never pass out.
    #[inline]
    pub(crate) fn undead_nostun(&self, s: usize) -> bool {
        let u = &self.world.units[self.ai[s].uidx];
        u.undead || (!u.curse.is_empty() && matches!(u.curse.as_str(), "HUSK" | "THRALL"))
    }

    /// NOT_LIVING: undead and husks do not bleed and ignore brain/heart wounds.
    #[inline]
    pub(crate) fn undead_bloodless(&self, s: usize) -> bool {
        let u = &self.world.units[self.ai[s].uidx];
        u.undead || (!u.curse.is_empty() && u.curse == "HUSK")
    }

    /// After a blow on a non-living unit: what destroys it (None: it keeps coming).
    /// `died` is what would have killed a living creature ("brain", "heart", "head").
    pub(crate) fn undead_blow(&mut self, d: usize, died: Option<&'static str>) -> Option<&'static str> {
        self.combat.f[d].paralyzed = false; // PARALYZEIMMUNE, no nerve paralysis
        if died == Some("head") {
            return Some("destroyed");
        }
        let f = &self.combat.f[d];
        if f.body == u16::MAX {
            return None;
        }
        let body = &self.combat.bodies[f.body as usize];
        let mut sev = 0;
        for w in &f.wounds {
            if let Some(p) = body.parts.get(w.part as usize) {
                if p.flags & (bp::UPPER | bp::LOWER) != 0 {
                    sev += w.sev;
                }
            }
        }
        (sev >= MANGLE_SEV).then_some("destroyed")
    }

    // ------------------------------------------------------------------ remains

    /// Hook from `kill` (corpse) and `sever` (body part): remember who the remains belong to
    /// while undead can rise, and count destroyed undead.
    pub(crate) fn undead_remains(&mut self, s: usize, item: i32, part: bool) {
        let u = &self.world.units[self.ai[s].uidx];
        let was_undead = u.undead || u.curse == "HUSK";
        let uid = u.id;
        if was_undead && !part {
            self.undead.rep.destroyed += 1;
        }
        if !part && !self.undead.necros.is_empty() {
            if let Ok(k) = self.undead.necros.binary_search(&uid) {
                self.undead.necros.remove(k); // a slain necromancer raises no more
            }
        }
        if item < 0 || !(self.reanimation_enabled() || !self.undead.necros.is_empty()) {
            return;
        }
        match self.undead.remains.binary_search_by_key(&item, |e| e.0) {
            Ok(_) => {}
            Err(k) => self.undead.remains.insert(k, (item, uid, part)),
        }
    }

    fn undead_prune_remains(&mut self) {
        if self.undead.remains.is_empty() {
            return;
        }
        let (idx, meta) = (&self.item_idx, &self.imeta);
        self.undead.remains.retain(|e| idx.get(&e.0).map(|&i| !meta[i as usize].dead).unwrap_or(false));
    }

    /// Item index `i` can rise (a loose corpse, or a known severed part).
    fn undead_can_rise(&self, i: usize) -> bool {
        if self.imeta[i].dead {
            return false;
        }
        let it = &self.world.items[i];
        let piece = match it.kind.as_str() {
            "CORPSE" => false,
            "CORPSEPIECE" => true,
            _ => return false,
        };
        if it.holder >= 0 || it.container >= 0 || it.building >= 0 || it.flags.in_building || it.flags.in_inventory || it.flags.in_container || it.flags.construction {
            return false;
        }
        !piece || self.undead.remains.binary_search_by_key(&it.id, |e| e.0).is_ok()
    }

    fn undead_reanimate(&mut self, now: u64) {
        let mut rising: Vec<usize> = Vec::new();
        for i in 0..self.world.items.len() {
            if !self.undead_can_rise(i) {
                continue;
            }
            let id = self.world.items[i].id;
            if Rng::for_unit(self.seed, id as u32 as usize, now, SALT_RISE).chance(1, RISE_ODDS) {
                rising.push(i);
            }
        }
        let mut n = 0;
        let mut at = None;
        for i in rising {
            if let Some(s) = self.undead_raise_item(i, now) {
                n += 1;
                at = at.or(Some(self.upos(s)));
            }
        }
        if n > 0 {
            self.announce(now, if n == 1 { "A corpse has risen from the dead!".into() } else { format!("{n} corpses have risen from the dead!") }, at, AnnouncementKind::Warning);
        }
    }

    /// Necromancers raise the nearest corpse in range now and then.
    fn undead_necro_raise(&mut self, now: u64) {
        let necros = self.undead.necros.clone();
        let mut alive = Vec::with_capacity(necros.len());
        for id in necros {
            let Some(&s) = self.unit_slot.get(&id) else { continue };
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            alive.push(id);
            if self.combat.f.get(s).map(|f| f.uncon_until > now).unwrap_or(false) || !Rng::for_unit(self.seed, s, now, SALT_RAISE).chance(1, 2) {
                continue;
            }
            let p = self.upos(s);
            let mut best: Option<(i32, usize)> = None;
            for i in 0..self.world.items.len() {
                let ip = self.world.items[i].pos;
                if (ip.z - p.z).abs() > 1 {
                    continue;
                }
                let d = ip.dist(p);
                if d > RAISE_RANGE || best.map(|b| d >= b.0).unwrap_or(false) || !self.undead_can_rise(i) {
                    continue;
                }
                best = Some((d, i));
            }
            if let Some((_, i)) = best {
                if let Some(z) = self.undead_raise_item(i, now) {
                    let (nn, zn) = (self.uname(s), self.uname(z));
                    self.combat.log_line(format!("{} raises {}!", crate::combat::cap_pub(&nn), zn));
                }
            }
        }
        self.undead.necros = alive;
    }

    /// Raise the corpse or body part at item index `i`; returns the new unit's slot.
    fn undead_raise_item(&mut self, i: usize, now: u64) -> Option<usize> {
        let it = &self.world.items[i];
        let (id, pos, piece) = (it.id, it.pos, it.kind == "CORPSEPIECE");
        let tile = self.world.idx(pos)?;
        if !self.nav.walkable(tile) {
            return None;
        }
        let rec = self.undead.remains.binary_search_by_key(&id, |e| e.0).ok().map(|k| self.undead.remains[k]);
        let src = rec.and_then(|r| self.world.unit(r.1));
        let race = match src {
            Some(u) => u.race.clone(),
            None => {
                // Corpse of a unit we did not see die: race from its material (`CREATURE:RACE:...`).
                let tok = self.world.material(it.mat).map(|m| m.token.as_str()).unwrap_or("");
                let mut parts = tok.split(':');
                match (parts.next(), parts.next()) {
                    (Some("CREATURE"), Some(r)) if !r.is_empty() => r.to_string(),
                    _ => return None,
                }
            }
        };
        let name = match src {
            Some(u) if u.undead && !u.name.is_empty() && !piece => u.name.clone(),
            _ if !it.desc.is_empty() => it.desc.clone(),
            _ => format!("{} corpse", race.to_ascii_lowercase().replace('_', " ")),
        };
        self.destroy_item(id);
        if let Ok(k) = self.undead.remains.binary_search_by_key(&id, |e| e.0) {
            self.undead.remains.remove(k);
        }
        let s = self.undead_spawn(&race, name, pos, if piece { 10 } else { 1 }, now)?;
        self.undead.rep.reanimated += 1;
        Some(s)
    }

    /// A new hostile undead unit at `pos` (body size divided by `size_div`); its slot.
    fn undead_spawn(&mut self, race: &str, name: String, pos: Pos, size_div: i32, now: u64) -> Option<usize> {
        let (bsz, wspd) = self
            .world
            .creature_raws
            .iter()
            .find(|c| c.token == race)
            .and_then(|c| c.castes.first())
            .map(|c| (if c.body_size > 0 { c.body_size } else { 60_000 }, if c.walk_speed > 0 { c.walk_speed } else { 900 }))
            .unwrap_or((60_000, 900));
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let unit = Unit {
            id,
            race: race.into(),
            name,
            profession: "STANDARD".into(),
            pos,
            flags: UnitFlags { hostile: true, ..Default::default() },
            civ_id: -1,
            hist_fig_id: -1,
            sex: -1,
            squad_id: -1,
            squad_position: -1,
            body_size: (bsz / size_div.max(1)).max(1),
            walk_speed: wspd,
            invader_civ: -1,
            undead: true,
            ..Default::default()
        };
        self.world.units.push(unit);
        self.init_units();
        self.combat_sync_units();
        let s = *self.unit_slot.get(&id)?;
        if self.ai[s].kind == Kind::Dead {
            return None;
        }
        self.combat.f[s].role = Role::Hostile;
        self.engage_now(s, now);
        self.undead.risen.push((id, now));
        Some(s)
    }

    // ------------------------------------------------------------------ necromancer sieges

    /// A necromancer tower entity of the world (id), if any.
    fn necro_tower(&self) -> Option<i32> {
        self.world
            .entities
            .iter()
            .find(|e| has_ci(&e.kind, "necromancer") || has_ci(&e.kind, "tower") || has_ci(&e.name, "necromancer"))
            .map(|e| e.id)
    }

    fn necro_sieges_possible(&self) -> bool {
        self.undead.necro_sieges.unwrap_or_else(|| self.necro_tower().is_some())
    }

    /// Season starts: a natural necromancer siege (1 in 4 once the fort meets the siege tier).
    fn undead_season(&mut self, now: u64) {
        let season = self.world.calendar.month() / 3 + 1;
        if self.undead.last_season == season {
            return;
        }
        let first = self.undead.last_season == 0;
        self.undead.last_season = season;
        if first || !self.combat.inv.enabled {
            return;
        }
        let Some(size) = self.siege_size() else { return };
        if !Rng::for_unit(self.seed, 0, now, SALT_SIEGE).chance(1, 4) {
            return;
        }
        let n = (size / 2).clamp(10, 60);
        self.necro_siege(n, true, now);
    }

    pub(crate) fn necro_siege(&mut self, n: usize, camp: bool, now: u64) -> usize {
        let civ = self.necro_tower();
        let camp = if camp { crate::raids::camp_ticks("HUMAN", &mut Rng::for_unit(self.seed, 1, now, SALT_SIEGE)) } else { 0 };
        let extra = crate::warcal::RaidExtra { mounts: 0, masters_pct: 0, civ };
        let k = self.spawn_raid_ex(crate::raids::RaidKind::Siege, "HUMAN", n + 1, 0, camp, extra);
        if k == 0 {
            return 0;
        }
        let Some(g) = self.combat.inv.sieges.last_mut() else { return 0 };
        let sid = g.id;
        let members = g.members.clone();
        g.leader = members[0];
        g.race = "undead".into();
        let mut necro = -1;
        for (j, &m) in members.iter().enumerate() {
            let uidx = self.ai[m as usize].uidx;
            let u = &mut self.world.units[uidx];
            if j == 0 {
                u.curse = "NECROMANCER".into();
                u.custom_profession = "necromancer".into();
                necro = u.id;
            } else {
                u.undead = true;
                u.custom_profession = "corpse".into();
                if j % 10 == 0 {
                    let id = u.id;
                    if let Err(p) = self.undead.intelligent.binary_search(&id) {
                        self.undead.intelligent.insert(p, id);
                    }
                }
            }
        }
        if let Err(p) = self.undead.necros.binary_search(&necro) {
            self.undead.necros.insert(p, necro);
        }
        self.undead.sieges.push((sid, necro, now));
        self.undead.rep.necro_sieges += 1;
        let at = Some(self.upos(members[0] as usize));
        self.announce(now, "The dead walk. Hide while you still can!".into(), at, AnnouncementKind::Warning);
        k
    }

    /// Ended sieges are forgotten; a breaking siege's undead do not retreat (they stay as a
    /// horde).
    fn undead_siege_upkeep(&mut self, now: u64) {
        let mut k = 0;
        while k < self.undead.sieges.len() {
            let sid = self.undead.sieges[k].0;
            let Some(gi) = self.combat.inv.sieges.iter().position(|g| g.id == sid) else {
                self.undead.sieges.remove(k);
                continue;
            };
            if self.combat.inv.sieges[gi].fleeing {
                let members = self.combat.inv.sieges[gi].members.clone();
                let mut keep = Vec::with_capacity(members.len());
                for m in members {
                    let s = m as usize;
                    if self.ai[s].kind == Kind::Dead || !self.world.units[self.ai[s].uidx].undead {
                        keep.push(m);
                        continue;
                    }
                    let f = &mut self.combat.f[s];
                    f.siege = 0;
                    f.raid = 0;
                    f.role = Role::Hostile;
                    f.target = NONE;
                    f.leave_since = 0;
                    self.engage_now(s, now);
                }
                self.combat.inv.sieges[gi].members = keep;
                self.undead.sieges.remove(k);
                continue;
            }
            k += 1;
        }
    }

    // ------------------------------------------------------------------ evil weather

    /// Units outside under a husk/thrall cloud or rain turn.
    pub(crate) fn undead_evil_exposure(&mut self, ew: &EvilWeather, now: u64) {
        let curse = match ew.effect.as_str() {
            "HUSK" => "HUSK",
            "THRALL" => "THRALL",
            _ => return,
        };
        let mut hit = Vec::new();
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            if u.undead || !u.curse.is_empty() || u.flags.caged {
                continue;
            }
            let t = self.ai[s].pos as usize;
            if self.world.tiles.get(t).map(|x| x.flags & tile_flags::OUTSIDE == 0).unwrap_or(true) {
                continue;
            }
            if let Some(c) = ew.center {
                let p = u.pos;
                if (p.x as i32 - c.x as i32).abs().max((p.y as i32 - c.y as i32).abs()) > ew.radius {
                    continue;
                }
            }
            if Rng::for_unit(self.seed, s, now, SALT_EVIL).chance(1, 2) {
                hit.push(s);
            }
        }
        for s in hit {
            self.undead_transform(s, curse, now);
        }
    }

    fn undead_transform(&mut self, s: usize, curse: &str, now: u64) {
        self.combat_sync_units();
        if self.ai[s].kind == Kind::Worker {
            self.cancel_task(s);
            self.ai[s].task = crate::Task::Idle;
            self.ai[s].kind = Kind::Other;
        }
        let name = self.uname(s);
        let uidx = self.ai[s].uidx;
        let u = &mut self.world.units[uidx];
        u.curse = curse.into();
        u.flags.citizen = false;
        u.flags.tame = false;
        u.squad_id = -1;
        u.squad_position = -1;
        let id = u.id;
        let f = &mut self.combat.f[s];
        f.squad = -1;
        f.duty = crate::military::Duty::None;
        f.sub = crate::combat::Sub::None;
        self.make_hostile(id);
        self.undead.risen.push((id, now));
        let what = if curse == "HUSK" {
            self.undead.rep.husks += 1;
            "a husk"
        } else {
            self.undead.rep.thralls += 1;
            "a thrall"
        };
        let pos = self.world.units[uidx].pos;
        self.announce(now, format!("{} has become {what}!", crate::combat::cap_pub(&name)), Some(pos), AnnouncementKind::Warning);
    }

    // ------------------------------------------------------------------ API hooks

    pub(crate) fn undead_threats(&self, out: &mut Vec<Threat>) {
        let mut in_siege: Vec<u32> = Vec::new();
        for &(sid, necro, since) in &self.undead.sieges {
            let Some(g) = self.combat.inv.sieges.iter().find(|g| g.id == sid) else { continue };
            let units: Vec<i32> = g.members.iter().filter(|&&m| self.ai[m as usize].kind != Kind::Dead).map(|&m| self.ai[m as usize].id).collect();
            if units.is_empty() {
                continue;
            }
            in_siege.extend(g.members.iter().copied());
            let lead = self.unit_slot.get(&necro).copied().filter(|&s| self.ai[s].kind != Kind::Dead).or_else(|| self.unit_slot.get(&units[0]).copied());
            out.push(Threat {
                kind: ThreatKind::NecromancerSiege,
                name: "Necromancer siege".into(),
                race: "HUMAN".into(),
                units,
                pos: lead.map(|s| self.upos(s)),
                since,
                entity_id: self.necro_tower().unwrap_or(-1),
            });
        }
        let mut units = Vec::new();
        let mut first = None;
        for &s in &self.combat.hostiles {
            let su = s as usize;
            if self.ai[su].kind == Kind::Dead || in_siege.contains(&s) || !self.undead_nostun(su) {
                continue;
            }
            first = first.or(Some(su));
            units.push(self.ai[su].id);
        }
        if let Some(s0) = first {
            let since = self.undead.risen.iter().filter(|r| units.contains(&r.0)).map(|r| r.1).min().unwrap_or(self.ticks);
            out.push(Threat {
                kind: ThreatKind::UndeadHorde,
                name: "Undead".into(),
                race: self.world.units[self.ai[s0].uidx].race.clone(),
                units,
                pos: Some(self.upos(s0)),
                since,
                entity_id: -1,
            });
        }
    }

    pub(crate) fn undead_unit_flags(&self, unit_id: i32, f: &mut UnitCurse) {
        if self.undead.intelligent.binary_search(&unit_id).is_ok() {
            f.intelligent_undead = true;
        }
    }

    pub(crate) fn set_reanimation_enabled_impl(&mut self, on: bool) {
        self.undead.reanimation = Some(on);
    }

    pub(crate) fn reanimation_enabled_impl(&self) -> bool {
        self.undead.reanimation.unwrap_or(self.world.climate.reanimating)
    }

    pub(crate) fn spawn_necromancer_siege_impl(&mut self, n: usize) -> usize {
        let now = self.ticks;
        self.necro_siege(n.max(1), false, now)
    }

    pub(crate) fn undead_report_impl(&self) -> UndeadReport {
        self.undead.rep
    }

    pub(crate) fn set_necromancer_sieges_impl(&mut self, on: bool) {
        self.undead.necro_sieges = Some(on);
    }
}

/// ASCII case-insensitive substring test without allocating (`needle` lower case).
fn has_ci(hay: &str, needle: &str) -> bool {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    h.len() >= n.len() && h.windows(n.len()).any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b))
}

#[cfg(test)]
#[path = "undead_tests.rs"]
mod tests;
