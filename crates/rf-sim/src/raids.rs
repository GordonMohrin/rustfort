//! [military2] Raids (round 8): ambushes, thieves, snatchers, siege camps, siege monsters and the
//! retreat rules. The siege itself (spawning, gear, wealth tiers) lives in `invasion.rs`.
//!
//! # DF rules implemented (wiki v50: Invasion, Ambush, Thief, Snatcher, Siege, Troll, Beak dog)
//! - **Sieges vs ambushes**: on normal difficulty a quarter of the invasions are sieges, the
//!   rest are ambushes (smaller, hidden squads). Sieges still need the siege tier (Town: 80
//!   citizens and 100k created or 10k exported wealth); ambushes come from the first tier on.
//! - **Ambushes**: the squads arrive **hidden** (sneaking) at a map edge and hunt for dwarves.
//!   Hidden creatures are invisible: civilians do not flee, soldiers do not engage, and they are
//!   not listed by [`Sim::hostiles`]. A citizen who comes close spots them (observer skill
//!   against the ambusher skill), or they reveal themselves by attacking: the whole group is
//!   revealed with DF's announcement "An ambush! Curse them!" and fights like invaders.
//! - **Thieves** (raws: kobolds have `ITEM_THIEF`): a hidden kobold sneaks to a valuable item,
//!   grabs it and runs for the map edge; the item is lost when it escapes. Spotted:
//!   "Thief! Protect the hoard from skulking filth!" and the thief flees. Killed thieves drop the
//!   loot.
//! - **Snatchers** (raws: goblins have `BABYSNATCHER`): a hidden goblin sneaks to a child, grabs
//!   it and carries it off the map; the child is lost. Spotted: "Snatcher! Protect the
//!   children!". A snatcher killed or knocked out drops the child.
//! - **Siege camps**: siegers gather at their arrival point and wait before they charge; while
//!   camping they only attack what comes close. Goblin sieges bring **monsters** from the City
//!   tier on (wiki Siege: +5 at City, +40 at Metropolis): **trolls** and **beak dogs**, with the
//!   bodies from the save's creature raws.
//! - **Retreat**: sieges and ambushes retreat when half of the force is dead or after two
//!   months; a siege also breaks when its **leader** dies. Thieves and snatchers leave on their
//!   own.
//!
//! # Estimates
//! - Invasion check at each season start, 1 in 3 (as before); then 1 in 4 a siege (if the siege
//!   tier is met), else an ambush. Ambush size: 1 squad from the first tier, 2 from the Town
//!   tier, 3 at Metropolis; 2-5 goblins per squad.
//! - Thief visits: 1 in 2 per season if a kobold civilization exists; snatchers: 1 in 3 per
//!   season with a goblin civilization at war and children in the fort; they arrive on a random
//!   day of the season.
//! - Spotting: within `3 + observer/3 - ambusher/4` tiles (at least 1, at most 8; soldiers on
//!   duty +3), same z +-1, same walk component, 1 in 2 per 10-tick check.
//! - Camp time: goblins 600-2400 ticks (gathering), other races 2-7 days. Campers attack fort
//!   units within 8 tiles. Monster mix 1:1 trolls and beak dogs; beak dogs walk beside the
//!   goblins (riding is not simulated). Leader = the member with the best weapon skill.
//! - Thieves prefer small valuables (crafts, goblets, instruments, gems, coins), else anything of
//!   value; snatchers take children and babies. Thieves and snatchers are unarmed and never fight
//!   back (they flee); ambusher skill 3-8.

use crate::combat::Role;
use crate::path::Goals;
use crate::rng::Rng;
use crate::{AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{Pos, Skill, Unit, UnitFlags, TICKS_PER_DAY, TICKS_PER_MONTH};

/// Kind of a raid (an invasion group on the map).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RaidKind {
    /// A siege: visible army, camps, then charges.
    #[default]
    Siege,
    /// Hidden squads that sneak up and attack.
    Ambush,
    /// A hidden item thief.
    Thief,
    /// A hidden child snatcher.
    Snatcher,
}

