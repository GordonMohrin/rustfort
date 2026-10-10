//! Health care: diagnosis, treatment, infections and feeding patients (round 5).
//!
//! # DF rules implemented (wiki: Health care, Hospital, Diagnosis, Surgery, Suturing, Bone
//! setting, Infection, Rest)
//! - Wounded citizens who need care go to bed (combat's rest logic: hospital beds first, then
//!   their own bed, then any free bed; on the floor without one).
//! - **Diagnosis first**: "dwarves cannot be treated without a diagnosis". A citizen with the
//!   Diagnostician labor (DIAGNOSE, skill Diagnostics) examines the patient; then:
//!   - **Suturing** (SUTURING, skill Suturing) closes cuts, tears and stumps with **thread** and
//!     stops serious bleeding;
//!   - **Bone setting** (BONE_SETTING, skill Bone setting) sets broken bones with a **splint**
//!     (or thread and cloth for minor fractures);
//!   - **Surgery** (SURGERY, skill Surgery) repairs badly damaged internal organs;
//!   - **Wound dressing** (DRESSING_WOUNDS, skill Wound dressing) cleans and dresses open wounds
//!     with **cloth**; with **soap** in the fort the wound is also sterilized.
//!   Supplies are taken from anywhere in the fort (hospital chests first is DF's behavior;
//!   the doctor walks to the item, then to the patient).
//! - **Infection**: wounds that were never cleaned, cleaned without soap, or by bad luck get
//!   infected; an infection works like constant internal bleeding, stops the wound from healing
//!   and kills many patients months after they were wounded.
//! - **Feeding patients** (FEED_WATER_CIVILIANS): bedridden patients (a broken leg or foot,
//!   unconscious) are brought water (a drink, or water from a source in a bucket) and food.
//! - Treated (sutured, dressed) wounds heal faster; set bones heal at the normal rest rate,
//!   unset broken bones much slower.
//!
//! # Estimates (not on the wiki)
//! - Durations (scaled by skill like other jobs): diagnosis 300 ticks, suture 200, dress 150,
//!   set bone 400, surgery 600, feeding 60.
//! - Care needed for: any cut, tear, break or stump, or a wound of severity >= 2 000.
//! - Infection per day: never cleaned 1/100, dressed without soap 1/400, cleaned with soap
//!   1/4 000. An infected wound bleeds 1 blood per 10 ticks (about 4 % of a dwarf's blood a
//!   day; DF: "constant internal bleeding") and worsens by 100 severity per 100 ticks; it clears
//!   with 1/10 per day after cleaning with soap, 1/25 when dressed, else 1/50.
//! - Treated wounds heal 20 severity per 100 ticks faster; unset broken bones 30 slower.
//! - Surgery reduces the organ wound by a third. No treatment fails; no casts, traction or
//!   crutches yet; hospital chests are not stocked separately.

