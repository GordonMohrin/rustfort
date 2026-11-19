//! [health15] M9 nerve damage and permanent disabilities, M10 crutches and crutch walking.

use super::{immobilized, mx, CRAWL_DEN, CRAWL_NUM};
use crate::combat::{bp, wf};
use crate::health::{ml, Treat};
use crate::{Kind, Sim};

/// How a unit gets around.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Walk {
    #[default]
    Normal,
    Crutch,
    Crawl,
}

/// Crutch walking delay (estimate: x2 at rating 0, falling linearly to x1 at Legendary+5 =
/// rating 20, like the wiki's Ambusher penalty shape).
pub(crate) fn crutch_delay(walk: i32, rating: i32) -> i32 {
    walk * (40 - rating.clamp(0, 20)) / 20
}

/// Crawling delay: the raws' crawl/walk gait ratio (DWARF 2990 / 900).
pub(crate) fn crawl_delay(walk: i32) -> i32 {
    walk * CRAWL_NUM / CRAWL_DEN
}

impl Sim {
    /// Status of the limb chain from part `i` up to the body: 0 usable, 1 function lost but a
    /// crutch helps (lost, motor nerve, immobilized fracture), 2 unusable (loose fracture).
    fn h15_chain(&self, s: usize, i: usize) -> u8 {
        let f = &self.combat.f[s];
        let body = &self.combat.bodies[f.body as usize];
        let mut st = 0u8;
        let mut k = i as i32;
        while k >= 0 {
            let ku = k as usize;
            if f.part_state.get(ku).map(|&x| x & (1 | 4) != 0).unwrap_or(false) {
                st = st.max(1);
            }
            // An immobilized fracture takes the function of a limb (crutch); on the torso it
            // no longer stops standing (spec M5: only immobilized STANCE limbs need a crutch).
            let limb = body.parts[ku].flags & (bp::LIMB | bp::STANCE | bp::GRASP) != 0;
            for w in &f.wounds {
                if w.part as usize == ku && w.flags & wf::BROKEN != 0 {
                    st = st.max(if !immobilized(w) { 2 } else if limb { 1 } else { 0 });
                }
            }
            k = body.parts[ku].parent;
        }
        st
    }

    /// (gait, needs a crutch, could use one). None for bodies without STANCE parts.
    pub(crate) fn h15_walk(&self, s: usize) -> Option<(Walk, bool, bool)> {
        let f = self.combat.f.get(s)?;
        if f.body == u16::MAX {
            return None;
        }
        let body = &self.combat.bodies[f.body as usize];
        let (mut n, mut good, mut hard, mut grasp) = (0, 0, false, false);
        for (i, p) in body.parts.iter().enumerate() {
            if p.flags & bp::STANCE != 0 {
                n += 1;
                match self.h15_chain(s, i) {
                    0 => good += 1,
                    2 => hard = true,
                    _ => {}
                }
            } else if p.flags & bp::GRASP != 0 && !grasp && self.h15_chain(s, i) == 0 {
                grasp = true;
            }
        }
        if n == 0 {
            return None;
        }
        if hard {
            return Some((Walk::Crawl, false, false));
        }
        if good == n {
            return Some((Walk::Normal, false, false));
        }
        // Wiki Crutch: no crutches for spinal nerve injuries.
        let capable = good >= 1 && grasp && !f.paralyzed;
        let has = self.h15_dis_idx(s).map(|k| self.h15.dis[k].crutch >= 0).unwrap_or(false);
        Some((if capable && has { Walk::Crutch } else { Walk::Crawl }, true, capable))
    }

    /// Hook at the top of `cannot_stand` (None: health15 off, use the old rule).
    pub(crate) fn h15_cannot_stand(&self, s: usize) -> Option<bool> {
        if self.h15.off || self.health.off {
            return None;
        }
        self.h15_walk(s).map(|(w, _, _)| w == Walk::Crawl)
    }

    /// Patient who could walk with a crutch and has none.
    pub(crate) fn h15_wants_crutch(&self, s: usize) -> bool {
        if self.ai[s].kind != Kind::Worker {
            return false;
        }
        matches!(self.h15_walk(s), Some((Walk::Crawl, true, true)))
    }

    /// Skill rating 0..=20 (Legendary+5 = 20) with focus, like other medical jobs.
    pub(crate) fn h15_rating(&self, s: usize, skill: &str) -> i32 {
        let raw = self.world.units[self.ai[s].uidx].skill(skill).clamp(0, 20);
        self.social_rating(s, raw).clamp(0, 20)
    }

