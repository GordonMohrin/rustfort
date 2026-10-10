//! [military2] Barracks, sparring, patrol routes, per-position uniforms and squad ammunition
//! (round 8).
//!
//! # DF rules implemented (wiki v50: Barracks, Sparring, Military, Patrol, Squad, Uniform,
//! Ammunition)
//! - **Barracks** are civzones assigned to squads (DF `squad.rooms`, exported as `barracks`).
//!   Training soldiers walk into their barracks and train there; on-duty soldiers sleep in the
//!   barracks' beds; uniform pieces stored in the barracks are taken first.
//! - **Sparring**: two soldiers training side by side in a barracks spar. Sparring teaches faster
//!   than individual drills and injuries are rare. **Demonstrations**: a much better squadmate
//!   nearby teaches the others.
//! - **Patrol routes**: a `Patrol` order walks the squad along the route's waypoints in a loop;
//!   patrolling soldiers engage hostiles they see like any soldier on duty.
//! - **Uniforms** per position (the DF uniform screen assigns per position) and an equipment
//!   report per squad; **ammunition** per squad: which ammo (type, material or class), how much,
//!   and whether it is for combat, training or both.
//!
//! # Estimates
//! - Training spots: the barracks' walkable tiles in row order, position p at tile `(p/2)*3`,
//!   the odd partner next to it. An injury (a bruise) in 1 of 2000 bouts. [skills9] Sparring
//!   needs both partners at Competent Fighting; a demonstration needs a squadmate within 3 tiles
//!   2+ levels better in the demonstrated skill (rotating weapon, Fighting, Discipline,
//!   Dodging). The experience amounts live in `skills.rs`.
//! - Soldiers on duty sleep in barracks beds whenever their squad has a barracks with a free bed
//!   (DF's per-routine sleep setting is not exported). Patrol waypoints: wait 50 ticks at each.
//! - Ammo amount: each soldier keeps up to `amount / members` rounds (at most a quiver of 50).

use crate::combat::Role;
use crate::military::Duty;
use crate::rng::Rng;
use crate::{Kind, Sim, NONE};
use rf_core::{Item, Pos, SquadAmmo};

/// Equipment of one squad position (DF squad equipment screen).
#[derive(Clone, Debug, Default)]
pub struct SquadEquipment {
    pub position: usize,
    /// Unit in the position, -1 = empty.
    pub unit: i32,
    /// Uniform entries for this position (`KIND[:SUBTYPE][@material]`).
    pub uniform: Vec<String>,
    /// Items worn or carried: (item id, description).
    pub worn: Vec<(i32, String)>,
    /// Uniform entries not worn yet.
    pub missing: Vec<String>,
    /// Rounds of ammunition carried.
    pub ammo: i32,
}

/// Barracks of one squad: walkable tiles and bed indices.
#[derive(Clone, Debug, Default)]
pub(crate) struct BarracksT {
    pub squad: i32,
    pub tiles: Vec<u32>,
    /// The same tiles, sorted (membership tests).
    pub sorted: Vec<u32>,
    pub beds: Vec<usize>,
}

fn entry_matches(entry: &str, kind: &str, subtype: &str) -> bool {
    let (k, sub, _) = uniform_parts(entry);
    !k.is_empty() && k == kind && (sub.is_empty() || sub == subtype)
}

/// [gaps10] A uniform entry `KIND[:SUBTYPE][@material]` split into (kind, subtype, material).
pub(crate) fn uniform_parts(entry: &str) -> (&str, &str, &str) {
    let (head, mat) = match entry.split_once('@') {
        Some((h, m)) => (h, m.trim()),
        None => (entry, ""),
    };
    let (k, sub) = head.split_once(':').unwrap_or((head, ""));
    (k.trim(), sub.trim(), mat)
}