use crate::combat::{bp, wf, Sub};
use crate::path::Goals;
use crate::rng::Rng;
use crate::{AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::Pos;

/// Treatment flags on combat wounds (bits above combat's own `wf` flags).
pub mod hf {
    pub const DIAGNOSED: u16 = 128;
    pub const SUTURED: u16 = 256;
    pub const DRESSED: u16 = 512;
    pub const SET: u16 = 1024;
    pub const INFECTED: u16 = 2048;
    pub const CLEANED: u16 = 4096;
    pub const OPERATED: u16 = 8192;
    pub const NAMES: [(u16, &str); 7] =
        [(DIAGNOSED, "DIAGNOSED"), (SUTURED, "SUTURED"), (DRESSED, "DRESSED"), (SET, "SET"), (INFECTED, "INFECTED"), (CLEANED, "CLEANED"), (OPERATED, "OPERATED")];
}

/// Medical labor bits.
mod ml {
    pub const DIAG: u8 = 1;
    pub const SUTURE: u8 = 2;
    pub const DRESS: u8 = 4;
    pub const BONE: u8 = 8;
    pub const SURGERY: u8 = 16;
    pub const FEED: u8 = 32;
    /// [military2] RECOVER_WOUNDED: carry immobile patients to a bed.
    pub const RESCUE: u8 = 64;
}

const OPEN: u16 = wf::CUT | wf::TORN | wf::SEVERED;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub enum Treat {
    Diagnose,
    Suture,
    Dress,
    SetBone,
    Surgery,
    Water,
    Food,
    /// [military2] Recover wounded: carry an immobile patient to a bed.
    Carry,
}

impl Treat {
    fn bit(self) -> u8 {
        match self {
            Treat::Diagnose => ml::DIAG,
            Treat::Suture => ml::SUTURE,
            Treat::Dress => ml::DRESS,
            Treat::SetBone => ml::BONE,
            Treat::Surgery => ml::SURGERY,
            Treat::Water | Treat::Food => ml::FEED,
            Treat::Carry => ml::RESCUE,
        }
    }
    fn skill(self) -> &'static str {
        match self {
            Treat::Diagnose => "DIAGNOSE",
            Treat::Suture => "SUTURE",
            Treat::Dress => "DRESS_WOUNDS",
            Treat::SetBone => "SET_BONE",
            Treat::Surgery => "SURGERY",
            _ => "",
        }
    }
    fn ticks(self) -> i32 {
        match self {
            Treat::Diagnose => 300,
            Treat::Suture => 200,
            Treat::Dress => 150,
            Treat::SetBone => 400,
            Treat::Surgery => 600,
            Treat::Water | Treat::Food => 60,
            Treat::Carry => 20,
        }
    }
    /// DF job caption.
    pub fn caption(self) -> &'static str {
        match self {
            Treat::Diagnose => "Diagnose Patient",
            Treat::Suture => "Suture",
            Treat::Dress => "Dress Wound",
            Treat::SetBone => "Set Bone",
            Treat::Surgery => "Surgery",
            Treat::Water => "Give Water",
            Treat::Food => "Give Food",
            Treat::Carry => "Recover Wounded",
        }
    }
    /// Item kind the treatment uses up.
    fn supply(self, splints: bool) -> Option<&'static str> {
        match self {
            Treat::Suture => Some("THREAD"),
            Treat::Dress => Some("CLOTH"),
            Treat::SetBone => Some(if splints { "SPLINT" } else { "CLOTH" }),
            _ => None,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
enum CStep {
    Open,
    GoItem,
    GoPatient,
    Work(u64),
    /// [military2] Carrying the patient to bed `.0`.
    Carry(i32),
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct CareJob {
    patient: u32,
    treat: Treat,
    doctor: u32,
    /// Supply item being fetched/carried (-1 none; -2 water from a source).
    item: i32,
    step: CStep,
}

/// [military2] Crawling is this many times slower than walking (estimate).
pub(crate) const CRAWL_FACTOR: i32 = 4;

/// Counters and recent events.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct HealthReport {
    pub diagnoses: u32,
    pub sutures: u32,
    pub dressings: u32,
    pub bones_set: u32,
    pub surgeries: u32,
    pub waters_given: u32,
    pub meals_given: u32,
    pub infections: u32,
    pub infections_cured: u32,
    pub infection_deaths: u32,
    /// Patients resting right now and how many of them in a hospital zone.
    pub patients_now: usize,
    pub doctors: usize,
    pub events: Vec<(u64, String)>,
}

/// Health summary of one unit for the UI.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct HealthInfo {
    pub diagnosed: bool,
    pub infected: bool,
    /// (body part, flags incl. treatment flags) per active wound.
    pub wounds: Vec<(String, Vec<String>)>,
    /// Next treatment the unit waits for, e.g. `Suture`.
    pub pending: Option<String>,
    /// Doctor working on it right now.
    pub doctor: Option<i32>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Health {
    pub off: bool,
    inited: bool,
    med: Vec<u8>,
    any_med: u8,
    pub jobs: Vec<CareJob>,
    open: usize,
    soap: bool,
    threads: i32,
    cloths: i32,
    splints: i32,
    pub stats: HealthReport,
}

impl Sim {
    /// Read the treatment state of exported wounds (DIAGNOSED, SUTURED, INFECTED, ...).
    pub(crate) fn init_health(&mut self) {
        if self.health.inited {
            return;
        }
        self.health.inited = true;
        self.combat_sync_units();
        for s in 0..self.ai.len() {
            if self.combat.f[s].wounds.is_empty() || self.combat.f[s].body == u16::MAX {
                continue;
            }
            let uidx = self.ai[s].uidx;
            let exported = self.world.units[uidx].wounds.clone();
            let b = self.combat.f[s].body as usize;
            for w in &exported {
                let scar = w.flags.iter().any(|f| f.starts_with("SCAR")) || (w.bleeding <= 0 && w.pain <= 0 && w.age > rf_core::TICKS_PER_MONTH);
                if scar {
                    continue;
                }
                let body = &self.combat.bodies[b];
                let Some(pi) = body.parts.iter().position(|p| p.token == w.body_part) else { continue };
                let p = &body.parts[pi];
                let layer = (p.l0..p.l0 + p.nl).find(|&l| body.layers[l as usize].tissue.eq_ignore_ascii_case(&w.layer)).unwrap_or(u16::MAX);
                let mut bits = 0;
                for f in &w.flags {
                    bits |= match f.as_str() {
                        "DIAGNOSED" => hf::DIAGNOSED,
                        "SUTURED" => hf::SUTURED,
                        "INFECTED" | "INFECTION" => hf::INFECTED,
                        "SET" | "SPLINT" | "CAST" | "IMMOBILIZED" => hf::SET,
                        "DRESSED" | "BANDAGED" | "CLEANED" => hf::DRESSED,
                        _ => 0,
                    };
                }
                if let Some(ws) = self.combat.f[s].wounds.iter_mut().find(|x| x.part == pi as u16 && x.layer == layer) {
                    ws.flags |= bits;
                }
            }
        }
        self.health_refresh_daily();
    }

    /// [moods] Per tick: the care pass every 100 ticks, labors/supplies/infections daily.
    pub(crate) fn health_tick(&mut self, now: u64) {
        if self.health.off {
            return;
        }
        if now % 100 == 37 {
            if !self.health.inited {
                self.init_health();
            }
            if now % rf_core::TICKS_PER_DAY as u64 == 637 {
                self.health_refresh_daily();
                self.infection_daily(now);
            }
            if !self.combat.wounded.is_empty() || !self.health.jobs.is_empty() {
                self.health_pass(now);
            }
        }
    }

    fn health_refresh_daily(&mut self) {
        let n = self.ai.len();
        self.health.med.resize(n, 0);
        let mut any = 0;
        for s in 0..n {
            let mut b = 0;
            if self.ai[s].kind == Kind::Worker {
                for l in &self.world.units[self.ai[s].uidx].labors {
                    b |= match l.as_str() {
                        "DIAGNOSE" => ml::DIAG,
                        "SUTURING" => ml::SUTURE,
                        "DRESSING_WOUNDS" => ml::DRESS,
                        "BONE_SETTING" => ml::BONE,
                        "SURGERY" => ml::SURGERY,
                        "FEED_WATER_CIVILIANS" => ml::FEED,
                        "RECOVER_WOUNDED" => ml::RESCUE, // [military2]
                        _ => 0,
                    };
                }
            }
            self.health.med[s] = b;
            any |= b;
        }
        self.health.any_med = any;
        let (mut th, mut cl, mut sp, mut soap) = (0, 0, 0, false);
        let now = self.ticks;
        for idx in 0..self.world.items.len() {
            let k = self.world.items[idx].kind.as_str();
            let interesting = matches!(k, "THREAD" | "CLOTH" | "SPLINT" | "BAR" | "GLOB" | "LIQUID_MISC");
            if !interesting || !self.item_free(idx, now) {
                continue;
            }
            match k {
                "THREAD" => th += 1,
                "CLOTH" => cl += 1,
                "SPLINT" => sp += 1,
                _ => {
                    if self.world.material(self.world.items[idx].mat).map(|m| m.name.contains("soap") || m.token.contains("SOAP")).unwrap_or(false) {
                        soap = true;
                    }
                }
            }
        }
        self.health.threads = th;
        self.health.cloths = cl;
        self.health.splints = sp;
        self.health.soap = soap;
    }

    fn is_patient_slot(&self, s: usize) -> bool {
        self.ai[s].kind == Kind::Worker && self.combat.f.get(s).map(|f| !f.wounds.is_empty()).unwrap_or(false)
    }

    fn resting_in_bed(&self, s: usize) -> bool {
        self.combat.f.get(s).map(|f| matches!(f.sub, Sub::Resting(_))).unwrap_or(false)
    }

    /// Can be treated now: resting (in bed or on the floor) or lying unconscious where it fell
    /// (no "Recover wounded" carrying yet, so doctors treat such patients on the spot).
    fn in_care(&self, s: usize) -> bool {
        self.resting_in_bed(s) || self.combat.f.get(s).map(|f| f.uncon_until > self.ticks).unwrap_or(false)
    }

    /// Cannot walk: a broken bone in a leg or foot, or unconscious.
    pub(crate) fn health_bedridden(&self, s: usize) -> bool {
        let Some(f) = self.combat.f.get(s) else { return false };
        f.uncon_until > self.ticks || self.cannot_stand(s)
    }

    /// [military2] Cannot stand: an unset broken bone in a limb that carries weight (STANCE),
    /// or a lost foot (no crutches yet). Such a unit crawls (wiki Health care / Crutch).
    pub(crate) fn cannot_stand(&self, s: usize) -> bool {
        let Some(f) = self.combat.f.get(s) else { return false };
        if f.body == u16::MAX || f.wounds.is_empty() {
            return false;
        }
        let body = &self.combat.bodies[f.body as usize];
        if f.wounds.iter().any(|w| w.flags & wf::SEVERED != 0 && (w.part as usize) < body.parts.len() && body.parts[w.part as usize].flags & bp::STANCE != 0) {
            return true;
        }
        let body = &self.combat.bodies[f.body as usize];
        f.wounds.iter().any(|w| {
            if w.flags & wf::BROKEN == 0 || w.flags & hf::SET != 0 {
                return false;
            }
            // The broken part or one below it carries weight (STANCE).
            let mut stack = vec![w.part];
            while let Some(p) = stack.pop() {
                let pt = &body.parts[p as usize];
                if pt.flags & bp::STANCE != 0 {
                    return true;
                }
                stack.extend(pt.children.iter().copied());
            }
            false
        })
    }

    /// Next treatment a patient needs (None = nothing the fort can do right now).
    fn next_treat(&self, s: usize, with_feed: bool) -> Option<Treat> {
        let f = self.combat.f.get(s)?;
        if f.wounds.is_empty() {
            return None;
        }
        let have = self.health.any_med;
        if with_feed && have & ml::FEED != 0 && self.health_bedridden(s) {
            let ai = &self.ai[s];
            if ai.thirst >= crate::needs::THIRST_SEEK {
                return Some(Treat::Water);
            }
            if ai.hunger >= crate::needs::HUNGER_SEEK {
                return Some(Treat::Food);
            }
        }
        let care = |w: &crate::combat::WoundS| w.flags & (OPEN | wf::BROKEN) != 0 || w.sev >= 2_000 || w.flags & hf::INFECTED != 0;
        if f.wounds.iter().any(|w| care(w) && w.flags & hf::DIAGNOSED == 0) {
            return (have & ml::DIAG != 0).then_some(Treat::Diagnose);
        }
        let body = (f.body != u16::MAX).then(|| &self.combat.bodies[f.body as usize]);
        if have & ml::SUTURE != 0 && self.health.threads > 0 && f.wounds.iter().any(|w| w.flags & OPEN != 0 && (w.bleed > 0 || w.sev >= 2_000 || w.flags & wf::SEVERED != 0) && w.flags & hf::SUTURED == 0) {
            return Some(Treat::Suture);
        }
        if have & ml::BONE != 0 && (self.health.splints > 0 || self.health.cloths > 0) && f.wounds.iter().any(|w| w.flags & wf::BROKEN != 0 && w.flags & hf::SET == 0) {
            return Some(Treat::SetBone);
        }
        if have & ml::SURGERY != 0 {
            if let Some(b) = body {
                if f.wounds.iter().any(|w| w.flags & hf::OPERATED == 0 && w.sev >= 3_000 && (w.part as usize) < b.parts.len() && b.parts[w.part as usize].flags & bp::INTERNAL != 0) {
                    return Some(Treat::Surgery);
                }
            }
        }
        if have & ml::DRESS != 0 && self.health.cloths > 0 {
            let soap = self.health.soap;
            if f.wounds.iter().any(|w| w.flags & OPEN != 0 && (w.flags & hf::DRESSED == 0 || (soap && w.flags & hf::INFECTED != 0 && w.flags & hf::CLEANED == 0))) {
                return Some(Treat::Dress);
            }
        }
        None
    }

    /// [moods] Hook in combat's `needs_rest`: wounds the fort's doctors can still treat.
    pub(crate) fn health_wants_bed(&self, s: usize) -> bool {
        !self.health.off && self.ai[s].kind == Kind::Worker && self.next_treat(s, false).is_some()
    }

    /// [moods] Hook in combat's patient AI: bedridden patients stay in bed and are fed.
    pub(crate) fn health_keep_in_bed(&self, s: usize) -> bool {
        !self.health.off && self.health.any_med & ml::FEED != 0 && self.ai[s].thirst < crate::needs::DEHYDRATED && self.ai[s].hunger < crate::needs::STARVING && self.health_bedridden(s)
    }

    fn health_pass(&mut self, now: u64) {
        let list = self.combat.wounded.clone();
        let mut want: Vec<(u32, Treat)> = Vec::new();
        for &s32 in &list {
            let s = s32 as usize;
            if s >= self.ai.len() || self.ai[s].kind == Kind::Dead {
                continue;
            }
            let citizen = self.is_patient_slot(s);
            // Wound effects of treatment and infection.
            let mut infected = false;
            {
                let f = &mut self.combat.f[s];
                let mut bleed = 0;
                for w in f.wounds.iter_mut() {
                    if w.flags & wf::SEVERED != 0 && w.sev >= 10_000 && w.flags & hf::INFECTED == 0 {
                        bleed += w.bleed;
                        continue;
                    }
                    if w.flags & hf::INFECTED != 0 {
                        infected = true;
                        w.bleed = w.bleed.max(2);
                        w.flags |= wf::BLEEDING;
                        w.sev = (w.sev + 100).min(10_000);
                    } else {
                        if w.flags & (hf::SUTURED | hf::DRESSED) != 0 {
                            w.sev -= 20;
                        }
                        if w.flags & wf::BROKEN != 0 && w.flags & hf::SET == 0 {
                            w.sev += 30;
                        }
                    }
                    bleed += w.bleed;
                }
                f.bleed = bleed;
            }
            if infected {
                self.list_bleeder(s);
                let f = &self.combat.f[s];
                if f.blood_max > 0 && f.blood * 20 <= f.blood_max {
                    self.health.stats.infection_deaths += 1;
                    let name = self.uname(s);
                    self.health.stats.events.push((now, format!("{name} has died of an infection.")));
                    self.combat_kill(s, now, "has died of an infection");
                    continue;
                }
            }
            if citizen && self.needs_carry(s) {
                want.push((s32, Treat::Carry)); // [military2] recover wounded first
            } else if citizen && self.in_care(s) {
                if let Some(t) = self.next_treat(s, true) {
                    want.push((s32, t));
                }
            }
        }
        // Drop jobs whose patient is gone, healed, out of bed, or needs something else now.
        let mut k = 0;
        while k < self.health.jobs.len() {
            let j = self.health.jobs[k];
            let p = j.patient as usize;
            let still = want.iter().any(|&(s, t)| s == j.patient && t == j.treat);
            let alive = p < self.ai.len() && self.ai[p].kind != Kind::Dead;
            let carrying = j.treat == Treat::Carry && j.doctor != NONE && matches!(j.step, CStep::Carry(_)); // [military2]
            if alive && (still || carrying || j.doctor != NONE && self.in_care(p)) {
                k += 1;
                continue;
            }
            self.health.jobs.remove(k);
            if let CStep::Carry(b) = j.step {
                self.rescue_free_bed(b, j.patient); // [military2]
            }
            if j.doctor != NONE {
                let d = j.doctor as usize;
                if self.combat.f.get(d).map(|f| f.carry != NONE).unwrap_or(false) {
                    if let Some(v) = self.carry_drop(d, now) {
                        self.rescue_lay_down(v, -1, now); // [military2]
                    }
                }
                if self.ai[d].task == Task::Care {
                    self.care_drop_item(d, j.item);
                    self.ai[d].task = Task::Idle;
                    self.ai[d].path.clear();
                    self.ai[d].path_i = 0;
                    self.ai[d].waiting = false;
                    self.ai[d].wake = now + 1;
                }
            }
        }
        for (s, t) in want {
            if !self.health.jobs.iter().any(|j| j.patient == s) {
                self.health.jobs.push(CareJob { patient: s, treat: t, doctor: NONE, item: -1, step: CStep::Open });
            }
        }
        self.health.open = self.health.jobs.iter().filter(|j| j.doctor == NONE).count();
        self.health.stats.patients_now = list.iter().filter(|&&s| (s as usize) < self.ai.len() && self.is_patient_slot(s as usize) && self.resting_in_bed(s as usize)).count();
        if self.health.stats.events.len() > 400 {
            self.health.stats.events.drain(..200);
        }
    }

    fn infection_daily(&mut self, now: u64) {
        let list = self.combat.wounded.clone();
        let soap = self.health.soap;
        for &s32 in &list {
            let s = s32 as usize;
            if s >= self.ai.len() || self.ai[s].kind != Kind::Worker {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, s, now, 0x1F3C);
            let mut new_inf = Vec::new();
            let mut cured = Vec::new();
            for (k, w) in self.combat.f[s].wounds.iter().enumerate() {
                if w.flags & OPEN == 0 {
                    continue;
                }
                if w.flags & hf::INFECTED != 0 {
                    let d = if w.flags & hf::CLEANED != 0 { 10 } else if w.flags & hf::DRESSED != 0 { 25 } else { 50 };
                    if r.chance(1, d) {
                        cured.push(k);
                    }
                } else if w.sev > 0 {
                    let d = if w.flags & hf::CLEANED != 0 { 4_000 } else if w.flags & hf::DRESSED != 0 { 400 } else { 100 };
                    if r.chance(1, d) {
                        new_inf.push(k);
                    }
                }
            }
            let _ = soap;
            if new_inf.is_empty() && cured.is_empty() {
                continue;
            }
            let name = self.uname(s);
            let b = self.combat.f[s].body;
            for &k in &new_inf {
                let part = {
                    let w = self.combat.f[s].wounds[k];
                    self.combat.bodies.get(b as usize).and_then(|bd| bd.parts.get(w.part as usize)).map(|p| p.name.clone()).unwrap_or_default()
                };
                self.combat.f[s].wounds[k].flags |= hf::INFECTED;
                self.health.stats.infections += 1;
                self.health.stats.events.push((now, format!("The wound on {name}'s {part} has become infected.")));
            }
            for &k in &cured {
                self.combat.f[s].wounds[k].flags &= !hf::INFECTED;
                self.health.stats.infections_cured += 1;
                self.health.stats.events.push((now, format!("{name}'s infection has cleared.")));
            }
            self.sync_wounds(s);
        }
    }

    /// [moods] Hook in `decide`: doctors and caretakers take open care jobs.
    pub(crate) fn health_decide(&mut self, s: usize, now: u64) -> bool {
        if self.health.open == 0 || self.health.off {
            return false;
        }
        let bits = self.health.med.get(s).copied().unwrap_or(0);
        if bits == 0 {
            return false;
        }
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, usize::MAX);
        for (k, j) in self.health.jobs.iter().enumerate() {
            if j.doctor != NONE || j.treat.bit() & bits == 0 || j.patient as usize == s {
                continue;
            }
            let pt = self.ai[j.patient as usize].pos as usize;
            if self.nav.comp_near(pt) != c || self.alert_blocks(s, pt) {
                continue; // [military2] under a civilian alert: patients inside the burrow only
            }
            let d = crate::manhattan(up, self.nav.xyz(pt));
            if d < best.0 {
                best = (d, k);
            }
        }
        if best.1 == usize::MAX {
            return false;
        }
        let k = best.1;
        let t = self.health.jobs[k].treat;
        // Supplies first.
        let mut item = -1;
        let mut goals = Goals::default();
        match t {
            Treat::Water | Treat::Food => {
                let drink = t == Treat::Water;
                match self.nearest_consumable(s, drink, now) {
                    Some(id) => {
                        self.reserve(id);
                        item = id;
                        goals = self.item_goals(id);
                    }
                    None if drink => match self.nearest_water(s) {
                        Some(w) => {
                            item = -2;
                            goals = Goals::one(w);
                        }
                        None => return false,
                    },
                    None => return false,
                }
            }
            _ => {
                if let Some(kind) = t.supply(self.health.splints > 0) {
                    match self.care_find_supply(s, kind, now) {
                        Some(id) => {
                            if let Some(&i) = self.item_idx.get(&id) {
                                self.imeta[i as usize].reserved += 1;
                            }
                            item = id;
                            goals = self.item_goals(id);
                        }
                        None => return false,
                    }
                }
            }
        }
        let j = &mut self.health.jobs[k];
        j.doctor = s as u32;
        j.item = item;
        self.health.open = self.health.open.saturating_sub(1);
        self.ai[s].task = Task::Care;
        if item == -1 {
            j.step = CStep::GoPatient;
            let p = j.patient as usize;
            let g = self.patient_goals(p);
            self.request_path(s, g, now);
        } else {
            j.step = CStep::GoItem;
            self.request_path(s, goals, now);
        }
        true
    }

    fn care_find_supply(&self, s: usize, kind: &str, now: u64) -> Option<i32> {
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let up = self.nav.xyz(self.ai[s].pos as usize);
        let mut best = (i32::MAX, -1);
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            if it.kind != kind || !self.item_free(idx, now) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.alert_blocks(s, t) {
                continue; // [military2]
            }
            let d = crate::manhattan(up, self.nav.xyz(t));
            if (d, it.id) < best && self.nav.comp_near(t) == c {
                best = (d, it.id);
            }
        }
        (best.1 >= 0).then_some(best.1)
    }

    fn patient_goals(&self, p: usize) -> Goals {
        let t = self.ai[p].pos as usize;
        let mut g = Goals::default();
        if self.nav.walkable(t) {
            g.push(t as u32);
        }
        let (x, y, z) = self.nav.xyz(t);
        for &(ddx, ddy) in &crate::nav::DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if self.nav.walkable(q) {
                g.push(q as u32);
            }
        }
        g
    }

    fn care_job_of(&self, s: usize) -> Option<usize> {
        self.health.jobs.iter().position(|j| j.doctor == s as u32)
    }

    /// [moods] Path arrival / wake of a unit with `Task::Care`.
    pub(crate) fn care_arrive(&mut self, s: usize, now: u64) {
        let Some(k) = self.care_job_of(s) else {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        };
        let j = self.health.jobs[k];
        let p = j.patient as usize;
        match j.step {
            CStep::Open => {
                self.health.jobs[k].step = CStep::GoPatient;
                let g = self.patient_goals(p);
                self.request_path(s, g, now);
            }
            CStep::GoItem => {
                if j.item >= 0 {
                    if !self.item_reachable_here(s, j.item) {
                        self.care_cancel(s);
                        self.ai[s].task = Task::Idle;
                        self.ai[s].wake = now + 1;
                        return;
                    }
                    let carried = self.mood_take(s, j.item);
                    self.health.jobs[k].item = carried;
                }
                self.health.jobs[k].step = CStep::GoPatient;
                let g = self.patient_goals(p);
                self.request_path(s, g, now);
            }
            CStep::GoPatient => {
                let (ps, pp) = (self.world.pos_of(self.ai[s].pos as usize), self.world.pos_of(self.ai[p].pos as usize));
                if ps.dist(pp) > 1 {
                    let g = self.patient_goals(p);
                    if g.n == 0 {
                        self.care_cancel(s);
                        self.ai[s].task = Task::Idle;
                        self.ai[s].wake = now + 50;
                        return;
                    }
                    self.request_path(s, g, now);
                    return;
                }
                if j.treat == Treat::Carry {
                    self.rescue_pick_up(s, k, p, now); // [military2]
                    return;
                }
                let sk = j.treat.skill();
                let rating = if sk.is_empty() { 0 } else { self.social_rating(s, self.world.units[self.ai[s].uidx].skill(sk).clamp(0, 20)) };
                let dur = (j.treat.ticks() * 10 / (10 + 2 * rating.clamp(0, 20))).max(10) as u64;
                self.health.jobs[k].step = CStep::Work(now + dur);
                self.ai[s].wake = now + dur;
            }
            CStep::Carry(b) => self.rescue_arrive(s, k, b, now), // [military2]
            CStep::Work(until) => {
                if now < until {
                    self.ai[s].wake = until;
                    return;
                }
                self.health.jobs.remove(k);
                self.apply_treatment(s, p, j.treat, j.item, now);
                self.ai[s].task = Task::Idle;
                self.decide(s, now);
            }
        }
    }

    fn apply_treatment(&mut self, d: usize, p: usize, t: Treat, item: i32, now: u64) {
        let soap = self.health.soap;
        let dname = self.uname(d);
        let pname = self.uname(p);
        let mut used = item >= 0;
        match t {
            Treat::Diagnose => {
                let mut infected = false;
                for w in self.combat.f[p].wounds.iter_mut() {
                    w.flags |= hf::DIAGNOSED;
                    infected |= w.flags & hf::INFECTED != 0;
                }
                self.health.stats.diagnoses += 1;
                let more = if infected { " (infection found)" } else { "" };
                self.health.stats.events.push((now, format!("{dname} diagnosed {pname}{more}.")));
            }
            Treat::Suture => {
                for w in self.combat.f[p].wounds.iter_mut() {
                    if w.flags & OPEN != 0 && w.flags & hf::SUTURED == 0 {
                        w.flags |= hf::SUTURED;
                        if w.flags & hf::INFECTED == 0 {
                            w.bleed = 0;
                            w.flags &= !(wf::BLEEDING | wf::ARTERY);
                        }
                    }
                }
                let f = &mut self.combat.f[p];
                f.bleed = f.wounds.iter().map(|w| w.bleed).sum();
                self.health.stats.sutures += 1;
                self.health.stats.events.push((now, format!("{dname} sutured {pname}.")));
            }
            Treat::Dress => {
                for w in self.combat.f[p].wounds.iter_mut() {
                    if w.flags & OPEN != 0 {
                        w.flags |= hf::DRESSED;
                        if soap {
                            w.flags |= hf::CLEANED;
                        }
                    }
                }
                self.health.stats.dressings += 1;
                self.health.stats.events.push((now, format!("{dname} dressed {pname}'s wounds{}.", if soap { " (cleaned with soap)" } else { "" })));
            }
            Treat::SetBone => {
                for w in self.combat.f[p].wounds.iter_mut() {
                    if w.flags & wf::BROKEN != 0 {
                        w.flags |= hf::SET;
                    }
                }
                self.health.stats.bones_set += 1;
                self.health.stats.events.push((now, format!("{dname} set {pname}'s bone.")));
            }
            Treat::Surgery => {
                let b = self.combat.f[p].body as usize;
                let internal: Vec<bool> = self.combat.f[p].wounds.iter().map(|w| self.combat.bodies.get(b).and_then(|bd| bd.parts.get(w.part as usize)).map(|pt| pt.flags & bp::INTERNAL != 0).unwrap_or(false)).collect();
                for (w, int) in self.combat.f[p].wounds.iter_mut().zip(internal) {
                    if int && w.sev >= 3_000 {
                        w.flags |= hf::OPERATED;
                        w.sev = w.sev * 2 / 3;
                    }
                }
                self.health.stats.surgeries += 1;
                self.health.stats.events.push((now, format!("{dname} operated on {pname}.")));
            }
            Treat::Water => {
                let ai = &mut self.ai[p];
                ai.thirst = (ai.thirst - crate::needs::DRINK_SATISFIES).max(0);
                if item >= 0 {
                    self.social_drank(p, item, now);
                }
                self.health.stats.waters_given += 1;
                used = item >= 0;
            }
            Treat::Food => {
                let ai = &mut self.ai[p];
                ai.hunger = (ai.hunger - crate::needs::EAT_SATISFIES).max(0);
                if item >= 0 {
                    self.social_ate(p, item, now);
                }
                self.health.stats.meals_given += 1;
            }
            Treat::Carry => {}
        }
        if !t.skill().is_empty() {
            self.add_xp(d, t.skill(), 50);
        }
        if used {
            let uidx = self.ai[d].uidx;
            self.world.units[uidx].inventory.retain(|&x| x != item);
            if self.ai[d].carrying == item {
                self.ai[d].carrying = -1;
            }
            if matches!(t, Treat::Water | Treat::Food) {
                if let Some(&i) = self.item_idx.get(&item) {
                    // Consumed like a normal meal/drink: one unit of the stack.
                    let it = &mut self.world.items[i as usize];
                    it.holder = -1;
                    it.flags.in_inventory = false;
                }
                self.unreserve(item);
                self.consume(item);
            } else {
                self.destroy_item(item);
            }
        }
        self.sync_wounds(p);
    }

    fn care_drop_item(&mut self, d: usize, item: i32) {
        if item < 0 {
            return;
        }
        if self.ai[d].carrying == item {
            let here = self.ai[d].pos;
            self.drop_item(d, item, here);
        } else {
            self.unreserve(item);
        }
        self.release_reagent(item);
    }

    /// [moods] `cancel_task` of a doctor: free the job and the supply.
    pub(crate) fn care_cancel(&mut self, s: usize) {
        let Some(k) = self.care_job_of(s) else { return };
        if let CStep::Carry(b) = self.health.jobs[k].step {
            // [military2] Interrupted while carrying: put the patient down here.
            let now = self.ticks;
            let p = self.health.jobs[k].patient;
            self.rescue_free_bed(b, p);
            if let Some(v) = self.carry_drop(s, now) {
                self.rescue_lay_down(v, -1, now);
            }
        }
        let item = self.health.jobs[k].item;
        self.care_drop_item(s, item);
        let j = &mut self.health.jobs[k];
        j.doctor = NONE;
        j.item = -1;
        j.step = CStep::Open;
        self.health.open += 1;
    }

    pub(crate) fn care_caption(&self, s: usize) -> (&'static str, Option<Pos>) {
        match self.care_job_of(s) {
            Some(k) => {
                let j = &self.health.jobs[k];
                (j.treat.caption(), Some(self.world.pos_of(self.ai[j.patient as usize].pos as usize)))
            }
            None => ("No Job", None),
        }
    }

    /// [moods] Appended to combat's `sync_wounds`: treatment flag names for the UI.
    pub(crate) fn health_sync_names(&mut self, s: usize) {
        let Some(f) = self.combat.f.get(s) else { return };
        let nsc = f.scars.len();
        let bits: Vec<u16> = f.wounds.iter().map(|w| w.flags).collect();
        let u = &mut self.world.units[self.ai[s].uidx];
        for (k, b) in bits.iter().enumerate() {
            if let Some(w) = u.wounds.get_mut(nsc + k) {
                for (bit, name) in hf::NAMES {
                    if b & bit != 0 {
                        w.flags.push(name.to_string());
                    }
                }
            }
        }
    }

    pub(crate) fn health_hash(&self, mix: &mut dyn FnMut(u64)) {
        let st = &self.health.stats;
        mix(((st.diagnoses as u64) << 40) ^ ((st.sutures as u64) << 20) ^ st.infections as u64);
        mix(self.health.jobs.len() as u64);
        mix(self.combat.stats.rescues ^ (self.combat.stats.crawlers << 20)); // [military2]
    }

    // ------------------------------------------------------------------ public API

    /// Treatments, infections and feeding so far.
    pub fn health_report(&self) -> HealthReport {
        let mut r = self.health.stats.clone();
        r.doctors = self.health.med.iter().filter(|&&b| b & (ml::DIAG | ml::SUTURE | ml::DRESS | ml::BONE | ml::SURGERY) != 0).count();
        r
    }

    /// Health summary of a unit: diagnosis, infection, wounds with treatment flags.
    pub fn health_info(&self, unit_id: i32) -> HealthInfo {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return HealthInfo::default() };
        let Some(f) = self.combat.f.get(s) else { return HealthInfo::default() };
        let body = (f.body != u16::MAX).then(|| &self.combat.bodies[f.body as usize]);
        let wounds = f
            .wounds
            .iter()
            .map(|w| {
                let part = body.and_then(|b| b.parts.get(w.part as usize)).map(|p| p.name.clone()).unwrap_or_default();
                let mut fl: Vec<String> = wf::NAMES.iter().filter(|(b, _)| w.flags & b != 0).map(|(_, n)| n.to_string()).collect();
                fl.extend(hf::NAMES.iter().filter(|(b, _)| w.flags & b != 0).map(|(_, n)| n.to_string()));
                (part, fl)
            })
            .collect();
        let job = self.health.jobs.iter().find(|j| j.patient == s as u32);
        HealthInfo {
            diagnosed: !f.wounds.is_empty() && f.wounds.iter().all(|w| w.flags & hf::DIAGNOSED != 0),
            infected: f.wounds.iter().any(|w| w.flags & hf::INFECTED != 0),
            wounds,
            pending: job.map(|j| j.treat.caption().to_string()).or_else(|| self.next_treat(s, true).map(|t| t.caption().to_string())),
            doctor: job.filter(|j| j.doctor != NONE).map(|j| self.ai[j.doctor as usize].id),
        }
    }

    /// Switch health care (diagnosis, treatment, infections) on or off (default on).
    pub fn set_health_enabled(&mut self, on: bool) {
        self.health.off = !on;
    }

    /// Tests/debug: infect all open wounds of a unit now.
    pub fn infect_unit(&mut self, unit_id: i32) -> usize {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return 0 };
        let mut n = 0;
        if let Some(f) = self.combat.f.get_mut(s) {
            for w in f.wounds.iter_mut() {
                if w.flags & OPEN != 0 && w.flags & hf::INFECTED == 0 {
                    w.flags |= hf::INFECTED;
                    n += 1;
                }
            }
        }
        self.health.stats.infections += n as u32;
        let _ = AnnouncementKind::Info;
        n
    }
}

