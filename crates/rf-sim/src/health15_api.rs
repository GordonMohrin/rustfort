//! [health15] Public API for the UI and tests: hospitals and their supplies, per-unit medical
//! state (fractures, nerves, crutches, DF severity tiers), report, switches, test helpers.

use crate::combat::{bp, wf};
use crate::health::hf;
use crate::health15::{mx, Health15Report, Sup, NSUP, SUP_NAMES};
use crate::Sim;

pub use crate::health15::Walk;

/// One hospital for the UI.
#[derive(Clone, Debug, Default)]
pub struct HospitalInfo {
    /// Location id, or -(zone id) for a plain hospital zone.
    pub key: i32,
    pub name: String,
    pub zones: Vec<i32>,
    pub beds: u32,
    pub tables: u32,
    pub chests: usize,
    /// Traction benches: (building id, patient unit id or -1).
    pub benches: Vec<(i32, i32)>,
    pub want: [i32; NSUP],
    pub have: [i32; NSUP],
}

/// Fracture class of one broken wound.
#[derive(Clone, Debug, Default)]
pub struct Fracture {
    pub part: String,
    /// "simple", "displaced", "compound", "overlapping".
    pub class: &'static str,
    pub set: bool,
    /// "", "splint", "cast", "sling", "traction".
    pub immobilized: &'static str,
}

/// Medical state of one unit.
#[derive(Clone, Debug, Default)]
pub struct UnitMedical {
    pub fractures: Vec<Fracture>,
    pub motor: Vec<String>,
    pub sensory: Vec<String>,
    pub missing: Vec<String>,
    pub permanent: usize,
    pub crutch: Option<i32>,
    pub walk: Walk,
    pub needs_crutch: bool,
    pub in_traction: bool,
    /// DF severity tier per affected part (shown after diagnosis): MISSING, BROKEN,
    /// FUNCTION LOSS, INHIBITED, MINOR.
    pub tiers: Vec<(String, &'static str)>,
    pub diagnosed: bool,
}

/// Names of the supply kinds in `HospitalInfo::want/have` order.
pub const SUPPLY_NAMES: [&str; NSUP] = SUP_NAMES;

impl Sim {
    /// Hospitals (zones and locations) with furniture and supplies.
    pub fn hospitals(&self) -> Vec<HospitalInfo> {
        let uid = |s: u32| if s == crate::NONE || s as usize >= self.ai.len() { -1 } else { self.ai[s as usize].id };
        self.h15
            .hosp
            .iter()
            .map(|h| HospitalInfo {
                key: h.key,
                name: h.name.clone(),
                zones: h.zones.clone(),
                beds: h.beds,
                tables: h.tables,
                chests: h.chests.len(),
                benches: self.h15.benches.iter().filter(|b| self.h15_hosp_at(self.world.pos_of(b.tile as usize)).map(|i| self.h15.hosp[i].key == h.key).unwrap_or(false)).map(|b| (b.building, if b.placed { uid(b.occupant) } else { -1 })).collect(),
                want: h.want,
                have: h.have,
            })
            .collect()
    }

    /// Desired items of a supply kind (0 = thread .. 6 = soap) in a hospital.
    pub fn set_hospital_supply(&mut self, key: i32, sup: usize, items: i32) {
        let Some(&k) = crate::health15::SUPS.get(sup) else { return };
        self.h15_set_want(key, k, items);
    }

    /// Medical details of a unit (fracture classes, nerves, crutch, tiers).
    pub fn unit_medical(&self, unit_id: i32) -> UnitMedical {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return UnitMedical::default() };
        let Some(f) = self.combat.f.get(s) else { return UnitMedical::default() };
        let Some(body) = self.combat.bodies.get(f.body as usize) else { return UnitMedical::default() };
        let pname = |p: u16| body.parts.get(p as usize).map(|x| x.name.clone()).unwrap_or_default();
        let mut m = UnitMedical { diagnosed: !f.wounds.is_empty() && f.wounds.iter().all(|w| w.flags & hf::DIAGNOSED != 0), ..Default::default() };
        for w in &f.wounds {
            if w.flags & wf::BROKEN == 0 {
                continue;
            }
            let class = if w.mx & mx::COMPOUND != 0 {
                "compound"
            } else if w.mx & mx::OVERLAP != 0 {
                "overlapping"
            } else if w.mx & mx::NEEDS_SET != 0 {
                "displaced"
            } else {
                "simple"
            };
            let immobilized = if w.mx & mx::TRACTION != 0 {
                "traction"
            } else if w.mx & mx::CAST != 0 {
                "cast"
            } else if w.mx & mx::SPLINT != 0 {
                "splint"
            } else if w.mx & mx::SLING != 0 {
                "sling"
            } else {
                ""
            };
            m.fractures.push(Fracture { part: pname(w.part), class, set: w.flags & hf::SET != 0, immobilized });
        }
        if let Some(k) = self.h15_dis_idx(s) {
            let d = &self.h15.dis[k];
            m.motor = d.motor.iter().map(|&p| pname(p)).collect();
            m.sensory = d.sensory.iter().map(|&p| pname(p)).collect();
            m.permanent = d.perm.len();
            m.crutch = (d.crutch >= 0).then_some(d.crutch);
        }
        m.missing = f.part_state.iter().enumerate().filter(|(_, &st)| st & 1 != 0).map(|(i, _)| pname(i as u16)).collect();
        if let Some((w, need, _)) = self.h15_walk(s) {
            m.walk = w;
            m.needs_crutch = need;
        }
        m.in_traction = self.h15_in_traction(s);
        // DF tiers per part, worst first.
        let mut tiers: Vec<(u16, u8)> = Vec::new();
        let mut put = |p: u16, t: u8| match tiers.iter_mut().find(|e| e.0 == p) {
            Some(e) => e.1 = e.1.max(t),
            None => tiers.push((p, t)),
        };
        for (i, &st) in f.part_state.iter().enumerate() {
            if st & 1 != 0 {
                put(i as u16, 5);
            } else if st & 4 != 0 {
                put(i as u16, 3);
            }
        }
        for w in &f.wounds {
            if w.flags & hf::DIAGNOSED == 0 || body.parts.get(w.part as usize).map(|p| p.flags & bp::INTERNAL != 0 && w.sev < 2_000).unwrap_or(false) {
                continue;
            }
            let t = if w.flags & wf::BROKEN != 0 {
                4
            } else if w.sev >= 5_000 || w.mx & (mx::MOTOR | mx::SENSORY) != 0 {
                2
            } else {
                1
            };
            put(w.part, t);
        }
        tiers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        const NAMES: [&str; 6] = ["NONE", "MINOR", "INHIBITED", "FUNCTION LOSS", "BROKEN", "MISSING"];
        m.tiers = tiers.into_iter().map(|(p, t)| (pname(p), NAMES[t as usize])).collect();
        m
    }

