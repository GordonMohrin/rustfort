//! [fire15] Burning units: catching fire, burn and melted-fat wounds per tissue layer, death,
//! extinguishing, material-based fire/magma immunity (w:Fire, w:Temperature, w:Magma crab,
//! w:Creature token FIREIMMUNE, w:Material science).

use super::heat::*;
use super::{BurningUnit, SALT};
use crate::combat::{bp, wf};
use crate::rng::Rng;
use crate::round13_api::WeatherKind;
use crate::{Kind, Sim};
use rf_core::{tile_flags, MaterialHeat, TissueLayerRaw, NO_MAT};

/// Burn severity per fire pass (ESTIMATE): a layer burns through in 100 ticks outside magma,
/// in 20 ticks in magma.
const BURN_SEV: i32 = 1_000;
const MAGMA_BURN_SEV: i32 = 5_000;

impl Sim {
    /// Heat properties of every flat tissue layer of combat body `b` (cached).
    pub(crate) fn fire15_body_heat(&mut self, b: u16) -> &[MaterialHeat] {
        let bi = b as usize;
        if self.fire.bodies.len() <= bi {
            self.fire.bodies.resize(bi + 1, Vec::new());
        }
        if self.fire.bodies[bi].is_empty() {
            let v = self.fire15_build_body_heat(b);
            self.fire.bodies[bi] = v;
        }
        &self.fire.bodies[bi]
    }

    fn fire15_build_body_heat(&self, b: u16) -> Vec<MaterialHeat> {
        let body = &self.combat.bodies[b as usize];
        let by_name = || body.layers.iter().map(|l| tissue_heat(&l.tissue)).collect::<Vec<_>>();
        // The (race, caste) this body was built for (unique value in the map: no order issue).
        let Some((race, caste)) = self.combat.body_of.iter().find(|(_, &v)| v == b).map(|(k, _)| k.clone()) else { return by_name() };
        let Some(raw) = self.world.creature_raws.iter().find(|c| c.token == race) else { return by_name() };
        let Some(c) = raw.castes.iter().find(|k| k.token == caste).or_else(|| raw.castes.first()) else { return by_name() };
        if c.body_parts.is_empty() {
            return by_name();
        }
        // Same layer order as `build_body_from_caste` (outermost first).
        let mut v = Vec::with_capacity(body.layers.len());
        for p in &c.body_parts {
            let mut ls: Vec<&TissueLayerRaw> = p.layers.iter().collect();
            let outer = |t: &TissueLayerRaw| {
                let u = t.tissue.to_ascii_uppercase();
                u.contains("SKIN") || u.contains("HAIR") || u.contains("SCALE") || u.contains("CHITIN") || u.contains("FEATHER")
            };
            if ls.len() > 1 && outer(ls[ls.len() - 1]) && !outer(ls[0]) {
                ls.reverse();
            }
            for t in ls {
                v.push(if t.mat != NO_MAT && (t.mat as usize) < self.world.materials.len() { self.fire15_heat(t.mat) } else { tissue_heat(&t.tissue) });
            }
        }
        if v.len() != body.layers.len() {
            return by_name();
        }
        v
    }

    /// Make sure the unit has its combat body (lazy in `combat`).
    fn fire15_body(&mut self, s: usize) -> Option<u16> {
        self.combat_sync_units();
        if s >= self.combat.f.len() {
            return None;
        }
        self.ensure_body(s);
        let b = self.combat.f[s].body;
        (b != u16::MAX).then_some(b)
    }

    /// Material-based immunity: every layer of every external part survives `t` (w:Creature
    /// token FIREIMMUNE: the flag alone does not protect; magma crab chitin 13000 does).
    pub(crate) fn fire15_proof_at(&mut self, s: usize, t: i32) -> bool {
        let Some(b) = self.fire15_body(s) else { return false };
        self.fire15_body_heat(b);
        let body = &self.combat.bodies[b as usize];
        let lh = &self.fire.bodies[b as usize];
        let mut any = false;
        for p in &body.parts {
            if p.flags & bp::INTERNAL != 0 || p.nl == 0 {
                continue;
            }
            any = true;
            for l in p.l0..p.l0 + p.nl {
                if burns_at(&lh[l as usize], t) || (is_set(lh[l as usize].melting_point) && lh[l as usize].melting_point <= t && !is_set(lh[l as usize].mat_fixed_temp)) {
                    return false;
                }
            }
        }
        any
    }

