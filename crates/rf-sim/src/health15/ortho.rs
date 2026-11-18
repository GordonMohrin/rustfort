//! [health15] M4-M8: fracture classes, setting and immobilizing (splints, casts), traction
//! benches, per-tissue healing and permanent damage, surgery indications.

use super::{mx, NO_HEAL};
use crate::combat::{bp, wf, LayerT, Sub, WoundS};
use crate::health::{hf, CStep, Treat};
use crate::path::Goals;
use crate::{Kind, Sim, Task, NONE};

const OPEN: u16 = wf::CUT | wf::TORN | wf::SEVERED;

/// DF HEALING_RATE by tissue name when the export has none (vanilla raws,
/// `tissue_template_default.txt`): bone and shell 1000, nervous and hard keratin/enamel tissue
/// none, hair-like tissue treated as 100 (regrowth is a separate DF mechanic), else 100.
pub(crate) fn heal_of_tissue(up: &str) -> u16 {
    const HAIR: [&str; 8] = ["HAIR", "FEATHER", "EYEBROW", "EYELASH", "WHISKER", "MOUSTACHE", "SIDEBURNS", "BEARD"];
    if HAIR.iter().any(|h| up.contains(h)) {
        return 100;
    }
    if up.contains("BONE") || up.contains("SHELL") {
        return 1000;
    }
    const NEVER: [&str; 11] = ["NERVE", "BRAIN", "CARTILAGE", "NAIL", "CLAW", "TALON", "TOOTH", "IVORY", "HORN", "HOOF", "SPINE"];
    if NEVER.iter().any(|h| up.contains(h)) {
        return NO_HEAL;
    }
    100
}

/// SETTABLE tissue by name (BONE, SHELL in the vanilla raws).
pub(crate) fn settable_tissue(up: &str) -> bool {
    up.contains("BONE") || up.contains("SHELL")
}

/// Exported layer values (HEALING_RATE, SETTABLE flag) override the name fallback.
pub(crate) fn layer_from_raw(l: &mut LayerT, t: &rf_core::TissueLayerRaw, raw_heal: bool) {
    if raw_heal && !l.hair {
        l.heal = if t.healing_rate > 0 { t.healing_rate.min(60_000) as u16 } else { NO_HEAL };
    }
    if t.flags.iter().any(|f| f == "SETTABLE" || f == "SPLINTABLE") {
        l.settable = true;
    }
}

/// `mx` bits from exported DF wound flag names.
pub(crate) fn mx_of_flags(flags: &[String]) -> u16 {
    let mut b = 0;
    for f in flags {
        b |= match f.as_str() {
            "NEEDS_SETTING" => mx::NEEDS_SET | mx::SEEN,
            "COMPOUND_FRACTURE" => mx::COMPOUND | mx::SEEN,
            "OVERLAPPING_FRACTURE" => mx::OVERLAP | mx::SEEN,
            "MOTOR_NERVE_SEVERED" => mx::MOTOR,
            "SENSORY_NERVE_SEVERED" => mx::SENSORY,
            "SPLINT" | "IMMOBILIZED" => mx::SPLINT | mx::SEEN,
            "CAST" => mx::CAST | mx::SEEN,
            _ => 0,
        };
    }
    b
}

/// The fracture is held still (splint, cast, sling or traction).
#[inline]
pub(crate) fn immobilized(w: &WoundS) -> bool {
    w.mx & (mx::IMMOB | mx::SLING) != 0 && !(w.mx & mx::OVERLAP != 0 && w.mx & mx::TRACTION == 0)
}

/// Bone setting may proceed (compound fractures need surgery first).
#[inline]
pub(crate) fn settable_now(w: &WoundS) -> bool {
    !(w.mx & mx::COMPOUND != 0 && w.flags & hf::OPERATED == 0)
}

/// A broken wound that a bone doctor must set (displaced; unclassified wounds: the old rule).
#[inline]
pub(crate) fn needs_setting(w: &WoundS) -> bool {
    if w.mx & mx::SEEN == 0 {
        return w.mx & mx::IMMOB == 0;
    }
    w.mx & mx::NEEDS_SET != 0 && settable_now(w)
}

