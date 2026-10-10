//! Invasions (round 3): sieges by hostile civilizations.
//!
//! # DF rules implemented (wiki: Siege, Invasion, Ambush)
//! - Invasions are checked when a season starts (DF checks at season changes), at most one
//!   siege at a time. Sieges need a population of at least 80 (DF v50; Gordon observed sieges
//!   from 80 citizens on). Invasions can be switched off ([`Sim::set_invasions`]); default on.
//! - [r7] **Siege size by wealth** (wiki Siege + Difficulty, Normal): the fort reaches an
//!   invasion tier when it meets the tier's population "and one of production or trade"
//!   (created or exported wealth): population 20/50/80/110/140, created wealth
//!   5 000/25 000/100 000/200 000/300 000, exported wealth 500/2 500/10 000/20 000/30 000.
//!   Sieges start at the third tier (Town: 80 citizens, so the Pop >= 80 rule holds) with 10
//!   regulars, then 40 regulars + 5 monsters (City) and 120 regulars + 40 monsters
//!   (Metropolis, also the default caps of 120 soldiers and 40 monsters).
//! - The invaders (goblins by default) arrive together at a map edge connected to the fort
//!   ("A vile force of darkness has arrived!"), armed with their civilization's weapons
//!   (whips, battle axes, spears, maces, short swords) and copper/bronze/iron mail shirts,
//!   helms and leggings, march on the fort and attack everybody they meet.
//! - They retreat when half of them are dead or after two months, walking back to their
//!   map edge and leaving the map.
//!
//! # Estimates
//! - Chance per season start once the threshold is met: 1 in 3. [r7] Created wealth is the
//!   current value of the fort's items (DF counts the value of everything the fort made);
//!   exported wealth is the value of the goods sold to caravans in this session. Siege
//!   monsters (trolls, beak dogs) are not spawned yet. Equipment shares (copper 45 %, bronze 25 %, iron 30 %; mail shirt 70 %,
//!   helm 60 %, leggings 40 %); invader skills 2-7 in the weapon, attributes 750-1250.

use crate::combat::Role;
use crate::path::Goals;
use crate::{AnnouncementKind, Kind, Sim, NONE};
use rf_core::{tile_flags, Pos, TICKS_PER_MONTH};

/// A raid group on the map (siege, ambush, thief or snatcher; see `raids.rs`).
pub(crate) struct Siege {
    pub members: Vec<u32>,
    pub start: u64,
    pub fleeing: bool,
    pub race: String,
    /// [military2] Stable id (`Fighter::siege`).
    pub id: u16,
    pub kind: crate::raids::RaidKind,
    /// [military2] Camp tile (arrival point) and the tick the siegers charge.
    pub camp: u32,
    pub charge_at: u64,
    pub charged: bool,
    /// [military2] Slot of the leader (best weapon skill), `NONE` if none.
    pub leader: u32,
    /// [military2] Spotted (hidden raids start unrevealed).
    pub revealed: bool,
}

pub(crate) struct Invasions {
    pub enabled: bool,
    pub sieges: Vec<Siege>,
    pub last_season: i32,
    /// [military2] Last raid id handed out.
    pub next_id: u16,
    /// [military2] Slots of hidden raiders, ascending.
    pub hidden: Vec<u32>,
    /// [military2] Scheduled visits (tick, kind): thieves and snatchers.
    pub pending: Vec<(u64, crate::raids::RaidKind)>,
}

impl Invasions {
    pub fn new() -> Self {
        Invasions { enabled: true, sieges: Vec::new(), last_season: -1, next_id: 0, hidden: Vec::new(), pending: Vec::new() }
    }
}

/// Population needed for sieges (DF v50; Gordon's observation).
pub const SIEGE_POPULATION: usize = 80;

