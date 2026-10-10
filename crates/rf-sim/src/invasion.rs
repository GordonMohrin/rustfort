//! Invasions (round 3): sieges by hostile civilizations.
//!
//! # DF rules implemented (wiki: Siege, Invasion, Ambush)
//! - Invasions are checked when a season starts (DF checks at season changes), at most one
//!   siege at a time. Sieges need a population of at least 80 (DF v50; Gordon observed sieges
//!   from 80 citizens on). Invasions can be switched off ([`Sim::set_invasions`]); default on.
//! - The invaders (goblins by default) arrive together at a map edge connected to the fort
//!   ("A vile force of darkness has arrived!"), armed with their civilization's weapons
//!   (whips, battle axes, spears, maces, short swords) and copper/bronze/iron mail shirts,
//!   helms and leggings, march on the fort and attack everybody they meet.
//! - They retreat when half of them are dead or after two months, walking back to their
//!   map edge and leaving the map.
//!
//! # Estimates
//! - Chance per season start once the threshold is met: 1 in 3; siege size 10 + population/8
//!   (at most 60); equipment shares (copper 45 %, bronze 25 %, iron 30 %; mail shirt 70 %,
//!   helm 60 %, leggings 40 %); invader skills 2-7 in the weapon, attributes 750-1250.

use crate::combat::Role;
use crate::path::Goals;
use crate::rng::Rng;
use crate::{AnnouncementKind, Kind, Sim, NONE};
use rf_core::{tile_flags, Pos, Skill, Unit, UnitFlags, TICKS_PER_MONTH};

pub(crate) struct Siege {
    pub members: Vec<u32>,
    pub start: u64,
    pub fleeing: bool,
    pub race: String,
}

pub(crate) struct Invasions {
    pub enabled: bool,
    pub sieges: Vec<Siege>,
    pub last_season: i32,
}

impl Invasions {
    pub fn new() -> Self {
        Invasions { enabled: true, sieges: Vec::new(), last_season: -1 }
    }
}

/// Population needed for sieges (DF v50; Gordon's observation).
pub const SIEGE_POPULATION: usize = 80;

const GOBLIN_WEAPONS: &[(&str, &str)] = &[
    ("ITEM_WEAPON_SPEAR", "spearman"),
    ("ITEM_WEAPON_SWORD_SHORT", "swordsman"),
    ("ITEM_WEAPON_MACE", "maceman"),
    ("ITEM_WEAPON_AXE_BATTLE", "axeman"),
    ("ITEM_WEAPON_WHIP", "lasher"),
];

impl Sim {
    /// Enable or disable invasions (sieges). Default: enabled, like DF.
    pub fn set_invasions(&mut self, on: bool) {
        self.combat.inv.enabled = on;
    }

    pub fn invasions_enabled(&self) -> bool {
        self.combat.inv.enabled
    }

    /// True while invaders are on the map.
    pub fn siege_active(&self) -> bool {
        !self.combat.inv.sieges.is_empty()
    }

    /// Debug/UI: start a goblin siege right now (size from the population). Returns the number
    /// of invaders that arrived (0 if no map edge connects to the fort).
    pub fn force_siege(&mut self) -> usize {
        let pop = self.world.citizens().count();
        let n = (10 + pop / 8).min(60);
        self.spawn_siege("GOBLIN", n)
    }