/// Surgery indications besides internal organs: compound fractures, old infections.
#[inline]
pub(crate) fn needs_surgery(w: &WoundS, _now: u64) -> bool {
    if w.flags & hf::OPERATED != 0 {
        return false;
    }
    (w.flags & wf::BROKEN != 0 && w.mx & mx::COMPOUND != 0) || (w.flags & hf::INFECTED != 0 && w.age >= 7 * rf_core::TICKS_PER_DAY)
}

/// Per-tissue healing (M8): `rate` from `heal_pass` scaled by DF HEALING_RATE (100 = as is,
/// bone 1000 = a tenth, none = 0); fractures heal only when set and held still; immobilized
/// bones heal also out of bed (DF: splinted dwarves leave the hospital).
#[inline]
pub(crate) fn scale_heal(rate: i32, w: &WoundS, layers: &[LayerT], rest: bool, _hosp: bool) -> i32 {
    let heal = layers.get(w.layer as usize).map(|l| l.heal).unwrap_or(100);
    if heal == NO_HEAL {
        return 0;
    }
    let mut rate = rate;
    if w.flags & wf::BROKEN != 0 && w.mx & mx::SEEN != 0 {
        if (w.mx & mx::NEEDS_SET != 0 && w.flags & hf::SET == 0) || !settable_now(w) || (w.mx & mx::OVERLAP != 0 && w.mx & mx::TRACTION == 0) {
            return 0;
        }
        if !rest && immobilized(w) {
            rate = rate.max(60);
        }
    }
    if heal == 0 || heal == 100 {
        rate
    } else {
        rate * 100 / heal as i32
    }
}

impl Sim {
    fn h15_layer(&self, s: usize, w: &WoundS) -> Option<&LayerT> {
        let b = self.combat.f[s].body;
        self.combat.bodies.get(b as usize).and_then(|b| b.layers.get(w.layer as usize))
    }

    /// The broken wound sits on settable tissue (part-level breaks count as bone).
    pub(crate) fn h15_wound_settable(&self, s: usize, w: &WoundS) -> bool {
        w.layer == u16::MAX || self.h15_layer(s, w).map(|l| l.settable).unwrap_or(false)
    }

    /// M4 + M9: classify new wounds once (fracture class, nerve damage).
    pub(crate) fn h15_classify(&mut self, s: usize, now: u64) {
        let nw = self.combat.f[s].wounds.len();
        for k in 0..nw {
            let w = self.combat.f[s].wounds[k];
            if w.mx & mx::SEEN != 0 {
                continue;
            }
            let mut m = mx::SEEN;
            let b = self.combat.f[s].body as usize;
            let pflags = self.combat.bodies[b].parts.get(w.part as usize).map(|p| p.flags).unwrap_or(0);
            if w.flags & wf::BROKEN != 0 && self.h15_wound_settable(s, &w) {
                self.h15.rep.fractures_classified += 1;
                if w.flags & hf::SET != 0 {
                    m |= mx::SPLINT; // imported: set and immobilized
                } else {
                    if w.sev >= 5_000 {
                        m |= mx::NEEDS_SET;
                    }
                    // Bone through the skin: an open wound on a soft outer layer of the part
                    // from the same blow.
                    let compound = self.combat.f[s].wounds.iter().any(|o| {
                        o.part == w.part && o.flags & (wf::CUT | wf::TORN) != 0 && (o.age - w.age).abs() <= 100 && o.layer != w.layer && self.h15_layer(s, o).map(|l| !l.rigid).unwrap_or(false)
                    });
                    if compound {
                        m |= mx::COMPOUND;
                        self.h15.rep.compound += 1;
                    } else if w.sev >= 8_000 && self.h15_rng(s, now, 0x4815 ^ k as u64).chance(1, 4) {
                        m |= mx::OVERLAP;
                        self.h15.rep.overlapping += 1;
                    }
                }
            }
            // Nerves (M9): a deep cut into the muscle of a limb.
            let muscle = self.h15_layer(s, &w).map(|l| l.tissue.contains("muscle")).unwrap_or(false);
            if muscle && w.flags & (wf::CUT | wf::TORN) != 0 && w.sev >= 6_000 && pflags & (bp::LIMB | bp::GRASP | bp::STANCE) != 0 {
                let mut r = self.h15_rng(s, now, 0x4E52 ^ k as u64);
                if r.chance(1, 6) {
                    m |= mx::MOTOR;
                }
                if r.chance(1, 6) {
                    m |= mx::SENSORY;
                }
            }
            self.combat.f[s].wounds[k].mx |= m;
            if m & (mx::MOTOR | mx::SENSORY) != 0 {
                self.h15_add_nerve(s, w.part, m & mx::MOTOR != 0, m & mx::SENSORY != 0, now);
            }
        }
    }