/// [r7] Invasion tier thresholds, Normal difficulty (wiki Difficulty): population, created
/// wealth and exported wealth.
pub const TIER_POPULATION: [usize; 5] = [20, 50, 80, 110, 140];
pub const TIER_CREATED: [i64; 5] = [5_000, 25_000, 100_000, 200_000, 300_000];
pub const TIER_EXPORTED: [i64; 5] = [500, 2_500, 10_000, 20_000, 30_000];
/// [r7] Siege force per tier (regulars, monsters), wiki Siege; tiers 1 and 2 send no sieges.
pub const SIEGE_FORCE: [(usize, usize); 5] = [(0, 0), (0, 0), (10, 0), (40, 5), (120, 40)];

/// [r7] Highest invasion tier (1-5) the fort meets: its population "and one of production or
/// trade". 0 = none.
pub fn invasion_tier(pop: usize, created: i64, exported: i64) -> usize {
    (0..5).rev().find(|&k| pop >= TIER_POPULATION[k] && (created >= TIER_CREATED[k] || exported >= TIER_EXPORTED[k])).map(|k| k + 1).unwrap_or(0)
}

/// [r7] Siege force (regulars, monsters) for a fort, `None` if it is not big or rich enough.
pub fn siege_force(pop: usize, created: i64, exported: i64) -> Option<(usize, usize)> {
    let t = invasion_tier(pop, created, exported);
    (t >= 3).then(|| SIEGE_FORCE[t - 1])
}

