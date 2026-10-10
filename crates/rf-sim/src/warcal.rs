//! [war9] Round 9 war calibration against the reference fort's real history (docs/VALIDATION.md items 1, 6
//! and 13): created and exported wealth, siege strength per consecutive siege, siege composition
//! (masters, mounts, monsters, ambush squads alongside), forgotten beasts per cavern layer and
//! the persistent counters in `World::fort_history`.
//!
//! # DF rules implemented (wiki v50: Difficulty, Siege, Forgotten beast, Building destroyer)
//! - **Invasion tier** = the highest level whose population trigger **or** production trigger
//!   (created wealth) **or** trade trigger (exported wealth) the fort meets (wiki Difficulty:
//!   "uses population tier OR (production OR trade)"). Triggers come from the save's
//!   `ENEMY_POP_TRIGGER` / `ENEMY_PROD_TRIGGER` / `ENEMY_TRADE_TRIGGER` (Normal defaults
//!   20/50/80/110/140, 5k/25k/100k/200k/300k, 500/2.5k/10k/20k/30k).
//! - **Siege strength** grows with each siege a civilization sends: `INVASION_DIFFICULTY_FIRST`,
//!   `_SECOND`, `_THIRD`, then `+ _SCALING` per siege (Normal 50/300/1000/+500). The number of
//!   sieges a civilization already sent is DF's `entity.siege_tier` (exported in
//!   `entities.json`: 2 at r4, 4 at r5, 5 at r6, exactly the sieges in the gamelog).
//! - **Invasion creature cap** `INVASION_UNIT_CAP` (Normal 140) "excluding mounts".
//! - **Siege frequency** `SIEGE_FREQUENCY` (Normal 25 %): the chance an invasion is a siege
//!   rather than a raid (ambush); `MIN_RAIDS_BEFORE_SIEGE` (2) and `MIN_RAIDS_BETWEEN_SIEGES`
//!   (3) ambush squads per civilization must come first.
//! - **Created wealth** counts everything the fort makes: each made item adds its value at
//!   creation (wiki Item value: base value x material value x quality multiplier).
//! - **Forgotten beasts**: "exactly one beast is spawned per each underground region", so each
//!   cavern layer sends at most one; they are attracted by wealth and noise: fortress wealth /
//!   `FORGOTTEN_WEALTH_DIV` (40) counts as irritation, and nothing comes at or below
//!   `FORGOTTEN_IRRITATE_MIN` (5 000); `FORGOTTEN_SENS` (10 000) more reaches the maximum chance.
//!
//! # Estimates (calibrated to the reference fort, see docs/VALIDATION.md)
//! - One siege regular costs 15 strength points: the 5th siege (2 000) brings 133 creatures,
//!   DF's 5th siege on Trussroads brought 136 goblins (r6 export). The wiki tier forces (Town 10,
//!   City 40 + 5, Metropolis 120 + 40) cap the size by tier; at least 10 regulars.
//! - Monsters (trolls, beak dogs): one per 20 creatures, at most the tier's monsters. Mounts
//!   (from strength 1 000 on, DF's "mounted" troops from the 3rd siege on): one per 20 regulars
//!   (r6: 7 mounts for 136 goblins); the mount race is a `MOUNT_EXOTIC` cave creature of the
//!   save's raws if there is one (the reference fort's goblins rode voracious cave crawlers), else beak dogs.
//! - Master troops: strength / 160 percent of the regulars, at most 25 % (r6: 17 of 136 at
//!   strength 2 000); weapon skill 10-14, regulars 3-9 (r6 means: sword 6.3, mace 9.5).
//! - 1-5 ambush squads of 3-6 arrive with each siege (gamelog: 1, 5, 5, 1, 4 "An ambush!" lines
//!   right after the five sieges), at most a third of the regulars; they count as raids.
//! - Invasion chance per season start: 2/3 (DF's real fort: a siege every 1.25-2 years).
//! - Forgotten-beast chance per season and available layer: 1/4 x the irritation share. Cavern
//!   layers already discovered when a fort is imported count as spent (an established fort has
//!   met its region's beasts; DF sent none in 3.5 years of the reference fort's history with 3 caverns open).
//! - Stackable made items keep `value` 0 (the trade formula then follows the stack size), so a
//!   split-off part of such a stack can be counted again; durable goods get their value stored.