    /// M8: wounds of tissue that never heals become permanent damage once they no longer bleed.
    pub(crate) fn h15_make_permanent(&mut self, s: usize, now: u64) {
        let b = self.combat.f[s].body as usize;
        let mut k = 0;
        let mut changed = false;
        while k < self.combat.f[s].wounds.len() {
            let w = self.combat.f[s].wounds[k];
            let never = w.layer != u16::MAX && self.combat.bodies[b].layers.get(w.layer as usize).map(|l| l.heal == NO_HEAL).unwrap_or(false);
            let settled = w.bleed == 0 && w.flags & (wf::SEVERED | hf::INFECTED) == 0 && (w.flags & (hf::SUTURED | hf::DRESSED) != 0 || w.age > rf_core::TICKS_PER_MONTH || w.flags & OPEN == 0);
            if !never || !settled {
                k += 1;
                continue;
            }
            let wound = self.combat.f[s].wounds.remove(k);
            let body = &self.combat.bodies[b];
            let part = body.parts.get(wound.part as usize).map(|p| p.token.clone()).unwrap_or_default();
            let layer = body.layers.get(wound.layer as usize).map(|l| l.tissue.to_ascii_uppercase()).unwrap_or_default();
            let mut flags: Vec<String> = wf::NAMES.iter().filter(|(bt, _)| wound.flags & bt != 0).map(|(_, n)| n.to_string()).collect();
            flags.extend(mx::NAMES.iter().filter(|(bt, _)| wound.mx & bt != 0).map(|(_, n)| n.to_string()));
            flags.push("PERMANENT".into());
            self.combat.f[s].scars.push(rf_core::Wound { body_part: part, layer, flags, severity: wound.sev, pain: 0, bleeding: 0, age: wound.age });
            let d = self.h15_dis_mut(s);
            if !d.perm.contains(&(wound.part, wound.layer)) {
                d.perm.push((wound.part, wound.layer));
            }
            self.h15.rep.permanent += 1;
            changed = true;
        }
        if changed {
            self.combat.f[s].wmirror_ok = false;
            self.sync_wounds(s);
            let _ = now;
        }
    }

    /// Hook at the end of `next_treat` (bone doctor present): immobilize, cast, traction, crutch.
    pub(crate) fn h15_next_treat(&self, s: usize) -> Option<Treat> {
        if self.h15.off {
            return None;
        }
        let f = self.combat.f.get(s)?;
        let eligible = |w: &WoundS| w.flags & wf::BROKEN != 0 && w.mx & mx::SEEN != 0 && settable_now(w) && !(w.mx & mx::NEEDS_SET != 0 && w.flags & hf::SET == 0) && self.h15_wound_settable(s, w);
        // Overlapping fractures: a free traction bench in reach.
        if f.wounds.iter().any(|w| eligible(w) && w.mx & mx::OVERLAP != 0 && w.mx & mx::TRACTION == 0) && self.h15_free_bench(s).is_some() {
            return Some(Treat::Traction);
        }
        let no_bench = self.h15.benches.is_empty();
        if let Some(w) = f.wounds.iter().find(|w| eligible(w) && w.mx & (mx::IMMOB | mx::SLING) == 0 && (w.mx & mx::OVERLAP == 0 || no_bench)) {
            if self.h15_cast_possible(s) {
                return Some(Treat::Cast);
            }
            if self.health.splints > 0 {
                return Some(Treat::Immobilize);
            }
            if self.health.cloths > 0 && w.sev < 5_000 && !self.h15_part_carries_weight(s, w.part) {
                return Some(Treat::Immobilize); // cloth sling for a minor fracture
            }
        }
        if self.h15.cnt.crutches > 0 && self.h15_wants_crutch(s) {
            return Some(Treat::Crutch);
        }
        None
    }

