//! Caverns, cave wildlife and forgotten beasts (round 5).
//!
//! # DF rules implemented (wiki: Cavern, Underground, Wildlife, Forgotten beast)
//! - **Cavern layers** are the large natural open regions underground. They are found by a 3D
//!   flood fill over subterranean, non-outside tiles that are open space or a floor with open
//!   space above (cavern ceilings are several levels high, dug rooms have a rock ceiling).
//!   Layers are numbered from the top (1 = first cavern); a region full of magma is the magma
//!   sea. A layer counts as *discovered* once one of its tiles is revealed, and as *breached*
//!   when it shares a walk component with the fort.
//! - **Cave wildlife**: discovered caverns get groups of cave creatures from the save's own
//!   creature raws (`BIOME_SUBTERRANEAN_CHASM` / `_WATER` for caverns, `_LAVA` for the magma
//!   sea; no vermin, no civilized races, no demons, no aquatic-only castes). Large predators
//!   (`LARGE_PREDATOR`) arrive hostile and attack what they see; the others wander.
//! - **Forgotten beasts** come out of the caverns once one is discovered: a procedurally made
//!   megabeast (quadruped or humanoid body, flesh or a shell of rock, gem or metal),
//!   DF's natural skills (6 in wrestling, biting, grasping/stance strikes, melee, dodging,
//!   situational awareness), a building destroyer that avoids traps (`[TRAPAVOID]`), that hunts
//!   the fort wherever it can walk. It never leaves. Announcement in DF's style:
//!   "The Forgotten Beast X has come! A great four-legged creature made of granite. ...".
//!
//! # Estimates (not on the wiki)
//! - Cavern detection: regions of at least 1 500 such tiles; regions overlapping in z are
//!   one layer; a layer with a region over 10 % magma is the magma sea.
//! - Wildlife: checked at each season start; a discovered layer below 25 living wild
//!   creatures gets a group with chance 2/3; group size 1-2 for predators, 2-5 otherwise.
//! - Forgotten beasts: checked at each season start once a cavern is discovered, chance 1/4,
//!   one at a time (DF: one per underground region, attracted by wealth and noise).
//!   Body size 1-5 million cm³, strength/toughness 1 500-2 500. A beast "made of" rock, gem or
//!   metal gets that material on the outermost tissue layer of every body part (no bleeding or
//!   pain there); the inner tissues stay flesh. With the material on all tissues the sim's wound
//!   model could not kill it at all (no bleeding, head too big to sever), and 77 soldiers
//!   fought it until they died of thirst.

use crate::rng::Rng;
use crate::{AnnouncementKind, Kind, Sim};
use rf_core::{tile_flags, Shape, Skill, Unit, UnitFlags, NO_MAT};

/// Minimum size of a cavern region in tiles (estimate).
const MIN_CAVERN: usize = 1_500;
/// Living wild cave creatures per layer above which no more arrive (estimate).
const WILD_CAP: usize = 25;
/// Spawn candidates kept per layer.
const MAX_SPAWN_TILES: usize = 4_096;
/// Marker civ id of forgotten beasts (they hunt the fort like invaders do).
pub(crate) const BEAST_CIV: i32 = 999_001;

/// One detected cavern layer.
#[derive(Clone, Debug, Default)]
pub struct CavernLayer {
    /// 1 = topmost cavern; the magma sea has `magma` set.
    pub id: u8,
    pub z_min: i16,
    pub z_max: i16,
    /// Open tiles of the region.
    pub tiles: usize,
    /// Tiles not hidden any more.
    pub revealed: usize,
    pub magma: bool,
    /// Shares a walk component with the fort's citizens.
    pub breached: bool,
    /// Walkable floor tiles (sample) where creatures can arrive.
    pub(crate) spawn: Vec<u32>,
}

impl CavernLayer {
    pub fn discovered(&self) -> bool {
        self.revealed > 0
    }
    pub fn label(&self) -> String {
        if self.magma {
            "magma sea".into()
        } else {
            format!("cavern {}", self.id)
        }
    }
}

/// A forgotten beast that came to the fort.
#[derive(Clone, Debug, Default)]
pub struct BeastRecord {
    pub unit_id: i32,
    pub name: String,
    pub description: String,
    pub material: String,
    pub body_size: i32,
    pub arrived: u64,
    pub layer: String,
    pub killed: Option<u64>,
}

