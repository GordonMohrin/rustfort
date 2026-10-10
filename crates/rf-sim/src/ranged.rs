//! Ranged combat (round 4): crossbows, bows and blowguns, ammunition and quivers, projectiles,
//! line of fire through fortifications, archery practice, goblin bowmen.
//!
//! # DF rules implemented (wiki v50: Crossbow, Bow, Blowgun, Ammunition, Projectile, Marksdwarf,
//! Archery target, Fortification, Quiver, Material science)
//! - A unit **shoots** when it wields a launcher (weapon raw with `RANGED:<skill>:<ammo class>`,
//!   e.g. crossbow -> CROSSBOW skill and BOLT ammo) and carries matching ammunition (in a quiver
//!   or loose in its inventory). Marksdwarves open fire at about **20 tiles**; a missed bolt flies
//!   on to about **30 tiles** (wiki Marksdwarf). Height differences shorten the range.
//! - **Projectile momentum** (wiki Material science): fired with momentum `SHOOT_FORCE / 20`, but
//!   the speed is capped at `SHOOT_MAXVEL / 10`, so `M = min(F / 20, maxvel / 10 * weight)`. With
//!   the wiki's threshold (bolts reach the full 50 above density 1666) the weight unit is
//!   `size (cm³) * density (kg/m³) / 100 000`: iron, silver or steel bolts hit with momentum 50,
//!   wooden bolts with about 18. The ammo's own attack (bolts/arrows: EDGE, contact 5,
//!   penetration 1000; blowdarts: contact 1, penetration 50) and material (shear values, edge)
//!   then go through the combat module's armor and tissue-layer resolution, so iron bolts are
//!   mostly deflected by iron armor of good quality and never cut steel, as the wiki notes
//!   ("equal or lesser metal might take over 4x more ammo").
//! - **Line of fire**: walls, solid rock, floors between z-levels, closed doors, statues,
//!   supports, windows and floodgates stop projectiles. **Fortifications** let projectiles pass
//!   but block movement; a shooter below **Accomplished** (skill 10) only fires through one when
//!   standing directly next to it (wiki Fortification). Elite enemy bowmen shoot through.
//! - **Ammo is used up**: each shot takes one bolt out of the stack; the projectile lands where
//!   it stopped (some break) and can be hauled back to an ammo stockpile. DF's standing order
//!   "forbid used ammunition" is honoured (`settings.other` key containing `USED_AMMO`; default
//!   on, matching the forbidden fired bolts lying around in Tileshtulon) and can be changed with
//!   [`Sim::set_forbid_used_ammo`].
//! - **Defending against projectiles**: the target can jump away (DODGING; higher shooter skill
//!   "significantly reduces the probability of the enemy dodging") or block with a shield
//!   (projectiles are blocked more often than parried; parrying projectiles is not modelled).
//! - **Marksdwarves** fetch a quiver and bolts matching their launcher (squad ammo settings are
//!   not exported: bolts for crossbows, arrows for bows, darts for blowguns; metal ammo first for
//!   combat, wooden or bone "training bolts" first for practice), shoot hostiles in range and line
//!   of fire, step back when an enemy is about to reach them, and fight in melee when the enemy
//!   is adjacent or the quiver is empty: they bash with the crossbow using the HAMMER skill (the
//!   crossbow's melee attack from the raws).
//! - **Experience**: about 30 per shot at live enemies, 7.5 per shot at an archery target, per
//!   shot fired, not per hit (wiki Marksdwarf: 17 vs 67 shots to Novice).
//! - **Archery practice**: soldiers on training duty with a launcher and ammo go to an archery
//!   range zone next to an archery target (one shooter per target, standing at least one tile
//!   away) and shoot at it.
//! - **Hostile archers** (goblin bowmen with bows and quivers of arrows) shoot at the fort's
//!   people and animals in range and line of fire, also across chasms they cannot walk over.
//!   Generated sieges bring bowmen (see [`Sim::ranged_arm_siege`]).
//!
//! # Estimates (not on the wiki)
//! - Hit chance `50 + 4*skill + archery - 2*distance`, scaled by target size (cube root of
//!   size/60 000), 5..95 %. Dodge `5 + 3*DODGING - 2*skill + agility`, 0..40 %; shield block
//!   `20 + 4*SHIELD - 2*skill`, 5..60 %.
//! - Fire interval `60 * 10 / (10 + skill)` ticks scaled by agility (DF: "higher skill ... a higher
//!   rate of fire"); projectiles arrive instantly; the momentum roll is 80..100 %.
//! - A hit breaks the projectile 1 in 3, a block 1 in 5, a miss 1 in 10; otherwise it drops.
//! - Height counts double for range (`max(|dx|,|dy|) + 2*|dz|`).
//! - Marksdwarves fill their quiver to 50 rounds (immigrants arrive with 30-40) and re-check
//!   their ammo every quarter day. 25 % of a generated siege are bowmen with 30-40 arrows.
//! - A shooter steps back at most once every 100 ticks, when the enemy is two tiles away.
//! - Projectiles do not hit units standing in the line of fire (no friendly fire yet).

use crate::combat::{AtkT, Blow, Role, Sub};
use crate::military::Duty;
use crate::nav::MAX_NEIGHBORS;
use crate::path::Goals;
use crate::rng::Rng;
use crate::{Kind, Sim, NONE};
use rf_core::{Item, ItemFlags, Pos, Shape, Skill};
use std::collections::HashMap;

/// Marksdwarves "usually open fire at a distance of 20 tiles or so" (wiki Marksdwarf).
pub const FIRE_RANGE: i32 = 20;
/// "A bolt may travel 30 or more tiles" (wiki Marksdwarf).
pub const FLIGHT_RANGE: i32 = 30;
/// Accomplished: fires through fortifications from a distance (wiki Fortification).
pub const FORTIFICATION_SKILL: i32 = 10;
/// Rounds a marksdwarf keeps in the quiver (estimate).
const QUIVER_WANT: i32 = 50;
/// Re-check quiver and ammo (estimate: a quarter day).
const EQUIP_PERIOD: u64 = 300;