/// [gaps10] Does a material fit the material part of a uniform entry? `spec` is a DF token
/// (`INORGANIC:STEEL`), a plain metal name (`steel`, `iron`, `bronze`, `copper`) or a DF entity
/// material class (`Armor`, `Metal`, `Leather`, `Wood`, `Bone`, `Shell`, `Cloth`, ...). An empty
/// spec fits everything; an unknown class fits everything (no filter rather than no uniform).
pub(crate) fn uniform_mat_fits(spec: &str, m: Option<&rf_core::MaterialInfo>) -> bool {
    if spec.is_empty() {
        return true;
    }
    let Some(m) = m else { return false };
    if spec.contains(':') {
        return m.token.eq_ignore_ascii_case(spec);
    }
    let up = spec.to_ascii_uppercase();
    let ends = |suffix: &str| m.token.ends_with(suffix);
    match up.as_str() {
        // DF entity material categories for metal gear (`Armor` = the civ's armor metals).
        "METAL" | "ARMOR" | "WEAPONMELEE" | "WEAPONRANGED" | "AMMO" | "AMMO2" | "PICK" | "ANVIL" => m.is_metal,
        "LEATHER" => ends(":LEATHER"),
        "BONE" => ends(":BONE"),
        "SHELL" => ends(":SHELL"),
        "HORN" => ends(":HORN"),
        "IVORY" | "TOOTH" => ends(":TOOTH") || ends(":IVORY"),
        "PEARL" => ends(":PEARL"),
        "WOOD" | "WOOD2" => m.is_wood,
        "STONE" => m.is_stone,
        "GEM" => m.is_gem,
        "GLASS" => m.is_glass,
        "SILK" => ends(":SILK"),
        "WOOL" | "YARN" => ends(":HAIR") || ends(":WOOL") || ends(":YARN"),
        "CLOTH" | "CLOTHING" | "PLANTFIBER" | "THREAD" => ends(":THREAD") || ends(":SILK") || ends(":HAIR") || ends(":WOOL") || ends(":YARN"),
        name => {
            // A plain material name: `steel` = INORGANIC:STEEL, or the material's own name. Other
            // unknown words (classes this table does not know) do not filter.
            const NAMED: &[&str] = &["STEEL", "IRON", "PIG_IRON", "BRONZE", "BISMUTH_BRONZE", "COPPER", "SILVER", "GOLD", "ADAMANTINE", "BRASS", "TIN"];
            m.token.strip_prefix("INORGANIC:") == Some(name) || m.name.eq_ignore_ascii_case(spec) || !NAMED.contains(&name)
        }
    }
}

/// [gaps10] "Best item available (e.g. steel rather than iron rather than copper)" (wiki Uniform)
/// when the uniform names no material: higher is better.
pub(crate) fn uniform_mat_rank(m: Option<&rf_core::MaterialInfo>) -> i32 {
    let Some(m) = m else { return 0 };
    match m.token.as_str() {
        "INORGANIC:ADAMANTINE" => 100,
        "INORGANIC:STEEL" => 60,
        "INORGANIC:IRON" => 50,
        "INORGANIC:BRONZE" | "INORGANIC:BISMUTH_BRONZE" => 40,
        "INORGANIC:COPPER" => 30,
        "INORGANIC:SILVER" => 25,
        _ if m.is_metal => 20,
        _ => m.value.clamp(0, 15),
    }
}

impl Sim {
    // ------------------------------------------------------------------ public API

    /// Set the uniform of one squad position (entries `KIND[:SUBTYPE][@material]`, e.g.
    /// `WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:STEEL`, `ARMOR@Armor`). An empty list makes the
    /// position use the squad's uniform again.
    pub fn set_squad_uniform(&mut self, squad_id: i32, position: usize, items: &[&str]) {
        if let Some(q) = self.world.squads.iter_mut().find(|q| q.id == squad_id) {
            if q.position_uniforms.len() <= position {
                q.position_uniforms.resize(position + 1, Vec::new());
            }
            q.position_uniforms[position] = items.iter().map(|s| s.to_string()).collect();
        }
        for f in &mut self.combat.f {
            if f.squad == squad_id {
                f.equip_at = 0;
            }
        }
    }

    /// [gaps10] DF uniform option "Exact matches only" (true) or "Partial matches okay" (false, DF's
    /// default): with exact matches a soldier goes without a piece rather than wear a substitute of
    /// another material. (Session setting: not part of the save yet.)
    pub fn set_squad_uniform_exact(&mut self, squad_id: i32, exact: bool) {
        let v = &mut self.combat.mil.exact;
        match (v.binary_search(&squad_id), exact) {
            (Err(p), true) => v.insert(p, squad_id),
            (Ok(p), false) => {
                v.remove(p);
            }
            _ => {}
        }
        for f in &mut self.combat.f {
            if f.squad == squad_id {
                f.equip_at = 0;
            }
        }
    }

