//! [health15] M11/M12: treatment outcome by skill, surgery dangers, cleaning with soap.

use super::mx;
use crate::combat::{bp, wf};
use crate::health::{hf, Treat};
use crate::Sim;

const OPEN: u16 = wf::CUT | wf::TORN | wf::SEVERED;

/// Success chance in percent for skill rating `r` (0..=20). All numbers are estimates: the wiki
/// only says that skill matters (Surgeon) and that diagnosis skill changes the time only.
pub(crate) fn success_pct(t: Treat, r: i32) -> u32 {
    let r = r.clamp(0, 20);
    let p = match t {
        Treat::Suture => 70 + 3 * r / 2,
        Treat::SetBone => 60 + 2 * r,
        Treat::Surgery => 50 + 5 * r / 2,
        _ => 100,
    };
    p.clamp(0, 100) as u32
}

/// Cleaning a wound (with soap) succeeds (estimate).
pub(crate) fn clean_pct(r: i32) -> u32 {
    (60 + 2 * r.clamp(0, 20)) as u32
}

fn salt(t: Treat) -> u64 {
    0x0C0E ^ match t {
        Treat::Suture => 1,
        Treat::SetBone => 2,
        Treat::Surgery => 3,
        Treat::Dress => 4,
        _ => 5,
    }
}

impl Sim {
    /// The patient lies in a hospital with a table (surgery at full skill).
    pub(crate) fn h15_on_table(&self, p: usize) -> bool {
        let pos = self.upos(p);
        self.h15_hosp_at(pos).map(|h| self.h15.hosp[h].tables > 0).unwrap_or(false)
    }

    /// Hook in `apply_treatment`: does the treatment succeed?
    pub(crate) fn h15_outcome(&self, d: usize, p: usize, t: Treat, now: u64) -> bool {
        if self.h15.off || !matches!(t, Treat::Suture | Treat::SetBone | Treat::Surgery) {
            return true;
        }
        let mut r = self.h15_rating(d, t_skill(t));
        if t == Treat::Surgery && !self.h15_on_table(p) {
            r /= 2; // wiki Surgeon: skill rolls halved off a table
        }
        let pct = success_pct(t, r);
        pct >= 100 || self.h15_rng(p, now, salt(t)).below(100) < pct
    }

    /// The failure effect of a treatment (the doctor still learns a little).
    pub(crate) fn h15_failed(&mut self, d: usize, p: usize, t: Treat, item: i32, now: u64) {
        let dname = self.uname(d);
        let pname = self.uname(p);
        match t {
            Treat::Suture => {
                // The thread is sewn in anyway; the bleeding is only slowed.
                if item >= 0 {
                    self.h15_hand_over(d, p, item);
                }
                for w in self.combat.f[p].wounds.iter_mut() {
                    if w.flags & OPEN != 0 && w.flags & hf::SUTURED == 0 {
                        w.bleed /= 2;
                    }
                }
                let f = &mut self.combat.f[p];
                f.bleed = f.wounds.iter().map(|w| w.bleed).sum();
                self.h15.rep.sutures_failed += 1;
                self.h15_event(now, format!("{dname} botched suturing {pname}."));
            }
            Treat::SetBone => {
                for w in self.combat.f[p].wounds.iter_mut() {
                    if w.flags & wf::BROKEN != 0 && w.flags & hf::SET == 0 {
                        w.sev = (w.sev + 500).min(10_000);
                        w.pain += 10;
                    }
                }
                self.h15.rep.settings_failed += 1;
                self.h15_event(now, format!("{dname} failed to set {pname}'s bone."));
            }
            Treat::Surgery => {
                let b = self.combat.f[p].body as usize;
                let mut roll = self.h15_rng(p, now, 0x5F1);
                let nw = self.combat.f[p].wounds.len();
                for k in 0..nw {
                    let w = self.combat.f[p].wounds[k];
                    if w.flags & hf::OPERATED != 0 {
                        continue;
                    }
                    let internal = self.combat.bodies.get(b).and_then(|bd| bd.parts.get(w.part as usize)).map(|pt| pt.flags & bp::INTERNAL != 0).unwrap_or(false);
                    let compound = w.flags & wf::BROKEN != 0 && w.mx & mx::COMPOUND != 0;
                    let infected = w.flags & hf::INFECTED != 0 && w.age >= 7 * rf_core::TICKS_PER_DAY;
                    if !(internal && w.sev >= 3_000) && !compound && !infected {
                        continue;
                    }
                    let ww = &mut self.combat.f[p].wounds[k];
                    ww.mx |= mx::SURG_FAIL;
                    ww.sev = (ww.sev + 1_500).min(10_000);
                    ww.bleed += 3;
                    ww.flags |= wf::BLEEDING;
                    if !compound {
                        ww.flags |= hf::OPERATED; // no second attempt on organs / excision
                    }
                    if ww.flags & OPEN != 0 && roll.chance(1, 3) {
                        ww.flags |= hf::INFECTED;
                    }
                }
                let f = &mut self.combat.f[p];
                f.bleed = f.wounds.iter().map(|w| w.bleed).sum();
                self.list_bleeder(p);
                self.h15.rep.surgeries_failed += 1;
                self.h15_event(now, format!("{dname}'s surgery on {pname} went wrong."));
            }
            _ => {}
        }
        if !t.skill_pub().is_empty() {
            self.add_xp(d, t.skill_pub(), 25);
        }
        self.sync_wounds(p);
    }

    /// Hook in the Dress treatment: clean with soap. Soap is used up even when the cleaning
    /// fails; false = the wounds are dressed but not sterilized.
    pub(crate) fn h15_clean(&mut self, d: usize, p: usize, now: u64) -> bool {
        if self.h15.off {
            return true;
        }
        if !self.h15_use_soap() {
            return false;
        }
        let r = self.h15_rating(d, "DRESS_WOUNDS");
        if self.h15_rng(p, now, salt(Treat::Dress)).below(100) < clean_pct(r) {
            return true;
        }
        for w in self.combat.f[p].wounds.iter_mut() {
            if w.flags & OPEN != 0 {
                w.mx |= mx::CLEAN_FAIL;
            }
        }
        self.h15.rep.cleanings_failed += 1;
        false
    }
}

fn t_skill(t: Treat) -> &'static str {
    t.skill_pub()
}