/// Report for tests, benches and the UI.
#[derive(Clone, Debug, Default)]
pub struct CavernReport {
    pub layers: Vec<CavernLayer>,
    /// (tick, layer label, creature name, count, hostile).
    pub wildlife: Vec<(u64, String, String, usize, bool)>,
    pub beasts: Vec<BeastRecord>,
}

#[derive(Default)]
pub(crate) struct Caverns {
    pub layers: Vec<CavernLayer>,
    detected: bool,
    last_season: i32,
    pub wildlife: Vec<(u64, String, String, usize, bool)>,
    /// (slot, layer index) of wild cave creatures we spawned.
    wild: Vec<(u32, u8)>,
    pub beasts: Vec<BeastRecord>,
    /// Slots of living forgotten beasts (trap avoiders).
    pub beast_slots: Vec<u32>,
    pub off: bool,
    beast_n: u32,
}

impl Sim {
    /// [moods] Cavern pass: detection on the first tick, season starts for wildlife and beasts.
    pub(crate) fn caverns_tick(&mut self, now: u64) {
        if self.caverns.off {
            return;
        }
        if !self.caverns.detected {
            self.caverns.detected = true;
            self.detect_caverns();
            self.caverns.last_season = self.world.calendar.month() / 3;
            return;
        }
        if now % rf_core::TICKS_PER_DAY as u64 != 611 {
            return;
        }
        self.beasts_daily(now);
        let season = self.world.calendar.month() / 3;
        if season == self.caverns.last_season {
            return;
        }
        self.caverns.last_season = season;
        self.detect_caverns();
        self.cave_wildlife(now);
        self.forgotten_beast_check(now);
    }