pub(crate) const GOBLIN_WEAPONS: &[(&str, &str)] = &[
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

    /// Debug/UI: start a goblin siege right now (size from the fort's invasion tier, at least
    /// the Town force of 10). Returns the number of invaders that arrived (0 if no map edge
    /// connects to the fort).
    pub fn force_siege(&mut self) -> usize {
        let n = self.siege_size().unwrap_or(SIEGE_FORCE[2].0);
        self.spawn_siege("GOBLIN", n)
    }

    /// [r7] Exported wealth: value of the goods the fort sold to caravans.
    pub fn exported_wealth(&self) -> i64 {
        self.trade.rep.trades.iter().map(|t| t.1.max(0)).sum()
    }

    /// [r7] Created wealth (approximation: current value of the fort's items).
    pub fn created_wealth(&self) -> i64 {
        self.fort_wealth()
    }

    /// [r7] Number of regulars a siege would bring now (`None`: below the siege tier).
    pub fn siege_size(&self) -> Option<usize> {
        let pop = self.world.citizens().count();
        siege_force(pop, self.created_wealth(), self.exported_wealth()).map(|f| f.0)
    }

    /// Spawn `n` invaders of `race` at a map edge connected to the fort. They charge at once
    /// (natural sieges camp first, see `raids.rs`).
    /// No camp and no leader rule (the round-3 behavior, kept for tests and debugging).
    pub fn spawn_siege(&mut self, race: &str, n: usize) -> usize {
        let k = self.spawn_raid(crate::raids::RaidKind::Siege, race, n, 0, 0); // [military2]
        if k > 0 {
            if let Some(g) = self.combat.inv.sieges.last_mut() {
                g.leader = NONE;
            }
        }
        k
    }

    /// Walkable map-edge tiles in the walk component with the most citizens (outside first).
    pub(crate) fn edge_tiles(&self) -> Vec<u32> {
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
        let edges = if outside.is_empty() { any } else { outside };
        if edges.is_empty() {
            return self.defense_sealed_edges(); // [defense] sealed fort: they still arrive outside
        }
        edges
    }

    /// Daily: season-start invasion check, visits, retreat and end of raids, monthly military
    /// refresh.
    pub(crate) fn invasion_daily(&mut self, now: u64) {
        use crate::raids::RaidKind;
        self.military_monthly();
        let season = self.world.calendar.month() / 3;
        let inv = &mut self.combat.inv;
        if season != inv.last_season {
            let first = inv.last_season < 0;
            inv.last_season = season;
            if !first && inv.enabled {
                // [military2] Sieges (25 %) or ambushes, plus thieves and snatchers.
                self.raids_season(now);
            }
        }
        if self.combat.inv.enabled {
            self.raids_pending(now); // [military2]
        }
        // Retreat and end.
        let mut k = 0;
        while k < self.combat.inv.sieges.len() {
            let (alive, total, fleeing, start, kind, leader) = {
                let sg = &self.combat.inv.sieges[k];
                let alive = sg.members.iter().filter(|&&s| self.ai[s as usize].kind != Kind::Dead).count();
                (alive, sg.members.len(), sg.fleeing, sg.start, sg.kind, sg.leader)
            };
            if alive == 0 {
                let sg = self.combat.inv.sieges.remove(k);
                if matches!(sg.kind, RaidKind::Siege | RaidKind::Ambush) && sg.revealed {
                    self.announce(now, format!("The {} invasion is over.", sg.race.to_ascii_lowercase()), None, AnnouncementKind::Info);
                }
                continue;
            }
            // [military2] Thieves and snatchers leave by themselves (at the latest after a month).
            if matches!(kind, RaidKind::Thief | RaidKind::Snatcher) {
                if !fleeing && now >= start + TICKS_PER_MONTH as u64 {
                    self.combat.inv.sieges[k].fleeing = true;
                    let members = self.combat.inv.sieges[k].members.clone();
                    for s in members {
                        let s = s as usize;
                        if self.ai[s].kind != Kind::Dead {
                            self.combat.f[s].role = Role::Leave;
                            self.combat.f[s].target = NONE;
                            self.engage_now(s, now);
                        }
                    }
                }
                k += 1;
                continue;
            }
            // [military2] A siege also breaks when its leader dies (estimate, see raids.rs).
            let leader_dead = kind == RaidKind::Siege && leader != NONE && self.ai[leader as usize].kind == Kind::Dead && !self.world.units[self.ai[leader as usize].uidx].flags.inactive;
            if !fleeing && (alive * 2 <= total || now >= start + 2 * TICKS_PER_MONTH as u64 || leader_dead) {
                self.combat.inv.sieges[k].fleeing = true;
                if leader_dead {
                    self.combat.stats.leaders_killed += 1;
                    self.combat.log_line("The invaders' leader has been slain!".into());
                }
                let race = self.combat.inv.sieges[k].race.to_ascii_lowercase();
                if self.combat.inv.sieges[k].revealed {
                    self.announce(now, format!("The {race}s are retreating!"), None, AnnouncementKind::Info);
                }
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

    pub(crate) fn leave_map(&mut self, s: usize) {
        let now = self.ticks;
        self.raid_left_map(s, now); // [military2] loot and kidnapped children leave too
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

#[cfg(test)]
mod r7_tests {
    use super::*;

    #[test]
    fn siege_tiers_follow_population_and_wealth() {
        // Below 80 citizens: no siege however rich.
        assert_eq!(siege_force(79, 10_000_000, 1_000_000), None);
        // 80 citizens but poor (no production, no trade): no siege.
        assert_eq!(siege_force(89, 50_000, 5_000), None);
        // Town: 80 citizens and 100k created or 10k exported -> 10 regulars.
        assert_eq!(siege_force(89, 100_000, 0), Some((10, 0)));
        assert_eq!(siege_force(89, 0, 10_000), Some((10, 0)));
        // A rich fort of 89 stays at the Town tier (population decides too).
        assert_eq!(siege_force(89, 5_000_000, 0), Some((10, 0)));
        // City and Metropolis.
        assert_eq!(siege_force(110, 200_000, 0), Some((40, 5)));
        assert_eq!(siege_force(139, 250_000, 25_000), Some((40, 5)));
        assert_eq!(siege_force(140, 300_000, 0), Some((120, 40)));
        assert_eq!(siege_force(200, 0, 30_000), Some((120, 40)));
        assert_eq!(invasion_tier(20, 5_000, 0), 1);
        assert_eq!(invasion_tier(19, 5_000_000, 0), 0);
    }
}