use crate::Sim;
use rf_core::Item;

/// Item kinds that are not made by the fort (natural goods and remains): no created wealth.
const NOT_MADE: &[&str] = &[
    "CORPSE", "CORPSEPIECE", "REMAINS", "BLOOD", "VOMIT", "BOULDER", "ROUGH", "WOOD", "PLANT", "PLANT_GROWTH", "SEEDS", "EGG", "VERMIN", "PET", "GLOB",
];

/// Strength points per siege creature (estimate, see the module docs).
pub const UNIT_COST: i64 = 15;

/// One planned siege (see [`Sim::siege_plan`]).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)] // [saves11]
#[serde(default)] // [saves11]
pub struct SiegePlan {
    /// Invading civilization (entity id; 9 999 when no civilizations were exported).
    pub civ: i32,
    /// Number of this siege for the civilization (1 = first).
    pub number: i32,
    pub strength: i64,
    pub tier: usize,
    /// Regulars, including the ambush squads.
    pub regulars: usize,
    pub monsters: usize,
    pub mounts: usize,
    /// Share of master troops among the regulars, percent.
    pub masters_pct: u32,
}

/// Extra parameters of a raid group (`raids::spawn_raid_ex`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct RaidExtra {
    pub mounts: usize,
    pub masters_pct: u32,
    pub civ: Option<i32>,
}

/// Strength of the `n`-th siege (1 = first) from the difficulty settings.
pub fn siege_strength(n: i32, first: i64, second: i64, third: i64, scaling: i64) -> i64 {
    match n {
        i32::MIN..=1 => first,
        2 => second,
        3 => third,
        _ => third + (n as i64 - 3) * scaling,
    }
}

/// Siege creatures (regulars, monsters, mounts) for a strength at a tier (3-5), capped at
/// `cap` creatures without mounts.
pub fn siege_units(strength: i64, tier: usize, cap: usize) -> (usize, usize, usize) {
    if tier < 3 {
        return (0, 0, 0);
    }
    let (reg_t, mon_t) = crate::invasion::SIEGE_FORCE[tier.min(5) - 1];
    let tier_max = (reg_t + mon_t).min(cap.max(1));
    let total = ((strength / UNIT_COST).max(0) as usize).max(reg_t.min(10)).min(tier_max);
    let monsters = (total / 20).min(mon_t);
    let regulars = total - monsters;
    let mounts = if strength >= 1_000 { regulars / 20 } else { 0 };
    (regulars, monsters, mounts)
}

impl Sim {
    /// Integer difficulty setting `KEY=a,b,c` of the save (`idx`-th value), else `default`.
    pub(crate) fn war_setting(&self, key: &str, idx: usize, default: i64) -> i64 {
        for s in &self.world.settings.other {
            if let Some(v) = s.strip_prefix(key).and_then(|r| r.strip_prefix('=')) {
                if let Some(x) = v.split(',').nth(idx).and_then(|x| x.trim().parse::<i64>().ok()) {
                    return x;
                }
            }
        }
        default
    }

    /// Invasion trigger levels (population, created, exported) of the save, Normal defaults.
    pub(crate) fn war_triggers(&self) -> ([usize; 5], [i64; 5], [i64; 5]) {
        let mut p = crate::invasion::TIER_POPULATION;
        let mut c = crate::invasion::TIER_CREATED;
        let mut e = crate::invasion::TIER_EXPORTED;
        for k in 0..5 {
            p[k] = self.war_setting("ENEMY_POP_TRIGGER", k, p[k] as i64).max(0) as usize;
            c[k] = self.war_setting("ENEMY_PROD_TRIGGER", k, c[k]);
            e[k] = self.war_setting("ENEMY_TRADE_TRIGGER", k, e[k]);
        }
        (p, c, e)
    }

