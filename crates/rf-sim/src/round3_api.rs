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
    /// DF v50 stress thresholds (wiki "Stress"): approximate until the social sim refines them.
    pub fn from_stress(stress: i32) -> StressLevel {
        match stress {
            i32::MIN..=-100_000 => StressLevel::Ecstatic,
            -99_999..=-25_000 => StressLevel::Happy,
            -24_999..=-1 => StressLevel::Content,
            0..=24_999 => StressLevel::Fine,
            25_000..=49_999 => StressLevel::Stressed,
            50_000..=99_999 => StressLevel::VeryStressed,
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
        format!("felt {} ({})", e.emotion.to_lowercase(), e.thought)
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
        if let Some(s) = self.world.squads.iter_mut().find(|s| s.id == squad_id) {
            if s.members.len() <= position {
                s.members.resize(position + 1, -1);
            }
            s.members[position] = unit_id;
        }
        if let Some(u) = self.world.units.iter_mut().find(|u| u.id == unit_id) {
            u.squad_id = squad_id;
            u.squad_position = position as i32;
        }
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
    pub fn set_civilian_alert(&mut self, _burrow_id: Option<i32>) {}

    /// Units currently hostile to the fort (invaders, wild predators, monsters).
    pub fn hostiles(&self) -> Vec<i32> {
        self.world.units.iter().filter(|u| u.flags.hostile && !u.flags.dead).map(|u| u.id).collect()
    }

    /// Recent combat report lines (DF "Reports" screen), newest last.
    pub fn combat_log(&self) -> Vec<String> {
        Vec::new()
    }
}
