//! [saves10] Save/load of the locations module: visitors (with their slots, stay timers and
//! petitions), pending arrivals, petitions, the petition members, reading timers, location
//! usage and the report counters.

use super::{Arrival, LocRt, LocKind, Petition, Visitor, VisitorRole};
use crate::persist::{get, put};
use crate::Sim;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "locations";

fn role_code(r: VisitorRole) -> u8 {
    match r {
        VisitorRole::Performer => 0,
        VisitorRole::Scholar => 1,
        VisitorRole::Mercenary => 2,
        VisitorRole::MonsterSlayer => 3,
        VisitorRole::Pilgrim => 4,
    }
}

fn role_of(c: u8) -> VisitorRole {
    match c {
        1 => VisitorRole::Scholar,
        2 => VisitorRole::Mercenary,
        3 => VisitorRole::MonsterSlayer,
        4 => VisitorRole::Pilgrim,
        _ => VisitorRole::Performer,
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct VisitorS {
    slot: u32,
    unit: i32,
    role: u8,
    loc: usize,
    until: u64,
    petition_at: u64,
    leaving: bool,
    resident: bool,
    tries: u8,
    thief: bool,
    loot: i32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct PetitionS {
    id: i32,
    unit: i32,
    name: String,
    kind: String,
    role: String,
    location: i32,
    tick: u64,
    status: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct LocsS {
    ready: bool,
    visitors: Vec<VisitorS>,
    /// (tick, role, location index).
    arrivals: Vec<(u64, u8, usize)>,
    petitions: Vec<PetitionS>,
    next_petition: i32,
    last_season: i32,
    last_month: i32,
    members: Vec<(i32, Vec<i32>)>,
    next_read: Vec<u64>,
    /// (location id, citizen passes).
    passes: Vec<(i32, u64)>,
    // Report counters.
    usage: Vec<(i32, String, String, u64)>,
    derived_locations: usize,
    drinks_served: u64,
    performances: u64,
    audience: u64,
    dedicated_prayers: u64,
    books_read: u64,
    books_written: u64,
    scholar_days: u64,
    rep_visitors: Vec<(u64, String, String)>,
    visitors_left: u64,
    rep_petitions: u64,
    accepted: u64,
    denied: u64,
}

impl Sim {
    pub(crate) fn locs_save(&self, m: &mut BTreeMap<String, String>) {
        let l = &self.locs;
        let r = &l.rep;
        let s = LocsS {
            ready: l.ready,
            visitors: l
                .visitors
                .iter()
                .map(|v| VisitorS {
                    slot: v.slot,
                    unit: v.unit,
                    role: role_code(v.role),
                    loc: v.loc,
                    until: v.until,
                    petition_at: v.petition_at,
                    leaving: v.leaving,
                    resident: v.resident,
                    tries: v.tries,
                    thief: v.thief,
                    loot: v.loot,
                })
                .collect(),
            arrivals: l.arrivals.iter().map(|a| (a.tick, role_code(a.role), a.loc)).collect(),
            petitions: l
                .petitions
                .iter()
                .map(|p| PetitionS {
                    id: p.id,
                    unit: p.unit,
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    role: p.role.clone(),
                    location: p.location,
                    tick: p.tick,
                    status: p.status.clone(),
                })
                .collect(),
            next_petition: l.next_petition,
            last_season: l.last_season,
            last_month: l.last_month,
            members: l.members.clone(),
            next_read: l.next_read.clone(),
            passes: l.rt.iter().map(|r| (r.id, r.passes)).collect(),
            usage: r.usage.clone(),
            derived_locations: r.derived_locations,
            drinks_served: r.drinks_served,
            performances: r.performances,
            audience: r.audience,
            dedicated_prayers: r.dedicated_prayers,
            books_read: r.books_read,
            books_written: r.books_written,
            scholar_days: r.scholar_days,
            rep_visitors: r.visitors.clone(),
            visitors_left: r.visitors_left,
            rep_petitions: r.petitions,
            accepted: r.accepted,
            denied: r.denied,
        };
        put(m, KEY, &s);
    }

    pub(crate) fn locs_restore(&mut self, m: &BTreeMap<String, String>) {
        let Some(s) = get::<LocsS>(m, KEY) else { return };
        let n = self.ai.len();
        let l = &mut self.locs;
        l.visitors = s
            .visitors
            .into_iter()
            .filter(|v| (v.slot as usize) < n)
            .map(|v| Visitor {
                slot: v.slot,
                unit: v.unit,
                role: role_of(v.role),
                loc: v.loc,
                until: v.until,
                petition_at: v.petition_at,
                leaving: v.leaving,
                resident: v.resident,
                tries: v.tries,
                thief: v.thief,
                loot: v.loot,
            })
            .collect();
        l.arrivals = s.arrivals.into_iter().map(|(tick, role, loc)| Arrival { tick, role: role_of(role), loc }).collect();
        l.petitions = s
            .petitions
            .into_iter()
            .map(|p| Petition { id: p.id, unit: p.unit, name: p.name, kind: p.kind, role: p.role, location: p.location, tick: p.tick, status: p.status })
            .collect();
        l.next_petition = s.next_petition;
        l.last_season = s.last_season;
        l.last_month = s.last_month;
        l.members = s.members;
        l.next_read = s.next_read;
        // Placeholders carry the usage counters into the rebuild (which keeps passes by id).
        l.rt = s
            .passes
            .iter()
            .map(|&(id, passes)| LocRt { li: 0, id, kind: LocKind::Other, deity: -1, zones: Vec::new(), bbox: (0, 0, 0, 0, 0, 0), passes, books: 0 })
            .collect();
        l.key = (usize::MAX, usize::MAX);
        l.ready = s.ready;
        let r = &mut l.rep;
        r.usage = s.usage;
        r.derived_locations = s.derived_locations;
        r.drinks_served = s.drinks_served;
        r.performances = s.performances;
        r.audience = s.audience;
        r.dedicated_prayers = s.dedicated_prayers;
        r.books_read = s.books_read;
        r.books_written = s.books_written;
        r.scholar_days = s.scholar_days;
        r.visitors = s.rep_visitors;
        r.visitors_left = s.visitors_left;
        r.petitions = s.rep_petitions;
        r.accepted = s.accepted;
        r.denied = s.denied;
        if self.locs.ready {
            // Build the runtime location table now (visitors walk to it before the next pass).
            self.loc_ensure();
        } else {
            self.locs.rt.clear();
            self.locs.key = (0, 0);
        }
    }
}