    /// Magma-proof unit (fluids immunity with temperature on).
    pub(crate) fn fire15_magma_proof(&mut self, s: usize) -> bool {
        self.fire15_proof_at(s, MAGMA_TEMP)
    }

    /// Is unit slot `s` burning?
    pub(crate) fn fire15_burning_slot(&self, s: usize) -> bool {
        self.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot).is_ok()
    }

    /// Set body parts of unit `s` on fire at source temperature `src`. `n` = how many external
    /// parts (0 = all; else up to n random ones, stance parts first, ESTIMATE). Parts whose
    /// outer layer does not burn at `src` do not catch.
    pub(crate) fn fire15_ignite_unit(&mut self, s: usize, src: i32, n: u32, magma: bool, now: u64) -> bool {
        if self.ai[s].kind == Kind::Dead || self.world.units[self.ai[s].uidx].flags.caged && !magma {
            return false;
        }
        let Some(b) = self.fire15_body(s) else { return false };
        self.fire15_body_heat(b);
        let body = &self.combat.bodies[b as usize];
        let lh = &self.fire.bodies[b as usize];
        let ps = &self.combat.f[s].part_state;
        let catches = |p: usize| {
            let pt = &body.parts[p];
            pt.flags & bp::INTERNAL == 0 && pt.nl > 0 && ps.get(p).map_or(true, |&st| st & 5 == 0) && burns_at(&lh[pt.l0 as usize], src)
        };
        let mut mask = 0u64;
        if n == 0 {
            for p in 0..body.parts.len() {
                if catches(p) {
                    mask |= 1 << p.min(63);
                }
            }
        } else {
            let mut r = Rng::for_unit(self.seed, s, now, SALT ^ 3);
            let stance: Vec<usize> = (0..body.parts.len()).filter(|&p| catches(p) && body.parts[p].flags & bp::STANCE != 0).collect();
            let cand: Vec<usize> = if stance.is_empty() { (0..body.parts.len()).filter(|&p| catches(p)).collect() } else { stance };
            if cand.is_empty() {
                return false;
            }
            let k = 1 + r.below(n.max(1));
            for _ in 0..k {
                let p = cand[r.below(cand.len() as u32) as usize];
                mask |= 1 << p.min(63);
            }
        }
        if mask == 0 {
            return false;
        }
        match self.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot) {
            Ok(k) => {
                let e = &mut self.fire.burning[k];
                e.parts |= mask;
                e.magma |= magma;
            }
            Err(k) => {
                self.fire.burning.insert(k, BurningUnit { slot: s as u32, parts: mask, since: now, magma });
                self.fire.rep.units_ignited += 1;
            }
        }
        true
    }

    /// Fluids hook (temperature on): a unit in magma catches fire at once on every external
    /// part (w:Magma "nearly-instant immolation"; ESTIMATE).
    pub(crate) fn fire15_unit_in_magma(&mut self, s: usize, now: u64) {
        if let Ok(k) = self.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot) {
            if self.fire.burning[k].magma {
                return;
            }
        }
        if !self.fire.ready || self.fire.heat.len() != self.world.materials.len() {
            self.fire15_init();
        }
        self.fire15_ignite_unit(s, MAGMA_TEMP, 0, true, now);
    }

    /// Fire pass, units: catching fire from burning tiles and burning neighbours, burn damage,
    /// going out, death.
    pub(crate) fn fire15_units_pass(&mut self, now: u64) {
        // 1. Catching fire.
        if !self.fire.cells.is_empty() || !self.fire.burn_tiles.is_empty() {
            let (dx, layer) = (self.nav.dx as i64, self.nav.layer);
            let _ = layer;
            for s in 0..self.ai.len() {
                if self.ai[s].kind == Kind::Dead || self.fire15_burning_slot(s) {
                    continue;
                }
                let i = self.ai[s].pos;
                let mut src = 0;
                if let Ok(c) = self.fire.cells.binary_search_by_key(&i, |c| c.tile) {
                    src = self.fire.cells[c].temp;
                }
                if let Ok(c) = self.fire.burn_tiles.binary_search_by_key(&i, |c| c.0) {
                    src = src.max(self.fire.burn_tiles[c].1);
                }
                let mut r = Rng::for_unit(self.seed, s, now, SALT ^ 4);
                if src >= GRASS_FIRE_START {
                    // "A chance to catch fire each turn" on burning ground (w:Grass): 1/2 per
                    // pass, 1-3 parts (ESTIMATE).
                    if r.chance(1, 2) {
                        self.fire15_ignite_unit(s, src, 3, false, now);
                    }
                    continue;
                }
                // Next to a burning unit: 1/10 per pass (ESTIMATE).
                let near = self.fire.burning.iter().any(|b| {
                    let j = self.ai[b.slot as usize].pos as i64;
                    let d = j - i as i64;
                    let (ax, ay, az) = self.nav.xyz(i as usize);
                    let (bx, by, bz) = self.nav.xyz(j as usize);
                    let _ = (d, dx);
                    az == bz && (ax as i64 - bx as i64).abs() <= 1 && (ay as i64 - by as i64).abs() <= 1
                });
                if near && r.chance(1, 10) {
                    self.fire15_ignite_unit(s, GRASS_FIRE_MAX, 1, false, now);
                }
            }
        }
        // 2. Burning units.
        let mut k = 0;
        while k < self.fire.burning.len() {
            let mut e = self.fire.burning[k];
            let s = e.slot as usize;
            if self.ai[s].kind == Kind::Dead {
                self.fire.burning.remove(k);
                continue;
            }
            let i = self.ai[s].pos as usize;
            let mut r = Rng::for_unit(self.seed, s, now, SALT ^ 5);
            e.magma = self.fire15_magma(i);
            let wet = self.fire15_wet(i);
            let rain = !e.magma && self.world.tiles[i].flags & tile_flags::OUTSIDE != 0 && matches!(self.weather_at(self.world.pos_of(i)), WeatherKind::Rain | WeatherKind::Snow) && r.chance(1, 4);
            if wet || rain {
                self.fire.burning.remove(k);
                self.fire.rep.extinguished += 1;
                continue;
            }
            let src = if e.magma { MAGMA_TEMP } else { GRASS_FIRE_MAX };
            let died = self.fire15_burn_parts(s, &mut e, src, &mut r, now);
            if let Some(why) = died {
                self.fire.burning.remove(k);
                self.fire15_burn_death(s, why, now);
                continue;
            }
            if e.parts == 0 {
                self.fire.burning.remove(k);
                self.fire.rep.extinguished += 1;
                continue;
            }
            // A burning unit sets its tile on fire (1/2 per pass, ESTIMATE).
            if r.chance(1, 2) {
                self.fire15_try_ignite_tile(i, GRASS_FIRE_MAX);
            }
            self.fire.burning[k] = e;
            k += 1;
        }
    }

    /// Burn one pass of damage into every burning part of unit `s`. Returns the death reason.
    fn fire15_burn_parts(&mut self, s: usize, e: &mut BurningUnit, src: i32, r: &mut Rng, now: u64) -> Option<&'static str> {
        let Some(b) = self.fire15_body(s) else {
            e.parts = 0;
            return None;
        };
        self.fire15_body_heat(b);
        let bi = b as usize;
        let nparts = self.combat.bodies[bi].parts.len();
        let sev = if e.magma { MAGMA_BURN_SEV } else { BURN_SEV };
        let mut died = None;
        let mut wounded = false;
        for p in 0..nparts.min(64) {
            if e.parts & (1 << p) == 0 {
                continue;
            }
            let (l0, nl, flags, vol) = {
                let pt = &self.combat.bodies[bi].parts[p];
                (pt.l0, pt.nl, pt.flags, pt.vol)
            };
            // Outermost layer not yet burned through.
            let f = &self.combat.f[s];
            let target = (l0..l0 + nl).find(|&l| f.wounds.iter().find(|w| w.part == p as u16 && w.layer == l).map_or(true, |w| w.sev < 10_000));
            let Some(l) = target else {
                // Charred through: the part is lost (function lost); vital parts kill.
                e.parts &= !(1 << p);
                self.combat.f[s].part_state[p] |= 4;
                let body = &self.combat.bodies[bi];
                let vital_inside = body.parts[p].internal_children.iter().any(|&c| body.parts[c as usize].flags & (bp::VITAL | bp::THOUGHT | bp::CIRC) != 0);
                if flags & (bp::VITAL | bp::THOUGHT | bp::UPPER | bp::HEAD) != 0 || vital_inside {
                    died = Some(if e.magma { "has burned to death in magma" } else { "has burned to death" });
                }
                continue;
            };
            let lh = self.fire.bodies[bi][l as usize];
            if !burns_at(&lh, src) && !(is_set(lh.melting_point) && lh.melting_point <= src) {
                // This layer does not burn at this temperature: the part goes out.
                e.parts &= !(1 << p);
                continue;
            }
            let melted = is_set(lh.melting_point) && src > lh.melting_point && !is_set(lh.mat_fixed_temp);
            let lt = self.combat.bodies[bi].layers[l as usize].clone();
            self.add_wound(s, p as u16, l, if melted { wf::MELTED } else { wf::BURN }, sev, vol, &lt, now);
            wounded = true;
            self.fire.rep.burn_wounds += 1;
            if melted {
                // Melted fat bleeds (w:Magma crab; amount: the cut formula of `add_wound`, ESTIMATE).
                let bleed = ((lt.vascular.max(1) as f32) * vol / 2000.0 * sev as f32 / 10_000.0).max(1.0) as i32;
                let f = &mut self.combat.f[s];
                if let Some(w) = f.wounds.iter_mut().find(|w| w.part == p as u16 && w.layer == l) {
                    w.bleed += bleed;
                    w.flags |= wf::BLEEDING;
                }
                f.bleed += bleed;
                self.list_bleeder(s);
            }
            // A part may go out on its own (the unit moved off the fire): 1/30 per pass
            // outside magma (ESTIMATE).
            if !e.magma && r.chance(1, 30) {
                e.parts &= !(1 << p);
            }
        }
        if wounded {
            self.sync_wounds(s);
        }
        died
    }

    /// A unit burned to death: its corpse keeps burning (w:Fire "Ghosts": no remains).
    fn fire15_burn_death(&mut self, s: usize, why: &'static str, now: u64) {
        let n = self.world.items.len();
        self.fire.rep.units_burned_to_death += 1;
        self.combat_kill(s, now, why);
        if self.world.items.len() > n {
            let it = &self.world.items[self.world.items.len() - 1];
            if it.kind == "CORPSE" {
                let id = it.id;
                self.fire15_heat_item(id, true, 0);
            }
        }
    }

    /// Fire pass, heat emitters (fire men): their tile and the 8 around ignite (w:Fire man,
    /// w:Creature token FIXED_TEMP).
    pub(crate) fn fire15_emitters_pass(&mut self, _now: u64) {
        if self.fire.emitters.is_empty() {
            return;
        }
        let (dx, dy) = (self.nav.dx as i32, self.nav.dy as i32);
        for k in 0..self.fire.emitters.len() {
            let (s, t) = self.fire.emitters[k];
            if self.ai[s as usize].kind == Kind::Dead {
                continue;
            }
            let i = self.ai[s as usize].pos as usize;
            let (x, y, z) = self.nav.xyz(i);
            for oy in -1..=1i32 {
                for ox in -1..=1i32 {
                    let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                    if nx >= 0 && ny >= 0 && nx < dx && ny < dy {
                        let n = self.nav.idx(nx as usize, ny as usize, z);
                        self.fire15_try_ignite_tile(n, t);
                    }
                }
            }
        }
    }

    /// Heat blast on one tile (dragonfire API): items heat to at least `temp`, flammable ground
    /// burns out at once, units catch fire unless immune at that temperature or
    /// FIREIMMUNE_SUPER (w:Dragonfire).
    pub(crate) fn fire15_heat_blast(&mut self, i: usize, temp: i32, now: u64) {
        if !self.fire.ready || self.fire.heat.len() != self.world.materials.len() {
            self.fire15_init();
        }
        let mut ids = Vec::new();
        for (ix, it) in self.world.items.iter().enumerate() {
            if !self.imeta[ix].dead && it.container < 0 && self.world.idx(it.pos) == Some(i) {
                ids.push(it.id);
            }
        }
        for id in ids {
            self.fire15_heat_item(id, false, temp);
        }
        if self.fire15_try_ignite_tile(i, temp) {
            self.fire15_merge_ignitions(now);
            if let Ok(k) = self.fire.cells.binary_search_by_key(&(i as u32), |c| c.tile) {
                if self.fire.cells[k].kind == super::tiles::K_GRASS {
                    self.fire.cells[k].fuel = 1; // grass turns to ashes at once (w:Grass)
                }
            }
        }
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead || self.ai[s].pos as usize != i {
                continue;
            }
            let race = self.world.units[self.ai[s].uidx].race.clone();
            let sup = self.world.creature_raws.iter().find(|c| c.token == race).map_or(false, |c| c.flags.iter().chain(c.castes.iter().flat_map(|k| k.flags.iter())).any(|f| f == "FIREIMMUNE_SUPER"));
            if !sup {
                self.fire15_ignite_unit(s, temp, 0, false, now);
            }
        }
    }
}