    /// The part or one below it is a STANCE part.
    pub(crate) fn h15_part_carries_weight(&self, s: usize, part: u16) -> bool {
        let f = &self.combat.f[s];
        let Some(body) = self.combat.bodies.get(f.body as usize) else { return false };
        // A STANCE part at or below `part` (walk up from every STANCE part).
        for (i, p) in body.parts.iter().enumerate() {
            if p.flags & bp::STANCE == 0 {
                continue;
            }
            let mut k = i as i32;
            while k >= 0 {
                if k == part as i32 {
                    return true;
                }
                k = body.parts[k as usize].parent;
            }
        }
        false
    }

    /// Plaster powder, cloth, an empty bucket and water are there (wiki Cast).
    fn h15_cast_possible(&self, s: usize) -> bool {
        self.h15.cnt.powder > 0 && self.h15.cnt.buckets > 0 && self.health.cloths > 0 && self.h15_water_near(s)
    }

    /// M5/M6/M7/M10: a finished immobilize / cast / crutch job (traction is done on arrival).
    pub(crate) fn h15_apply(&mut self, d: usize, p: usize, t: Treat, item: i32, now: u64) {
        let dname = self.uname(d);
        let pname = self.uname(p);
        match t {
            Treat::Immobilize | Treat::Cast => {
                let kind_item = self.item_idx.get(&item).map(|&i| self.world.items[i as usize].kind.clone()).unwrap_or_default();
                let (bit, kind) = match t {
                    Treat::Cast => (mx::CAST, 1u8),
                    _ if kind_item == "CLOTH" => (mx::SLING, 2u8),
                    _ => (mx::SPLINT, 0u8),
                };
                let nw = self.combat.f[p].wounds.len();
                let mut part = u16::MAX;
                for k in 0..nw {
                    let w = self.combat.f[p].wounds[k];
                    let ok = w.flags & wf::BROKEN != 0 && w.mx & mx::SEEN != 0 && settable_now(&w) && !(w.mx & mx::NEEDS_SET != 0 && w.flags & hf::SET == 0) && w.mx & (mx::IMMOB | mx::SLING) == 0 && self.h15_wound_settable(p, &w);
                    if ok && (part == u16::MAX || w.part == part) {
                        part = w.part;
                        self.combat.f[p].wounds[k].mx |= bit;
                    }
                }
                if part == u16::MAX {
                    return;
                }
                let pn = self.h15_part_name(p, part);
                let attach = if t == Treat::Cast {
                    // The powder is used up (generic path); cloth goes under the cast; the cast
                    // itself is a new item of hardened plaster (gypsum).
                    if let Some(c) = self.care_find_supply(d, "CLOTH", now) {
                        self.destroy_item(c);
                    }
                    let gm = self.ensure_material("INORGANIC:GYPSUM");
                    let tile = self.ai[p].pos as usize;
                    let id = self.create_item("ORTHOPEDIC_CAST", gm, tile, "plaster cast".into());
                    self.h15_hand_over(d, p, id);
                    self.h15.rep.casts_applied += 1;
                    self.h15_event(now, format!("{dname} put {pname}'s {pn} in a cast."));
                    id
                } else {
                    if kind == 2 {
                        self.h15.rep.slings_applied += 1;
                    } else {
                        self.h15.rep.splints_applied += 1;
                    }
                    self.h15_event(now, format!("{dname} immobilized {pname}'s {pn}."));
                    item
                };
                if attach >= 0 {
                    self.h15_dis_mut(p).attached.push((attach, part, kind));
                }
            }
            Treat::Crutch => {
                if item >= 0 {
                    let d_ = self.h15_dis_mut(p);
                    d_.crutch = item;
                    d_.crutch_free_since = 0;
                    self.h15.rep.crutches_given += 1;
                    self.h15_event(now, format!("{dname} gave {pname} a crutch."));
                }
            }
            _ => {}
        }
    }