    /// Hook in `health_gait`: walk, crutch or crawl delay. True if handled.
    pub(crate) fn h15_gait(&mut self, s: usize) -> bool {
        if self.h15.off || self.health.off {
            return false;
        }
        let Some((mode, _, _)) = self.h15_walk(s) else { return false };
        let base = self.world.units[self.ai[s].uidx].walk_speed;
        let base = if base > 0 { base } else { 900 };
        let (key, speed) = match mode {
            Walk::Normal => (0u16, base),
            Walk::Crutch => {
                let r = self.h15_rating(s, "CRUTCH_WALK");
                (1 + 4 * r as u16, crutch_delay(base, r))
            }
            Walk::Crawl => (2, crawl_delay(base)),
        };
        let prev = self.h15_dis_idx(s).map(|k| self.h15.dis[k].walk_key).unwrap_or(0);
        let crawl = mode == Walk::Crawl;
        if key == prev && self.combat.f[s].crawl == crawl {
            return true;
        }
        if crawl && !self.combat.f[s].crawl {
            self.combat.stats.crawlers += 1;
        }
        self.combat.f[s].crawl = crawl;
        self.ai[s].speed = speed;
        if key != 0 || self.h15_dis_idx(s).is_some() {
            self.h15_dis_mut(s).walk_key = key;
        }
        true
    }

    /// Record nerve damage on a part (M9).
    pub(crate) fn h15_add_nerve(&mut self, s: usize, part: u16, motor: bool, sensory: bool, now: u64) {
        let name = self.uname(s);
        let pn = self.h15_part_name(s, part);
        let d = self.h15_dis_mut(s);
        let mut new_m = false;
        let mut new_s = false;
        if motor {
            if let Err(i) = d.motor.binary_search(&part) {
                d.motor.insert(i, part);
                new_m = true;
            }
        }
        if sensory {
            if let Err(i) = d.sensory.binary_search(&part) {
                d.sensory.insert(i, part);
                new_s = true;
            }
        }
        if new_m {
            self.h15.rep.motor_nerves += 1;
            self.h15_event(now, format!("A motor nerve in {name}'s {pn} has been severed."));
        }
        if new_s {
            self.h15.rep.sensory_nerves += 1;
            self.h15_event(now, format!("A sensory nerve in {name}'s {pn} has been severed."));
        }
        self.h15_apply_disab(s);
    }

    /// Re-apply permanent effects: motor nerve -> function lost (part_state bit 4) on the part
    /// and every part beyond it; sensory nerve -> no pain there.
    pub(crate) fn h15_apply_disab(&mut self, s: usize) {
        let Some(k) = self.h15_dis_idx(s) else { return };
        if self.h15.dis[k].motor.is_empty() && self.h15.dis[k].sensory.is_empty() {
            return;
        }
        let b = self.combat.f[s].body;
        if b == u16::MAX {
            return;
        }
        let mut stack = std::mem::take(&mut self.h15.scratch);
        stack.clear();
        stack.extend(self.h15.dis[k].motor.iter().copied());
        {
            let body = &self.combat.bodies[b as usize];
            let ps = &mut self.combat.f[s].part_state;
            while let Some(p) = stack.pop() {
                if let Some(x) = ps.get_mut(p as usize) {
                    *x |= 4;
                }
                if let Some(pt) = body.parts.get(p as usize) {
                    stack.extend(pt.children.iter().copied());
                }
            }
        }
        self.h15.scratch = stack;
        if !self.h15.dis[k].sensory.is_empty() {
            let body = &self.combat.bodies[b as usize];
            let sens = &self.h15.dis[k].sensory;
            for w in self.combat.f[s].wounds.iter_mut() {
                let mut q = w.part as i32;
                while q >= 0 {
                    if sens.binary_search(&(q as u16)).is_ok() {
                        w.pain = 0;
                        break;
                    }
                    q = body.parts.get(q as usize).map(|p| p.parent).unwrap_or(-1);
                }
            }
        }
    }