    /// Counters and recent events of hospitals, fractures, nerves and crutches.
    pub fn health15_report(&self) -> Health15Report {
        self.h15.rep.clone()
    }

    /// Switch the health15 rules on or off (default on; off = the round-5 health care).
    pub fn set_health15_enabled(&mut self, on: bool) {
        self.h15.off = !on;
    }

    /// Unit id of the chief medical dwarf, if appointed (read only; appointments belong to the
    /// nobles screen). DF: the position only enables the health screens.
    pub fn chief_medical_dwarf(&self) -> Option<i32> {
        self.world.nobles.iter().find(|n| n.token == "CHIEF_MEDICAL_DWARF" && n.holder >= 0).map(|n| n.holder)
    }

    /// Units with active wounds or disabilities (unit ids, sorted), for the health overview.
    pub fn health15_patients(&self) -> Vec<i32> {
        let mut v: Vec<i32> = self.combat.wounded.iter().map(|&s| s as usize).chain(self.h15.dis.iter().map(|d| d.slot as usize)).filter(|&s| s < self.ai.len() && self.ai[s].kind == crate::Kind::Worker).map(|s| self.ai[s].id).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Supply kind name.
    pub fn supply_name(sup: Sup) -> &'static str {
        SUP_NAMES[sup as usize]
    }

    /// Tests/debug: break the bone of a part (`class`: "simple", "displaced", "compound",
    /// "overlapping"). True if the part was found.
    pub fn h15_break_bone(&mut self, unit_id: i32, part_token: &str, class: &str) -> bool {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return false };
        self.ensure_body(s);
        let b = self.combat.f[s].body as usize;
        let Some(pi) = self.combat.bodies[b].parts.iter().position(|p| p.token == part_token) else { return false };
        let p = &self.combat.bodies[b].parts[pi];
        let bone = (p.l0..p.l0 + p.nl).find(|&l| self.combat.bodies[b].layers[l as usize].settable).unwrap_or(u16::MAX);
        let soft = (p.l0..p.l0 + p.nl).find(|&l| !self.combat.bodies[b].layers[l as usize].rigid);
        let mut m = mx::SEEN;
        let sev = match class {
            "displaced" => {
                m |= mx::NEEDS_SET;
                6_000
            }
            "compound" => {
                m |= mx::NEEDS_SET | mx::COMPOUND;
                7_000
            }
            "overlapping" => {
                m |= mx::OVERLAP;
                4_000
            }
            _ => 3_000,
        };
        let f = &mut self.combat.f[s];
        f.wounds.push(crate::combat::WoundS { part: pi as u16, layer: bone, flags: wf::BROKEN, sev, bleed: 0, pain: 20, age: 0, mx: m });
        if class == "compound" {
            if let Some(l) = soft {
                f.wounds.push(crate::combat::WoundS { part: pi as u16, layer: l, flags: wf::CUT, sev: 3_000, bleed: 0, pain: 10, age: 0, mx: mx::SEEN });
            }
        }
        if let Some(x) = f.part_state.get_mut(pi) {
            *x |= 2;
        }
        self.list_wounded(s);
        self.sync_wounds(s);
        true
    }

    /// Tests/debug: sever the motor (or sensory) nerve of a part. True if the part was found.
    pub fn h15_sever_nerve(&mut self, unit_id: i32, part_token: &str, motor: bool) -> bool {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return false };
        self.ensure_body(s);
        let b = self.combat.f[s].body as usize;
        let Some(pi) = self.combat.bodies[b].parts.iter().position(|p| p.token == part_token) else { return false };
        let now = self.ticks;
        self.h15_add_nerve(s, pi as u16, motor, !motor, now);
        self.health_gait(s);
        true
    }

    /// Tests/debug: the unit's current move delay (ticks per tile at full speed units).
    pub fn h15_move_delay(&self, unit_id: i32) -> i32 {
        self.unit_slot.get(&unit_id).map(|&s| self.ai[s].speed).unwrap_or(0)
    }
}