impl RaidKind {
    pub(crate) fn code(self) -> u8 {
        match self {
            RaidKind::Siege => 1,
            RaidKind::Ambush => 2,
            RaidKind::Thief => 3,
            RaidKind::Snatcher => 4,
        }
    }
    pub(crate) fn of(code: u8) -> Option<RaidKind> {
        match code {
            1 => Some(RaidKind::Siege),
            2 => Some(RaidKind::Ambush),
            3 => Some(RaidKind::Thief),
            4 => Some(RaidKind::Snatcher),
            _ => None,
        }
    }
    fn sneaks(self) -> bool {
        self != RaidKind::Siege
    }
    /// DF announcement when the group is spotted.
    fn spotted_text(self) -> &'static str {
        match self {
            RaidKind::Ambush => "An ambush! Curse them!",
            RaidKind::Thief => "Thief! Protect the hoard from skulking filth!",
            RaidKind::Snatcher => "Snatcher! Protect the children!",
            RaidKind::Siege => "A vile force of darkness has arrived!",
        }
    }
}

/// Small valuables thieves go for first (DF item types).
const LOOT_KINDS: &[&str] =
    &["CRAFTS", "GOBLET", "FIGURINE", "AMULET", "BRACELET", "RING", "EARRING", "CROWN", "SCEPTER", "INSTRUMENT", "TOY", "GEM", "SMALLGEM", "ROUGH", "COIN", "TOTEM", "BOOK"];

/// Camp time of a siege (estimate; goblins gather briefly, other races wait days).
pub(crate) fn camp_ticks(race: &str, r: &mut Rng) -> u64 {
    if race == "GOBLIN" {
        600 + r.below(1801) as u64
    } else {
        (2 + r.below(6) as u64) * TICKS_PER_DAY as u64
    }
}

impl Sim {
    // ------------------------------------------------------------------ spawning

    /// Debug/UI: start a raid right now: `n` raiders (`n = 0`: the size the fort would get) and,
    /// for sieges, `monsters` trolls and beak dogs. Sieges camp first, like natural ones.
    /// Returns the number of creatures that arrived.
    pub fn force_raid(&mut self, kind: RaidKind, n: usize, monsters: usize) -> usize {
        let now = self.ticks;
        let mut r = Rng::for_unit(self.seed, 7, now, 0xF0CE);
        match kind {
            RaidKind::Siege => {
                let n = if n == 0 { self.siege_size().unwrap_or(crate::invasion::SIEGE_FORCE[2].0) } else { n };
                let camp = camp_ticks("GOBLIN", &mut r);
                self.spawn_raid(RaidKind::Siege, "GOBLIN", n, monsters, camp)
            }
            RaidKind::Ambush => {
                let n = if n == 0 { self.ambush_size(&mut r) } else { n };
                self.spawn_raid(RaidKind::Ambush, "GOBLIN", n, 0, 0)
            }
            RaidKind::Thief => {
                let race = self.thief_race().unwrap_or("KOBOLD").to_string();
                self.spawn_raid(RaidKind::Thief, &race, n.max(1), 0, 0)
            }
            RaidKind::Snatcher => self.spawn_raid(RaidKind::Snatcher, "GOBLIN", n.max(1), 0, 0),
        }
    }

    /// Kinds of the raids on the map right now, with the number of living members and whether
    /// the group has been spotted.
    pub fn raids(&self) -> Vec<(RaidKind, usize, bool)> {
        self.combat
            .inv
            .sieges
            .iter()
            .map(|g| (g.kind, g.members.iter().filter(|&&s| self.ai[s as usize].kind != Kind::Dead).count(), g.revealed))
            .collect()
    }

    /// Is this unit a hidden (not yet spotted) raider?
    pub fn unit_hidden(&self, unit_id: i32) -> bool {
        self.unit_slot.get(&unit_id).and_then(|&s| self.combat.f.get(s)).map(|f| f.hidden).unwrap_or(false)
    }

    /// Ambush size from the invasion tier (estimate): 1-3 squads of 2-5.
    pub(crate) fn ambush_size(&self, r: &mut Rng) -> usize {
        let pop = self.world.citizens().count();
        let tier = crate::invasion::invasion_tier(pop, self.created_wealth(), self.exported_wealth());
        let squads = 1 + (tier >= 3) as usize + (tier >= 5) as usize;
        (0..squads).map(|_| 2 + r.below(4) as usize).sum()
    }

