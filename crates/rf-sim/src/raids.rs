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
//! - [war9] Invasion check at each season start, 2 in 3 (was 1 in 3); then `SIEGE_FREQUENCY`
//!   (25 %) a siege if the siege tier is met and the civilization sent enough raids since its
//!   last siege (`MIN_RAIDS_BEFORE_SIEGE` / `_BETWEEN_SIEGES`), else an ambush. Siege size and
//!   composition: see `warcal.rs`. Ambush size: 1 squad from the first tier, 2 from the Town
//!   tier, 3 at Metropolis; 2-5 goblins per squad.
//! - Thief visits: 1 in 2 per season if a kobold civilization exists; snatchers: 1 in 3 per
//!   season with a goblin civilization at war and children in the fort; they arrive on a random
//!   day of the season.
//! - Spotting: within `3 + observer/3 - ambusher/4` tiles (at least 1, at most 8; soldiers on
//!   duty +3), same z +-1, same walk component, 1 in 2 per 10-tick check. Sleeping or
//!   unconscious dwarves and children spot nobody.
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
                if n == 0 && monsters == 0 {
                    if let Some(p) = self.siege_plan() {
                        return self.war_spawn_siege(&p, now, &mut r, false); // [war9]
                    }
                }
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
        let tier = self.invasion_tier_now(); // [war9]
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
        self.spawn_raid_ex(kind, race, n, monsters, camp, crate::warcal::RaidExtra::default())
    }

    /// [war9] A siege as planned by [`Sim::siege_plan`]: the siege group (regulars, monsters and
    /// mounts) plus 1-5 ambush squads arriving with it. `record`: count it in the fort's
    /// history (natural sieges). Returns the number of creatures that arrived.
    pub(crate) fn war_spawn_siege(&mut self, p: &crate::warcal::SiegePlan, _now: u64, r: &mut Rng, record: bool) -> usize {
        use crate::warcal::RaidExtra;
        let mut squads = Vec::new();
        if p.regulars >= 15 {
            let mut left = p.regulars / 3;
            for _ in 0..1 + r.below(5) {
                let n = (3 + r.below(4) as usize).min(left);
                if n < 2 {
                    break;
                }
                squads.push(n);
                left -= n;
            }
        }
        let main = p.regulars - squads.iter().sum::<usize>();
        let camp = camp_ticks("GOBLIN", r);
        let extra = RaidExtra { mounts: p.mounts, masters_pct: p.masters_pct, civ: Some(p.civ) };
        let mut total = self.spawn_raid_ex(RaidKind::Siege, "GOBLIN", main, p.monsters, camp, extra);
        if total == 0 {
            return 0;
        }
        if record {
            Self::war_bump(&mut self.world.fort_history.sieges, p.civ, Some(p.number), 0);
            Self::war_bump(&mut self.world.fort_history.raids, p.civ, Some(0), 0);
        }
        for n in squads {
            let extra = RaidExtra { mounts: 0, masters_pct: p.masters_pct, civ: Some(p.civ) };
            let k = self.spawn_raid_ex(RaidKind::Ambush, "GOBLIN", n, 0, 0, extra);
            if k > 0 {
                total += k;
                self.combat.stats.siege_ambush_squads += 1;
                if record {
                    Self::war_bump(&mut self.world.fort_history.raids, p.civ, None, 1);
                }
            }
        }
        total
    }

    /// [war9] `spawn_raid` with mounts, master troops and the invading civilization.
    pub(crate) fn spawn_raid_ex(&mut self, kind: RaidKind, race: &str, n: usize, monsters: usize, camp: u64, extra: crate::warcal::RaidExtra) -> usize {
        let now = self.ticks;
        let mounts = extra.mounts;
        self.combat_sync_units();
        let tiles = self.edge_tiles();
        if tiles.is_empty() || n + monsters + mounts == 0 {
            if kind == RaidKind::Siege {
                self.announce(now, "The invaders could not find a way to the fortress.".into(), None, AnnouncementKind::Warning);
            }
            return 0;
        }
        // [war9] Salted with the group number, so groups arriving together spread out.
        let mut r = Rng::for_unit(self.seed, kind.code() as usize, now, 0x5E1E ^ ((self.combat.inv.next_id as u64) << 24));
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
        let civ = extra.civ.filter(|&c| c != 9_999).unwrap_or_else(|| {
            self.world
                .entities
                .iter()
                .find(|e| e.race == race && (e.relation == "War" || kind == RaidKind::Thief))
                .map(|e| e.id)
                .or_else(|| self.world.units.iter().filter(|u| u.flags.hostile && u.invader_civ >= 0).map(|u| u.invader_civ).next())
                .unwrap_or(9_999)
        });
        let mount_race = if mounts > 0 { self.war_mount_race() } else { String::new() };
        // [war9] Weapons of Gordon's goblin sieges (r6 export), if the save has their raws.
        let weapons: Vec<(&str, &str, &str, u32)> = {
            let have = |t: &str| self.world.weapon_raws.iter().any(|w| w.token == t);
            let v: Vec<_> = WAR_WEAPONS.iter().copied().filter(|w| have(w.0)).collect();
            if v.len() >= 4 {
                v
            } else {
                crate::invasion::GOBLIN_WEAPONS.iter().map(|&(t, p)| (t, p, weapon_skill(t), 1)).collect()
            }
        };
        let wsum: u32 = weapons.iter().map(|w| w.3).sum();
        let calibrated = weapons.len() > crate::invasion::GOBLIN_WEAPONS.len() || extra.civ.is_some();
        let mut next_id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let mut kit = Vec::new();
        let armed = matches!(kind, RaidKind::Siege | RaidKind::Ambush);
        for k in 0..n + monsters + mounts {
            let tile = if kind == RaidKind::Siege { near[k % near.len()] } else { near[(k * 3) % near.len().min(12).max(1)] };
            let p = self.world.pos_of(tile as usize);
            let monster = k >= n;
            let mount = k >= n + monsters; // [war9]
            let urace = if mount {
                mount_race.as_str()
            } else if monster {
                if k % 2 == 0 {
                    "TROLL"
                } else {
                    "BEAK_DOG"
                }
            } else {
                race
            };
            let (bsz, wspd) = if monster { caste_of(urace) } else { (body_size, walk) };
            let pick = {
                let mut x = r.below(wsum.max(1));
                let mut i = 0;
                while i + 1 < weapons.len() && x >= weapons[i].3 {
                    x -= weapons[i].3;
                    i += 1;
                }
                i
            };
            let (wtok, prof, skill, _) = weapons[pick];
            // [war9] r6: goblin weapons iron (47 %) or silver (53 %), their armor iron.
            let metal = if calibrated {
                if r.chance(47, 100) {
                    "INORGANIC:IRON"
                } else {
                    "INORGANIC:SILVER"
                }
            } else {
                match r.below(100) {
                    0..=44 => "INORGANIC:COPPER",
                    45..=69 => "INORGANIC:BRONZE",
                    _ => "INORGANIC:IRON",
                }
            };
            let master = armed && !monster && extra.masters_pct > 0 && r.chance(extra.masters_pct, 100);
            let attr = |r: &mut Rng| 750 + r.below(501) as i32;
            let mut skills = if calibrated && armed && !monster {
                // [war9] r6 goblins: dodging, armor, shield and awareness about 4, melee 3.6.
                vec![
                    Skill { id: "DODGING".into(), rating: 2 + r.below(5) as i32, xp: 0 },
                    Skill { id: "ARMOR".into(), rating: 2 + r.below(5) as i32, xp: 0 },
                    Skill { id: "SHIELD".into(), rating: 2 + r.below(5) as i32, xp: 0 },
                    Skill { id: "SITUATIONAL_AWARENESS".into(), rating: 2 + r.below(5) as i32, xp: 0 },
                    Skill { id: "MELEE_COMBAT".into(), rating: 1 + r.below(6) as i32, xp: 0 },
                ]
            } else {
                vec![
                    Skill { id: "DODGING".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                    Skill { id: "ARMOR".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                    Skill { id: "MELEE_COMBAT".into(), rating: 1 + r.below(4) as i32, xp: 0 },
                ]
            };
            if armed && !monster {
                // [war9] r6: regular weapon skill about 6 (3-9), masters 10-14.
                let rating = if master {
                    10 + r.below(5) as i32
                } else if calibrated {
                    3 + r.below(7) as i32
                } else {
                    2 + r.below(6) as i32
                };
                skills.push(Skill { id: skill.into(), rating, xp: 0 });
            }
            if kind.sneaks() {
                skills.push(Skill { id: "SNEAK".into(), rating: 3 + r.below(6) as i32, xp: 0 });
            }
            let (profession, custom) = match (kind, monster) {
                (_, true) => ("STANDARD".to_string(), String::new()),
                (RaidKind::Thief, _) => ("THIEF".into(), "thief".into()),
                (RaidKind::Snatcher, _) => ("THIEF".into(), "snatcher".into()),
                (RaidKind::Ambush, _) => (prof.to_ascii_uppercase(), format!("{prof} ambusher")),
                _ if master => (format!("MASTER_{}", prof.to_ascii_uppercase()), format!("master {prof}")), // [war9]
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
                flags: UnitFlags { hostile: true, animal: mount || monster && urace == "BEAK_DOG", ..Default::default() },
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
            // [war9] r6 gear shares: torso 31 % (breastplate or mail), helm 54 %, greaves or
            // leggings 31 %, shield 34 % (r8 before: mail 70 %, helm 60 %, leggings 40 %).
            let (mail, helm, legs, shield) = if calibrated {
                (gear && r.chance(31, 100), gear && r.chance(54, 100), gear && r.chance(31, 100), gear && r.chance(34, 100))
            } else {
                (gear && r.chance(70, 100), gear && r.chance(60, 100), gear && r.chance(40, 100), false)
            };
            let heavy = r.chance(60, 100);
            kit.push((next_id, gear, wtok, metal, mail, helm, legs, tile, monster, shield, heavy, calibrated));
            self.world.units.push(unit);
            next_id += 1;
        }
        self.init_units();
        self.combat_sync_units();
        self.combat.inv.next_id = self.combat.inv.next_id.wrapping_add(1).max(1);
        let sid = self.combat.inv.next_id;
        let mut members = Vec::new();
        let mut regulars = Vec::new();
        for (uid, gear, wtok, metal, mail, helm, legs, tile, monster, shield, heavy, cal) in kit {
            if gear {
                self.give_item(uid, "WEAPON", wtok, metal, 0);
                let am = if cal { "INORGANIC:IRON" } else { metal }; // [war9]
                if mail {
                    self.give_item(uid, "ARMOR", if cal && heavy { "ITEM_ARMOR_BREASTPLATE" } else { "ITEM_ARMOR_MAIL_SHIRT" }, am, 0);
                }
                if helm {
                    self.give_item(uid, "HELM", if cal { "ITEM_HELM_CAP" } else { "ITEM_HELM_HELM" }, am, 0);
                }
                if legs {
                    self.give_item(uid, "PANTS", if cal && heavy { "ITEM_PANTS_GREAVES" } else { "ITEM_PANTS_LEGGINGS" }, am, 0);
                }
                if shield {
                    self.give_item(uid, "SHIELD", if heavy { "ITEM_SHIELD_SHIELD" } else { "ITEM_SHIELD_BUCKLER" }, am, 0);
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
                st.mounts_spawned += mounts as u64; // [war9]
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
                let mut extra = if monsters > 0 { format!(" and {monsters} monsters") } else { String::new() };
                if mounts > 0 {
                    extra.push_str(&format!(", {mounts} mounts")); // [war9]
                }
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
                // Sleeping, unconscious or carried units and children raise no alarm (estimate).
                if ai.kind != Kind::Worker || ai.sleeping || self.combat.f[t].carried_by != NONE || self.combat.f[t].uncon_until > now || is_child(&self.world.units[ai.uidx]) {
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
        if !army && r.chance(2, 3) {
            // [war9] Siege when eligible (tier, population, raids since the last siege) and
            // SIEGE_FREQUENCY percent; otherwise a raid (ambush squads), counted per civ.
            let tier = self.invasion_tier_now();
            let civ = self.war_enemy_civ();
            let first = Self::war_count(&self.world.fort_history.sieges, civ) == 0;
            let need = if first { self.war_setting("MIN_RAIDS_BEFORE_SIEGE", 0, 2) } else { self.war_setting("MIN_RAIDS_BETWEEN_SIEGES", 0, 3) } as i32;
            let raids = Self::war_count(&self.world.fort_history.raids, civ);
            let freq = self.war_setting("SIEGE_FREQUENCY", 0, 25).clamp(0, 100) as u32;
            let plan = if pop >= crate::invasion::SIEGE_POPULATION && raids >= need { self.siege_plan() } else { None };
            match plan {
                Some(p) if r.chance(freq, 100) => {
                    self.war_spawn_siege(&p, now, &mut r, true);
                }
                _ if tier >= 1 => {
                    let n = self.ambush_size(&mut r);
                    let extra = crate::warcal::RaidExtra { civ: Some(civ), ..Default::default() };
                    if self.spawn_raid_ex(RaidKind::Ambush, "GOBLIN", n, 0, 0, extra) > 0 {
                        Self::war_bump(&mut self.world.fort_history.raids, civ, None, 1);
                    }
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

/// [war9] Weapons of Gordon's 137 sieging goblins (r6 export): token, profession, skill, count.
const WAR_WEAPONS: &[(&str, &str, &str, u32)] = &[
    ("ITEM_WEAPON_PIKE", "pikeman", "PIKE", 15),
    ("ITEM_WEAPON_SWORD_LONG", "swordsman", "SWORD", 14),
    ("ITEM_WEAPON_SPEAR", "spearman", "SPEAR", 12),
    ("ITEM_WEAPON_SWORD_SHORT", "swordsman", "SWORD", 12),
    ("ITEM_WEAPON_MAUL", "hammerman", "HAMMER", 12),
    ("ITEM_WEAPON_MACE", "maceman", "MACE", 11),
    ("ITEM_WEAPON_HAMMER_WAR", "hammerman", "HAMMER", 11),
    ("ITEM_WEAPON_SCOURGE", "lasher", "WHIP", 9),
    ("ITEM_WEAPON_WHIP", "lasher", "WHIP", 8),
    ("ITEM_WEAPON_SCIMITAR", "swordsman", "SWORD", 7),
    ("ITEM_WEAPON_SWORD_2H", "swordsman", "SWORD", 7),
    ("ITEM_WEAPON_MORNINGSTAR", "maceman", "MACE", 3),
    ("ITEM_WEAPON_HALBERD", "pikeman", "PIKE", 3),
    ("ITEM_WEAPON_AXE_BATTLE", "axeman", "AXE", 3),
];

/// Weapon skill of an r8 goblin weapon token.
fn weapon_skill(t: &str) -> &'static str {
    match t {
        "ITEM_WEAPON_SPEAR" => "SPEAR",
        "ITEM_WEAPON_SWORD_SHORT" => "SWORD",
        "ITEM_WEAPON_MACE" => "MACE",
        "ITEM_WEAPON_AXE_BATTLE" => "AXE",
        _ => "WHIP",
    }
}

/// Children and babies (DF professions `CHILD`, `BABY`; age below 12 otherwise).
pub(crate) fn is_child(u: &Unit) -> bool {
    matches!(u.profession.as_str(), "CHILD" | "BABY") || (u.age > 0.0 && u.age < 12.0 && u.race == "DWARF")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Item, ItemFlags, World};

    /// Synthetic fort (hall underground, surface above) with invasions off.
    fn fort(seed: u64) -> (Sim, i16, i16) {
        let mut w = World::synthetic(48, 48, 20);
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        let hall_z = w.units[0].pos.z;
        let surface = (20i16 * 2 / 3).max(2);
        let mut sim = Sim::new(w, seed);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        (sim, hall_z, surface)
    }

    fn texts(sim: &mut Sim) -> Vec<String> {
        sim.drain_announcements().into_iter().map(|a| a.text).collect()
    }

    fn goblet(sim: &mut Sim, p: Pos) -> i32 {
        let id = sim.next_item_id;
        sim.next_item_id += 1;
        let item = Item { id, kind: "GOBLET".into(), mat: 0, pos: p, stack: 1, value: 600, container: -1, holder: -1, building: -1, desc: "gold goblet".into(), flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() };
        sim.push_item(item);
        id
    }

    #[test]
    fn ambush_is_hidden_until_spotted() {
        let (mut sim, _hz, _sz) = fort(41);
        let n = sim.force_raid(RaidKind::Ambush, 4, 0);
        assert_eq!(n, 4);
        assert!(sim.hostiles().is_empty(), "hidden ambushers are not listed");
        let ids: Vec<i32> = sim.world.units.iter().filter(|u| u.race == "GOBLIN").map(|u| u.id).collect();
        assert!(ids.iter().all(|&id| sim.unit_hidden(id)));
        let mut fled_while_hidden = false;
        let mut seen = Vec::new();
        for _ in 0..300 {
            sim.run_ticks(20);
            seen.extend(texts(&mut sim));
            if sim.combat.stats.raids_spotted > 0 {
                break;
            }
            fled_while_hidden |= sim.combat.f.iter().any(|f| f.role == Role::Civilian);
        }
        assert!(!fled_while_hidden, "civilians do not see hidden ambushers");
        assert!(seen.iter().any(|t| t == "An ambush! Curse them!"), "DF announcement: {seen:?}");
        assert_eq!(sim.hostiles().len(), 4, "revealed ambushers are listed");
        assert!(ids.iter().all(|&id| !sim.unit_hidden(id)));
        assert_eq!(sim.combat.stats.ambushes, 1);
    }

    #[test]
    fn thief_steals_a_valuable_and_escapes() {
        let (mut sim, _hz, sz) = fort(43);
        // A gold goblet on the surface, far from the dwarves (who stay underground).
        let id = goblet(&mut sim, Pos::new(6, 40, sz));
        let n = sim.force_raid(RaidKind::Thief, 1, 0);
        assert_eq!(n, 1);
        let thief = *sim.combat.inv.hidden.first().unwrap() as usize;
        assert_eq!(sim.world.units[sim.ai[thief].uidx].race, "KOBOLD");
        for _ in 0..400 {
            sim.run_ticks(25);
            if sim.ai[thief].kind == Kind::Dead {
                break;
            }
        }
        assert_eq!(sim.combat.stats.items_stolen, 1, "the thief grabbed the goblet");
        assert_eq!(sim.combat.stats.items_lost, 1, "and carried it off the map");
        assert!(sim.item_idx.get(&id).map(|&i| sim.imeta[i as usize].dead).unwrap_or(true), "the goblet is gone");
        assert!(sim.world.units[sim.ai[thief].uidx].flags.inactive, "the thief left");
    }

    #[test]
    fn spotted_thief_flees_without_fighting() {
        let (mut sim, hz, _sz) = fort(44);
        // The goblet lies in the hall, among the dwarves: the thief is seen.
        let p = sim.world.units[0].pos;
        goblet(&mut sim, Pos::new(p.x + 1, p.y + 1, hz));
        sim.force_raid(RaidKind::Thief, 1, 0);
        let mut seen = Vec::new();
        for _ in 0..400 {
            sim.run_ticks(25);
            seen.extend(texts(&mut sim));
            if sim.combat.stats.raids_spotted > 0 {
                break;
            }
        }
        assert!(seen.iter().any(|t| t == "Thief! Protect the hoard from skulking filth!"), "{seen:?}");
        let thief = sim.combat.inv.sieges[0].members[0] as usize;
        sim.run_ticks(20);
        assert!(sim.combat.f[thief].role == Role::Leave || sim.ai[thief].kind == Kind::Dead, "a spotted thief runs");
    }

    fn child_on_surface(sim: &mut Sim, sz: i16) -> (usize, usize) {
        let cs = sim.unit_slot[&7];
        let uidx = sim.ai[cs].uidx;
        sim.world.units[uidx].profession = "CHILD".into();
        sim.world.units[uidx].age = 6.0;
        let q = sim.world.idx(Pos::new(10, 10, sz)).unwrap() as u32;
        sim.move_to(cs, q, 0);
        sim.ai[cs].wake = u64::MAX - 1; // plays right there
        (cs, uidx)
    }

    #[test]
    fn snatcher_carries_a_child_off_the_map() {
        let (mut sim, _hz, sz) = fort(45);
        let (cs, uidx) = child_on_surface(&mut sim, sz);
        sim.force_raid(RaidKind::Snatcher, 1, 0);
        let sn = sim.combat.inv.hidden[0] as usize;
        let mut grabbed = false;
        let mut seen = Vec::new();
        for _ in 0..600 {
            sim.run_ticks(20);
            seen.extend(texts(&mut sim));
            grabbed |= sim.combat.f[cs].carried_by == sn as u32;
            if sim.combat.stats.children_abducted > 0 {
                break;
            }
        }
        assert!(grabbed, "the snatcher picked the child up");
        assert_eq!(sim.combat.stats.children_abducted, 1);
        assert!(sim.world.units[uidx].flags.inactive, "the child is gone");
        assert!(seen.iter().any(|t| t.ends_with("has been abducted!")), "{seen:?}");
    }

    #[test]
    fn killed_snatcher_drops_the_child() {
        let (mut sim, _hz, sz) = fort(46);
        let (cs, uidx) = child_on_surface(&mut sim, sz);
        sim.force_raid(RaidKind::Snatcher, 1, 0);
        let sn = sim.combat.inv.hidden[0] as usize;
        for _ in 0..600 {
            sim.run_ticks(10);
            if sim.combat.f[cs].carried_by != NONE {
                break;
            }
        }
        assert_eq!(sim.combat.f[cs].carried_by, sn as u32);
        sim.run_ticks(30);
        assert_eq!(sim.ai[cs].pos, sim.ai[sn].pos, "the child moves with the snatcher");
        let now = sim.ticks;
        sim.combat_kill(sn, now, "has been struck down");
        assert_eq!(sim.combat.f[cs].carried_by, NONE, "dropped");
        assert_eq!(sim.combat.carriers, 0);
        sim.run_ticks(50);
        assert!(!sim.world.units[uidx].flags.inactive && sim.ai[cs].kind == Kind::Worker, "the child is back");
    }

    #[test]
    fn siege_camps_then_charges_with_monsters() {
        let (mut sim, _hz, _sz) = fort(47);
        let n = sim.force_raid(RaidKind::Siege, 6, 2);
        assert_eq!(n, 8);
        let g = &sim.combat.inv.sieges[0];
        let (camp, charge_at) = (sim.world.pos_of(g.camp as usize), g.charge_at);
        assert!(charge_at >= sim.ticks + 600, "goblins gather before the attack");
        let races: Vec<String> = g.members.iter().map(|&s| sim.world.units[sim.ai[s as usize].uidx].race.clone()).collect();
        assert_eq!(races.iter().filter(|r| *r == "TROLL").count(), 1);
        assert_eq!(races.iter().filter(|r| *r == "BEAK_DOG").count(), 1);
        assert_eq!(sim.combat.stats.monsters_spawned, 2);
        assert_eq!(sim.combat.stats.siege_camps, 1);
        let members = g.members.clone();
        while sim.ticks + 50 < charge_at {
            sim.run_ticks(50);
            for &m in &members {
                let p = sim.upos(m as usize);
                assert!(p.dist(camp) <= 8 || sim.combat.f[m as usize].target != NONE, "camper {p:?} far from camp {camp:?}");
            }
        }
        sim.run_ticks(4000);
        assert!(sim.combat.inv.sieges.is_empty() || sim.combat.inv.sieges[0].charged);
        assert!(sim.combat.stats.attacks > 0, "the siege attacked after the camp");
    }

    #[test]
    fn a_dead_leader_breaks_the_siege() {
        let (mut sim, _hz, _sz) = fort(48);
        sim.force_raid(RaidKind::Siege, 8, 0);
        let leader = sim.combat.inv.sieges[0].leader;
        assert_ne!(leader, NONE);
        let now = sim.ticks;
        sim.combat_kill(leader as usize, now, "has been struck down");
        sim.run_ticks(rf_core::TICKS_PER_DAY as u32 + 5);
        assert!(sim.combat.inv.sieges[0].fleeing, "the siege breaks with its leader");
        assert_eq!(sim.combat.stats.leaders_killed, 1);
    }

    #[test]
    fn raids_are_deterministic_across_threads() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            pool.install(|| {
                let (mut sim, _, _) = fort(49);
                sim.set_parallel_threshold(1);
                sim.force_raid(RaidKind::Ambush, 5, 0);
                sim.force_raid(RaidKind::Thief, 1, 0);
                sim.run_ticks(5000);
                (sim.state_hash(), sim.combat.stats.attacks, sim.combat.stats.raids_spotted)
            })
        };
        assert_eq!(run(1), run(4));
    }
}
