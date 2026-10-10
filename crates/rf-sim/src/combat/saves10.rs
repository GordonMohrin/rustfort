//! [saves10] Save/load of the combat module: raid groups (sieges, ambushes, thieves,
//! snatchers) with camps, charge and retreat timers and leaders; hidden raiders and scheduled
//! visits; and per unit the combat state that is not mirrored into `rf_core::Unit`: role
//! (hostile, soldier, fleeing civilian, leaving), hidden flag, raid membership, loot, carried
//! units, leave timers, building destroyers, patrol waypoint, training spot, unconsciousness,
//! the sim-side wounds, blood and broken parts.

use super::{Fighter, Role, Sub, WoundS};
use crate::military::Duty;
use crate::persist::{get, put};
use crate::raids::RaidKind;
use crate::{Kind, Sim, NONE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "combat";

fn role_code(r: Role) -> u8 {
    match r {
        Role::None => 0,
        Role::Hostile => 1,
        Role::Soldier => 2,
        Role::Civilian => 3,
        Role::Fighter => 4,
        Role::Patient => 5,
        Role::Leave => 6,
    }
}

fn role_of(c: u8) -> Role {
    match c {
        1 => Role::Hostile,
        2 => Role::Soldier,
        3 => Role::Civilian,
        4 => Role::Fighter,
        5 => Role::Patient,
        6 => Role::Leave,
        _ => Role::None,
    }
}

fn sub_enc(s: Sub) -> (u8, i32) {
    match s {
        Sub::None => (0, 0),
        Sub::Equip(x) => (1, x),
        Sub::Rest(x) => (2, x),
        Sub::Resting(x) => (3, x),
    }
}

fn sub_dec(c: (u8, i32)) -> Sub {
    match c.0 {
        1 => Sub::Equip(c.1),
        2 => Sub::Rest(c.1),
        3 => Sub::Resting(c.1),
        _ => Sub::None,
    }
}

fn duty_enc(d: Duty) -> (u8, i64) {
    match d {
        Duty::None => (0, 0),
        Duty::Train => (1, 0),
        Duty::Station(t) => (2, t as i64),
        Duty::Kill(id) => (3, id as i64),
        Duty::Patrol => (4, 0),
    }
}

fn duty_dec(c: (u8, i64)) -> Duty {
    match c.0 {
        1 => Duty::Train,
        2 => Duty::Station(c.1 as u32),
        3 => Duty::Kill(c.1 as i32),
        4 => Duty::Patrol,
        _ => Duty::None,
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct FighterS {
    slot: u32,
    uid: i32,
    role: u8,
    sub: (u8, i32),
    target: u32,
    next_atk: u64,
    uncon_until: u64,
    paralyzed: bool,
    threat: u32,
    calm_since: u64,
    equip_at: u64,
    train_at: u64,
    siege: u16,
    home: u32,
    duty: (u8, i64),
    squad: i32,
    /// (part, layer, flags, severity, bleed, pain, age).
    wounds: Vec<(u16, u16, u16, i32, i32, i32, i32)>,
    part_state: Vec<u8>,
    bleed: i32,
    blood: i32,
    blood_max: i32,
    dormant_until: u64,
    syn_until: u64,
    hidden: bool,
    raid: u8,
    want: i32,
    loot: i32,
    carry: u32,
    carried_by: u32,
    patrol_i: u16,
    spot: u32,
    crawl: bool,
    leave_since: u64,
    brk_until: u64,
    wreck: i32,
    wreck_until: u64,
    wreck_at: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SiegeS {
    members: Vec<u32>,
    start: u64,
    fleeing: bool,
    race: String,
    id: u16,
    kind: u8,
    camp: u32,
    charge_at: u64,
    charged: bool,
    leader: u32,
    revealed: bool,
    fled_at: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct CombatS {
    fighters: Vec<FighterS>,
    hostiles: Vec<u32>,
    bleeders: Vec<u32>,
    wounded: Vec<u32>,
    sieges: Vec<SiegeS>,
    enabled: bool,
    last_season: i32,
    next_id: u16,
    hidden: Vec<u32>,
    pending: Vec<(u64, u8)>,
    exported_base: i64,
    exported_base_set: bool,
    log_total: u64,
}

impl Sim {
    pub(crate) fn raids_save(&self, m: &mut BTreeMap<String, String>) {
        let c = &self.combat;
        let d = Fighter::default();
        let mut fighters = Vec::new();
        for (s, f) in c.f.iter().enumerate() {
            if s >= self.ai.len() || self.ai[s].kind == Kind::Dead {
                continue;
            }
            // Skip untouched units (most animals and calm citizens).
            let plain = f.role == d.role
                && f.sub == d.sub
                && f.wounds.is_empty()
                && !f.hidden
                && f.raid == 0
                && f.carry == NONE
                && f.carried_by == NONE
                && f.leave_since == 0
                && f.wreck < 0
                && f.uncon_until == 0
                && f.syn_until == 0
                && f.patrol_i == 0
                && f.spot == NONE
                && f.duty == d.duty
                && f.squad == d.squad
                && f.siege == 0
                && f.bleed == 0
                && f.blood == f.blood_max
                && f.part_state.iter().all(|&p| p == 0);
            if plain {
                continue;
            }
            fighters.push(FighterS {
                slot: s as u32,
                uid: self.ai[s].id,
                role: role_code(f.role),
                sub: sub_enc(f.sub),
                target: f.target,
                next_atk: f.next_atk,
                uncon_until: f.uncon_until,
                paralyzed: f.paralyzed,
                threat: f.threat,
                calm_since: f.calm_since,
                equip_at: f.equip_at,
                train_at: f.train_at,
                siege: f.siege,
                home: f.home,
                duty: duty_enc(f.duty),
                squad: f.squad,
                wounds: f.wounds.iter().map(|w| (w.part, w.layer, w.flags, w.sev, w.bleed, w.pain, w.age)).collect(),
                part_state: f.part_state.clone(),
                bleed: f.bleed,
                blood: f.blood,
                blood_max: f.blood_max,
                dormant_until: f.dormant_until,
                syn_until: f.syn_until,
                hidden: f.hidden,
                raid: f.raid,
                want: f.want,
                loot: f.loot,
                carry: f.carry,
                carried_by: f.carried_by,
                patrol_i: f.patrol_i,
                spot: f.spot,
                crawl: f.crawl,
                leave_since: f.leave_since,
                brk_until: f.brk_until,
                wreck: f.wreck,
                wreck_until: f.wreck_until,
                wreck_at: f.wreck_at,
            });
        }
        let inv = &c.inv;
        let s = CombatS {
            fighters,
            hostiles: c.hostiles.clone(),
            bleeders: c.bleeders.clone(),
            wounded: c.wounded.clone(),
            sieges: inv
                .sieges
                .iter()
                .map(|g| SiegeS {
                    members: g.members.clone(),
                    start: g.start,
                    fleeing: g.fleeing,
                    race: g.race.clone(),
                    id: g.id,
                    kind: g.kind.code(),
                    camp: g.camp,
                    charge_at: g.charge_at,
                    charged: g.charged,
                    leader: g.leader,
                    revealed: g.revealed,
                    fled_at: g.fled_at,
                })
                .collect(),
            enabled: inv.enabled,
            last_season: inv.last_season,
            next_id: inv.next_id,
            hidden: inv.hidden.clone(),
            pending: inv.pending.iter().map(|&(t, k)| (t, k.code())).collect(),
            exported_base: inv.exported_base,
            exported_base_set: inv.exported_base_set,
            log_total: c.log_total,
        };
        put(m, KEY, &s);
    }

    pub(crate) fn raids_restore(&mut self, m: &BTreeMap<String, String>) {
        let Some(s) = get::<CombatS>(m, KEY) else { return };
        let n = self.ai.len();
        if self.combat.f.len() < n {
            self.combat_sync_units();
        }
        let ok = |slot: u32| (slot as usize) < n;
        for fs in s.fighters {
            let sl = fs.slot as usize;
            if !ok(fs.slot) || self.ai[sl].id != fs.uid {
                continue;
            }
            if !fs.part_state.is_empty() {
                self.ensure_body(sl);
            }
            let f = &mut self.combat.f[sl];
            f.role = role_of(fs.role);
            f.sub = sub_dec(fs.sub);
            f.target = fs.target;
            f.next_atk = fs.next_atk;
            f.uncon_until = fs.uncon_until;
            f.paralyzed = fs.paralyzed;
            f.threat = fs.threat;
            f.calm_since = fs.calm_since;
            f.equip_at = fs.equip_at;
            f.train_at = fs.train_at;
            f.siege = fs.siege;
            f.home = fs.home;
            f.duty = duty_dec(fs.duty);
            f.squad = fs.squad;
            f.wounds = fs.wounds.iter().map(|w| WoundS { part: w.0, layer: w.1, flags: w.2, sev: w.3, bleed: w.4, pain: w.5, age: w.6 }).collect();
            if fs.part_state.len() == f.part_state.len() {
                f.part_state = fs.part_state;
            }
            f.bleed = fs.bleed;
            f.blood = fs.blood;
            f.blood_max = fs.blood_max;
            f.dormant_until = fs.dormant_until;
            f.syn_until = fs.syn_until;
            f.hidden = fs.hidden;
            f.raid = fs.raid;
            f.want = fs.want;
            f.loot = fs.loot;
            f.carry = if ok(fs.carry) { fs.carry } else { NONE };
            f.carried_by = if ok(fs.carried_by) { fs.carried_by } else { NONE };
            f.patrol_i = fs.patrol_i;
            f.spot = fs.spot;
            f.crawl = fs.crawl;
            f.leave_since = fs.leave_since;
            f.brk_until = fs.brk_until;
            f.wreck = fs.wreck;
            f.wreck_until = fs.wreck_until;
            f.wreck_at = fs.wreck_at;
        }
        let keep = |v: Vec<u32>| -> Vec<u32> {
            let mut v: Vec<u32> = v.into_iter().filter(|&x| ok(x)).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        self.combat.hostiles = keep(s.hostiles);
        self.combat.bleeders = keep(s.bleeders);
        self.combat.wounded = keep(s.wounded);
        self.combat.carriers = self.combat.f.iter().filter(|f| f.carry != NONE).count() as u32;
        self.combat.log_total = s.log_total;
        let inv = &mut self.combat.inv;
        inv.sieges = s
            .sieges
            .into_iter()
            .map(|g| crate::invasion::Siege {
                members: g.members.into_iter().filter(|&x| ok(x)).collect(),
                start: g.start,
                fleeing: g.fleeing,
                race: g.race,
                id: g.id,
                kind: RaidKind::of(g.kind).unwrap_or_default(),
                camp: g.camp,
                charge_at: g.charge_at,
                charged: g.charged,
                leader: if ok(g.leader) { g.leader } else { NONE },
                revealed: g.revealed,
                fled_at: g.fled_at,
            })
            .collect();
        inv.enabled = s.enabled;
        inv.last_season = s.last_season;
        inv.next_id = s.next_id;
        inv.hidden = keep(s.hidden);
        inv.pending = s.pending.into_iter().filter_map(|(t, k)| RaidKind::of(k).map(|k| (t, k))).collect();
        inv.exported_base = s.exported_base;
        inv.exported_base_set = s.exported_base_set;
        self.rebuild_fort_list();
    }
}