    /// M10 every 100 ticks: crutch walking practice, crutch returned when no longer needed.
    pub(crate) fn h15_crutch_tick(&mut self, s: usize, now: u64) {
        let Some(k) = self.h15_dis_idx(s) else { return };
        let crutch = self.h15.dis[k].crutch;
        if crutch < 0 {
            return;
        }
        let uid = self.ai[s].id;
        let held = self.item_idx.get(&crutch).map(|&i| !self.imeta[i as usize].dead && self.world.items[i as usize].holder == uid).unwrap_or(false);
        if !held {
            self.h15.dis[k].crutch = -1;
            return;
        }
        let walk = self.h15_walk(s);
        let pos = self.ai[s].pos;
        if matches!(walk, Some((Walk::Crutch, _, _))) && pos != self.h15.dis[k].last_pos {
            self.add_xp(s, "CRUTCH_WALK", 10);
        }
        self.h15.dis[k].last_pos = pos;
        let need = matches!(walk, Some((_, true, _)));
        if need {
            self.h15.dis[k].crutch_free_since = 0;
        } else if self.h15.dis[k].crutch_free_since == 0 {
            self.h15.dis[k].crutch_free_since = now;
        } else if now >= self.h15.dis[k].crutch_free_since + rf_core::TICKS_PER_DAY as u64 {
            self.h15.dis[k].crutch = -1;
            self.h15.dis[k].crutch_free_since = 0;
            self.h15_put_down(s, crutch);
            self.h15.rep.crutches_returned += 1;
        }
    }

    /// Hook in `health_pass`: crutch jobs for disabled units that are not patients in bed.
    pub(crate) fn h15_want(&self, want: &mut Vec<(u32, Treat)>) {
        if self.h15.off || self.h15.cnt.crutches <= 0 || self.health.any_med & ml::BONE == 0 {
            return;
        }
        for d in &self.h15.dis {
            let s = d.slot as usize;
            if d.crutch >= 0 || s >= self.ai.len() || want.iter().any(|w| w.0 == d.slot) {
                continue;
            }
            if self.h15_wants_crutch(s) && self.world.units[self.ai[s].uidx].flags.citizen {
                want.push((d.slot, Treat::Crutch));
            }
        }
    }

    /// Import (first run): nerve damage on exported wounds and scars, held crutches, splints
    /// and casts.
    pub(crate) fn h15_import_disabilities(&mut self) {
        let now = self.ticks;
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead || self.combat.f.get(s).map(|f| f.body == u16::MAX).unwrap_or(true) {
                continue;
            }
            let uidx = self.ai[s].uidx;
            let b = self.combat.f[s].body as usize;
            let mut nerves: Vec<(u16, bool, bool)> = Vec::new();
            for w in &self.world.units[uidx].wounds {
                let m = w.flags.iter().any(|f| f == "MOTOR_NERVE_SEVERED");
                let sn = w.flags.iter().any(|f| f == "SENSORY_NERVE_SEVERED");
                if !(m || sn) {
                    continue;
                }
                if let Some(pi) = self.combat.bodies[b].parts.iter().position(|p| p.token == w.body_part) {
                    nerves.push((pi as u16, m, sn));
                }
            }
            for (p, m, sn) in nerves {
                let d = self.h15_dis_mut(s);
                if m && d.motor.binary_search(&p).is_err() {
                    let i = d.motor.binary_search(&p).unwrap_err();
                    d.motor.insert(i, p);
                }
                if sn && d.sensory.binary_search(&p).is_err() {
                    let i = d.sensory.binary_search(&p).unwrap_err();
                    d.sensory.insert(i, p);
                }
                self.h15.rep.motor_nerves += m as u32;
                self.h15.rep.sensory_nerves += sn as u32;
            }
            // Held crutches, splints and casts.
            let inv = self.world.units[uidx].inventory.clone();
            for id in inv {
                let Some(&i) = self.item_idx.get(&id) else { continue };
                let kind = self.world.items[i as usize].kind.as_str();
                match kind {
                    "CRUTCH" => {
                        let d = self.h15_dis_mut(s);
                        if d.crutch < 0 {
                            d.crutch = id;
                        }
                    }
                    "SPLINT" | "ORTHOPEDIC_CAST" => {
                        let (bit, k) = if kind == "SPLINT" { (mx::SPLINT, 0u8) } else { (mx::CAST, 1u8) };
                        let part = self.combat.f[s].wounds.iter().find(|w| w.flags & wf::BROKEN != 0).map(|w| w.part);
                        if let Some(part) = part {
                            for w in self.combat.f[s].wounds.iter_mut() {
                                if w.part == part && w.flags & wf::BROKEN != 0 {
                                    w.mx |= bit | mx::SEEN;
                                    w.mx &= !mx::NEEDS_SET;
                                }
                            }
                            self.h15_dis_mut(s).attached.push((id, part, k));
                        }
                    }
                    _ => {}
                }
            }
            self.h15_apply_disab(s);
        }
        let _ = now;
    }
}