/// Counters for reports and tests.
#[derive(Clone, Debug, Default)]
pub struct RangedStats {
    /// Shots at units (combat), by the fort's side and by hostiles.
    pub shots: u64,
    pub fort_shots: u64,
    pub hostile_shots: u64,
    pub hits: u64,
    pub misses: u64,
    pub dodged: u64,
    pub blocked: u64,
    /// Hits stopped by armor (no wound).
    pub deflected: u64,
    pub kills: u64,
    pub fort_kills: u64,
    /// Rounds fired (combat and practice).
    pub ammo_used: u64,
    pub ammo_broken: u64,
    pub ammo_dropped: u64,
    /// Practice shots at archery targets and how many hit the target.
    pub practice_shots: u64,
    pub practice_hits: u64,
    /// Quivers and ammo stacks picked up by soldiers.
    pub quivers_fetched: u64,
    pub ammo_fetched: u64,
    /// Times a shooter fought in melee (adjacent enemy or no ammo).
    pub melee_fallbacks: u64,
    pub out_of_ammo: u64,
    pub bowmen_spawned: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct Launcher {
    pub skill: String,
    pub class: String,
    pub force: f32,
    pub maxvel: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct AmmoT {
    pub name: String,
    pub class: String,
    pub size: f32,
    pub atk: AtkT,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShooterS {
    pub check_at: u64,
    pub launcher: Option<u16>,
    pub ammo: i32,
    pub next_shot: u64,
    pub scan_at: u64,
    pub retreat_at: u64,
    pub equip_at: u64,
    /// Archery target in use (index into `Ranged::targets`) and the spot to shoot from.
    pub train: Option<usize>,
    pub spot: u32,
    pub shots: u32,
    pub melee: bool,
    /// No archery practice before this tick (after a path failure).
    pub no_practice_until: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ArcheryT {
    pub pos: Pos,
    /// Archery range zone next to/around the target: (x1, y1, x2, y2, z), or the target itself.
    pub zone: (i16, i16, i16, i16, i16),
    pub user: u32,
}

#[derive(Default)]
pub(crate) struct Ranged {
    pub init: bool,
    pub s: Vec<ShooterS>,
    pub launchers: Vec<Launcher>,
    pub launcher_of: HashMap<String, Option<u16>>,
    pub ammo: Vec<AmmoT>,
    pub ammo_of: HashMap<String, Option<u16>>,
    pub stats: RangedStats,
    /// Tiles whose building stops projectiles (bitset over the map).
    pub block: Vec<u64>,
    pub block_n: usize,
    pub targets: Vec<ArcheryT>,
    pub forbid_used: bool,
    buf: Vec<(i32, u32)>,
}

enum Aim {
    Unit(usize),
    Target(usize),
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Rounded integer division (symmetric).
fn rdiv(num: i32, den: i32) -> i32 {
    if num >= 0 {
        (2 * num + den) / (2 * den)
    } else {
        -((-2 * num + den) / (2 * den))
    }
}

/// Range with height counting double (estimate).
pub(crate) fn fire_dist(a: Pos, b: Pos) -> i32 {
    let dx = (a.x as i32 - b.x as i32).abs();
    let dy = (a.y as i32 - b.y as i32).abs();
    let dz = (a.z as i32 - b.z as i32).abs();
    dx.max(dy) + 2 * dz
}

fn blocks_projectiles(kind: &str) -> bool {
    matches!(kind, "Door" | "Hatch" | "Statue" | "Support" | "WindowGlass" | "WindowGem" | "Floodgate" | "GearAssembly")
}

/// Default launchers (vanilla item_weapon.txt) by token.
fn default_launcher(token: &str) -> Option<Launcher> {
    let (skill, class, force, maxvel) = match token {
        "ITEM_WEAPON_CROSSBOW" => ("CROSSBOW", "BOLT", 1000.0, 200.0),
        "ITEM_WEAPON_BOW" => ("BOW", "ARROW", 1000.0, 200.0),
        "ITEM_WEAPON_BLOWGUN" => ("BLOWGUN", "BLOWDART", 100.0, 1000.0),
        _ => return None,
    };
    Some(Launcher { skill: skill.into(), class: class.into(), force, maxvel })
}

/// Projectile penetration relative to the combat module's melee convention (estimate: the wiki
/// calls penetration "poorly understood"). With x12 a bolt (penetration 1000) goes 120 mm deep,
/// so an iron bolt that is not stopped by armor goes through skin, fat, muscle and bone of a
/// goblin's chest into the lungs or heart, as DF's combat reports show.
const PROJ_PEN_K: f32 = 12.0;

fn ammo_atk(contact: f32, pen: f32, edge: bool) -> AtkT {
    let pen = pen * PROJ_PEN_K;
    AtkT {
        verb2: "strike".into(),
        verb3: "strikes".into(),
        part: -1,
        contact,
        pen,
        vel: 1.0,
        prep: 3,
        rec: 3,
        edge,
        weight: 1,
        skill: String::new(),
    }
}

impl Sim {
    // ------------------------------------------------------------------ setup

    fn ranged_init(&mut self) {
        if self.ranged.init {
            return;
        }
        self.ranged.init = true;
        // DF standing order "forbid used ammunition" (default on, see module docs).
        self.ranged.forbid_used = true;
        for kv in &self.world.settings.other {
            let up = kv.to_ascii_uppercase();
            if up.contains("USED_AMMO") || up.contains("USEDAMMO") {
                let v = up.rsplit(['=', ':']).next().unwrap_or("").trim().to_string();
                self.ranged.forbid_used = !matches!(v.as_str(), "0" | "FALSE" | "NO" | "OFF");
            }
        }
        self.ranged_rebuild_blockers();
        // Archery targets and the range zones next to them.
        let zones: Vec<(i16, i16, i16, i16, i16)> = self
            .world
            .buildings
            .iter()
            .filter(|b| b.kind == "Civzone" && b.subtype.contains("Archery"))
            .map(|b| (b.x1, b.y1, b.x2, b.y2, b.z))
            .collect();
        let mut targets = Vec::new();
        for b in self.world.buildings.iter().filter(|b| b.kind == "ArcheryTarget") {
            let p = b.center;
            if !self.world.in_bounds(p) {
                continue;
            }
            let zone = zones
                .iter()
                .copied()
                .find(|&(x1, y1, x2, y2, z)| z == p.z && p.x >= x1 - 1 && p.x <= x2 + 1 && p.y >= y1 - 1 && p.y <= y2 + 1)
                .unwrap_or((p.x - 10, p.y - 10, p.x + 10, p.y + 10, p.z));
            targets.push(ArcheryT { pos: p, zone, user: NONE });
        }
        self.ranged.targets = targets;
    }

    fn ranged_rebuild_blockers(&mut self) {
        let n = self.world.tiles.len();
        let mut bits = vec![0u64; n.div_ceil(64)];
        for b in &self.world.buildings {
            if !blocks_projectiles(&b.kind) {
                continue;
            }
            // Open doors/hatches let projectiles pass (round-4 building flags).
            if b.flags.iter().any(|f| f == "OPEN") {
                continue;
            }
            for y in b.y1..=b.y2 {
                for x in b.x1..=b.x2 {
                    let p = Pos::new(x, y, b.z);
                    if b.contains(p) {
                        if let Some(i) = self.world.idx(p) {
                            bits[i / 64] |= 1 << (i % 64);
                        }
                    }
                }
            }
        }
        self.ranged.block = bits;
        self.ranged.block_n = self.world.buildings.len();
    }

    fn ranged_sync(&mut self) {
        if !self.ranged.init {
            self.ranged_init();
        }
        if self.ranged.s.len() < self.ai.len() {
            self.ranged.s.resize(self.ai.len(), ShooterS::default());
        }
        if self.ranged.block_n != self.world.buildings.len() {
            self.ranged_rebuild_blockers();
        }
    }

    pub(crate) fn launcher_t(&mut self, token: &str) -> Option<u16> {
        if let Some(&l) = self.ranged.launcher_of.get(token) {
            return l;
        }
        let raw = self.world.weapon_raws.iter().find(|w| w.token == token);
        let l = match raw {
            Some(w) if w.kind != "AMMO" && !w.token.starts_with("ITEM_AMMO") && (!w.skill_ranged.is_empty() || w.shoot_force > 0) => {
                let class = w.ranged_ammo.clone();
                let skill = if !w.skill_ranged.is_empty() {
                    w.skill_ranged.clone()
                } else {
                    match class.as_str() {
                        "BOLT" => "CROSSBOW",
                        "ARROW" => "BOW",
                        _ => "BLOWGUN",
                    }
                    .to_string()
                };
                let d = default_launcher(token);
                Some(Launcher {
                    skill,
                    class,
                    force: if w.shoot_force > 0 { w.shoot_force as f32 } else { d.as_ref().map(|d| d.force).unwrap_or(1000.0) },
                    maxvel: if w.shoot_maxvel > 0 { w.shoot_maxvel as f32 } else { d.as_ref().map(|d| d.maxvel).unwrap_or(200.0) },
                })
            }
            Some(w) if !w.ranged_ammo.is_empty() && w.kind.is_empty() && !w.token.starts_with("ITEM_AMMO") => {
                // Older exports: launcher fields missing, fall back to vanilla values.
                default_launcher(token).or_else(|| {
                    let skill = match w.ranged_ammo.as_str() {
                        "BOLT" => "CROSSBOW",
                        "ARROW" => "BOW",
                        _ => "BLOWGUN",
                    };
                    Some(Launcher { skill: skill.into(), class: w.ranged_ammo.clone(), force: 1000.0, maxvel: 200.0 })
                })
            }
            Some(_) => None,
            None => default_launcher(token),
        };
        let id = l.map(|l| {
            self.ranged.launchers.push(l);
            (self.ranged.launchers.len() - 1) as u16
        });
        self.ranged.launcher_of.insert(token.to_string(), id);
        id
    }

    pub(crate) fn ammo_t(&mut self, token: &str) -> Option<u16> {
        if let Some(&a) = self.ranged.ammo_of.get(token) {
            return a;
        }
        let raw = self.world.weapon_raws.iter().find(|w| w.token == token && (w.kind == "AMMO" || w.token.starts_with("ITEM_AMMO")));
        let a = match raw {
            Some(w) => {
                let atk = w.attacks.first().map(|x| {
                    let edge = x.flags.iter().any(|f| f == "EDGE" || f == "ATTACK_FLAG_EDGE");
                    ammo_atk(x.contact_area.max(1) as f32, x.penetration as f32, edge)
                });
                Some(AmmoT {
                    name: if w.name.is_empty() { token.to_ascii_lowercase() } else { w.name.clone() },
                    class: w.ranged_ammo.clone(),
                    size: w.size.max(1) as f32,
                    atk: atk.unwrap_or_else(|| ammo_atk(5.0, 1000.0, true)),
                })
            }
            None => match token {
                "ITEM_AMMO_BOLTS" => Some(AmmoT { name: "bolt".into(), class: "BOLT".into(), size: 150.0, atk: ammo_atk(5.0, 1000.0, true) }),
                "ITEM_AMMO_ARROWS" => Some(AmmoT { name: "arrow".into(), class: "ARROW".into(), size: 150.0, atk: ammo_atk(5.0, 1000.0, true) }),
                "ITEM_AMMO_BLOWDARTS" => Some(AmmoT { name: "blowdart".into(), class: "BLOWDART".into(), size: 20.0, atk: ammo_atk(1.0, 50.0, true) }),
                _ => None,
            },
        };
        let id = a.map(|a| {
            self.ranged.ammo.push(a);
            (self.ranged.ammo.len() - 1) as u16
        });
        self.ranged.ammo_of.insert(token.to_string(), id);
        id
    }

    // ------------------------------------------------------------------ inventory

    /// Wielded launcher: (weapon item index, launcher template).
    fn ranged_weapon(&mut self, s: usize) -> Option<(usize, u16)> {
        let uidx = self.ai[s].uidx;
        let n = self.world.units[uidx].inventory.len();
        for k in 0..n {
            let id = self.world.units[uidx].inventory[k];
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if self.imeta[i].dead || self.world.items[i].kind != "WEAPON" {
                continue;
            }
            let sub = self.world.items[i].subtype.clone();
            // The first wielded weapon decides, as in `wielded`.
            return self.launcher_t(&sub).map(|l| (i, l));
        }
        None
    }

    /// Ammo items of `class` the unit carries (in quivers or loose): item indices.
    fn ammo_items(&mut self, s: usize, class: &str, out: &mut Vec<usize>) {
        out.clear();
        let uidx = self.ai[s].uidx;
        let inv = self.world.units[uidx].inventory.clone();
        for id in inv {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if self.imeta[i].dead {
                continue;
            }
            match self.world.items[i].kind.as_str() {
                "AMMO" => {
                    let sub = self.world.items[i].subtype.clone();
                    if self.ammo_t(&sub).map(|a| self.ranged.ammo[a as usize].class == class).unwrap_or(false) {
                        out.push(i);
                    }
                }
                "QUIVER" => {
                    let ids = self.contents.get(&id).cloned().unwrap_or_default();
                    for cid in ids {
                        let Some(&ci) = self.item_idx.get(&cid) else { continue };
                        let ci = ci as usize;
                        if self.imeta[ci].dead || self.world.items[ci].kind != "AMMO" || self.world.items[ci].container != id {
                            continue;
                        }
                        let sub = self.world.items[ci].subtype.clone();
                        if self.ammo_t(&sub).map(|a| self.ranged.ammo[a as usize].class == class).unwrap_or(false) {
                            out.push(ci);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn count_ammo(&mut self, s: usize, class: &str) -> i32 {
        let mut v = Vec::new();
        self.ammo_items(s, class, &mut v);
        v.iter().map(|&i| self.world.items[i].stack.max(0)).sum()
    }

    fn quiver_of(&self, s: usize) -> Option<i32> {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.iter().copied().find(|id| self.item_idx.get(id).map(|&i| !self.imeta[i as usize].dead && self.world.items[i as usize].kind == "QUIVER").unwrap_or(false))
    }

    fn is_metal(&self, mat: u16) -> bool {
        self.world.material(mat).map(|m| m.is_metal || m.token.starts_with("INORGANIC:")).unwrap_or(false)
    }

    /// Take one round out of the unit's ammo (training: wooden/bone rounds first, else metal
    /// first). Returns the projectile (stack 1, not in the world).
    fn take_round(&mut self, s: usize, class: &str, training: bool) -> Option<Item> {
        let mut v = Vec::new();
        self.ammo_items(s, class, &mut v);
        let pick = v.iter().copied().max_by_key(|&i| {
            let metal = self.is_metal(self.world.items[i].mat);
            ((metal != training) as i32, self.world.items[i].quality as i32, std::cmp::Reverse(self.world.items[i].id))
        })?;
        let it = &mut self.world.items[pick];
        let mut round = it.clone();
        round.stack = 1;
        if it.stack > 1 {
            it.stack -= 1;
            let n = it.stack;
            it.desc = restack_desc(&it.desc, n);
        } else {
            let id = it.id;
            self.destroy_item(id);
        }
        round.desc = restack_desc(&round.desc, 1);
        self.ranged.stats.ammo_used += 1;
        Some(round)
    }

    /// Ammo picked up through the uniform path ends up in the inventory: put it into the quiver
    /// (and take quivers and ammo out of the bins they came from).
    fn ranged_stow(&mut self, s: usize) {
        let uidx = self.ai[s].uidx;
        let quiver = self.quiver_of(s);
        let inv = self.world.units[uidx].inventory.clone();
        for id in inv {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            let (kind, cont) = (self.world.items[i].kind.clone(), self.world.items[i].container);
            if kind != "AMMO" && kind != "QUIVER" {
                continue;
            }
            if cont >= 0 {
                if let Some(v) = self.contents.get_mut(&cont) {
                    v.retain(|&x| x != id);
                }
                let it = &mut self.world.items[i];
                it.container = -1;
                it.flags.in_container = false;
            }
            let bld = self.world.items[i].building;
            if bld >= 0 {
                // Taken out of the workshop that made it.
                self.world.items[i].building = -1;
                if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == bld) {
                    b.items.retain(|&x| x != id);
                }
            }
            if kind == "AMMO" {
                if let Some(q) = quiver {
                    let it = &mut self.world.items[i];
                    it.container = q;
                    it.holder = -1;
                    it.flags.in_container = true;
                    it.flags.in_inventory = true;
                    it.flags.on_ground = false;
                    self.contents.entry(q).or_default().push(id);
                    self.world.units[uidx].inventory.retain(|&x| x != id);
                }
            }
        }
    }

    /// Re-evaluate launcher and ammo of a unit (cheap, cached for 200 ticks).
    fn shooter_check(&mut self, s: usize, now: u64) {
        if now < self.ranged.s[s].check_at {
            return;
        }
        self.ranged.s[s].check_at = now + 200;
        let l = self.ranged_weapon(s).map(|(_, l)| l);
        self.ranged.s[s].launcher = l;
        let n = match l {
            Some(l) => {
                let class = self.ranged.launchers[l as usize].class.clone();
                self.count_ammo(s, &class)
            }
            None => 0,
        };
        self.ranged.s[s].ammo = n;
    }

    // ------------------------------------------------------------------ line of fire

    /// Projectiles can fly from `a` to `b` (walls, floors between levels, closed doors and
    /// fortifications per the shooter's `skill`). Public for tests and the UI.
    pub fn line_of_fire(&self, a: Pos, b: Pos, skill: i32) -> bool {
        self.los_walk(a, b, skill, None)
    }

    fn tile_blocks(&self, i: usize) -> bool {
        self.ranged.block.get(i / 64).map(|w| w & (1 << (i % 64)) != 0).unwrap_or(false)
    }

    /// Walk the line from `a` to `b`. With `out`, record the last tile reached (for landing).
    fn los_walk(&self, a: Pos, b: Pos, skill: i32, mut out: Option<&mut Pos>) -> bool {
        let (dx, dy, dz) = (b.x as i32 - a.x as i32, b.y as i32 - a.y as i32, b.z as i32 - a.z as i32);
        let n = dx.abs().max(dy.abs()).max(dz.abs());
        let mut prev = a;
        for k in 1..=n {
            let cur = Pos::new((a.x as i32 + rdiv(dx * k, n)) as i16, (a.y as i32 + rdiv(dy * k, n)) as i16, (a.z as i32 + rdiv(dz * k, n)) as i16);
            if !self.world.in_bounds(cur) {
                return false;
            }
            // Crossing between levels: through the floor of the upper tile.
            if cur.z > prev.z && self.world.shape(cur).has_floor() {
                return false;
            }
            if cur.z < prev.z && self.world.shape(prev).has_floor() {
                return false;
            }
            if k < n {
                let sh = self.world.shape(cur);
                match sh {
                    Shape::Wall | Shape::None => return false,
                    Shape::Fortification => {
                        if skill < FORTIFICATION_SKILL && k != 1 {
                            return false;
                        }
                    }
                    _ => {}
                }
                if self.tile_blocks(self.world.idx_unchecked(cur)) {
                    return false;
                }
            }
            if let Some(o) = out.as_deref_mut() {
                *o = cur;
            }
            prev = cur;
        }
        true
    }

    // ------------------------------------------------------------------ targets

    fn shooter_skill(&self, s: usize, l: u16) -> i32 {
        let u = &self.world.units[self.ai[s].uidx];
        u.skill(&self.ranged.launchers[l as usize].skill)
    }

    /// Nearest enemy in range and line of fire.
    fn ranged_find_target(&mut self, s: usize, skill: i32) -> u32 {
        let me = self.upos(s);
        let hostile = self.combat.f[s].role == Role::Hostile;
        let mut buf = std::mem::take(&mut self.ranged.buf);
        buf.clear();
        let list = if hostile { &self.combat.fort } else { &self.combat.hostiles };
        for &t in list {
            if t as usize == s || !self.alive(t) {
                continue;
            }
            let ti = t as usize;
            let u = &self.world.units[self.ai[ti].uidx];
            if u.flags.inactive || u.flags.caged || self.combat.f.get(ti).map(|f| f.hidden).unwrap_or(false) {
                continue; // [military2] hidden raiders cannot be targeted
            }
            if !hostile && self.combat.f.get(ti).map(|f| f.role == Role::None).unwrap_or(false) && !u.flags.hostile {
                continue;
            }
            let d = fire_dist(me, self.world.pos_of(self.ai[ti].pos as usize));
            if d <= FIRE_RANGE && d >= 1 {
                buf.push((d, t));
            }
        }
        buf.sort_unstable();
        let mut found = NONE;
        for &(_, t) in buf.iter().take(12) {
            if self.line_of_fire(me, self.upos(t as usize), skill) {
                found = t;
                break;
            }
        }
        self.ranged.buf = buf;
        found
    }

    fn adjacent_enemy(&self, s: usize, me: Pos) -> Option<u32> {
        let hostile = self.combat.f[s].role == Role::Hostile;
        let list = if hostile { &self.combat.fort } else { &self.combat.hostiles };
        list.iter().copied().find(|&t| {
            t as usize != s && self.alive(t) && {
                let tp = self.upos(t as usize);
                tp.z == me.z && (tp.x as i32 - me.x as i32).abs() <= 1 && (tp.y as i32 - me.y as i32).abs() <= 1 && self.can_fight(t as usize, self.ticks)
            }
        })
    }

    fn release_target(&mut self, s: usize) {
        if let Some(k) = self.ranged.s[s].train.take() {
            if let Some(t) = self.ranged.targets.get_mut(k) {
                if t.user == s as u32 {
                    t.user = NONE;
                }
            }
        }
    }

    /// Hook at the start of `pursue`: shoot instead of closing in. `None` = melee logic decides.
    pub(crate) fn ranged_pursue(&mut self, s: usize, now: u64) -> Option<bool> {
        let role = self.combat.f[s].role;
        if !matches!(role, Role::Soldier | Role::Hostile | Role::Fighter) {
            return None;
        }
        self.ranged_sync();
        self.shooter_check(s, now);
        let l = self.ranged.s[s].launcher?;
        let t0 = self.combat.f[s].target;
        if self.ranged.s[s].ammo <= 0 {
            if self.alive(t0) && !self.ranged.s[s].melee {
                self.ranged.s[s].melee = true;
                self.ranged.stats.melee_fallbacks += 1;
                self.ranged.stats.out_of_ammo += 1;
            }
            return None;
        }
        let skill = self.shooter_skill(s, l);
        let me = self.upos(s);
        // An enemy next to us: fight it in melee (bash with the launcher).
        if let Some(e) = self.adjacent_enemy(s, me) {
            self.combat.f[s].target = e;
            if !self.ranged.s[s].melee {
                self.ranged.s[s].melee = true;
                self.ranged.stats.melee_fallbacks += 1;
            }
            return None;
        }
        let valid = |sim: &Sim, t: u32| {
            sim.alive(t) && !sim.world.units[sim.ai[t as usize].uidx].flags.inactive && {
                let tp = sim.upos(t as usize);
                fire_dist(me, tp) <= FIRE_RANGE && sim.line_of_fire(me, tp, skill)
            }
        };
        let mut t = t0;
        if !valid(self, t) {
            if now < self.ranged.s[s].scan_at {
                return None;
            }
            self.ranged.s[s].scan_at = now + 10;
            let nt = self.ranged_find_target(s, skill);
            if nt == NONE {
                return None;
            }
            t = nt;
            self.combat.f[s].target = t;
            self.combat.f[s].dormant_until = 0;
        }
        let ti = t as usize;
        let tp = self.upos(ti);
        let cheb = (me.x as i32 - tp.x as i32).abs().max((me.y as i32 - tp.y as i32).abs());
        if cheb <= 1 && me.z == tp.z {
            // Adjacent: melee (bash with the crossbow).
            if !self.ranged.s[s].melee {
                self.ranged.s[s].melee = true;
                self.ranged.stats.melee_fallbacks += 1;
            }
            return None;
        }
        self.ranged.s[s].melee = false;
        self.release_target(s);
        // Step back when the enemy is about to reach us (fort side).
        if role == Role::Soldier && cheb <= 2 && me.z == tp.z && now >= self.ranged.s[s].retreat_at {
            self.ranged.s[s].retreat_at = now + 100;
            let here = self.ai[s].pos as usize;
            let mut nb = [0u32; MAX_NEIGHBORS];
            let n = self.nav.neighbors(here, &mut nb);
            let mut best = (cheb, NONE);
            for &q in &nb[..n] {
                let qp = self.world.pos_of(q as usize);
                if qp.z != me.z {
                    continue;
                }
                let d = (qp.x as i32 - tp.x as i32).abs().max((qp.y as i32 - tp.y as i32).abs());
                if d > best.0 && self.line_of_fire(qp, tp, skill) {
                    best = (d, q);
                }
            }
            if best.1 != NONE {
                self.ai[s].path.clear();
                self.ai[s].path_i = 0;
                self.move_to(s, best.1, now);
                return Some(true);
            }
        }
        if now >= self.ranged.s[s].next_shot {
            self.fire(s, l, Aim::Unit(ti), now);
            let iv = self.fire_interval(s, skill);
            self.ranged.s[s].next_shot = now + iv;
        }
        if self.ai[s].kind == Kind::Dead {
            return Some(true);
        }
        self.ai[s].path.clear();
        self.ai[s].path_i = 0;
        self.ai[s].wake = self.ranged.s[s].next_shot.max(now + 1);
        Some(true)
    }

    fn fire_interval(&self, s: usize, skill: i32) -> u64 {
        let u = &self.world.units[self.ai[s].uidx];
        let agi = u.phys_attrs.iter().find(|(n, _)| n == "AGILITY").map(|(_, v)| *v).unwrap_or(1000);
        let base = 600 / (10 + skill.clamp(0, 20));
        ((base * 1000) / (500 + agi / 2)).max(12) as u64
    }

    // ------------------------------------------------------------------ firing

    fn fire(&mut self, s: usize, l: u16, aim: Aim, now: u64) {
        let lt = self.ranged.launchers[l as usize].clone();
        let training = matches!(aim, Aim::Target(_));
        let Some(round) = self.take_round(s, &lt.class, training) else {
            self.ranged.s[s].ammo = 0;
            return;
        };
        self.ranged.s[s].ammo = (self.ranged.s[s].ammo - 1).max(0);
        let Some(at) = self.ammo_t(&round.subtype) else { return };
        let am = self.ranged.ammo[at as usize].clone();
        let mut r = Rng::for_unit(self.seed, s, now, 0xA770);
        let (skill, archery) = {
            let u = &self.world.units[self.ai[s].uidx];
            (u.skill(&lt.skill), u.skill("RANGED_COMBAT"))
        };
        let me = self.upos(s);
        let (tp, size) = match aim {
            Aim::Unit(t) => {
                let u = &self.world.units[self.ai[t].uidx];
                (self.upos(t), if u.body_size > 0 { u.body_size } else { 60_000 })
            }
            Aim::Target(k) => (self.ranged.targets[k].pos, 60_000),
        };
        let dist = fire_dist(me, tp);
        // Experience per shot fired (wiki Marksdwarf: ~30 at live enemies, ~7.5 at targets).
        let shots = self.ranged.s[s].shots;
        self.ranged.s[s].shots = shots.wrapping_add(1);
        if training {
            let xp = 7 + (shots & 1) as i32;
            self.add_xp(s, &lt.skill, xp);
            self.add_xp(s, "RANGED_COMBAT", xp / 2);
        } else {
            self.add_xp(s, &lt.skill, 30);
            self.add_xp(s, "RANGED_COMBAT", 15);
        }
        let size_k = (size.max(100) as f32 / 60_000.0).cbrt();
        let p = ((50 + 4 * skill + archery - 2 * dist) as f32 * size_k).clamp(5.0, 95.0) as u32;
        let hit = r.below(100) < p;
        let mname = self.world.material(round.mat).map(|m| m.name.clone()).unwrap_or_default();
        let what = format!("the flying {}", format!("{mname} {}", am.name).trim());
        match aim {
            Aim::Target(_) => {
                self.ranged.stats.practice_shots += 1;
                if hit {
                    self.ranged.stats.practice_hits += 1;
                    self.land_round(round, tp, None, 10, &mut r);
                } else {
                    self.land_round(round, me, Some(tp), 10, &mut r);
                }
            }
            Aim::Unit(t) => {
                self.ranged.stats.shots += 1;
                if self.combat.f[s].role == Role::Hostile {
                    self.ranged.stats.hostile_shots += 1;
                } else {
                    self.ranged.stats.fort_shots += 1;
                }
                let dn = self.uname(t);
                if !hit {
                    self.ranged.stats.misses += 1;
                    self.combat.log_line(format!("{} misses {}!", cap(&what), dn));
                    self.land_round(round, me, Some(tp), 10, &mut r);
                    self.after_attacked(t, s, now);
                    return;
                }
                // Dodge and block.
                if self.can_fight(t, now) {
                    let (dodge, shield_sk, agi, sex) = {
                        let u = &self.world.units[self.ai[t].uidx];
                        let agi = u.phys_attrs.iter().find(|(n, _)| n == "AGILITY").map(|(_, v)| *v).unwrap_or(1000);
                        (u.skill("DODGING"), u.skill("SHIELD"), agi, u.sex)
                    };
                    let pd = (5 + 3 * dodge - 2 * skill + (agi - 1000) / 100).clamp(0, 40) as u32;
                    if r.below(100) < pd {
                        self.ranged.stats.dodged += 1;
                        self.add_xp(t, "DODGING", 10);
                        self.combat.log_line(format!("{} jumps away from {}!", cap(&dn), what));
                        self.land_round(round, me, Some(tp), 10, &mut r);
                        self.after_attacked(t, s, now);
                        return;
                    }
                    if let Some(sh) = self.shield_item(t) {
                        let pb = (20 + 4 * shield_sk - 2 * skill).clamp(5, 60) as u32;
                        if r.below(100) < pb {
                            self.ranged.stats.blocked += 1;
                            self.add_xp(t, "SHIELD", 10);
                            let sdesc = self.world.items[sh].desc.clone();
                            let sname = if sdesc.is_empty() { "shield".to_string() } else { sdesc };
                            let pron = match sex {
                                0 => "her",
                                1 => "his",
                                _ => "its",
                            };
                            self.combat.log_line(format!("{} blocks {} with {pron} {sname}!", cap(&dn), what));
                            self.land_round(round, tp, None, 5, &mut r);
                            self.after_attacked(t, s, now);
                            return;
                        }
                    }
                }
                // The projectile strikes: momentum from the launcher and the round's weight.
                self.ranged.stats.hits += 1;
                self.ensure_body(s);
                self.ensure_body(t);
                let wmech = self.mech_of_mat(round.mat);
                let weight = am.size * wmech.dens.max(1.0) / 100_000.0;
                let m = (lt.force / 20.0).min(lt.maxvel / 10.0 * weight) * (80 + r.below(21)) as f32 / 100.0;
                let deflected0 = self.combat.stats.deflected;
                let killed = self.strike(
                    s,
                    t,
                    now,
                    &mut r,
                    Blow {
                        atk: am.atk.clone(),
                        armed: true,
                        wmech,
                        wsize: am.size,
                        wquality: round.quality.min(5) as f32,
                        wdesc: None,
                        skill,
                        an: what,
                        dn,
                        momentum: Some(m),
                    },
                );
                if self.combat.stats.deflected > deflected0 {
                    self.ranged.stats.deflected += 1;
                }
                if killed {
                    self.ranged.stats.kills += 1;
                    if self.combat.f[s].role != Role::Hostile {
                        self.ranged.stats.fort_kills += 1;
                    }
                    self.combat.f[s].target = NONE;
                }
                self.land_round(round, tp, None, 33, &mut r);
            }
        }
    }

    fn shield_item(&self, s: usize) -> Option<usize> {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.iter().filter_map(|id| self.item_idx.get(id)).map(|&i| i as usize).find(|&i| !self.imeta[i].dead && self.world.items[i].kind == "SHIELD")
    }

    /// Put a fired round on the map: at `at`, or flying on from `from` past `at` (a miss) until
    /// it hits something or its flight range ends. It breaks with `break_pct` %.
    fn land_round(&mut self, mut round: Item, at: Pos, past: Option<Pos>, break_pct: u32, r: &mut Rng) {
        if r.below(100) < break_pct {
            self.ranged.stats.ammo_broken += 1;
            return;
        }
        let mut p = at;
        if let Some(t) = past {
            let (dx, dy, dz) = (t.x as i32 - at.x as i32, t.y as i32 - at.y as i32, t.z as i32 - at.z as i32);
            let n = dx.abs().max(dy.abs()).max(dz.abs()).max(1);
            let k = FLIGHT_RANGE as f32 / n as f32;
            let clampx = |v: i32, m: u16| v.clamp(0, m as i32 - 1) as i16;
            let end = Pos::new(
                clampx(at.x as i32 + (dx as f32 * k) as i32, self.world.dims.x),
                clampx(at.y as i32 + (dy as f32 * k) as i32, self.world.dims.y),
                clampx(at.z as i32 + (dz as f32 * k) as i32, self.world.dims.z),
            );
            let mut last = at;
            self.los_walk(at, end, FORTIFICATION_SKILL, Some(&mut last));
            p = last;
        }
        // Fall down to the first floor.
        for _ in 0..40 {
            let sh = self.world.shape(p);
            if sh.has_floor() || p.z == 0 {
                break;
            }
            if sh.is_solid() {
                p = p.offset(0, 0, 1);
                break;
            }
            p = p.offset(0, 0, -1);
        }
        let Some(tile) = self.world.idx(p) else { return };
        round.id = self.next_item_id;
        round.pos = p;
        round.stack = 1;
        round.container = -1;
        round.holder = -1;
        round.building = -1;
        round.flags = ItemFlags { on_ground: true, forbid: self.ranged.forbid_used, ..Default::default() };
        let id = round.id;
        let idx = self.push_item(round);
        self.pile_occ_add(tile as u32, 1);
        if self.haulable(idx) && !self.is_stored(idx) {
            self.haul_add(id);
        }
        self.ranged.stats.ammo_dropped += 1;
    }

    // ------------------------------------------------------------------ soldiers

    /// Free item for a soldier: not held, built in, reserved, forbidden or in a busy container.
    fn ranged_find_item(&self, s: usize, want: impl Fn(&Item) -> Option<i32>) -> Option<i32> {
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return None;
        }
        let up = self.nav.xyz(pos);
        let now = self.ticks;
        let mut best = (i32::MIN, i32::MAX, -1);
        for (i, it) in self.world.items.iter().enumerate() {
            let Some(score) = want(it) else { continue };
            let m = &self.imeta[i];
            let free = if it.building >= 0 {
                // Finished bolts still lying in the forge that made them (DF lets soldiers take them).
                let f = &it.flags;
                !m.dead && m.reserved == 0 && m.blocked_until <= now && it.holder < 0 && !f.forbid && !f.in_job && !f.construction && !f.in_building
                    && self.world.building(it.building).map(|b| b.kind == "Workshop" || b.kind == "Furnace").unwrap_or(false)
            } else {
                m.reserved == 0 && self.item_free(i, now)
            };
            if !free {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(t));
            if (score, -d) > (best.0, -best.1) {
                best = (score, d, it.id);
            }
        }
        (best.2 >= 0).then_some(best.2)
    }

    /// Hook in `soldier_think` before the uniform search: quiver, ammo, archery practice.
    pub(crate) fn ranged_soldier(&mut self, s: usize, now: u64) -> bool {
        self.ranged_sync();
        self.shooter_check(s, now);
        let Some(l) = self.ranged.s[s].launcher else {
            self.release_target(s);
            return false;
        };
        let class = self.ranged.launchers[l as usize].class.clone();
        let duty = self.combat.f[s].duty;
        if now >= self.ranged.s[s].equip_at {
            self.ranged.s[s].equip_at = now + EQUIP_PERIOD;
            self.ranged_stow(s);
            let n = self.count_ammo(s, &class);
            self.ranged.s[s].ammo = n;
            let quiver = self.quiver_of(s);
            let pick = if quiver.is_none() {
                self.ranged_find_item(s, |it| (it.kind == "QUIVER").then_some(it.quality as i32)).map(|id| (id, true))
            } else if n < self.squad_ammo_spec(s, duty == Duty::Train).1.min(QUIVER_WANT) {
                let training = duty == Duty::Train;
                let (spec, _) = self.squad_ammo_spec(s, training); // [military2] squad ammo settings
                let mut classes: Vec<(String, bool)> = Vec::new();
                for it in &self.world.items {
                    if it.kind == "AMMO" && !classes.iter().any(|(k, _)| *k == it.subtype) {
                        classes.push((it.subtype.clone(), false));
                    }
                }
                for e in classes.iter_mut() {
                    e.1 = self.ammo_t(&e.0).map(|a| self.ranged.ammo[a as usize].class == class).unwrap_or(false);
                }
                let ok: Vec<String> = classes.into_iter().filter(|e| e.1).map(|e| e.0).collect();
                let mats: Vec<bool> = self.world.materials.iter().map(|m| m.is_metal || m.token.starts_with("INORGANIC:")).collect();
                self.ranged_find_item(s, |it| {
                    if it.kind != "AMMO" || !ok.contains(&it.subtype) || !self.ammo_fits(&spec, it) {
                        return None;
                    }
                    let metal = mats.get(it.mat as usize).copied().unwrap_or(false);
                    Some(if metal != training { 2 } else { 1 })
                })
                .map(|id| (id, false))
            } else {
                None
            };
            if let Some((id, is_quiver)) = pick {
                self.release_target(s);
                if is_quiver {
                    self.ranged.stats.quivers_fetched += 1;
                } else {
                    self.ranged.stats.ammo_fetched += 1;
                }
                // Next check right after the pickup (stow it, maybe fetch more).
                self.ranged.s[s].equip_at = now + 1;
                self.ranged.s[s].check_at = 0;
                self.reserve(id);
                self.combat.f[s].sub = Sub::Equip(id);
                let g = self.item_goals(id);
                self.request_path(s, g, now);
                return true;
            }
        }
        if duty == Duty::Train && self.ranged.s[s].ammo > 0 && now >= self.ranged.s[s].no_practice_until {
            return self.ranged_practice(s, l, now);
        }
        self.release_target(s);
        false
    }

    /// Archery practice: walk to a shooting spot of a free target, then shoot at it.
    fn ranged_practice(&mut self, s: usize, l: u16, now: u64) -> bool {
        let k = match self.ranged.s[s].train {
            Some(k) => k,
            None => {
                let Some((k, spot)) = self.practice_spot(s, l) else { return false };
                self.ranged.targets[k].user = s as u32;
                self.ranged.s[s].train = Some(k);
                self.ranged.s[s].spot = spot;
                k
            }
        };
        let spot = self.ranged.s[s].spot;
        if self.ai[s].pos != spot {
            let ai = &self.ai[s];
            if ai.path_i < ai.path.len() && *ai.path.last().unwrap() == spot {
                self.step(s, now);
            } else if self.combat.f[s].repath_at > now && ai.path.is_empty() {
                // Could not get there: give the target up for a while.
                self.release_target(s);
                self.ranged.s[s].equip_at = now + EQUIP_PERIOD;
                return false;
            } else {
                self.request_path(s, Goals::one(spot), now);
            }
            return true;
        }
        if now >= self.ranged.s[s].next_shot {
            self.fire(s, l, Aim::Target(k), now);
            let skill = self.shooter_skill(s, l);
            let iv = self.fire_interval(s, skill);
            self.ranged.s[s].next_shot = now + iv;
            if self.ranged.s[s].ammo <= 0 {
                // Quiver empty: fetch more next time.
                self.ranged.s[s].equip_at = now;
                self.ranged.s[s].check_at = 0;
            }
        }
        self.ai[s].wake = self.ranged.s[s].next_shot.max(now + 1);
        true
    }

    fn practice_spot(&self, s: usize, l: u16) -> Option<(usize, u32)> {
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let skill = self.shooter_skill(s, l);
        for (k, t) in self.ranged.targets.iter().enumerate() {
            let busy = t.user != NONE && t.user != s as u32 && {
                let u = t.user as usize;
                u < self.ai.len() && self.ai[u].kind != Kind::Dead && self.ranged.s.get(u).map(|x| x.train == Some(k)).unwrap_or(false)
            };
            if busy {
                continue;
            }
            let (x1, y1, x2, y2, z) = t.zone;
            let mut best = (i32::MIN, NONE);
            for y in y1.max(0)..=y2.min(self.world.dims.y as i16 - 1) {
                for x in x1.max(0)..=x2.min(self.world.dims.x as i16 - 1) {
                    let p = Pos::new(x, y, z);
                    let Some(i) = self.world.idx(p) else { continue };
                    if !self.nav.walkable(i) || self.nav.comp_near(i) != c {
                        continue;
                    }
                    let d = fire_dist(p, t.pos);
                    // At least one tile between shooter and target; farthest spot up to 10 tiles.
                    if !(2..=10).contains(&d) || !self.line_of_fire(p, t.pos, skill.max(FORTIFICATION_SKILL)) {
                        continue;
                    }
                    if d > best.0 {
                        best = (d, i as u32);
                    }
                }
            }
            if best.1 != NONE {
                return Some((k, best.1));
            }
        }
        None
    }

    /// Hook in `combat_cancel`: drop the archery target (path failure, interrupted).
    pub(crate) fn ranged_cancel(&mut self, s: usize) {
        if s >= self.ranged.s.len() {
            return;
        }
        let now = self.ticks;
        if self.ranged.s[s].train.is_some() {
            self.ranged.s[s].no_practice_until = now + rf_core::TICKS_PER_DAY as u64;
        }
        self.release_target(s);
        // A quiver or ammo stack that could not be reached: ignore it for a day.
        if let Some(Sub::Equip(id)) = self.combat.f.get(s).map(|f| f.sub) {
            if let Some(&i) = self.item_idx.get(&id) {
                let it = &self.world.items[i as usize];
                if matches!(it.kind.as_str(), "QUIVER" | "AMMO") {
                    self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                }
            }
        }
    }

    // ------------------------------------------------------------------ invaders

    /// Hook in `spawn_siege`: a quarter of the invaders become bowmen with a bow, a quiver and
    /// 30-40 arrows of their weapon's metal (estimate; Tileshtulon's running siege has 13
    /// bowmen among 36 goblins).
    pub(crate) fn ranged_arm_siege(&mut self, members: &[u32]) {
        self.ranged_sync();
        let now = self.ticks;
        let leather = self.world.materials.iter().position(|m| m.token.ends_with(":LEATHER")).map(|i| self.world.materials[i].token.clone()).unwrap_or_else(|| "CREATURE:COW:LEATHER".into());
        for &m in members {
            let s = m as usize;
            let mut r = Rng::for_unit(self.seed, s, now, 0xB0B0);
            if !r.chance(1, 4) {
                continue;
            }
            let uidx = self.ai[s].uidx;
            let uid = self.world.units[uidx].id;
            // Replace the melee weapon by a bow of the same material.
            let inv = self.world.units[uidx].inventory.clone();
            let mut metal = "INORGANIC:IRON".to_string();
            for id in inv {
                if let Some(&i) = self.item_idx.get(&id) {
                    let it = &self.world.items[i as usize];
                    if it.kind == "WEAPON" {
                        metal = self.world.material(it.mat).map(|m| m.token.clone()).unwrap_or(metal);
                        self.destroy_item(id);
                    }
                }
            }
            self.give_item(uid, "WEAPON", "ITEM_WEAPON_BOW", &metal, 0);
            let Some(q) = self.give_item(uid, "QUIVER", "", &leather, 0) else { continue };
            if let Some(&qi) = self.item_idx.get(&q) {
                let it = &mut self.world.items[qi as usize];
                it.desc = format!("{} quiver", it.desc.trim_end_matches(" quiver").trim());
            }
            if let Some(a) = self.give_item(uid, "AMMO", "ITEM_AMMO_ARROWS", &metal, 0) {
                let n = 30 + r.below(11) as i32;
                if let Some(&ai_) = self.item_idx.get(&a) {
                    let it = &mut self.world.items[ai_ as usize];
                    it.stack = n;
                    it.desc = format!("{} [{n}]", it.desc);
                }
            }
            let rating = 2 + r.below(6) as i32;
            let u = &mut self.world.units[uidx];
            u.profession = "BOWMAN".into();
            u.custom_profession = "bowman".into();
            u.skills.retain(|k| !matches!(k.id.as_str(), "SPEAR" | "SWORD" | "MACE" | "AXE" | "WHIP"));
            u.skills.push(Skill { id: "BOW".into(), rating, xp: 0 });
            u.skills.push(Skill { id: "RANGED_COMBAT".into(), rating: rating / 2, xp: 0 });
            self.ranged_stow(s);
            self.ranged.s[s].check_at = 0;
            self.ranged.stats.bowmen_spawned += 1;
        }
    }

    // ------------------------------------------------------------------ public API

    /// Ranged combat counters (shots, hits, kills, ammo used, practice).
    pub fn ranged_stats(&self) -> RangedStats {
        self.ranged.stats.clone()
    }

    /// DF standing order "forbid used ammunition" (default on).
    pub fn set_forbid_used_ammo(&mut self, on: bool) {
        self.ranged_sync();
        self.ranged.forbid_used = on;
    }

    /// Rounds of ammunition a unit carries for its wielded launcher (0 without one).
    pub fn unit_ammo(&mut self, unit_id: i32) -> i32 {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return 0 };
        self.ranged_sync();
        match self.ranged_weapon(s) {
            Some((_, l)) => {
                let class = self.ranged.launchers[l as usize].class.clone();
                self.count_ammo(s, &class)
            }
            None => 0,
        }
    }
}

/// Update the `[n]` stack count in a DF item description (`iron bolts [25]` -> `[24]`).
fn restack_desc(desc: &str, n: i32) -> String {
    if let (Some(a), Some(b)) = (desc.rfind('['), desc.rfind(']')) {
        if a < b && desc[a + 1..b].chars().all(|c| c.is_ascii_digit()) {
            if n <= 1 {
                // "iron bolts [1]" -> "iron bolt"
                let head = desc[..a].trim_end();
                let tail = &desc[b + 1..];
                let head = head.strip_suffix('s').unwrap_or(head);
                return format!("{head}{tail}");
            }
            return format!("{}[{n}]{}", &desc[..a], &desc[b + 1..]);
        }
    }
    desc.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restack_updates_counts() {
        assert_eq!(restack_desc("iron bolts [25]", 24), "iron bolts [24]");
        assert_eq!(restack_desc("*iron bolts [25]*", 1), "*iron bolt*");
        assert_eq!(restack_desc("iron bolt", 1), "iron bolt");
    }

    #[test]
    fn projectile_momentum_matches_wiki_threshold() {
        // Wiki: bolts reach momentum 50 (speed 20) above density 1666.
        let f = |dens: f32| (1000.0f32 / 20.0).min(200.0 / 10.0 * 150.0 * dens / 100_000.0);
        assert!((f(1667.0) - 50.0).abs() < 0.1);
        assert!(f(7850.0) == 50.0);
        assert!(f(600.0) < 20.0);
    }

    #[test]
    fn fire_dist_counts_height_double() {
        assert_eq!(fire_dist(Pos::new(0, 0, 0), Pos::new(5, 3, 0)), 5);
        assert_eq!(fire_dist(Pos::new(0, 0, 0), Pos::new(5, 3, 2)), 9);
    }
}
