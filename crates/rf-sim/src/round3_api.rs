//! Round-3 player API: thoughts/stress readouts, squads/military orders, burrows, reports.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the social and combat simulations replace them.

use crate::Sim;
use rf_core::{Burrow, Emotion, Pos, Squad};

/// DF-style stress category of a unit (shown in the unit sheet / citizens list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StressLevel {
    Ecstatic,
    Happy,
    Content,
    Fine,
    Stressed,
    VeryStressed,
    Breaking,
}

impl StressLevel {
    /// [social] DF v50 stress buckets of short-term stress (DFHack `Units::stress_cutoffs`
    /// 50 000 / 25 000 / 10 000 / -10 000 / -25 000 / -50 000, i.e. DF's stress face).
    pub fn from_stress(stress: i32) -> StressLevel {
        match stress {
            i32::MIN..=-50_001 => StressLevel::Ecstatic,
            -50_000..=-25_001 => StressLevel::Happy,
            -25_000..=-10_001 => StressLevel::Content,
            -10_000..=9_999 => StressLevel::Fine,
            10_000..=24_999 => StressLevel::Stressed,
            25_000..=49_999 => StressLevel::VeryStressed,
            _ => StressLevel::Breaking,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            StressLevel::Ecstatic => "Ecstatic",
            StressLevel::Happy => "Happy",
            StressLevel::Content => "Content",
            StressLevel::Fine => "Fine",
            StressLevel::Stressed => "Stressed",
            StressLevel::VeryStressed => "Very stressed",
            StressLevel::Breaking => "Breaking down",
        }
    }
}

/// A squad order the player can give (DF military screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SquadOrder {
    /// Go to and hold a position.
    Station(Pos),
    /// Attack a specific unit until it is dead.
    Kill(i32),
    /// Train (follow the schedule).
    Train,
    /// Cancel orders (go off duty).
    Cancel,
}

impl Sim {
    /// One DF-style sentence for a remembered emotion, e.g.
    /// "felt satisfaction after drinking a good drink".
    pub fn describe_emotion(&self, e: &Emotion) -> String {
        self.describe(e) // [social]
    }

    pub fn stress_level(&self, unit_id: i32) -> StressLevel {
        StressLevel::from_stress(self.world.unit(unit_id).map(|u| u.stress).unwrap_or(0))
    }

    /// Create a new squad, returns its id.
    pub fn create_squad(&mut self, name: &str) -> i32 {
        let id = self.world.squads.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        self.world.squads.push(Squad { id, name: name.into(), members: vec![-1; 10], ..Default::default() });
        id
    }

    /// Put a unit into a squad position (or remove it with `unit_id = -1`).
    pub fn assign_squad_member(&mut self, squad_id: i32, position: usize, unit_id: i32) {
        // [chaos10] an unknown unit (stale UI selection) or squad changes nothing; a unit sits
        // in one squad position only, and the unit it replaces leaves the squad (as in DF)
        if (unit_id >= 0 && self.world.unit(unit_id).is_none()) || !self.world.squads.iter().any(|s| s.id == squad_id) || position > 64 {
            return;
        }
        if unit_id >= 0 {
            for s in self.world.squads.iter_mut() {
                for (k, m) in s.members.iter_mut().enumerate() {
                    if *m == unit_id && (s.id != squad_id || k != position) {
                        *m = -1;
                    }
                }
            }
        }
        if let Some(s) = self.world.squads.iter_mut().find(|s| s.id == squad_id) {
            if s.members.len() <= position {
                s.members.resize(position + 1, -1);
            }
            let prev = s.members[position];
            s.members[position] = unit_id;
            if prev >= 0 && prev != unit_id {
                if let Some(u) = self.world.units.iter_mut().find(|u| u.id == prev && u.squad_id == squad_id) {
                    u.squad_id = -1;
                    u.squad_position = -1;
                }
            }
        }
        if let Some(u) = self.world.units.iter_mut().find(|u| u.id == unit_id) {
            u.squad_id = squad_id;
            u.squad_position = position as i32;
        }
        self.refresh_military(); // [combat]
    }

    pub fn order_squad(&mut self, squad_id: i32, order: SquadOrder) {
        if let Some(s) = self.world.squads.iter_mut().find(|s| s.id == squad_id) {
            s.orders = match order {
                SquadOrder::Station(p) => vec![format!("Station {},{},{}", p.x, p.y, p.z)],
                SquadOrder::Kill(u) => vec![format!("Kill {u}")],
                SquadOrder::Train => vec!["Train".into()],
                SquadOrder::Cancel => vec![],
            };
        }
        self.refresh_military(); // [combat]
    }

    /// Squad routines the player can pick (DF v50 squad schedule names).
    pub fn squad_schedules(&self) -> &'static [&'static str] {
        &["Off duty", "Staggered training", "Constant training", "Ready"]
    }

    /// Set a squad's routine (see [`Sim::squad_schedules`]); soldiers re-read their duty.
    /// "Off duty" lets them return to civilian work, as after standing down in DF.
    pub fn set_squad_schedule(&mut self, squad_id: i32, schedule: &str) {
        if let Some(s) = self.world.squads.iter_mut().find(|s| s.id == squad_id) {
            s.schedule = schedule.to_string();
        }
        self.refresh_military();
    }

    /// Create a burrow from the inclusive box (all z between a and b). Returns its id.
    pub fn create_burrow(&mut self, name: &str, a: Pos, b: Pos) -> i32 {
        let id = self.world.burrows.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let mut tiles = Vec::new();
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for x in a.x.min(b.x)..=a.x.max(b.x) {
                    tiles.push(Pos::new(x, y, z));
                }
            }
        }
        self.world.burrows.push(Burrow { id, name: name.into(), tiles, units: vec![] });
        id
    }

    /// Civilian alert: restrict all citizens to `burrow_id` (None = lift the alert).
    pub fn set_civilian_alert(&mut self, burrow_id: Option<i32>) {
        self.military_set_alert(burrow_id); // [combat]
    }

    /// Units currently hostile to the fort (invaders, wild predators, monsters).
    pub fn hostiles(&self) -> Vec<i32> {
        // [combat] caged and off-map units are no threat
        // [military2] hidden (not yet spotted) ambushers, thieves and snatchers are not shown
        self.world.units.iter().filter(|u| u.flags.hostile && !u.flags.dead && !u.flags.caged && !u.flags.inactive && !self.unit_hidden(u.id)).map(|u| u.id).collect()
    }

    /// Recent combat report lines (DF "Reports" screen), newest last.
    pub fn combat_log(&self) -> Vec<String> {
        self.combat.log.iter().cloned().collect() // [combat]
    }
}