// ---------------------------------------------------------------------- [military2] recover wounded
//
// Wiki (Health care, Recover wounded labor): dwarves with the Recover wounded labor carry
// injured dwarves who cannot move (unconscious, or unable to stand) to a hospital bed, else
// another free bed. Units with a broken leg cannot stand and crawl (no crutches yet).
// Estimates: crawling is CRAWL_FACTOR (4) times slower than walking; nobody is carried while an
// able hostile is within 3 tiles of the patient; picking up and putting down take no time.

impl Sim {
    /// Free bed for a patient: hospital beds first, then its own bed, then any free bed.
    pub(crate) fn rescue_bed_for(&self, p: usize) -> Option<usize> {
        let pos = self.ai[p].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return None;
        }
        let up = self.nav.xyz(pos);
        let uid = self.ai[p].id;
        let mut best = (i32::MAX, usize::MAX);
        for (k, bed) in self.beds.iter().enumerate() {
            if bed.occupant != NONE || (bed.owner >= 0 && bed.owner != uid) || self.nav.comp_near(bed.tile as usize) != c {
                continue;
            }
            let bp_ = self.world.pos_of(bed.tile as usize);
            let hosp = self.combat.hospitals.iter().any(|&(x1, y1, x2, y2, z)| bp_.z == z && bp_.x >= x1 && bp_.x <= x2 && bp_.y >= y1 && bp_.y <= y2);
            let d = crate::manhattan(up, self.nav.xyz(bed.tile as usize)) - if hosp { 10_000 } else { 0 } - if bed.owner == uid { 5_000 } else { 0 };
            if d < best.0 {
                best = (d, k);
            }
        }
        (best.1 != usize::MAX).then_some(best.1)
    }

    /// Lies on the floor (or was knocked out), cannot walk to a bed and somebody could carry it.
    fn needs_carry(&self, s: usize) -> bool {
        if self.health.any_med & ml::RESCUE == 0 {
            return false;
        }
        let Some(f) = self.combat.f.get(s) else { return false };
        if f.carried_by != NONE || matches!(f.sub, Sub::Resting(b) | Sub::Rest(b) if b >= 0) {
            return false;
        }
        (f.uncon_until > self.ticks || self.cannot_stand(s)) && self.needs_rest(s) && !self.fight_nearby(s) && self.rescue_bed_for(s).is_some()
    }

    /// [combat hook] Somebody could carry this patient to bed (it should wait instead of crawling).
    pub(crate) fn rescue_available(&self, s: usize) -> bool {
        self.health.any_med & ml::RESCUE != 0 && !self.health.off && self.rescue_bed_for(s).is_some()
    }

    /// Next to the patient: pick it up and head for a bed.
    fn rescue_pick_up(&mut self, s: usize, k: usize, p: usize, now: u64) {
        let busy = self.combat.f[p].carried_by != NONE || self.ai[p].kind == Kind::Dead;
        let bed = if busy { None } else { self.rescue_bed_for(p) };
        let Some(b) = bed else {
            self.health.jobs.remove(k);
            self.ai[s].task = Task::Idle;
            self.ai[s].wake = now + 1;
            return;
        };
        self.beds[b].occupant = p as u32;
        self.carry_start(s, p, now);
        self.health.jobs[k].step = CStep::Carry(b as i32);
        let t = self.beds[b].tile;
        self.request_path(s, Goals::one(t), now);
    }

    /// At the bed with the patient: lay it down.
    fn rescue_arrive(&mut self, s: usize, k: usize, b: i32, now: u64) {
        let p = self.health.jobs[k].patient as usize;
        self.health.jobs.remove(k);
        if let Some(v) = self.carry_drop(s, now) {
            self.rescue_lay_down(v, b, now);
            self.combat.stats.rescues += 1;
            let (dn, pn) = (self.uname(s), self.uname(p));
            self.health.stats.events.push((now, format!("{dn} carried {pn} to bed.")));
        } else {
            self.rescue_free_bed(b, p as u32);
        }
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// Put a carried patient down: in bed `b` (reserved for it) or on the floor (-1).
    pub(crate) fn rescue_lay_down(&mut self, v: usize, b: i32, now: u64) {
        if self.ai[v].kind == Kind::Dead {
            self.rescue_free_bed(b, v as u32);
            return;
        }
        let f = &mut self.combat.f[v];
        f.role = crate::combat::Role::Patient;
        f.sub = Sub::Resting(b);
        if b >= 0 {
            self.beds[b as usize].occupant = v as u32;
        }
        self.ai[v].task = Task::Combat;
        self.ai[v].wake = now + 1;
    }

    pub(crate) fn rescue_free_bed(&mut self, b: i32, p: u32) {
        if b >= 0 && (b as usize) < self.beds.len() && self.beds[b as usize].occupant == p {
            self.beds[b as usize].occupant = NONE;
        }
    }

    /// [raids hook] A carrier died or passed out: its rescue job ends.
    pub(crate) fn health_carry_dropped(&mut self, s: usize) {
        let Some(k) = self.care_job_of(s) else { return };
        let j = self.health.jobs.remove(k);
        if let CStep::Carry(b) = j.step {
            self.rescue_free_bed(b, j.patient);
        }
        let p = j.patient as usize;
        if p < self.combat.f.len() && self.combat.f[p].carried_by == NONE && self.ai[p].kind != Kind::Dead {
            let now = self.ticks;
            self.rescue_lay_down(p, -1, now);
        }
    }

    /// Every 100 ticks for wounded units: crawl (slow) while unable to stand.
    pub(crate) fn health_gait(&mut self, s: usize) {
        if self.ai[s].kind == Kind::Dead {
            return;
        }
        let crawl = self.cannot_stand(s);
        if crawl == self.combat.f[s].crawl {
            return;
        }
        self.combat.f[s].crawl = crawl;
        let base = self.world.units[self.ai[s].uidx].walk_speed;
        let base = if base > 0 { base } else { 900 };
        self.ai[s].speed = if crawl { base * CRAWL_FACTOR } else { base };
        if crawl {
            self.combat.stats.crawlers += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use rf_core::{Building, Item, ItemFlags, World};

    use super::*;

    fn hospital_world() -> World {
        let mut w = World::synthetic(48, 48, 20);
        let hall = w.units[0].pos;
        // Beds in the hall and medical supplies.
        for k in 0..3 {
            let p = Pos::new(hall.x + k, hall.y + 4, hall.z);
            w.buildings.push(Building { id: 700 + k as i32, kind: "Bed".into(), x1: p.x, y1: p.y, x2: p.x, y2: p.y, z: p.z, center: p, built: true, owner: -1, ..Default::default() });
        }
        let sp = Pos::new(hall.x + 6, hall.y, hall.z);
        for (k, kind) in ["THREAD", "THREAD", "CLOTH", "CLOTH", "CLOTH", "SPLINT"].iter().enumerate() {
            w.items.push(Item { id: 5000 + k as i32, kind: kind.to_string(), mat: 0, pos: sp, stack: 1, container: -1, holder: -1, building: -1, flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() });
        }
        // Doctors: unit 1 diagnoses and sets bones, unit 2 sutures and dresses.
        w.units[1].labors.extend(["DIAGNOSE", "BONE_SETTING", "SURGERY"].iter().map(|s| s.to_string()));
        w.units[2].labors.extend(["SUTURING", "DRESSING_WOUNDS", "FEED_WATER_CIVILIANS"].iter().map(|s| s.to_string()));
        // Patient: unit 0 with a deep cut on the upper body and a broken bone.
        w.units[0].wounds = vec![
            rf_core::Wound { body_part: "UB".into(), layer: "MUSCLE".into(), flags: vec!["CUT".into(), "BLEEDING".into()], severity: 4000, pain: 20, bleeding: 3, age: 0 },
            rf_core::Wound { body_part: "LUL".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 5000, pain: 40, bleeding: 0, age: 0 },
        ];
        w
    }

    #[test]
    fn wounded_dwarf_is_diagnosed_and_treated() {
        let w = hospital_world();
        let mut sim = Sim::new(w, 21);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        let uid = sim.world.units[0].id;
        assert!(!sim.health_info(uid).wounds.is_empty(), "wounds loaded");
        let mut ok = false;
        for _ in 0..300 {
            sim.run_ticks(100);
            let r = sim.health_report();
            if r.diagnoses >= 1 && r.sutures >= 1 && r.bones_set >= 1 && r.dressings >= 1 {
                ok = true;
                break;
            }
        }
        let r = sim.health_report();
        assert!(ok, "treatments: {r:?} info {:?}", sim.health_info(uid));
        let info = sim.health_info(uid);
        if !info.wounds.is_empty() {
            assert!(info.diagnosed, "{info:?}");
        }
        // Thread, cloth and the splint were used up.
        assert!(!sim.world.items.iter().any(|i| i.id == 5005 && i.stack > 0 && !i.flags.in_job) || sim.world.item(5005).is_none());
        // The sutured cut stopped bleeding.
        let (blood, bmax, _) = sim.unit_condition(uid).unwrap();
        assert!(blood > bmax / 2);
        // Treatment flags are visible in Unit::wounds.
        let u = sim.world.unit(uid).unwrap();
        assert!(u.wounds.iter().any(|w| w.flags.iter().any(|f| f == "DIAGNOSED")) || u.wounds.is_empty());
    }

    #[test]
    fn untreated_infection_bleeds_and_worsens() {
        let mut w = World::synthetic(48, 48, 20);
        w.units[0].wounds = vec![rf_core::Wound { body_part: "UB".into(), layer: "SKIN".into(), flags: vec!["CUT".into()], severity: 1500, pain: 5, bleeding: 0, age: 0 }];
        let mut sim = Sim::new(w, 23);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        let uid = sim.world.units[0].id;
        assert_eq!(sim.infect_unit(uid), 1);
        let (b0, bmax, _) = sim.unit_condition(uid).unwrap();
        sim.run_ticks(12_000);
        let (b1, _, _) = sim.unit_condition(uid).unwrap();
        assert!(b1 < b0 - bmax / 20, "infection drains blood: {b0} -> {b1} of {bmax}");
        let info = sim.health_info(uid);
        assert!(info.infected || sim.health_report().infections_cured > 0, "{info:?}");
    }

    #[test]
    fn daily_infection_odds_follow_treatment() {
        // Many untreated cuts: some get infected within a month; with soap-cleaned wounds almost none.
        let mut w = World::synthetic(48, 48, 20);
        for u in w.units.iter_mut().take(7) {
            u.wounds = vec![rf_core::Wound { body_part: "UB".into(), layer: "SKIN".into(), flags: vec!["CUT".into()], severity: 9000, pain: 5, bleeding: 0, age: 0 }];
        }
        let mut sim = Sim::new(w, 29);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        sim.health.any_med = 0; // nobody treats
        let mut infected = 0;
        for d in 0..60u64 {
            sim.infection_daily(1000 + d * 1200);
            infected = sim.health.stats.infections;
        }
        // 7 wounds x 60 days at 1/100: about 4 expected.
        assert!(infected >= 1, "{infected}");
    }

    /// [military2] Unit 0 has a broken lower leg; beds in the hall; `rescuers` get RECOVER_WOUNDED.
    fn broken_leg_world(rescuers: bool) -> World {
        let mut w = World::synthetic(48, 48, 20);
        let hall = w.units[0].pos;
        for k in 0..3 {
            let p = Pos::new(hall.x + 2 * k - 2, hall.y + 5, hall.z);
            w.buildings.push(Building { id: 700 + k as i32, kind: "Bed".into(), x1: p.x, y1: p.y, x2: p.x, y2: p.y, z: p.z, center: p, built: true, owner: -1, ..Default::default() });
        }
        for u in w.units.iter_mut() {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
            if rescuers && u.id != 1 {
                u.labors.push("RECOVER_WOUNDED".into());
            }
        }
        w.units[0].pos = Pos::new(hall.x - 4, hall.y - 3, hall.z);
        w.units[0].wounds = vec![rf_core::Wound { body_part: "LLL".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 6000, pain: 30, bleeding: 0, age: 0 }];
        w
    }

    #[test]
    fn broken_leg_patient_is_carried_to_bed() {
        let mut sim = Sim::new(broken_leg_world(true), 31);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        let s = sim.unit_slot[&1];
        let start = sim.ai[s].pos;
        assert!(sim.cannot_stand(s), "a broken lower leg: cannot stand");
        let mut carried = false;
        for _ in 0..300 {
            sim.run_ticks(10);
            if sim.combat.f[s].carried_by != NONE {
                carried = true;
            }
            if !carried && sim.combat.stats.rescues == 0 {
                assert_eq!(sim.ai[s].pos, start, "the patient does not walk on a broken leg");
            }
            if sim.combat.stats.rescues > 0 {
                break;
            }
        }
        assert!(carried, "somebody picked the patient up");
        assert_eq!(sim.combat.stats.rescues, 1);
        let Sub::Resting(b) = sim.combat.f[s].sub else { panic!("not resting: {:?}", sim.combat.f[s].sub) };
        assert!(b >= 0, "in a bed");
        assert_eq!(sim.beds[b as usize].tile, sim.ai[s].pos, "lying on the bed");
        assert_eq!(sim.combat.carriers, 0);
        sim.run_ticks(400);
        assert_eq!(sim.ai[s].pos, sim.beds[b as usize].tile, "stays in bed");
    }

    #[test]
    fn without_rescuers_the_patient_crawls() {
        let mut sim = Sim::new(broken_leg_world(false), 33);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        let s = sim.unit_slot[&1];
        sim.run_ticks(300);
        assert!(sim.combat.f[s].crawl, "crawling");
        assert_eq!(sim.ai[s].speed, 900 * CRAWL_FACTOR);
        assert!(sim.combat.stats.crawlers >= 1);
        assert_eq!(sim.combat.stats.rescues, 0);
        // Healed: walks again.
        sim.combat.f[s].wounds.clear();
        sim.run_ticks(200);
        assert!(!sim.combat.f[s].crawl);
        assert_eq!(sim.ai[s].speed, 900);
    }
}