    /// [gaps10] Is "Exact matches only" set for this squad?
    pub fn squad_uniform_exact(&self, squad_id: i32) -> bool {
        self.combat.mil.exact.binary_search(&squad_id).is_ok()
    }

    /// Equipment report of a squad: per position the uniform, what is worn, what is missing.
    pub fn squad_equipment(&self, squad_id: i32) -> Vec<SquadEquipment> {
        let Some(q) = self.world.squads.iter().find(|q| q.id == squad_id) else { return Vec::new() };
        let mut out = Vec::new();
        for (pos, &uid) in q.members.iter().enumerate() {
            let uniform = self.uniform_for(q, pos);
            let mut row = SquadEquipment { position: pos, unit: uid, uniform: uniform.clone(), ..Default::default() };
            if let Some(u) = (uid >= 0).then(|| self.world.unit(uid)).flatten() {
                let mut have: Vec<(String, String, u16)> = Vec::new();
                for &id in &u.inventory {
                    let Some(&i) = self.item_idx.get(&id) else { continue };
                    let it = &self.world.items[i as usize];
                    if self.imeta[i as usize].dead {
                        continue;
                    }
                    row.worn.push((id, it.desc.clone()));
                    have.push((it.kind.clone(), it.subtype.clone(), it.mat));
                    if it.kind == "AMMO" {
                        row.ammo += it.stack.max(0);
                    }
                    if it.kind == "QUIVER" {
                        for cid in self.contents.get(&id).into_iter().flatten() {
                            if let Some(&ci) = self.item_idx.get(cid) {
                                let c = &self.world.items[ci as usize];
                                if c.kind == "AMMO" && !self.imeta[ci as usize].dead {
                                    row.ammo += c.stack.max(0);
                                }
                            }
                        }
                    }
                }
                // [gaps10] the material counts too, unless partial matches are okay (DF default)
                let exact = self.squad_uniform_exact(squad_id);
                row.missing = uniform
                    .iter()
                    .filter(|e| {
                        let mat = uniform_parts(e).2;
                        !have.iter().any(|(k, st, m)| entry_matches(e, k, st) && (!exact || uniform_mat_fits(mat, self.world.material(*m))))
                    })
                    .cloned()
                    .collect();
            } else {
                row.missing = uniform;
            }
            out.push(row);
        }
        out
    }

    /// Squad ammunition settings (DF squad ammo screen); soldiers fetch only matching ammo.
    pub fn set_squad_ammo(&mut self, squad_id: i32, ammo: Vec<SquadAmmo>) {
        if let Some(q) = self.world.squads.iter_mut().find(|q| q.id == squad_id) {
            q.ammo = ammo;
        }
    }

    /// Assign barracks civzones to a squad (DF barracks zone assignment).
    pub fn set_squad_barracks(&mut self, squad_id: i32, zones: &[i32]) {
        if let Some(q) = self.world.squads.iter_mut().find(|q| q.id == squad_id) {
            q.barracks = zones.to_vec();
        }
        self.refresh_military();
    }

    /// Give a squad a patrol route and order it to patrol (an empty route cancels the patrol).
    pub fn order_squad_patrol(&mut self, squad_id: i32, waypoints: &[Pos]) {
        if let Some(q) = self.world.squads.iter_mut().find(|q| q.id == squad_id) {
            q.patrol = waypoints.to_vec();
            q.orders = if waypoints.is_empty() { vec![] } else { vec!["Patrol".into()] };
        }
        for f in &mut self.combat.f {
            if f.squad == squad_id {
                f.patrol_i = 0;
            }
        }
        self.refresh_military();
    }

    // ------------------------------------------------------------------ internals