    /// Invasion tier of the fort now (0-5), with the save's triggers.
    pub fn invasion_tier_now(&self) -> usize {
        let (p, c, e) = self.war_triggers();
        let pop = self.world.citizens().count();
        let (cw, ew) = (self.created_wealth(), self.exported_wealth());
        (0..5).rev().find(|&k| pop >= p[k] || cw >= c[k] || ew >= e[k]).map(|k| k + 1).unwrap_or(0)
    }

    /// The goblin civilization at war that sieges the fort (most sieges so far first).
    pub(crate) fn war_enemy_civ(&self) -> i32 {
        let h = &self.world.fort_history;
        self.world
            .entities
            .iter()
            .filter(|e| e.race == "GOBLIN" && e.relation == "War" && (e.kind == "Civilization" || e.kind.is_empty()))
            .max_by_key(|e| (h.sieges.iter().find(|x| x.0 == e.id).map(|x| x.1).unwrap_or(0), std::cmp::Reverse(e.id)))
            .map(|e| e.id)
            .unwrap_or(9_999)
    }

    pub(crate) fn war_count(list: &[(i32, i32)], civ: i32) -> i32 {
        list.iter().find(|x| x.0 == civ).map(|x| x.1).unwrap_or(0)
    }

    pub(crate) fn war_bump(list: &mut Vec<(i32, i32)>, civ: i32, set: Option<i32>, add: i32) {
        match list.iter_mut().find(|x| x.0 == civ) {
            Some(x) => x.1 = set.unwrap_or(x.1 + add),
            None => list.push((civ, set.unwrap_or(add))),
        }
    }

    /// The siege the fort would get now from its enemy civilization (`None`: below the siege
    /// tier). Strength from the civilization's siege count, size from strength and tier.
    pub fn siege_plan(&self) -> Option<SiegePlan> {
        let tier = self.invasion_tier_now();
        if tier < 3 {
            return None;
        }
        let civ = self.war_enemy_civ();
        let number = Self::war_count(&self.world.fort_history.sieges, civ) + 1;
        let strength = siege_strength(
            number,
            self.war_setting("INVASION_DIFFICULTY_FIRST", 0, 50),
            self.war_setting("INVASION_DIFFICULTY_SECOND", 0, 300),
            self.war_setting("INVASION_DIFFICULTY_THIRD", 0, 1_000),
            self.war_setting("INVASION_DIFFICULTY_SCALING", 0, 500),
        );
        let cap = self.war_setting("INVASION_UNIT_CAP", 0, 140).max(1) as usize;
        let (regulars, monsters, mounts) = siege_units(strength, tier, cap);
        let masters_pct = ((strength / 160).clamp(0, 25)) as u32;
        Some(SiegePlan { civ, number, strength, tier, regulars, monsters, mounts, masters_pct })
    }

    /// Mount race of the invaders: the first `MOUNT_EXOTIC` cave creature in the save's raws
    /// (DF's raw order; the reference world: voracious cave crawlers, as in DF's r6 siege), else beak
    /// dogs (wiki: goblins ride beak dogs).
    pub(crate) fn war_mount_race(&self) -> String {
        self.world
            .creature_raws
            .iter()
            .find(|c| c.flags.iter().any(|f| f == "BIOME_SUBTERRANEAN_CHASM") && c.castes.iter().any(|k| k.flags.iter().any(|f| f == "MOUNT_EXOTIC")))
            .map(|c| c.token.clone())
            .unwrap_or_else(|| "BEAK_DOG".into())
    }