    /// Body part name of slot `s`.
    pub(crate) fn h15_part_name(&self, s: usize, part: u16) -> String {
        let f = &self.combat.f[s];
        self.combat.bodies.get(f.body as usize).and_then(|b| b.parts.get(part as usize)).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// M3 hook before supplies are destroyed: true if the item stays with the patient.
    pub(crate) fn h15_keep_item(&mut self, d: usize, p: usize, t: Treat, item: i32) -> bool {
        if self.h15.off || item < 0 || p == d {
            return false;
        }
        match t {
            Treat::Suture => {
                self.h15_hand_over(d, p, item);
                self.h15.rep.threads_sewn += 1;
                true
            }
            Treat::Immobilize | Treat::Crutch => {
                self.h15_hand_over(d, p, item);
                true
            }
            _ => false,
        }
    }

    /// M3: strapped splints, slings and casts come off when the bone has healed.
    pub(crate) fn h15_detach(&mut self, s: usize, now: u64) {
        let Some(k) = self.h15_dis_idx(s) else { return };
        if self.h15.dis[k].attached.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.h15.dis[k].attached.len() {
            let (item, part, kind) = self.h15.dis[k].attached[i];
            let broken = self.combat.f[s].wounds.iter().any(|w| w.part == part && w.flags & wf::BROKEN != 0);
            if broken {
                i += 1;
                continue;
            }
            self.h15.dis[k].attached.remove(i);
            self.h15.rep.items_removed += 1;
            match kind {
                0 => self.h15_put_down(s, item), // splints can be used again
                _ => self.destroy_item(item),    // casts are cut off, slings are soiled (estimate)
            }
            let _ = now;
        }
    }

    // ------------------------------------------------------------------ traction (M7)

    /// Free traction bench reachable from the patient (index into `h15.benches`).
    pub(crate) fn h15_free_bench(&self, p: usize) -> Option<usize> {
        let pos = self.ai[p].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return None;
        }
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, usize::MAX);
        for (k, b) in self.h15.benches.iter().enumerate() {
            if (b.occupant != NONE && b.occupant != p as u32) || self.nav.comp_near(b.tile as usize) != c {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(b.tile as usize));
            if d < best.0 {
                best = (d, k);
            }
        }
        (best.1 != usize::MAX).then_some(best.1)
    }

    /// The patient lies in a traction bench.
    pub(crate) fn h15_in_traction(&self, s: usize) -> bool {
        !self.h15.benches.is_empty() && self.combat.f.get(s).map(|f| f.wounds.iter().any(|w| w.mx & mx::TRACTION != 0)).unwrap_or(false)
    }

    /// Doctor next to the patient: pick it up and carry it to a free bench.
    pub(crate) fn h15_traction_pick_up(&mut self, s: usize, k: usize, p: usize, now: u64) {
        let busy = self.combat.f[p].carried_by != NONE || self.ai[p].kind == Kind::Dead;
        let bench = if busy { None } else { self.h15_free_bench(p) };
        let Some(b) = bench else {
            self.health.jobs.remove(k);
            self.ai[s].task = Task::Idle;
            self.ai[s].wake = now + 1;
            return;
        };
        self.h15.benches[b].occupant = p as u32;
        self.h15.benches[b].placed = false;
        self.carry_start(s, p, now);
        self.health.jobs[k].step = CStep::Carry(-(b as i32) - 10);
        let t = self.h15.benches[b].tile;
        self.request_path(s, Goals::one(t), now);
    }

    /// At the bench with the patient: lay it into traction.
    pub(crate) fn h15_traction_arrive(&mut self, s: usize, k: usize, code: i32, now: u64) {
        let p = self.health.jobs[k].patient as usize;
        self.health.jobs.remove(k);
        let b = (-(code + 10)) as usize;
        let v = self.carry_drop(s, now);
        let ok = v == Some(p) && b < self.h15.benches.len() && self.h15.benches[b].occupant == p as u32 && self.ai[s].pos == self.h15.benches[b].tile;
        if let Some(v) = v {
            self.rescue_lay_down(v, -1, now);
        }
        if ok {
            for w in self.combat.f[p].wounds.iter_mut() {
                if w.mx & mx::OVERLAP != 0 && w.flags & wf::BROKEN != 0 {
                    w.mx |= mx::TRACTION;
                }
            }
            let bench = &mut self.h15.benches[b];
            bench.placed = true;
            bench.since = now;
            self.h15.rep.placed_in_traction += 1;
            self.add_xp(s, "SET_BONE", 50);
            let (dn, pn) = (self.uname(s), self.uname(p));
            self.h15_event(now, format!("{dn} placed {pn} in traction."));
        } else if b < self.h15.benches.len() && self.h15.benches[b].occupant == p as u32 {
            self.h15.benches[b].occupant = NONE;
        }
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// Every 100 ticks: patients that left the bench early rebreak; healed ones are released.
    pub(crate) fn h15_traction_check(&mut self, now: u64) {
        for k in 0..self.h15.benches.len() {
            let b = self.h15.benches[k];
            if b.occupant == NONE {
                continue;
            }
            let p = b.occupant as usize;
            let dead = p >= self.ai.len() || self.ai[p].kind == Kind::Dead;
            if !b.placed {
                // Reserved: free it unless a traction carry is under way.
                let carried = !dead && self.combat.f[p].carried_by != NONE;
                let job = self.health.jobs.iter().any(|j| j.patient == b.occupant && j.treat == Treat::Traction && j.doctor != NONE);
                if dead || !(carried || job) {
                    self.h15.benches[k].occupant = NONE;
                }
                continue;
            }
            if dead {
                self.h15.benches[k].occupant = NONE;
                self.h15.benches[k].placed = false;
                continue;
            }
            let overlap = self.combat.f[p].wounds.iter().any(|w| w.mx & mx::OVERLAP != 0 && w.flags & wf::BROKEN != 0);
            if !overlap {
                self.h15.benches[k].occupant = NONE;
                self.h15.benches[k].placed = false;
                for w in self.combat.f[p].wounds.iter_mut() {
                    w.mx &= !mx::TRACTION;
                }
                self.h15.rep.released_from_traction += 1;
                let pn = self.uname(p);
                self.h15_event(now, format!("{pn} was released from traction."));
                continue;
            }
            let lying = matches!(self.combat.f[p].sub, Sub::Resting(_)) && self.ai[p].pos == b.tile && self.combat.f[p].carried_by == NONE;
            if !lying {
                self.h15.benches[k].occupant = NONE;
                self.h15.benches[k].placed = false;
                self.h15_rebreak(p, now);
            }
        }
    }

    /// Overlapping fractures out of traction rebreak fully (Toady One).
    pub(crate) fn h15_rebreak(&mut self, p: usize, now: u64) {
        if p >= self.ai.len() || self.ai[p].kind == Kind::Dead {
            return;
        }
        let mut any = false;
        for w in self.combat.f[p].wounds.iter_mut() {
            if w.mx & mx::TRACTION != 0 {
                w.mx &= !mx::TRACTION;
                if w.mx & mx::OVERLAP != 0 && w.flags & wf::BROKEN != 0 {
                    w.sev = 10_000;
                    any = true;
                }
            }
        }
        if any {
            self.h15.rep.rebroken += 1;
            let pn = self.uname(p);
            self.h15_event(now, format!("{pn}'s bone has broken again outside traction."));
            self.list_wounded(p);
        }
    }

    // ------------------------------------------------------------------ surgery (M12)

    /// Hook at the end of a successful surgery: compound fractures are cleaned up, infected
    /// tissue is cut away.
    pub(crate) fn h15_surgery_done(&mut self, d: usize, p: usize, now: u64) {
        if self.h15.off {
            return;
        }
        let r = self.h15_rating(d, "SURGERY");
        let mut roll = self.h15_rng(p, now, 0x5E7);
        let mut excised = 0;
        for w in self.combat.f[p].wounds.iter_mut() {
            if w.flags & hf::OPERATED != 0 {
                continue;
            }
            if w.flags & wf::BROKEN != 0 && w.mx & mx::COMPOUND != 0 {
                w.flags |= hf::OPERATED;
                w.mx &= !mx::COMPOUND;
            } else if w.flags & hf::INFECTED != 0 && w.age >= 7 * rf_core::TICKS_PER_DAY {
                w.flags = (w.flags | hf::OPERATED) & !hf::INFECTED;
                w.sev = (w.sev - 1_000).max(1);
                excised += 1;
            } else {
                continue;
            }
            // Infection after the operation (estimate 1/(20 + rating)).
            if w.flags & OPEN != 0 && roll.chance(1, 20 + r as u32) {
                w.flags |= hf::INFECTED;
            }
        }
        self.h15.rep.infections_excised += excised;
    }
}