    /// Uniform entries of a squad position (position override, else the squad's uniform).
    pub(crate) fn uniform_for(&self, q: &rf_core::Squad, pos: usize) -> Vec<String> {
        match q.position_uniforms.get(pos) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => q.uniform.clone(),
        }
    }

    /// Rebuild the barracks tiles and beds of every squad (from `refresh_military`).
    pub(crate) fn barracks_refresh(&mut self) {
        let mut out = Vec::new();
        for q in &self.world.squads {
            if q.barracks.is_empty() {
                continue;
            }
            let mut tiles = Vec::new();
            let mut beds = Vec::new();
            for &bid in &q.barracks {
                let Some(b) = self.world.buildings.iter().find(|b| b.id == bid && b.kind == "Civzone" && b.subtype.contains("Barracks")) else { continue };
                for y in b.y1..=b.y2 {
                    for x in b.x1..=b.x2 {
                        let p = Pos::new(x, y, b.z);
                        if !b.contains(p) {
                            continue;
                        }
                        if let Some(i) = self.world.idx(p) {
                            if self.nav.walkable(i) {
                                tiles.push(i as u32);
                            }
                        }
                    }
                }
                for (k, bed) in self.beds.iter().enumerate() {
                    if b.contains(self.world.pos_of(bed.tile as usize)) {
                        beds.push(k);
                    }
                }
            }
            if !tiles.is_empty() {
                let mut sorted = tiles.clone();
                sorted.sort_unstable();
                sorted.dedup();
                out.push(BarracksT { squad: q.id, tiles, sorted, beds });
            }
        }
        self.combat.mil.barracks = out;
    }

    fn barracks_of(&self, squad: i32) -> Option<&BarracksT> {
        self.combat.mil.barracks.iter().find(|b| b.squad == squad)
    }

    /// Is tile `t` inside a barracks of `squad`?
    pub(crate) fn in_barracks(&self, squad: i32, t: u32) -> bool {
        self.barracks_of(squad).map(|b| b.sorted.binary_search(&t).is_ok()).unwrap_or(false)
    }

    /// Training soldier: walk to the training spot in the barracks. True while walking.
    pub(crate) fn barracks_walk(&mut self, s: usize, now: u64) -> bool {
        let sq = self.combat.f[s].squad;
        let Some(b) = self.barracks_of(sq) else {
            self.combat.f[s].spot = NONE;
            return false;
        };
        let pos = self.world.units[self.ai[s].uidx].squad_position.max(0) as usize;
        let n = b.tiles.len();
        let k = ((pos / 2) * 3 + pos % 2) % n;
        let spot = b.tiles[k];
        self.combat.f[s].spot = spot;
        let here = self.ai[s].pos;
        if here == spot || self.nav.comp_near(spot as usize) != self.nav.comp_near(here as usize) {
            return false;
        }
        self.walk_to(s, spot, now);
        true
    }

    /// Sparring and demonstrations for a soldier training at its barracks spot: who is around
    /// ([skills9] the experience itself is given by `skills_train_round`). `skill` is the
    /// soldier's weapon skill.
    pub(crate) fn barracks_spar(&mut self, s: usize, now: u64, skill: &str) -> crate::skills::TrainScene {
        let mut scene = crate::skills::TrainScene::default();
        let spot = self.combat.f[s].spot;
        if spot == NONE || self.ai[s].pos != spot {
            return scene;
        }
        let sq = self.combat.f[s].squad;
        let me = self.upos(s);
        // [skills9] The demonstrated skill rotates: weapon, Fighting, Discipline, Dodging.
        scene.demo_skill = ((now / 100) % 4) as u8;
        let demo = match scene.demo_skill {
            0 => skill,
            1 => "MELEE_COMBAT",
            2 => "DISCIPLINE",
            _ => "DODGING",
        };
        let my_rating = self.world.units[self.ai[s].uidx].skill(demo);
        // [skills9] w:Sparring: both need at least Competent (3) Fighting.
        let my_fight = self.world.units[self.ai[s].uidx].skill("MELEE_COMBAT");
        let members: Vec<i32> = self.world.squads.iter().find(|q| q.id == sq).map(|q| q.members.clone()).unwrap_or_default();
        let mut partner = false;
        let mut near = 0;
        let mut teacher: Option<(i32, usize)> = None;
        for uid in members {
            let Some(&t) = self.unit_slot.get(&uid) else { continue };
            if t == s || t >= self.combat.f.len() || self.ai[t].kind != Kind::Worker {
                continue;
            }
            let f = &self.combat.f[t];
            if f.role != Role::Soldier || f.duty != Duty::Train || f.target != NONE || f.spot == NONE || self.ai[t].pos != f.spot {
                continue;
            }
            let d = me.dist(self.upos(t));
            let ut = &self.world.units[self.ai[t].uidx];
            if d <= 1 && my_fight >= 3 && ut.skill("MELEE_COMBAT") >= 3 {
                partner = true;
            }
            if d <= 5 {
                near += 1;
            }
            let tr = ut.skill(demo);
            if d <= 3 && tr >= my_rating + 2 && teacher.map(|b| tr > b.0).unwrap_or(true) {
                teacher = Some((tr, t));
            }
        }
        scene.partner = partner;
        scene.watching = near >= 2;
        scene.teacher = teacher.map(|t| t.1);
        let mut r = Rng::for_unit(self.seed, s, now, 0x5BA2);
        if partner {
            self.combat.stats.sparring += 1;
            if r.chance(1, 2000) {
                self.sparring_bruise(s, &mut r);
            }
        }
        scene
    }

    /// A rare sparring injury: a bruise on an outer body part.
    fn sparring_bruise(&mut self, s: usize, r: &mut Rng) {
        self.ensure_body(s);
        let b = self.combat.f[s].body as usize;
        let body = &self.combat.bodies[b];
        if body.targets.is_empty() {
            return;
        }
        let total = body.targets.last().map(|t| t.1).unwrap_or(1).max(1);
        let roll = r.below(total);
        let part = body.targets.iter().find(|t| roll < t.1).map(|t| t.0).unwrap_or(body.targets[0].0);
        let layer = body.parts[part as usize].l0;
        self.combat.f[s].wounds.push(crate::combat::WoundS { part, layer, flags: crate::combat::wf::BRUISED, sev: 300, bleed: 0, pain: 5, age: 0 });
        self.list_wounded(s);
        self.combat.stats.sparring_injuries += 1;
        let n = self.uname(s);
        self.combat.log_line(format!("{} is bruised while sparring.", crate::combat::cap_pub(&n)));
    }

    /// Patrol: walk the route's waypoints in a loop.
    pub(crate) fn patrol_think(&mut self, s: usize, now: u64) {
        let sq = self.combat.f[s].squad;
        let route: Vec<u32> = self.world.squads.iter().find(|q| q.id == sq).map(|q| q.patrol.iter().filter_map(|&p| self.world.idx(p)).filter(|&i| self.nav.walkable(i)).map(|i| i as u32).collect()).unwrap_or_default();
        if route.is_empty() {
            self.ai[s].wake = now + 100;
            return;
        }
        let k = self.combat.f[s].patrol_i as usize % route.len();
        let goal = route[k];
        let here = self.ai[s].pos;
        if self.world.pos_of(here as usize).dist(self.world.pos_of(goal as usize)) <= 1 || self.nav.comp_near(goal as usize) != self.nav.comp_near(here as usize) {
            self.combat.f[s].patrol_i = ((k + 1) % route.len()) as u16;
            self.combat.stats.patrol_points += 1;
            self.ai[s].wake = now + 50;
            return;
        }
        self.walk_to(s, goal, now);
    }

    /// [lib hook] On-duty soldier going to sleep: a free bed in the squad's barracks.
    pub(crate) fn barracks_bed(&mut self, s: usize) -> Option<i32> {
        let f = self.combat.f.get(s)?;
        if f.duty == Duty::None || f.squad < 0 || self.combat.mil.barracks.is_empty() {
            return None;
        }
        let uid = self.ai[s].id;
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let b = self.barracks_of(f.squad)?;
        let k = b.beds.iter().copied().find(|&k| {
            let bed = &self.beds[k];
            bed.occupant == NONE && (bed.owner < 0 || bed.owner == uid) && c != 0 && self.nav.comp_near(bed.tile as usize) == c
        })?;
        self.combat.stats.barracks_sleeps += 1;
        Some(k as i32)
    }

    /// Ammo spec of soldier `s` for combat or training: (subtype, material selector), empty =
    /// any ammo; and how many rounds the soldier keeps.
    pub(crate) fn squad_ammo_spec(&self, s: usize, training: bool) -> (Vec<(String, String)>, i32) {
        let sq = self.combat.f.get(s).map(|f| f.squad).unwrap_or(-1);
        let Some(q) = self.world.squads.iter().find(|q| q.id == sq) else { return (Vec::new(), 50) };
        if q.ammo.is_empty() {
            return (Vec::new(), 50);
        }
        let members = q.members.iter().filter(|&&m| m >= 0).count().max(1) as i32;
        let mut spec = Vec::new();
        let mut amount = 0;
        for a in &q.ammo {
            if (training && !a.training) || (!training && !a.combat) {
                continue;
            }
            let mut it = a.item.splitn(2, '@');
            let sub = it.next().unwrap_or("").trim_start_matches("AMMO:").to_string();
            let mat = it.next().unwrap_or("").to_string();
            spec.push((sub, mat));
            amount += a.amount.max(0);
        }
        if spec.is_empty() {
            // Nothing allowed for this use: an impossible selector.
            return (vec![("-".into(), String::new())], 0);
        }
        (spec, (amount / members).clamp(1, 50))
    }

    /// Does ammo item `it` fit the spec?
    pub(crate) fn ammo_fits(&self, spec: &[(String, String)], it: &Item) -> bool {
        spec.is_empty()
            || spec.iter().any(|(sub, mat)| {
                (sub.is_empty() || *sub == it.subtype)
                    && (mat.is_empty() || {
                        let m = self.world.material(it.mat);
                        match mat.as_str() {
                            "WOOD" => m.map(|m| m.is_wood).unwrap_or(false),
                            "BONE" => m.map(|m| m.token.ends_with(":BONE")).unwrap_or(false),
                            "METAL" => m.map(|m| m.is_metal || m.token.starts_with("INORGANIC:")).unwrap_or(false),
                            tok => m.map(|m| m.token == tok).unwrap_or(false),
                        }
                    })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SquadOrder;
    use rf_core::{Building, ItemFlags, World};

    /// Synthetic fort: barracks zone in the west of the hall (x 18..23, y 20..24) with a bed.
    fn world() -> (World, i32) {
        let mut w = World::synthetic(48, 48, 20);
        let z = w.units[0].pos.z;
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
            // [skills9] w:Sparring needs Competent (3) Fighting.
            u.skills.push(rf_core::Skill { id: "MELEE_COMBAT".into(), rating: 3, xp: 0 });
        }
        w.buildings.push(Building { id: 900, kind: "Civzone".into(), subtype: "Barracks".into(), x1: 18, y1: 20, x2: 23, y2: 24, z, center: Pos::new(20, 22, z), built: true, owner: -1, name: "Kaserne".into(), ..Default::default() });
        let bp = Pos::new(23, 24, z);
        w.buildings.push(Building { id: 901, kind: "Bed".into(), x1: bp.x, y1: bp.y, x2: bp.x, y2: bp.y, z, center: bp, built: true, owner: -1, ..Default::default() });
        (w, 900)
    }

    fn sim_of(w: World, seed: u64) -> Sim {
        let mut sim = Sim::new(w, seed);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        sim
    }

    #[test]
    fn soldiers_train_and_spar_in_their_barracks() {
        let (w, zone) = world();
        let mut sim = sim_of(w, 51);
        let sq = sim.create_squad("Wache");
        for (k, id) in [1, 2, 3, 4].iter().enumerate() {
            sim.assign_squad_member(sq, k, *id);
        }
        sim.set_squad_barracks(sq, &[zone]);
        sim.order_squad(sq, SquadOrder::Train);
        sim.run_ticks(3000);
        for id in [1, 2, 3, 4] {
            let p = sim.world.unit(id).unwrap().pos;
            assert!((18..=23).contains(&p.x) && (20..=24).contains(&p.y), "soldier {id} trains in the barracks: {p:?}");
        }
        assert!(sim.combat.stats.sparring > 0, "partners spar");
        // Sparring teaches more than drills: compare with a squad without barracks.
        let (w2, _) = world();
        let mut solo = sim_of(w2, 51);
        let q2 = solo.create_squad("Solo");
        for (k, id) in [1, 2, 3, 4].iter().enumerate() {
            solo.assign_squad_member(q2, k, *id);
        }
        solo.order_squad(q2, SquadOrder::Train);
        solo.run_ticks(3000);
        let xp = |s: &Sim| s.world.units.iter().filter(|u| (1..=4).contains(&u.id)).map(|u| u.skills.iter().find(|k| k.id == "WRESTLING").map(|k| k.xp + k.rating * 1000).unwrap_or(0)).sum::<i32>();
        assert!(xp(&sim) > xp(&solo), "sparring {} vs drills {}", xp(&sim), xp(&solo));
        assert_eq!(solo.combat.stats.sparring, 0);
    }

    #[test]
    fn soldiers_on_duty_sleep_in_barracks_beds() {
        let (w, zone) = world();
        let mut sim = sim_of(w, 52);
        let sq = sim.create_squad("Wache");
        sim.assign_squad_member(sq, 0, 3);
        sim.set_squad_barracks(sq, &[zone]);
        sim.order_squad(sq, SquadOrder::Train);
        sim.run_ticks(10);
        let s = sim.unit_slot[&3];
        let now = sim.ticks;
        sim.start_sleep(s, now);
        let crate::Task::GoSleep(b) = sim.ai[s].task else { panic!("{:?}", sim.ai[s].task) };
        assert_eq!(sim.world.pos_of(sim.beds[b as usize].tile as usize), Pos::new(23, 24, sim.world.units[0].pos.z));
        assert_eq!(sim.combat.stats.barracks_sleeps, 1);
    }

    #[test]
    fn patrol_walks_its_route() {
        let (w, _) = world();
        let z = w.units[0].pos.z;
        let mut sim = sim_of(w, 53);
        let sq = sim.create_squad("Patrouille");
        sim.assign_squad_member(sq, 0, 2);
        sim.order_squad_patrol(sq, &[Pos::new(19, 21, z), Pos::new(29, 27, z)]);
        let s = sim.unit_slot[&2];
        sim.run_ticks(100);
        assert_eq!(sim.combat_caption(s).0, "Patrol");
        sim.run_ticks(2400);
        assert!(sim.combat.stats.patrol_points >= 3, "waypoints reached: {}", sim.combat.stats.patrol_points);
    }

    #[test]
    fn position_uniform_and_equipment_report() {
        let (w, _) = world();
        let z = w.units[0].pos.z;
        let mut sim = sim_of(w, 54);
        let mat = sim.ensure_material("INORGANIC:IRON");
        let id = sim.next_item_id;
        sim.next_item_id += 1;
        sim.push_item(Item { id, kind: "WEAPON".into(), subtype: "ITEM_WEAPON_AXE_BATTLE".into(), mat, pos: Pos::new(27, 22, z), stack: 1, container: -1, holder: -1, building: -1, desc: "iron battle axe".into(), flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() });
        let sq = sim.create_squad("Axt");
        sim.assign_squad_member(sq, 0, 4);
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE"]);
        let before = sim.squad_equipment(sq);
        assert_eq!(before[0].missing, vec!["WEAPON:ITEM_WEAPON_AXE_BATTLE".to_string()]);
        assert!(before[1].uniform.is_empty(), "other positions use the (empty) squad uniform");
        sim.order_squad(sq, SquadOrder::Train);
        sim.run_ticks(1500);
        let after = sim.squad_equipment(sq);
        assert!(after[0].worn.iter().any(|(i, _)| *i == id), "the axe was picked up: {after:?}");
        assert!(after[0].missing.is_empty());
    }

    #[test]
    fn uniform_materials_exact_and_partial_matches() {
        // [gaps10] wiki Uniform: the material must match; without a material the best item;
        // "Partial matches okay" (default) substitutes, "Exact matches only" goes without.
        let (w, _) = world();
        let z = w.units[0].pos.z;
        let mut sim = sim_of(w, 57);
        let iron = sim.ensure_material("INORGANIC:IRON");
        let steel = sim.ensure_material("INORGANIC:STEEL");
        let mut axe = |sim: &mut Sim, mat: u16, x: i16| {
            let id = sim.next_item_id;
            sim.next_item_id += 1;
            sim.push_item(Item { id, kind: "WEAPON".into(), subtype: "ITEM_WEAPON_AXE_BATTLE".into(), mat, pos: Pos::new(x, 22, z), stack: 1, container: -1, holder: -1, building: -1, desc: "battle axe".into(), flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() });
            id
        };
        let near_iron = axe(&mut sim, iron, 21);
        let far_steel = axe(&mut sim, steel, 30);
        let sq = sim.create_squad("Axt");
        sim.assign_squad_member(sq, 0, 4);
        let s = sim.unit_slot[&4];
        sim.combat.f[s].squad = sq;
        sim.world.units[sim.ai[s].uidx].squad_position = 0;
        let q = sim.world.idx(Pos::new(22, 22, z)).unwrap() as u32;
        sim.move_to(s, q, 0);
        // A named material: only steel fits, though the iron axe lies nearer.
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:STEEL"]);
        assert_eq!(sim.find_uniform_item(s), Some(far_steel));
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE@steel"]);
        assert_eq!(sim.find_uniform_item(s), Some(far_steel), "plain metal names");
        // No material: the best metal (steel rather than iron).
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE"]);
        assert_eq!(sim.find_uniform_item(s), Some(far_steel));
        // A class: `@Metal` fits both, the best first; `@Leather` fits neither.
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE@Metal"]);
        assert_eq!(sim.find_uniform_item(s), Some(far_steel));
        // Partial matches (default): a copper axe is wanted, none exists, iron/steel substitute.
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:COPPER"]);
        assert_eq!(sim.find_uniform_item(s), Some(far_steel), "something close: the best substitute");
        sim.set_squad_uniform_exact(sq, true);
        assert_eq!(sim.find_uniform_item(s), None, "exact matches only: go without");
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:IRON"]);
        assert_eq!(sim.find_uniform_item(s), Some(near_iron));
        // The equipment report honours the material with exact matches.
        sim.set_squad_uniform(sq, 0, &["WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:STEEL"]);
        sim.equip(s, near_iron);
        let rep = sim.squad_equipment(sq);
        assert_eq!(rep[0].missing.len(), 1, "an iron axe is no steel axe: {rep:?}");
        sim.set_squad_uniform_exact(sq, false);
        assert!(sim.squad_equipment(sq)[0].missing.is_empty(), "partial match accepted");
    }

    #[test]
    fn squad_ammo_settings_pick_ammo_by_use() {
        let (w, _) = world();
        let mut sim = sim_of(w, 55);
        let sq = sim.create_squad("Schuetzen");
        sim.assign_squad_member(sq, 0, 5);
        sim.assign_squad_member(sq, 1, 6);
        sim.set_squad_ammo(
            sq,
            vec![
                SquadAmmo { item: "ITEM_AMMO_BOLTS@WOOD".into(), amount: 100, combat: false, training: true },
                SquadAmmo { item: "ITEM_AMMO_BOLTS@METAL".into(), amount: 300, combat: true, training: false },
            ],
        );
        let s = sim.unit_slot[&5];
        sim.combat.f[s].squad = sq;
        let iron = sim.ensure_material("INORGANIC:IRON");
        let bolt = |mat: u16| Item { kind: "AMMO".into(), subtype: "ITEM_AMMO_BOLTS".into(), mat, stack: 25, ..Default::default() };
        let (train, n_train) = sim.squad_ammo_spec(s, true);
        let (fight, n_fight) = sim.squad_ammo_spec(s, false);
        assert_eq!((n_train, n_fight), (50, 50), "amount per soldier, at most a quiver");
        assert!(sim.ammo_fits(&train, &bolt(3)) && !sim.ammo_fits(&train, &bolt(iron)), "wooden bolts for training");
        assert!(sim.ammo_fits(&fight, &bolt(iron)) && !sim.ammo_fits(&fight, &bolt(3)), "metal bolts for combat");
    }

    #[test]
    fn civilian_alert_prefers_drinks_and_beds_inside_the_burrow() {
        let (w, _) = world();
        let z = w.units[0].pos.z;
        let mut sim = sim_of(w, 56);
        // Drinks lie at (28, 21..25); the burrow holds only the one at (28, 24) and the bed at (23, 24).
        let b = sim.create_burrow("Zuflucht", Pos::new(23, 24, z), Pos::new(28, 24, z));
        let s = sim.unit_slot[&1];
        let q = sim.world.idx(Pos::new(28, 20, z)).unwrap() as u32;
        sim.move_to(s, q, 0);
        let now = sim.ticks;
        let near = sim.nearest_consumable(s, true, now).unwrap();
        assert_eq!(sim.world.item(near).unwrap().pos, Pos::new(28, 21, z), "without an alert: the nearest drink");
        sim.set_civilian_alert(Some(b));
        assert!(sim.alert_restricted(s));
        let inside = sim.nearest_consumable(s, true, now).unwrap();
        assert_eq!(sim.world.item(inside).unwrap().pos, Pos::new(28, 24, z), "under the alert: the drink inside the burrow");
        sim.start_sleep(s, now);
        let crate::Task::GoSleep(bed) = sim.ai[s].task else { panic!("{:?}", sim.ai[s].task) };
        assert_eq!(sim.world.pos_of(sim.beds[bed as usize].tile as usize), Pos::new(23, 24, z), "the bed inside the burrow");
    }
}