    /// Seed the history counters once: created wealth from the imported items, cavern layers
    /// discovered at import count as having sent their forgotten beast (see the estimates).
    pub(crate) fn war_seed(&mut self) {
        if self.world.fort_history.seeded {
            return;
        }
        self.world.fort_history.seeded = true;
        let w = self.fort_wealth();
        self.world.fort_history.created_wealth += w;
        if !self.caverns.detected {
            self.caverns.detected = true;
            self.detect_caverns();
        }
        let spent: Vec<u8> = self.caverns.layers.iter().filter(|l| l.discovered() && !l.magma).map(|l| l.id).collect();
        for id in spent {
            if !self.world.fort_history.beast_layers.contains(&id) {
                self.world.fort_history.beast_layers.push(id);
            }
        }
        // Raids since the last siege are not exported: assume enough for the next siege.
        let civ = self.war_enemy_civ();
        if self.world.fort_history.raids.iter().all(|x| x.0 != civ) {
            let need = self.war_setting("MIN_RAIDS_BETWEEN_SIEGES", 0, 3) as i32;
            self.world.fort_history.raids.push((civ, need));
        }
    }

    /// Daily: keep the persistent exported-wealth counter in step with this session's trades.
    pub(crate) fn war_daily(&mut self) {
        if !self.combat.inv.exported_base_set {
            self.combat.inv.exported_base_set = true;
            self.combat.inv.exported_base = self.world.fort_history.exported_wealth;
        }
        self.war_seed();
        let session: i64 = self.trade.rep.trades.iter().map(|t| t.1.max(0)).sum();
        self.world.fort_history.exported_wealth = self.combat.inv.exported_base + session;
    }