    /// Race of the fort's thieves: kobolds (`ITEM_THIEF`) if a kobold civilization exists (or no
    /// civilizations were exported at all).
    pub(crate) fn thief_race(&self) -> Option<&'static str> {
        (self.world.entities.is_empty() || self.world.entities.iter().any(|e| e.race == "KOBOLD")).then_some("KOBOLD")
    }

    /// A goblin civilization at war (`BABYSNATCHER`), or no civilizations exported.
    pub(crate) fn snatcher_civ(&self) -> bool {
        self.world.entities.is_empty() || self.world.entities.iter().any(|e| e.race == "GOBLIN" && e.relation == "War")
    }

    pub(crate) fn children_count(&self) -> usize {
        self.world.units.iter().filter(|u| u.is_worker() && is_child(u)).count()
    }

    /// Spawn a raid group of `n` (+ `monsters`) at a map edge connected to the fort.
    pub(crate) fn spawn_raid(&mut self, kind: RaidKind, race: &str, n: usize, monsters: usize, camp: u64) -> usize {
        let now = self.ticks;
        self.combat_sync_units();
        let tiles = self.edge_tiles();
        if tiles.is_empty() || n + monsters == 0 {
            if kind == RaidKind::Siege {
                self.announce(now, "The invaders could not find a way to the fortress.".into(), None, AnnouncementKind::Warning);
            }
            return 0;
        }
        let mut r = Rng::for_unit(self.seed, kind.code() as usize, now, 0x5E1E);
        let start = tiles[r.below(tiles.len() as u32) as usize];
        let sp = self.nav.xyz(start as usize);
        let mut near = tiles.clone();
        near.sort_by_key(|&t| (crate::manhattan(sp, self.nav.xyz(t as usize)), t));
        let caste_of = |race: &str| {
            self.world
                .creature_raws
                .iter()
                .find(|c| c.token == race)
                .and_then(|c| c.castes.first())
                .map(|c| (if c.body_size > 0 { c.body_size } else { 60_000 }, if c.walk_speed > 0 { c.walk_speed } else { 900 }))
                .unwrap_or((60_000, 900))
        };
        let (body_size, walk) = caste_of(race);
        let civ = self
            .world
            .entities
            .iter()
            .find(|e| e.race == race && (e.relation == "War" || kind == RaidKind::Thief))
            .map(|e| e.id)
            .or_else(|| self.world.units.iter().filter(|u| u.flags.hostile && u.invader_civ >= 0).map(|u| u.invader_civ).next())
            .unwrap_or(9_999);
        let mut next_id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let mut kit = Vec::new();
        let armed = matches!(kind, RaidKind::Siege | RaidKind::Ambush);
        for k in 0..n + monsters {
            let tile = if kind == RaidKind::Siege { near[k % near.len()] } else { near[(k * 3) % near.len().min(12).max(1)] };
            let p = self.world.pos_of(tile as usize);
            let monster = k >= n;
            let urace = if monster { if k % 2 == 0 { "TROLL" } else { "BEAK_DOG" } } else { race };
            let (bsz, wspd) = if monster { caste_of(urace) } else { (body_size, walk) };
            let (wtok, prof) = crate::invasion::GOBLIN_WEAPONS[r.below(crate::invasion::GOBLIN_WEAPONS.len() as u32) as usize];
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
            let mut skills = vec![
                Skill { id: "DODGING".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                Skill { id: "ARMOR".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                Skill { id: "MELEE_COMBAT".into(), rating: 1 + r.below(4) as i32, xp: 0 },
            ];
            if armed && !monster {
                skills.push(Skill { id: skill.into(), rating: 2 + r.below(6) as i32, xp: 0 });
            }
            if kind.sneaks() {
                skills.push(Skill { id: "SNEAK".into(), rating: 3 + r.below(6) as i32, xp: 0 });
            }
            let (profession, custom) = match (kind, monster) {
                (_, true) => ("STANDARD".to_string(), String::new()),
                (RaidKind::Thief, _) => ("THIEF".into(), "thief".into()),
                (RaidKind::Snatcher, _) => ("THIEF".into(), "snatcher".into()),
                (RaidKind::Ambush, _) => (prof.to_ascii_uppercase(), format!("{prof} ambusher")),
                _ => (prof.to_ascii_uppercase(), prof.into()),
            };
            let unit = Unit {
                id: next_id,
                race: urace.into(),
                caste: if r.chance(1, 2) { "MALE".into() } else { "FEMALE".into() },
                name: String::new(),
                profession,
                custom_profession: custom,
                pos: p,
                flags: UnitFlags { hostile: true, animal: monster && urace == "BEAK_DOG", ..Default::default() },
                civ_id: civ,
                hist_fig_id: -1,
                sex: r.below(2) as i8,
                age: 20.0 + r.below(30) as f32,
                skills,
                phys_attrs: vec![("STRENGTH".into(), attr(&mut r)), ("AGILITY".into(), attr(&mut r)), ("TOUGHNESS".into(), attr(&mut r))],
                ment_attrs: vec![("WILLPOWER".into(), attr(&mut r))],
                squad_id: -1,
                squad_position: -1,
                body_size: bsz,
                walk_speed: wspd,
                invader_civ: civ,
                ..Default::default()
            };
            let gear = armed && !monster;
            let mail = gear && r.chance(70, 100);
            let helm = gear && r.chance(60, 100);
            let legs = gear && r.chance(40, 100);
            kit.push((next_id, gear, wtok, metal, mail, helm, legs, tile, monster));
            self.world.units.push(unit);
            next_id += 1;
        }
        self.init_units();
        self.combat_sync_units();
        self.combat.inv.next_id = self.combat.inv.next_id.wrapping_add(1).max(1);
        let sid = self.combat.inv.next_id;
        let mut members = Vec::new();
        let mut regulars = Vec::new();
        for (uid, gear, wtok, metal, mail, helm, legs, tile, monster) in kit {
            if gear {
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
            }
            let Some(&s) = self.unit_slot.get(&uid) else { continue };
            let f = &mut self.combat.f[s];
            f.siege = sid;
            f.home = tile;
            f.role = Role::Hostile;
            f.raid = kind.code();
            f.hidden = kind.sneaks();
            members.push(s as u32);
            if !monster {
                regulars.push(s as u32);
            }
        }
        if kind == RaidKind::Siege {
            self.ranged_arm_siege(&regulars); // [ranged] bowmen
        }
        // Leader: the best weapon skill among the regulars.
        let leader = regulars
            .iter()
            .copied()
            .max_by_key(|&s| {
                let u = &self.world.units[self.ai[s as usize].uidx];
                (u.skills.iter().filter(|k| !matches!(k.id.as_str(), "DODGING" | "ARMOR" | "MELEE_COMBAT" | "SNEAK")).map(|k| k.rating).max().unwrap_or(0), std::cmp::Reverse(s))
            })
            .unwrap_or(NONE);
        let charge_at = now + camp;
        let hidden = kind.sneaks();
        self.combat.inv.sieges.push(crate::invasion::Siege {
            members: members.clone(),
            start: now,
            fleeing: false,
            race: race.into(),
            id: sid,
            kind,
            camp: start,
            charge_at,
            leader,
            revealed: !hidden,
            charged: camp == 0,
        });
        if hidden {
            for &s in &members {
                if let Err(p) = self.combat.inv.hidden.binary_search(&s) {
                    self.combat.inv.hidden.insert(p, s);
                }
            }
        }
        let st = &mut self.combat.stats;
        match kind {
            RaidKind::Siege => {
                st.sieges += 1;
                st.invaders_spawned += members.len() as u64;
                st.monsters_spawned += monsters as u64;
                if camp > 0 {
                    st.siege_camps += 1;
                }
            }
            RaidKind::Ambush => {
                st.ambushes += 1;
                st.invaders_spawned += members.len() as u64;
            }
            RaidKind::Thief => st.thieves += members.len() as u64,
            RaidKind::Snatcher => st.snatchers += members.len() as u64,
        }
        self.rebuild_fort_list();
        let p = self.world.pos_of(start as usize);
        let rname = race.to_ascii_lowercase();
        match kind {
            RaidKind::Siege => {
                self.announce(now, "A vile force of darkness has arrived!".into(), Some(p), AnnouncementKind::Warning);
                let extra = if monsters > 0 { format!(" and {monsters} monsters") } else { String::new() };
                self.combat.log_line(format!("A vile force of darkness has arrived! ({} {rname}s{extra})", regulars.len()));
                if camp > 0 {
                    self.combat.log_line(format!("The {rname}s are gathering at the edge of the map."));
                }
            }
            _ => {
                // Hidden: nothing is announced until they are spotted.
                self.combat.log_line(format!("({} hidden {rname} {} entered the map.)", members.len(), match kind {
                    RaidKind::Ambush => "ambushers",
                    RaidKind::Thief => "thieves",
                    _ => "snatchers",
                }));
            }
        }
        members.len()
    }

    // ------------------------------------------------------------------ think

    /// Hook at the top of `fighter_think`: raid-specific behavior. True = handled.
    pub(crate) fn raid_think(&mut self, s: usize, now: u64) -> bool {
        let Some(kind) = RaidKind::of(self.combat.f[s].raid) else { return false };
        match kind {
            RaidKind::Siege => self.camp_think(s, now),
            RaidKind::Ambush => false,
            RaidKind::Thief => {
                self.thief_think(s, now);
                true
            }
            RaidKind::Snatcher => {
                self.snatcher_think(s, now);
                true
            }
        }
    }

    fn raid_index(&self, s: usize) -> Option<usize> {
        let sid = self.combat.f[s].siege;
        self.combat.inv.sieges.iter().position(|g| g.id == sid)
    }

    /// Siegers wait at their camp until the charge; they attack only what comes close.
    fn camp_think(&mut self, s: usize, now: u64) -> bool {
        let Some(k) = self.raid_index(s) else { return false };
        let g = &self.combat.inv.sieges[k];
        if g.charged || g.fleeing {
            return false;
        }
        if now >= g.charge_at {
            let race = g.race.to_ascii_lowercase();
            self.combat.inv.sieges[k].charged = true;
            self.combat.log_line(format!("The {race}s charge!"));
            return false;
        }
        if self.alive(self.combat.f[s].target) {
            return false; // fighting back
        }
        let camp = g.camp;
        let t = self.find_target(s, Some(8));
        if t != NONE {
            self.combat.f[s].target = t;
            return false;
        }
        let here = self.upos(s);
        let cp = self.world.pos_of(camp as usize);
        let mut r = Rng::for_unit(self.seed, s, now, 0xCA3B);
        if here.dist(cp) > 5 && self.nav.comp_near(camp as usize) == self.nav.comp_near(self.ai[s].pos as usize) {
            let ai = &self.ai[s];
            if ai.path_i < ai.path.len() && *ai.path.last().unwrap() == camp {
                self.step(s, now);
            } else if self.combat.f[s].repath_at <= now {
                self.request_path(s, Goals::one(camp), now);
            } else {
                self.ai[s].wake = self.combat.f[s].repath_at;
            }
            return true;
        }
        if r.chance(1, 3) {
            let _ = self.random_step(s, now, &mut r, None);
        }
        self.ai[s].wake = now + 60 + r.below(120) as u64;
        true
    }

    /// Item thief: sneak to a valuable item, grab it, run for the edge.
    fn thief_think(&mut self, s: usize, now: u64) {
        if self.combat.f[s].loot >= 0 || !self.combat.f[s].hidden && self.combat.f[s].want < 0 {
            self.raid_flee(s, now);
            return;
        }
        let want = self.combat.f[s].want;
        if want >= 0 {
            let gone = self.item_idx.get(&want).map(|&i| self.world.items[i as usize].holder >= 0 || self.imeta[i as usize].dead).unwrap_or(true);
            if gone {
                self.unreserve(want);
                self.combat.f[s].want = -1;
            } else if self.item_reachable_here(s, want) {
                self.unreserve(want);
                self.take_loot(s, want, now);
                self.raid_flee(s, now);
                return;
            } else {
                let ai = &self.ai[s];
                if ai.path_i < ai.path.len() {
                    self.step(s, now);
                } else if self.combat.f[s].repath_at > now {
                    self.ai[s].wake = self.combat.f[s].repath_at;
                } else {
                    let g = self.item_goals(want);
                    self.combat.f[s].repath_at = now + 20;
                    self.request_path(s, g, now);
                }
                return;
            }
        }
        if !self.combat.f[s].hidden {
            self.raid_flee(s, now);
            return;
        }
        match self.pick_loot(s, now) {
            Some(id) => {
                self.reserve(id);
                self.combat.f[s].want = id;
                let g = self.item_goals(id);
                self.request_path(s, g, now);
            }
            None => self.raid_flee(s, now),
        }
    }

    /// The most valuable free item the thief can reach (small valuables first).
    fn pick_loot(&self, s: usize, now: u64) -> Option<i32> {
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return None;
        }
        let mut best: Option<(bool, i32, i32)> = None;
        for (i, it) in self.world.items.iter().enumerate() {
            if it.value <= 0 || it.holder >= 0 || it.container >= 0 || it.building >= 0 {
                continue;
            }
            let m = &self.imeta[i];
            let f = &it.flags;
            if m.dead || m.reserved > 0 || m.hauling || m.blocked_until > now || f.in_job || f.in_building || f.construction || f.artifact || !f.on_ground {
                continue;
            }
            let small = LOOT_KINDS.contains(&it.kind.as_str());
            let key = (small, it.value, -it.id);
            if best.map(|b| key <= (b.0, b.1, b.2)).unwrap_or(false) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            best = Some((small, it.value, -it.id));
        }
        best.map(|b| -b.2)
    }

    fn take_loot(&mut self, s: usize, id: i32, now: u64) {
        let desc = self.item_idx.get(&id).map(|&i| self.world.items[i as usize].desc.clone()).unwrap_or_default();
        self.equip(s, id);
        self.combat.f[s].loot = id;
        self.combat.f[s].want = -1;
        self.combat.stats.items_stolen += 1;
        let n = self.uname(s);
        self.combat.log_line(format!("{} has stolen {}!", crate::combat::cap_pub(&n), if desc.is_empty() { "an item".into() } else { desc }));
        let _ = now;
    }

    /// Snatcher: sneak to a child, grab it, carry it off the map.
    fn snatcher_think(&mut self, s: usize, now: u64) {
        if self.combat.f[s].carry != NONE || !self.combat.f[s].hidden {
            self.raid_flee(s, now);
            return;
        }
        let mut t = self.combat.f[s].target;
        if !self.alive(t) || self.combat.f[t as usize].carried_by != NONE {
            t = self.find_child(s);
            self.combat.f[s].target = t;
        }
        if t == NONE {
            self.raid_flee(s, now);
            return;
        }
        let t = t as usize;
        if self.upos(s).dist(self.upos(t)) <= 1 {
            self.carry_start(s, t, now);
            let (sn, cn) = (self.uname(s), self.uname(t));
            self.combat.log_line(format!("{} grabs {cn}!", crate::combat::cap_pub(&sn)));
            self.raid_flee(s, now);
            return;
        }
        let tt = self.ai[t].pos;
        let ai = &self.ai[s];
        let end_ok = ai.path_i < ai.path.len() && self.world.pos_of(*ai.path.last().unwrap() as usize).dist(self.upos(t)) <= 1;
        if end_ok {
            self.step(s, now);
        } else if self.combat.f[s].repath_at > now {
            self.ai[s].wake = self.combat.f[s].repath_at;
        } else {
            self.combat.f[s].repath_at = now + 20;
            self.request_path(s, Goals::one(tt), now);
        }
    }

    fn find_child(&self, s: usize) -> u32 {
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, NONE);
        for &t in &self.combat.fort {
            let ai = &self.ai[t as usize];
            if ai.kind != Kind::Worker || !is_child(&self.world.units[ai.uidx]) || self.combat.f[t as usize].carried_by != NONE {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(ai.pos as usize));
            if (d, t) < best && self.nav.comp_near(ai.pos as usize) == c {
                best = (d, t);
            }
        }
        best.1
    }

    /// Run for the map edge (thieves and snatchers never fight).
    fn raid_flee(&mut self, s: usize, now: u64) {
        let f = &mut self.combat.f[s];
        if f.role != Role::Leave {
            f.role = Role::Leave;
            f.target = NONE;
            if f.want >= 0 {
                let w = f.want;
                f.want = -1;
                self.unreserve(w);
            }
        }
        self.leave_think(s, now);
    }

    // ------------------------------------------------------------------ hidden raiders

    /// Every 10 ticks: citizens near a hidden raider may spot it (the whole group is revealed).
    pub(crate) fn raid_detect(&mut self, now: u64) {
        let list = self.combat.inv.hidden.clone();
        let mut reveal: Vec<u16> = Vec::new();
        for &h in &list {
            let h = h as usize;
            if self.ai[h].kind == Kind::Dead || !self.combat.f[h].hidden {
                continue;
            }
            let sid = self.combat.f[h].siege;
            if reveal.contains(&sid) {
                continue;
            }
            let hp = self.nav.xyz(self.ai[h].pos as usize);
            let hc = self.nav.comp_near(self.ai[h].pos as usize);
            let sneak = self.world.units[self.ai[h].uidx].skill("SNEAK");
            let mut r = Rng::for_unit(self.seed, h, now, 0x5EE);
            for k in 0..self.combat.fort.len() {
                let t = self.combat.fort[k] as usize;
                let ai = &self.ai[t];
                if ai.kind != Kind::Worker || self.combat.f[t].carried_by != NONE || self.combat.f[t].uncon_until > now {
                    continue;
                }
                let tp = self.nav.xyz(ai.pos as usize);
                if (hp.2 as i32 - tp.2 as i32).abs() > 1 {
                    continue;
                }
                let cheb = (hp.0 as i32 - tp.0 as i32).abs().max((hp.1 as i32 - tp.1 as i32).abs());
                if cheb > 11 {
                    continue;
                }
                let obs = self.world.units[ai.uidx].skill("SITUATIONAL_AWARENESS");
                let soldier = self.combat.f[t].duty != crate::military::Duty::None;
                let radius = (3 + obs / 3 - sneak / 4).clamp(1, 8) + if soldier { 3 } else { 0 };
                if cheb <= radius && self.nav.comp_near(ai.pos as usize) == hc && r.chance(1, 2) {
                    reveal.push(sid);
                    break;
                }
            }
        }
        for sid in reveal {
            self.raid_reveal_group(sid, now);
        }
    }

    /// A hidden raider attacks: it (and its group) is revealed.
    pub(crate) fn raid_reveal(&mut self, s: usize, now: u64) {
        let sid = self.combat.f[s].siege;
        self.raid_reveal_group(sid, now);
    }

    fn raid_reveal_group(&mut self, sid: u16, now: u64) {
        let Some(k) = self.combat.inv.sieges.iter().position(|g| g.id == sid) else {
            return;
        };
        if self.combat.inv.sieges[k].revealed {
            return;
        }
        self.combat.inv.sieges[k].revealed = true;
        let kind = self.combat.inv.sieges[k].kind;
        let members = self.combat.inv.sieges[k].members.clone();
        let mut at = None;
        for &m in &members {
            let s = m as usize;
            self.combat.f[s].hidden = false;
            if let Ok(p) = self.combat.inv.hidden.binary_search(&m) {
                self.combat.inv.hidden.remove(p);
            }
            if self.ai[s].kind != Kind::Dead {
                at.get_or_insert(self.upos(s));
                // Spotted thieves and snatchers run (their think does it); wake them now.
                self.engage_now(s, now);
            }
        }
        self.combat.stats.raids_spotted += 1;
        let text = kind.spotted_text().to_string();
        self.announce(now, text.clone(), at, AnnouncementKind::Warning);
        self.combat.log_line(text);
    }

    // ------------------------------------------------------------------ carrying units

    /// `carrier` picks up `victim` (a kidnapped child or a patient being rescued).
    pub(crate) fn carry_start(&mut self, carrier: usize, victim: usize, now: u64) {
        if self.combat.f[victim].carried_by != NONE || self.combat.f[carrier].carry != NONE {
            return;
        }
        if self.ai[victim].task != Task::Combat {
            self.cancel_task(victim);
        }
        self.release_rest(victim);
        self.combat.f[carrier].carry = victim as u32;
        self.combat.f[victim].carried_by = carrier as u32;
        self.combat.carriers += 1;
        let q = self.ai[carrier].pos;
        let p = self.world.pos_of(q as usize);
        let v = &mut self.ai[victim];
        v.task = Task::Combat;
        v.path.clear();
        v.path_i = 0;
        v.pos = q;
        v.wake = u64::MAX;
        let uidx = v.uidx;
        self.world.units[uidx].pos = p;
        let _ = now;
    }

    /// Put the carried unit down where the carrier stands; returns its slot.
    pub(crate) fn carry_drop(&mut self, carrier: usize, now: u64) -> Option<usize> {
        let v = self.combat.f.get(carrier).map(|f| f.carry).unwrap_or(NONE);
        if v == NONE {
            return None;
        }
        let v = v as usize;
        self.combat.f[carrier].carry = NONE;
        self.combat.f[v].carried_by = NONE;
        self.combat.carriers = self.combat.carriers.saturating_sub(1);
        if self.ai[v].kind != Kind::Dead {
            let q = self.ai[carrier].pos;
            self.ai[v].pos = q;
            let uidx = self.ai[v].uidx;
            self.world.units[uidx].pos = self.world.pos_of(q as usize);
            self.ai[v].wake = now + 1;
            self.ai[v].task = Task::Combat;
        }
        Some(v)
    }

    /// Hook in `move_to`: the carried unit moves with its carrier.
    #[inline]
    pub(crate) fn carry_follow(&mut self, s: usize, q: u32, p: Pos) {
        let v = match self.combat.f.get(s) {
            Some(f) if f.carry != NONE => f.carry as usize,
            _ => return,
        };
        self.ai[v].pos = q;
        let uidx = self.ai[v].uidx;
        self.world.units[uidx].pos = p;
    }

    /// Hook in `combat_kill` / knock-outs: a carrier that dies or passes out drops its load; a
    /// carried unit that dies is let go.
    pub(crate) fn carry_unlink(&mut self, s: usize, now: u64) {
        if s >= self.combat.f.len() {
            return;
        }
        if self.combat.f[s].carry != NONE {
            if let Some(v) = self.carry_drop(s, now) {
                let (cn, vn) = (self.uname(s), self.uname(v));
                self.combat.log_line(format!("{} drops {vn}.", crate::combat::cap_pub(&cn)));
                if self.combat.f[v].role == Role::None || self.ai[v].kind == Kind::Worker {
                    self.combat.f[v].role = if self.needs_rest(v) { Role::Patient } else { Role::None };
                }
            }
            self.health_carry_dropped(s);
        }
        let by = self.combat.f[s].carried_by;
        if by != NONE {
            let by = by as usize;
            self.combat.f[by].carry = NONE;
            self.combat.f[s].carried_by = NONE;
            self.combat.carriers = self.combat.carriers.saturating_sub(1);
        }
    }

    /// Hook in `leave_map`: loot and kidnapped children leave with the raider.
    pub(crate) fn raid_left_map(&mut self, s: usize, now: u64) {
        let (loot, v) = (self.combat.f[s].loot, self.combat.f[s].carry);
        if loot >= 0 {
            self.combat.stats.items_lost += 1;
            let n = self.uname(s);
            self.combat.log_line(format!("{} has escaped with the loot.", crate::combat::cap_pub(&n)));
        }
        if v != NONE {
            let v = v as usize;
            self.combat.f[s].carry = NONE;
            self.combat.f[v].carried_by = NONE;
            self.combat.carriers = self.combat.carriers.saturating_sub(1);
            let name = self.uname(v);
            let pos = self.upos(v);
            let uidx = self.ai[v].uidx;
            self.world.units[uidx].flags.inactive = true;
            self.ai[v].kind = Kind::Dead;
            self.ai[v].wake = u64::MAX;
            self.combat.f[v].role = Role::None;
            self.combat.stats.children_abducted += 1;
            self.announce(now, format!("{name} has been abducted!"), Some(pos), AnnouncementKind::Death);
            self.combat.log_line(format!("{name} has been abducted!"));
            self.rebuild_fort_list();
        }
        if let Ok(p) = self.combat.inv.hidden.binary_search(&(s as u32)) {
            self.combat.inv.hidden.remove(p);
        }
    }

    // ------------------------------------------------------------------ schedule

    /// Season start (from `invasion_daily`): invasions (siege or ambush) and visits of thieves
    /// and snatchers.
    pub(crate) fn raids_season(&mut self, now: u64) {
        let mut r = Rng::for_unit(self.seed, 1, now, 0x51E6);
        let pop = self.world.citizens().count();
        let army = self.combat.inv.sieges.iter().any(|g| matches!(g.kind, RaidKind::Siege | RaidKind::Ambush));
        if !army && r.chance(1, 3) {
            let tier = crate::invasion::invasion_tier(pop, self.created_wealth(), self.exported_wealth());
            let siege = if pop >= crate::invasion::SIEGE_POPULATION { self.siege_size() } else { None };
            match siege {
                Some(n) if r.chance(1, 4) => {
                    let monsters = crate::invasion::siege_force(pop, self.created_wealth(), self.exported_wealth()).map(|f| f.1).unwrap_or(0);
                    let camp = camp_ticks("GOBLIN", &mut r);
                    self.spawn_raid(RaidKind::Siege, "GOBLIN", n, monsters, camp);
                }
                _ if tier >= 1 => {
                    let n = self.ambush_size(&mut r);
                    self.spawn_raid(RaidKind::Ambush, "GOBLIN", n, 0, 0);
                }
                _ => {}
            }
        }
        // Visits on a random day of the coming season.
        let season = 3 * TICKS_PER_MONTH as u64;
        if pop >= 1 && self.thief_race().is_some() && r.chance(1, 2) {
            let at = now + r.below(season as u32) as u64;
            self.combat.inv.pending.push((at, RaidKind::Thief));
        }
        if self.snatcher_civ() && self.children_count() > 0 && r.chance(1, 3) {
            let at = now + r.below(season as u32) as u64;
            self.combat.inv.pending.push((at, RaidKind::Snatcher));
        }
    }

    /// Daily: spawn due visits.
    pub(crate) fn raids_pending(&mut self, now: u64) {
        if self.combat.inv.pending.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.combat.inv.pending.retain(|&(t, k)| {
            if t <= now {
                due.push(k);
                false
            } else {
                true
            }
        });
        for k in due {
            match k {
                RaidKind::Thief => {
                    let race = self.thief_race().unwrap_or("KOBOLD");
                    self.spawn_raid(RaidKind::Thief, race, 1, 0, 0);
                }
                RaidKind::Snatcher => {
                    if self.children_count() > 0 {
                        self.spawn_raid(RaidKind::Snatcher, "GOBLIN", 1, 0, 0);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Children and babies (DF professions `CHILD`, `BABY`; age below 12 otherwise).
pub(crate) fn is_child(u: &Unit) -> bool {
    matches!(u.profession.as_str(), "CHILD" | "BABY") || (u.age > 0.0 && u.age < 12.0 && u.race == "DWARF")
}
