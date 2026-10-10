//! [war9] Round 9 war calibration against Gordon's real history (docs/VALIDATION.md items 1, 6
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
//! # Estimates (calibrated to Gordon's fort, see docs/VALIDATION.md)
//! - One siege regular costs 15 strength points: the 5th siege (2 000) brings 133 creatures,
//!   DF's 5th siege on Trussroads brought 136 goblins (r6 export). The wiki tier forces (Town 10,
//!   City 40 + 5, Metropolis 120 + 40) cap the size by tier; at least 10 regulars.
//! - Monsters (trolls, beak dogs): one per 20 creatures, at most the tier's monsters. Mounts
//!   (from strength 1 000 on, DF's "mounted" troops from the 3rd siege on): one per 20 regulars
//!   (r6: 7 mounts for 136 goblins); the mount race is a `MOUNT_EXOTIC` cave creature of the
//!   save's raws if there is one (Gordon's goblins rode voracious cave crawlers), else beak dogs.
//! - Master troops: strength / 160 percent of the regulars, at most 25 % (r6: 17 of 136 at
//!   strength 2 000); weapon skill 10-14, regulars 3-9 (r6 means: sword 6.3, mace 9.5).
//! - 1-5 ambush squads of 3-6 arrive with each siege (gamelog: 1, 5, 5, 1, 4 "An ambush!" lines
//!   right after the five sieges), at most a third of the regulars; they count as raids.
//! - Invasion chance per season start: 2/3 (DF's real fort: a siege every 1.25-2 years).
//! - Forgotten-beast chance per season and available layer: 1/4 x the irritation share. Cavern
//!   layers already discovered when a fort is imported count as spent (an established fort has
//!   met its region's beasts; DF sent none in 3.5 years of Gordon's history with 3 caverns open).
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
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, Default)]
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

    /// Mount race of the invaders: a `MOUNT_EXOTIC` cave creature of the save's raws, else
    /// beak dogs (wiki: goblins ride beak dogs).
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