    /// Spawn `n` invaders of `race` at a map edge connected to the fort.
    pub fn spawn_siege(&mut self, race: &str, n: usize) -> usize {
        let now = self.ticks;
        self.combat_sync_units();
        let tiles = self.edge_tiles();
        if tiles.is_empty() || n == 0 {
            self.announce(now, "The invaders could not find a way to the fortress.".into(), None, AnnouncementKind::Warning);
            return 0;
        }
        let mut r = Rng::for_unit(self.seed, 0, now, 0x5E1E);
        let start = tiles[r.below(tiles.len() as u32) as usize];
        let sp = self.nav.xyz(start as usize);
        let mut near = tiles.clone();
        near.sort_by_key(|&t| (crate::manhattan(sp, self.nav.xyz(t as usize)), t));
        let raw = self.world.creature_raws.iter().find(|c| c.token == race);
        let (body_size, walk) = raw
            .and_then(|c| c.castes.first())
            .map(|c| (if c.body_size > 0 { c.body_size } else { 60_000 }, if c.walk_speed > 0 { c.walk_speed } else { 900 }))
            .unwrap_or((60_000, 900));
        let civ = self.world.units.iter().filter(|u| u.flags.hostile && u.invader_civ >= 0).map(|u| u.invader_civ).next().unwrap_or(9_999);
        let mut next_id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let first = self.world.units.len();
        let mut kit = Vec::new();
        for k in 0..n {
            let tile = near[k % near.len()];
            let p = self.world.pos_of(tile as usize);
            let (wtok, prof) = GOBLIN_WEAPONS[r.below(GOBLIN_WEAPONS.len() as u32) as usize];
            let metal = match r.below(100) {
                0..=44 => "INORGANIC:COPPER",
                45..=69 => "INORGANIC:BRONZE",
                _ => "INORGANIC:IRON",
            };
            let skill = match wtok {
                "ITEM_WEAPON_SPEAR" => "SPEAR",
                "ITEM_WEAPON_SWORD_SHORT" => "SWORD",
                "ITEM_WEAPON_MACE" => "MACE",
                "ITEM_WEAPON_AXE_BATTLE" => "AXE",
                _ => "WHIP",
            };
            let attr = |r: &mut Rng| 750 + r.below(501) as i32;
            let unit = Unit {
                id: next_id,
                race: race.into(),
                caste: if r.chance(1, 2) { "MALE".into() } else { "FEMALE".into() },
                name: String::new(),
                profession: prof.to_ascii_uppercase(),
                custom_profession: prof.into(),
                pos: p,
                flags: UnitFlags { hostile: true, ..Default::default() },
                civ_id: civ,
                hist_fig_id: -1,
                sex: r.below(2) as i8,
                age: 20.0 + r.below(30) as f32,
                skills: vec![
                    Skill { id: skill.into(), rating: 2 + r.below(6) as i32, xp: 0 },
                    Skill { id: "DODGING".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                    Skill { id: "ARMOR".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                    Skill { id: "MELEE_COMBAT".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                ],
                phys_attrs: vec![("STRENGTH".into(), attr(&mut r)), ("AGILITY".into(), attr(&mut r)), ("TOUGHNESS".into(), attr(&mut r))],
                ment_attrs: vec![("WILLPOWER".into(), attr(&mut r))],
                squad_id: -1,
                squad_position: -1,
                body_size,
                walk_speed: walk,
                invader_civ: civ,
                ..Default::default()
            };
            let mail = r.chance(70, 100);
            let helm = r.chance(60, 100);
            let legs = r.chance(40, 100);
            kit.push((next_id, wtok, metal, mail, helm, legs, tile));
            self.world.units.push(unit);
            next_id += 1;
        }
        self.init_units();
        self.combat_sync_units();
        let sid = self.combat.inv.sieges.len() as u16 + 1;
        let mut members = Vec::new();
        for (uid, wtok, metal, mail, helm, legs, tile) in kit {
            self.give_item(uid, "WEAPON", wtok, metal, 0);
            if mail {
                self.give_item(uid, "ARMOR", "ITEM_ARMOR_MAIL_SHIRT", metal, 0);
            }
            if helm {
                self.give_item(uid, "HELM", "ITEM_HELM_HELM", metal, 0);
            }
            if legs {
                self.give_item(uid, "PANTS", "ITEM_PANTS_LEGGINGS", metal, 0);
            }
            let s = self.unit_slot[&uid];
            let f = &mut self.combat.f[s];
            f.siege = sid;
            f.home = tile;
            f.role = Role::Hostile;
            members.push(s as u32);
        }
        let _ = first;
        self.ranged_arm_siege(&members); // [ranged] bowmen
        self.combat.inv.sieges.push(Siege { members: members.clone(), start: now, fleeing: false, race: race.into() });
        self.combat.stats.sieges += 1;
        self.combat.stats.invaders_spawned += members.len() as u64;
        self.rebuild_fort_list();
        let p = self.world.pos_of(start as usize);
        self.announce(now, "A vile force of darkness has arrived!".into(), Some(p), AnnouncementKind::Warning);
        self.combat.log_line(format!("A vile force of darkness has arrived! ({} {}s)", members.len(), race.to_ascii_lowercase()));
        members.len()
    }

    /// Walkable map-edge tiles in the walk component with the most citizens (outside first).
    fn edge_tiles(&self) -> Vec<u32> {
        let mut counts: Vec<(u32, u32)> = Vec::new();
        for ai in &self.ai {
            if ai.kind != Kind::Worker {
                continue;
            }
            let c = self.nav.comp_near(ai.pos as usize);
            if c == 0 {
                continue;
            }
            match counts.iter_mut().find(|e| e.0 == c) {
                Some(e) => e.1 += 1,
                None => counts.push((c, 1)),
            }
        }
        let Some(&(c, _)) = counts.iter().max_by_key(|e| (e.1, std::cmp::Reverse(e.0))) else { return vec![] };
        let (dx, dy, dz) = (self.nav.dx, self.nav.dy, self.nav.dz);
        let mut outside = Vec::new();
        let mut any = Vec::new();
        for z in 0..dz {
            for y in 0..dy {
                for x in 0..dx {
                    if !(x == 0 || y == 0 || x == dx - 1 || y == dy - 1) {
                        continue;
                    }
                    let i = self.nav.idx(x, y, z);
                    if !self.nav.walkable(i) || self.nav.comp_root(i) != c {
                        continue;
                    }
                    if self.world.tiles[i].flags & tile_flags::OUTSIDE != 0 {
                        outside.push(i as u32);
                    } else {
                        any.push(i as u32);
                    }
                }
            }
        }
        if outside.is_empty() {
            any
        } else {
            outside
        }
    }

    /// Daily: season-start siege check, retreat and end of sieges, monthly military refresh.
    pub(crate) fn invasion_daily(&mut self, now: u64) {
        self.military_monthly();
        let season = self.world.calendar.month() / 3;
        let inv = &mut self.combat.inv;
        if season != inv.last_season {
            let first = inv.last_season < 0;
            inv.last_season = season;
            if !first && inv.enabled && inv.sieges.is_empty() && self.world.citizens().count() >= SIEGE_POPULATION {
                let mut r = Rng::for_unit(self.seed, 1, now, 0x51E6);
                if r.chance(1, 3) {
                    self.force_siege();
                }
            }
        }
        // Retreat and end.
        let mut k = 0;
        while k < self.combat.inv.sieges.len() {
            let (alive, total, fleeing, start) = {
                let sg = &self.combat.inv.sieges[k];
                let alive = sg.members.iter().filter(|&&s| self.ai[s as usize].kind != Kind::Dead).count();
                (alive, sg.members.len(), sg.fleeing, sg.start)
            };
            if alive == 0 {
                let sg = self.combat.inv.sieges.remove(k);
                self.announce(now, format!("The {} invasion is over.", sg.race.to_ascii_lowercase()), None, AnnouncementKind::Info);
                continue;
            }
            if !fleeing && (alive * 2 <= total || now >= start + 2 * TICKS_PER_MONTH as u64) {
                self.combat.inv.sieges[k].fleeing = true;
                let race = self.combat.inv.sieges[k].race.to_ascii_lowercase();
                self.announce(now, format!("The {race}s are retreating!"), None, AnnouncementKind::Info);
                self.combat.log_line(format!("The {race}s are retreating!"));
                let members = self.combat.inv.sieges[k].members.clone();
                for s in members {
                    let s = s as usize;
                    if self.ai[s].kind == Kind::Dead {
                        continue;
                    }
                    let f = &mut self.combat.f[s];
                    f.role = Role::Leave;
                    f.target = NONE;
                    self.engage_now(s, now);
                }
            }
            k += 1;
        }
    }

    /// Invaders walking off the map.
    pub(crate) fn leave_think(&mut self, s: usize, now: u64) {
        let home = self.combat.f[s].home;
        let here = self.ai[s].pos;
        let p = self.upos(s);
        let edge = p.x == 0 || p.y == 0 || p.x as usize == self.nav.dx - 1 || p.y as usize == self.nav.dy - 1;
        if here == home || edge || home == NONE {
            self.leave_map(s);
            return;
        }
        let ai = &self.ai[s];
        if ai.path_i < ai.path.len() && *ai.path.last().unwrap() == home {
            self.step(s, now);
        } else if self.combat.f[s].repath_at > now && ai.path.is_empty() {
            // Recent path failure: give up and vanish (DF units leaving through any edge).
            self.leave_map(s);
        } else {
            self.request_path(s, Goals::one(home), now);
        }
    }

    fn leave_map(&mut self, s: usize) {
        let uidx = self.ai[s].uidx;
        let inv = std::mem::take(&mut self.world.units[uidx].inventory);
        for id in inv {
            self.destroy_item(id);
        }
        self.world.units[uidx].flags.inactive = true;
        self.ai[s].kind = Kind::Dead;
        self.ai[s].wake = u64::MAX;
        let f = &mut self.combat.f[s];
        f.role = Role::None;
        if let Ok(p) = self.combat.hostiles.binary_search(&(s as u32)) {
            self.combat.hostiles.remove(p);
        }
        self.combat.stats.invaders_left += 1;
        let _ = Pos::default();
    }
}