    /// Find cavern layers (3D flood fill over natural open subterranean tiles).
    pub(crate) fn detect_caverns(&mut self) {
        let (dx, dy, dz) = (self.world.dims.x as usize, self.world.dims.y as usize, self.world.dims.z as usize);
        let layer = dx * dy;
        let n = self.world.tiles.len();
        if n == 0 {
            return;
        }
        let tt = &self.world.tiletypes;
        let tiles = &self.world.tiles;
        let sub_open = |i: usize| -> bool {
            let t = &tiles[i];
            if t.flags & tile_flags::SUBTERRANEAN == 0 || t.flags & tile_flags::OUTSIDE != 0 {
                return false;
            }
            let s = tt.get(t.tt).shape;
            if s == Shape::Empty {
                return true;
            }
            if !s.has_floor() || i + layer >= n {
                return false;
            }
            let a = &tiles[i + layer];
            a.flags & tile_flags::SUBTERRANEAN != 0 && tt.get(a.tt).shape == Shape::Empty
        };
        let mut seen = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        let mut found: Vec<CavernLayer> = Vec::new();
        for start in 0..n {
            if seen[start] || !sub_open(start) {
                continue;
            }
            seen[start] = true;
            stack.push(start);
            let (mut size, mut revealed, mut magma, mut zmin, mut zmax) = (0usize, 0usize, 0usize, i16::MAX, i16::MIN);
            let mut floors: Vec<u32> = Vec::new();
            while let Some(i) = stack.pop() {
                size += 1;
                let t = &tiles[i];
                if t.flags & tile_flags::HIDDEN == 0 {
                    revealed += 1;
                }
                if t.is_magma() && t.liquid_depth() > 0 {
                    magma += 1;
                }
                let z = (i / layer) as i16;
                zmin = zmin.min(z);
                zmax = zmax.max(z);
                if self.nav.walkable(i) && t.liquid_depth() < 4 {
                    floors.push(i as u32);
                }
                let x = i % dx;
                let y = (i / dx) % dy;
                let zz = i / layer;
                let mut push = |j: usize, stack: &mut Vec<usize>| {
                    if !seen[j] && sub_open(j) {
                        seen[j] = true;
                        stack.push(j);
                    }
                };
                if x > 0 {
                    push(i - 1, &mut stack);
                }
                if x + 1 < dx {
                    push(i + 1, &mut stack);
                }
                if y > 0 {
                    push(i - dx, &mut stack);
                }
                if y + 1 < dy {
                    push(i + dx, &mut stack);
                }
                if zz > 0 {
                    push(i - layer, &mut stack);
                }
                if zz + 1 < dz {
                    push(i + layer, &mut stack);
                }
            }
            if size < MIN_CAVERN {
                continue;
            }
            floors.sort_unstable();
            let spawn = if floors.len() > MAX_SPAWN_TILES {
                let step = floors.len() as f64 / MAX_SPAWN_TILES as f64;
                (0..MAX_SPAWN_TILES).map(|k| floors[(k as f64 * step) as usize]).collect()
            } else {
                floors
            };
            found.push(CavernLayer { id: 0, z_min: zmin, z_max: zmax, tiles: size, revealed, magma: magma * 10 > size, breached: false, spawn });
        }
        found.sort_by_key(|l| (std::cmp::Reverse(l.z_max), l.z_min));
        // Regions whose z ranges overlap or touch are one layer (a cavern is split by pillars
        // and fungus forests into several flood-fill regions); open space next to the magma sea
        // belongs to it.
        let mut merged: Vec<CavernLayer> = Vec::new();
        for l in found.drain(..) {
            match merged.iter_mut().find(|m| l.z_max + 1 >= m.z_min && l.z_min <= m.z_max + 1) {
                Some(m) => {
                    m.z_min = m.z_min.min(l.z_min);
                    m.z_max = m.z_max.max(l.z_max);
                    m.tiles += l.tiles;
                    m.revealed += l.revealed;
                    m.magma |= l.magma;
                    m.spawn.extend(l.spawn);
                }
                None => merged.push(l),
            }
        }
        for m in merged.iter_mut() {
            m.spawn.sort_unstable();
            if m.spawn.len() > MAX_SPAWN_TILES {
                let step = m.spawn.len() as f64 / MAX_SPAWN_TILES as f64;
                m.spawn = (0..MAX_SPAWN_TILES).map(|k| m.spawn[(k as f64 * step) as usize]).collect();
            }
        }
        let mut found = merged;
        let mut k = 0u8;
        for l in found.iter_mut() {
            if !l.magma {
                k += 1;
                l.id = k;
            } else {
                l.id = 0;
            }
        }
        // Breached: a spawn tile in the citizens' walk component.
        let fort_c: Vec<u32> = {
            let mut v: Vec<u32> = self.ai.iter().filter(|a| a.kind == Kind::Worker).map(|a| self.nav.comp_near(a.pos as usize)).filter(|&c| c != 0).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        for l in found.iter_mut() {
            l.breached = l.spawn.iter().any(|&t| fort_c.binary_search(&self.nav.comp_root(t as usize)).is_ok());
        }
        self.caverns.layers = found;
    }

    fn cave_species(&self, magma: bool) -> Vec<(String, String, bool, i32, i32)> {
        // (token, caste, predator, size, walk speed)
        let mut out = Vec::new();
        for c in &self.world.creature_raws {
            let f = |t: &str| c.flags.iter().any(|x| x == t);
            let biome = if magma { f("BIOME_SUBTERRANEAN_LAVA") } else { f("BIOME_SUBTERRANEAN_CHASM") || f("BIOME_SUBTERRANEAN_WATER") };
            if !biome || !f("LARGE_ROAMING") || f("OCCURS_AS_ENTITY_RACE") || f("HAS_ANY_DEMON") || f("GENERATED") || f("SMALL_RACE") {
                continue;
            }
            if c.flags.iter().any(|x| x.starts_with("VERMIN")) {
                continue;
            }
            let Some(k) = c.castes.first() else { continue };
            let kf = |t: &str| k.flags.iter().any(|x| x == t);
            if kf("AQUATIC") || kf("DEMON") || kf("MEGABEAST") || kf("FEATURE_BEAST") {
                continue;
            }
            out.push((c.token.clone(), k.token.clone(), kf("LARGE_PREDATOR"), if k.body_size > 0 { k.body_size } else { 50_000 }, if k.walk_speed > 0 { k.walk_speed } else { 900 }));
        }
        if out.is_empty() && self.world.creature_raws.is_empty() {
            // Fallback without raws (synthetic worlds): vanilla first-cavern creatures.
            let l: &[(&str, bool, i32)] = if magma {
                &[("MAGMA_CRAB", false, 20_000)]
            } else {
                &[("CRUNDLE", false, 15_000), ("ELK_BIRD", false, 50_000), ("JABBERER", true, 150_000), ("VORACIOUS_CAVE_CRAWLER", true, 70_000)]
            };
            out = l.iter().map(|(t, p, sz)| (t.to_string(), "FEMALE".to_string(), *p, *sz, 900)).collect();
        }
        out
    }

    /// Season start: groups of cave creatures arrive in discovered caverns.
    pub(crate) fn cave_wildlife(&mut self, now: u64) {
        let mut r = Rng::for_unit(self.seed, 7, now, 0xCA7E);
        // Count living wild creatures per layer.
        self.caverns.wild.retain(|&(s, _)| (s as usize) < self.ai.len() && self.ai[s as usize].kind != Kind::Dead);
        for li in 0..self.caverns.layers.len() {
            let (disc, magma, label, empty) = {
                let l = &self.caverns.layers[li];
                (l.discovered(), l.magma, l.label(), l.spawn.is_empty())
            };
            if !disc || empty {
                continue;
            }
            let here = self.caverns.wild.iter().filter(|w| w.1 as usize == li).count();
            // Creatures already living there (e.g. the export's magma crabs) count too.
            let native = self.native_wild_count(li);
            if here + native >= WILD_CAP || !r.chance(2, 3) {
                continue;
            }
            let sp = self.cave_species(magma);
            if sp.is_empty() {
                continue;
            }
            let (tok, caste, pred, size, walk) = sp[r.below(sp.len() as u32) as usize].clone();
            let n = if pred { 1 + r.below(2) } else { 2 + r.below(4) } as usize;
            let tiles = self.caverns.layers[li].spawn.clone();
            let start = tiles[r.below(tiles.len() as u32) as usize];
            let mut near: Vec<u32> = tiles.iter().copied().filter(|&t| self.world.pos_of(t as usize).dist(self.world.pos_of(start as usize)) <= 6).collect();
            if near.is_empty() {
                near.push(start);
            }
            let mut ids = Vec::new();
            let mut next_id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
            for k in 0..n {
                let tile = near[k % near.len()];
                let unit = Unit {
                    id: next_id,
                    race: tok.clone(),
                    caste: caste.clone(),
                    profession: "STANDARD".into(),
                    pos: self.world.pos_of(tile as usize),
                    flags: UnitFlags { animal: true, hostile: pred, ..Default::default() },
                    civ_id: -1,
                    hist_fig_id: -1,
                    sex: r.below(2) as i8,
                    age: 2.0 + r.below(8) as f32,
                    squad_id: -1,
                    squad_position: -1,
                    body_size: size,
                    walk_speed: walk,
                    invader_civ: -1,
                    ..Default::default()
                };
                self.world.units.push(unit);
                ids.push(next_id);
                next_id += 1;
            }
            self.init_units();
            self.combat_sync_units();
            for id in &ids {
                if let Some(&s) = self.unit_slot.get(id) {
                    self.caverns.wild.push((s as u32, li as u8));
                }
            }
            let name = self.world.creature_raws.iter().find(|c| c.token == tok).map(|c| if n > 1 { c.name_plural.clone() } else { c.name.clone() }).filter(|s| !s.is_empty()).unwrap_or_else(|| tok.to_ascii_lowercase().replace('_', " "));
            self.caverns.wildlife.push((now, label, name, n, pred));
        }
    }

    fn native_wild_count(&self, li: usize) -> usize {
        let l = &self.caverns.layers[li];
        self.ai
            .iter()
            .filter(|a| a.kind != Kind::Dead && a.kind != Kind::Worker)
            .filter(|a| {
                let u = &self.world.units[a.uidx];
                !u.flags.tame && !u.flags.caged && u.flags.animal && u.pos.z >= l.z_min && u.pos.z <= l.z_max && self.world.tiles[a.pos as usize].flags & tile_flags::SUBTERRANEAN != 0
            })
            .count()
    }

    /// Season start: a forgotten beast may come out of a discovered cavern.
    pub(crate) fn forgotten_beast_check(&mut self, now: u64) {
        if !self.combat.inv.enabled {
            return;
        }
        self.caverns.beast_slots.retain(|&s| (s as usize) < self.ai.len() && self.ai[s as usize].kind != Kind::Dead);
        if !self.caverns.beast_slots.is_empty() {
            return;
        }
        if !self.caverns.layers.iter().any(|l| l.discovered() && !l.magma && !l.spawn.is_empty()) {
            return;
        }
        let mut r = Rng::for_unit(self.seed, 8, now, 0xFB);
        if r.chance(1, 4) {
            self.spawn_forgotten_beast();
        }
    }

    /// Debug/UI and tests: a forgotten beast arrives now in a discovered cavern (any cavern if
    /// none is discovered). Returns its unit id.
    pub fn spawn_forgotten_beast(&mut self) -> Option<i32> {
        let now = self.ticks;
        if !self.caverns.detected {
            self.caverns.detected = true;
            self.detect_caverns();
        }
        let mut r = Rng::for_unit(self.seed, 9, now, 0xFB0 + self.caverns.beast_n as u64);
        self.caverns.beast_n += 1;
        // Prefer a breached, discovered cavern, then any discovered, then any.
        let pick = |want_breach: bool, want_disc: bool, l: &CavernLayer| !l.magma && !l.spawn.is_empty() && (!want_breach || l.breached) && (!want_disc || l.discovered());
        let li = (0..self.caverns.layers.len())
            .find(|&i| pick(true, true, &self.caverns.layers[i]))
            .or_else(|| (0..self.caverns.layers.len()).find(|&i| pick(false, true, &self.caverns.layers[i])))
            .or_else(|| (0..self.caverns.layers.len()).find(|&i| pick(false, false, &self.caverns.layers[i])));
        let (tile, label) = match li {
            Some(li) => {
                let l = &self.caverns.layers[li];
                (l.spawn[r.below(l.spawn.len() as u32) as usize], l.label())
            }
            None => {
                // No cavern (synthetic worlds): the deepest walkable tile connected to the fort.
                let c = self.ai.iter().find(|a| a.kind == Kind::Worker).map(|a| self.nav.comp_near(a.pos as usize)).unwrap_or(0);
                let t = (0..self.world.tiles.len()).find(|&i| self.nav.walkable(i) && (c == 0 || self.nav.comp_root(i) == c))?;
                (t as u32, "the depths".to_string())
            }
        };
        let name = beast_name(&mut r);
        let quad = r.chance(2, 3);
        let size = 1_000_000 + r.below(4_000_001) as i32;
        // Material: flesh (40 %), else a rock, gem or metal of the save's palette.
        let solid: Vec<u16> = self
            .world
            .materials
            .iter()
            .enumerate()
            .filter(|(_, m)| (m.is_stone || m.is_gem || m.is_metal) && !m.is_ore && !m.name.is_empty() && m.token.starts_with("INORGANIC:"))
            .map(|(i, _)| i as u16)
            .collect();
        let mat = if !solid.is_empty() && r.chance(3, 5) { solid[r.below(solid.len() as u32) as usize] } else { NO_MAT };
        let mat_name = if mat == NO_MAT { String::new() } else { self.world.materials[mat as usize].name.clone() };
        let race = format!("FORGOTTEN_BEAST_{}", self.caverns.beast_n);
        let attr = |r: &mut Rng| 1500 + r.below(1001) as i32;
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let six = |s: &str| Skill { id: s.into(), rating: 6, xp: 0 };
        let unit = Unit {
            id,
            race: race.clone(),
            caste: "NEUTER".into(),
            name: format!("{name}, the Forgotten Beast"),
            profession: "STANDARD".into(),
            pos: self.world.pos_of(tile as usize),
            flags: UnitFlags { hostile: true, animal: quad, ..Default::default() },
            civ_id: -1,
            hist_fig_id: -1,
            sex: -1,
            age: 500.0,
            skills: ["WRESTLING", "BITE", "GRASP_STRIKE", "STANCE_STRIKE", "MELEE_COMBAT", "DODGING", "SITUATIONAL_AWARENESS"].iter().map(|s| six(s)).collect(),
            phys_attrs: vec![("STRENGTH".into(), attr(&mut r)), ("AGILITY".into(), 800 + r.below(500) as i32), ("TOUGHNESS".into(), attr(&mut r))],
            ment_attrs: vec![("WILLPOWER".into(), attr(&mut r))],
            squad_id: -1,
            squad_position: -1,
            body_size: size,
            walk_speed: 900 + r.below(600) as i32,
            invader_civ: BEAST_CIV,
            ..Default::default()
        };
        self.world.units.push(unit);
        self.init_units();
        self.combat_sync_units();
        let s = *self.unit_slot.get(&id)?;
        // Its body: synthetic quadruped/humanoid, tissues of its material.
        self.ensure_body(s);
        if mat != NO_MAT {
            let mech = self.mech_of_mat(mat);
            let b = self.combat.f[s].body as usize;
            if let Some(body) = self.combat.bodies.get_mut(b) {
                // The outermost tissue of every part is the beast's material (see the estimates).
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
        self.caverns.beast_slots.push(s as u32);
        let f = &mut self.combat.f[s];
        f.role = crate::combat::Role::Hostile;
        f.home = tile;
        if let Err(p) = self.combat.hostiles.binary_search(&(s as u32)) {
            self.combat.hostiles.insert(p, s as u32);
        }
        self.rebuild_fort_list();
        self.engage_now(s, now);
        let adj = ["great", "huge", "towering", "massive", "gigantic"][r.below(5) as usize];
        let shape = if quad {
            ["four-legged creature", "rodent", "lizard", "beetle", "bat", "toad"][r.below(6) as usize]
        } else {
            ["humanoid", "ogre-like thing", "hulking monster"][r.below(3) as usize]
        };
        let feature = ["It has a long tail", "It has large horns", "It has a pair of wings", "It has a stout tail", "It has a long, twisted beak", "It has a gaunt appearance"][r.below(6) as usize];
        let made = if mat_name.is_empty() { String::new() } else { format!(" made of {mat_name}") };
        let desc = format!("A {adj} {shape}{made}. {feature}.");
        let text = format!("The Forgotten Beast {name} has come! {desc}");
        let p = self.world.pos_of(tile as usize);
        self.announce(now, text.clone(), Some(p), AnnouncementKind::Warning);
        self.combat.log_line(text);
        self.caverns.beasts.push(BeastRecord {
            unit_id: id,
            name: name.clone(),
            description: desc,
            material: if mat_name.is_empty() { "flesh".into() } else { mat_name },
            body_size: size,
            arrived: now,
            layer: label,
            killed: None,
        });
        Some(id)
    }

    fn beasts_daily(&mut self, now: u64) {
        for k in 0..self.caverns.beasts.len() {
            if self.caverns.beasts[k].killed.is_some() {
                continue;
            }
            let id = self.caverns.beasts[k].unit_id;
            let dead = self.unit_slot.get(&id).map(|&s| self.ai[s].kind == Kind::Dead).unwrap_or(true);
            if dead {
                self.caverns.beasts[k].killed = Some(now);
                let name = self.caverns.beasts[k].name.clone();
                self.announce(now, format!("The Forgotten Beast {name} has been slain."), None, AnnouncementKind::Info);
            }
        }
        self.caverns.beast_slots.retain(|&s| (s as usize) < self.ai.len() && self.ai[s as usize].kind != Kind::Dead);
    }

    /// [moods] Forgotten beasts avoid traps (DF `[TRAPAVOID]`).
    #[inline]
    pub(crate) fn caverns_trapavoid(&self, s: usize) -> bool {
        !self.caverns.beast_slots.is_empty() && self.caverns.beast_slots.contains(&(s as u32))
    }

    pub(crate) fn caverns_hash(&self, mix: &mut dyn FnMut(u64)) {
        mix(self.caverns.layers.len() as u64);
        mix(self.caverns.wildlife.len() as u64);
        mix(self.caverns.beasts.len() as u64);
    }

    // ------------------------------------------------------------------ public API

    /// Cavern layers, cave wildlife arrivals and forgotten beasts so far.
    pub fn cavern_report(&self) -> CavernReport {
        CavernReport { layers: self.caverns.layers.clone(), wildlife: self.caverns.wildlife.clone(), beasts: self.caverns.beasts.clone() }
    }

    /// Switch cave wildlife and forgotten beasts on or off (default on; beasts also need
    /// invasions to be enabled, like DF's monster setting).
    pub fn set_caverns_enabled(&mut self, on: bool) {
        self.caverns.off = !on;
    }

    /// Re-run cavern detection now (e.g. after editing tiles).
    pub fn redetect_caverns(&mut self) {
        self.caverns.detected = true;
        self.detect_caverns();
    }
}

/// A DF-like generated name of two words.
fn beast_name(r: &mut Rng) -> String {
    const A: &[&str] = &["ur", "ak", "zan", "ost", "ith", "ul", "nek", "dab", "vath", "kog", "esh", "mor", "thur", "zol", "ngi", "bes", "lor", "rak", "sod", "ubal"];
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
    let n2 = 2 + (a.len() as u32 % 2);
    let b = word(n2);
    format!("{a} {b}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Pos, Tile, World};

    /// A synthetic world with a big hollow cavern below the fort level.
    fn cavern_world() -> World {
        let mut w = World::synthetic(48, 48, 20);
        let open = w.tiletypes.by_name("OpenSpace").unwrap();
        let floor = w.tiletypes.by_name("StoneFloor1").unwrap();
        // Cavern: z 2..=5 open, floor at z 1 (x,y in 4..44), hidden.
        for z in 1..=5i16 {
            for y in 4..44i16 {
                for x in 4..44i16 {
                    let i = w.idx_unchecked(Pos::new(x, y, z));
                    let tt = if z == 1 { floor } else { open };
                    w.tiles[i] = Tile { tt, mat: 0, flags: tile_flags::SUBTERRANEAN | tile_flags::HIDDEN, ..Default::default() };
                }
            }
        }
        w
    }

    #[test]
    fn detects_cavern_layer_and_discovery() {
        let w = cavern_world();
        let mut sim = Sim::new(w, 1);
        sim.redetect_caverns();
        let rep = sim.cavern_report();
        assert_eq!(rep.layers.len(), 1, "{:?}", rep.layers.iter().map(|l| (l.z_min, l.z_max, l.tiles)).collect::<Vec<_>>());
        let l = &rep.layers[0];
        assert_eq!((l.id, l.z_min, l.z_max, l.magma), (1, 1, 5, false));
        assert!(!l.discovered());
        assert!(!l.breached);
        // Reveal a tile: discovered.
        let i = sim.world.idx_unchecked(Pos::new(10, 10, 3));
        sim.world.tiles[i].flags &= !tile_flags::HIDDEN;
        sim.redetect_caverns();
        assert!(sim.cavern_report().layers[0].discovered());
    }

    #[test]
    fn dug_hall_is_not_a_cavern() {
        let sim = {
            let mut s = Sim::new(World::synthetic(48, 48, 20), 1);
            s.redetect_caverns();
            s
        };
        assert!(sim.cavern_report().layers.is_empty());
    }

    #[test]
    fn wildlife_arrives_in_discovered_caverns() {
        let mut w = cavern_world();
        for t in w.tiles.iter_mut() {
            t.flags &= !tile_flags::HIDDEN;
        }
        let mut sim = Sim::new(w, 3);
        sim.redetect_caverns();
        let before = sim.world.units.len();
        for k in 0..12 {
            sim.cave_wildlife(1000 + k * 33_600);
        }
        let rep = sim.cavern_report();
        assert!(!rep.wildlife.is_empty(), "no cave creatures arrived");
        assert!(sim.world.units.len() > before);
        let l = &rep.layers[0];
        for u in &sim.world.units[before..] {
            assert!(u.pos.z >= l.z_min && u.pos.z <= l.z_max, "{} at {:?}", u.race, u.pos);
            assert!(sim.is_walkable(u.pos));
        }
        // The cap holds.
        let alive = sim.world.units[before..].iter().filter(|u| !u.flags.dead).count();
        assert!(alive <= WILD_CAP + 5, "{alive}");
    }

    #[test]
    fn forgotten_beast_arrives_and_hunts() {
        let mut w = cavern_world();
        for t in w.tiles.iter_mut() {
            t.flags &= !tile_flags::HIDDEN;
        }
        let mut sim = Sim::new(w, 5);
        let id = sim.spawn_forgotten_beast().expect("beast");
        let ann = sim.drain_announcements();
        assert!(ann.iter().any(|a| a.text.starts_with("The Forgotten Beast ") && a.text.contains(" has come! A ")), "{:?}", ann.iter().map(|a| &a.text).collect::<Vec<_>>());
        let u = sim.world.unit(id).unwrap();
        assert!(u.flags.hostile && u.body_size >= 1_000_000);
        assert_eq!(u.skill("BITE"), 6);
        assert!(sim.hostiles().contains(&id));
        let s = sim.unit_slot[&id];
        assert!(sim.caverns_trapavoid(s));
        let rep = sim.cavern_report();
        assert_eq!(rep.beasts.len(), 1);
        assert_eq!(rep.beasts[0].layer, "cavern 1");
    }
}
