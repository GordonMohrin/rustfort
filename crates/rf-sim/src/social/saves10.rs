//! [saves10] Save/load of the social module: per unit the running breakdown (tantrum,
//! depression, obliviousness) and its end, the breakdown count, the recent meals and drinks
//! (variety thoughts), admired buildings, the thought timers and the eustress/distress
//! counters; plus the monthly stress history and the report counters. Stress, emotions and
//! needs live in `rf_core::Unit` and were saved before.

use super::{Breakdown, MonthStress};
use crate::persist::{get, put};
use crate::Sim;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "social";

fn bd_code(b: Breakdown) -> u8 {
    match b {
        Breakdown::None => 0,
        Breakdown::Tantrum => 1,
        Breakdown::Depressed => 2,
        Breakdown::Oblivious => 3,
    }
}

fn bd_of(c: u8) -> Breakdown {
    match c {
        1 => Breakdown::Tantrum,
        2 => Breakdown::Depressed,
        3 => Breakdown::Oblivious,
        _ => Breakdown::None,
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct PsyS {
    slot: u32,
    uid: i32,
    breakdown: u8,
    until: u64,
    breakdowns: u16,
    meals: [u16; 2],
    drinks: [u16; 2],
    seen: Vec<i32>,
    next_admire: u64,
    next_need_thought: u64,
    next_injury: u64,
    without_distress: i32,
    without_eustress: i32,
    /// [calib11-social] hardening to death (0..=100); 0 in older saves
    hardened: u8,
    /// [calib11-social] strongest grief memory (strength, divider, calendar tick)
    grief: (i32, i32, i64),
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct MonthS {
    year: i32,
    month: i32,
    counts: [u32; 7],
    mean_stress: i32,
    mean_longterm: i32,
    mean_focus: i32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SocialS {
    psy: Vec<PsyS>,
    last_month: i32,
    months: Vec<MonthS>,
    tantrums: u64,
    depressions: u64,
    oblivious: u64,
    insane: u64,
    meals_at_table: u64,
    meals_without_table: u64,
    prayers: u64,
    conversations: u64,
}

impl Sim {
    pub(crate) fn social_save(&self, m: &mut BTreeMap<String, String>) {
        let so = &self.social;
        let psy = so
            .psy
            .iter()
            .enumerate()
            .filter(|(s, p)| p.ready && *s < self.ai.len() && self.ai[*s].kind != crate::Kind::Dead)
            .map(|(s, p)| PsyS {
                slot: s as u32,
                uid: self.ai[s].id,
                breakdown: bd_code(p.breakdown),
                until: p.until,
                breakdowns: p.breakdowns,
                meals: p.meals,
                drinks: p.drinks,
                seen: p.seen.clone(),
                next_admire: p.next_admire,
                next_need_thought: p.next_need_thought,
                next_injury: p.next_injury,
                without_distress: p.without_distress,
                without_eustress: p.without_eustress,
                hardened: p.hardened, // [calib11-social]
                grief: p.grief,
            })
            .collect();
        let s = SocialS {
            psy,
            last_month: so.last_month,
            months: so
                .months
                .iter()
                .map(|x| MonthS { year: x.year, month: x.month, counts: x.counts, mean_stress: x.mean_stress, mean_longterm: x.mean_longterm, mean_focus: x.mean_focus })
                .collect(),
            tantrums: so.tantrums,
            depressions: so.depressions,
            oblivious: so.oblivious,
            insane: so.insane,
            meals_at_table: so.meals_at_table,
            meals_without_table: so.meals_without_table,
            prayers: so.prayers,
            conversations: so.conversations,
        };
        put(m, KEY, &s);
    }

    pub(crate) fn social_restore(&mut self, m: &BTreeMap<String, String>) {
        let Some(s) = get::<SocialS>(m, KEY) else { return };
        if self.social.off {
            return;
        }
        self.social_ensure();
        for ps in s.psy {
            let sl = ps.slot as usize;
            if sl >= self.ai.len() || self.ai[sl].id != ps.uid || sl >= self.social.psy.len() {
                continue;
            }
            let p = &mut self.social.psy[sl];
            p.breakdown = bd_of(ps.breakdown);
            p.until = ps.until;
            p.breakdowns = ps.breakdowns;
            p.meals = ps.meals;
            p.drinks = ps.drinks;
            p.seen = ps.seen;
            p.next_admire = ps.next_admire;
            p.next_need_thought = ps.next_need_thought;
            p.next_injury = ps.next_injury;
            p.without_distress = ps.without_distress;
            p.without_eustress = ps.without_eustress;
            p.hardened = p.hardened.max(ps.hardened); // [calib11-social] older saves: from the memories
            if ps.grief.0 > 0 {
                p.grief = ps.grief;
            }
        }
        let so = &mut self.social;
        so.last_month = s.last_month;
        so.months = s
            .months
            .into_iter()
            .map(|x| MonthStress { year: x.year, month: x.month, counts: x.counts, mean_stress: x.mean_stress, mean_longterm: x.mean_longterm, mean_focus: x.mean_focus })
            .collect();
        so.tantrums = s.tantrums;
        so.depressions = s.depressions;
        so.oblivious = s.oblivious;
        so.insane = s.insane;
        so.meals_at_table = s.meals_at_table;
        so.meals_without_table = s.meals_without_table;
        so.prayers = s.prayers;
        so.conversations = s.conversations;
    }
}