    /// [push_item hook] A new item: if it was made by the fort, store its value (durable goods)
    /// and add it to the created wealth.
    pub(crate) fn war_item_made(&mut self, it: &mut Item) {
        if it.value != 0 || NOT_MADE.contains(&it.kind.as_str()) {
            return;
        }
        if it.holder >= 0 {
            // Invader gear and other non-citizens' items are not the fort's production.
            let citizen = self.unit_slot.get(&it.holder).map(|&s| self.world.units[self.ai[s].uidx].flags.citizen).unwrap_or(false);
            if !citizen {
                return;
            }
        }
        let mv = self.world.material(it.mat).map(|m| m.value).filter(|&v| v > 0).unwrap_or(1) as i64;
        let v = crate::trade::base_value(&it.kind) * mv * crate::trade::quality_mult(it.quality) * it.stack.max(1) as i64;
        if it.stack <= 1 {
            it.value = v.min(i32::MAX as i64) as i32;
        }
        self.world.fort_history.created_wealth += v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Role;
    use crate::raids::RaidKind;
    use crate::{Kind, NONE};
    use rf_core::{Building, ItemFlags, Pos, World};

    /// Synthetic fort (hall underground, surface above) with invasions off.
    fn fort(seed: u64) -> (Sim, i16) {
        let mut w = World::synthetic(48, 48, 20);
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        let surface = (20i16 * 2 / 3).max(2);
        let mut sim = Sim::new(w, seed);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        (sim, surface)
    }

    #[test]
    fn made_items_add_created_wealth() {
        let (mut sim, sz) = fort(61);
        sim.run_ticks(1_300); // seeds the history on the first day
        assert!(sim.world.fort_history.seeded);
        let before = sim.created_wealth();
        let mk = |sim: &mut Sim, kind: &str, holder: i32| {
            let id = sim.next_item_id;
            sim.next_item_id += 1;
            let it = Item { id, kind: kind.into(), mat: 0, pos: Pos::new(5, 5, sz), stack: 1, quality: 2, container: -1, holder, building: -1, flags: ItemFlags { on_ground: holder < 0, ..Default::default() }, ..Default::default() };
            let i = sim.push_item(it);
            sim.world.items[i].value
        };
        let v = mk(&mut sim, "FIGURINE", -1);
        // base 10 x granite value x quality 2 (x3).
        let mv = sim.world.materials[0].value.max(1) as i64;
        assert_eq!(v as i64, 10 * mv * 3, "durable goods store their value");
        assert_eq!(sim.created_wealth(), before + v as i64);
        // Corpses and invader gear are not production.
        mk(&mut sim, "CORPSE", -1);
        assert_eq!(sim.created_wealth(), before + v as i64);
    }

    #[test]
    fn natural_siege_follows_the_civs_siege_count() {
        let (mut sim, _sz) = fort(62);
        sim.run_ticks(1_300);
        // The reference fort at r5: Metropolis tier by created wealth, 4 sieges so far.
        sim.world.fort_history.created_wealth = 3_200_000;
        sim.world.fort_history.sieges = vec![(9_999, 4)];
        let p = sim.siege_plan().expect("siege tier");
        assert_eq!((p.tier, p.number, p.strength), (5, 5, 2_000));
        assert_eq!(p.regulars + p.monsters, 133);
        let mut r = crate::rng::Rng::for_unit(1, 2, 3, 4);
        let now = sim.ticks;
        let n = sim.war_spawn_siege(&p, now, &mut r, true);
        assert_eq!(n, p.regulars + p.monsters + p.mounts, "every creature arrived");
        assert_eq!(Sim::war_count(&sim.world.fort_history.sieges, 9_999), 5);
        let st = sim.combat_stats();
        assert_eq!(st.sieges, 1);
        assert!(st.siege_ambush_squads >= 1 && st.ambushes == st.siege_ambush_squads, "ambush squads come along");
        assert_eq!(st.mounts_spawned as usize, p.mounts);
        assert_eq!(Sim::war_count(&sim.world.fort_history.raids, 9_999), st.siege_ambush_squads as i32);
        let masters = sim.world.units.iter().filter(|u| u.profession.starts_with("MASTER_")).count();
        assert!(masters >= 4 && masters <= 35, "about 12 % masters: {masters}");
    }

    #[test]
    fn destroyer_wrecks_a_nearby_workshop() {
        let (mut sim, sz) = fort(63);
        let mut b = Building { id: 9_001, kind: "Workshop".into(), subtype: "Carpenters".into(), x1: 11, y1: 9, x2: 13, y2: 11, z: sz, center: Pos::new(12, 10, sz), built: true, ..Default::default() };
        b.stage = -1;
        sim.world.buildings.push(b);
        assert_eq!(sim.spawn_raid(RaidKind::Siege, "TROLL", 1, 0, 0), 1);
        let t = sim.combat.inv.sieges[0].members[0] as usize;
        let q = sim.world.idx(Pos::new(6, 10, sz)).unwrap() as u32;
        let now = sim.ticks;
        sim.move_to(t, q, now);
        for _ in 0..100 {
            sim.run_ticks(20);
            if sim.combat_stats().buildings_wrecked > 0 {
                break;
            }
        }
        assert_eq!(sim.combat_stats().buildings_wrecked, 1);
        assert!(!sim.world.buildings.iter().any(|b| b.id == 9_001), "the workshop is gone");
    }

    #[test]
    fn stuck_leavers_leave_their_group() {
        let (mut sim, _sz) = fort(64);
        assert_eq!(sim.spawn_siege("GOBLIN", 2), 2);
        let m = sim.combat.inv.sieges[0].members.clone();
        let now = sim.ticks;
        sim.combat.inv.sieges[0].fleeing = true;
        sim.combat.inv.sieges[0].fled_at = now.max(1);
        for &s in &m {
            sim.combat.f[s as usize].role = Role::Leave;
        }
        // One is paralyzed in place (a trap victim): the r8 group never ended.
        sim.combat.f[m[0] as usize].paralyzed = true;
        sim.run_ticks((crate::invasion::LEAVE_GIVE_UP + 3 * rf_core::TICKS_PER_DAY as u64) as u32);
        assert!(sim.combat.inv.sieges.is_empty(), "the invasion ended");
        assert!(sim.combat_stats().leave_gave_up >= 1);
        let g = m[0] as usize;
        assert!(sim.ai[g].kind != Kind::Dead && sim.combat.f[g].role == Role::Hostile && sim.combat.f[g].siege == 0, "the helpless one stays as a plain hostile");
    }

    #[test]
    fn fleeing_civilian_breaks_off_to_drink() {
        let (mut sim, sz) = fort(65);
        assert_eq!(sim.spawn_siege("GOBLIN", 1), 1);
        let g = sim.combat.inv.sieges[0].members[0] as usize;
        let d = sim.unit_slot[&sim.world.units[0].id];
        let dp = sim.upos(d);
        // The goblin stands still 6 tiles away (on the dwarf's level).
        let dc = sim.nav.comp_near(sim.ai[d].pos as usize);
        let q = (-9i16..=9)
            .flat_map(|dy| (-9i16..=9).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| dx.abs().max(dy.abs()) >= 5)
            .filter_map(|(dx, dy)| sim.world.idx(Pos::new(dp.x + dx, dp.y + dy, dp.z)))
            .find(|&i| sim.nav.walkable(i) && sim.nav.comp_root(i) == dc)
            .expect("a hall tile 5+ tiles away") as u32;
        let now = sim.ticks;
        sim.move_to(g, q, now);
        sim.ai[g].wake = u64::MAX - 1;
        let _ = sz;
        sim.combat.f[d].role = Role::Civilian;
        sim.combat.f[d].threat = g as u32;
        sim.ai[d].thirst = crate::needs::THIRST_SEEK + 10;
        assert!(sim.upos(d).dist(sim.upos(g)) > 2);
        sim.engage_now(d, now);
        sim.run_ticks(3);
        assert_eq!(sim.combat_stats().flee_breaks, 1, "thirsty civilian goes to drink");
        assert_ne!(sim.combat.f[d].role, Role::Civilian);
        // On its way it only flees again once it comes within 2 tiles of the goblin.
        let mut was_free = true;
        for _ in 0..200 {
            sim.run_ticks(1);
            let grabbed = sim.combat.f[d].role == Role::Civilian && sim.combat.f[d].threat != NONE;
            if grabbed && was_free {
                assert!(sim.upos(d).dist(sim.upos(g)) <= 2, "grabbed again while the goblin was not adjacent");
            }
            was_free = !grabbed;
        }
    }

    #[test]
    fn siege_strength_follows_the_difficulty_sequence() {
        let s = |n| siege_strength(n, 50, 300, 1_000, 500);
        assert_eq!((s(1), s(2), s(3), s(4), s(5), s(6)), (50, 300, 1_000, 1_500, 2_000, 2_500));
        // Metropolis tier: the 5th siege is about the 136 goblins + 7 mounts of DF's r6 siege.
        let (reg, mon, mounts) = siege_units(s(5), 5, 140);
        assert_eq!(reg + mon, 133);
        assert!((5..=8).contains(&mounts), "{mounts}");
        assert!(mon <= 7);
        // Later sieges hit the 140 cap (mounts not counted).
        let (reg, mon, _) = siege_units(s(7), 5, 140);
        assert_eq!(reg + mon, 140);
        // First siege: at least 10 regulars; Town tier caps at 10, City at 45.
        assert_eq!(siege_units(s(1), 5, 140), (10, 0, 0));
        assert_eq!(siege_units(s(5), 3, 140), (10, 0, 0));
        assert_eq!(siege_units(s(5), 4, 140).0 + siege_units(s(5), 4, 140).1, 45);
        assert_eq!(siege_units(s(5), 2, 140), (0, 0, 0));
    }
}
