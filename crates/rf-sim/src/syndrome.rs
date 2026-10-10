//! Forgotten beast syndromes, breath and webs (round 6, simplified).
//!
//! # DF rules implemented (wiki v50: Forgotten beast, Syndrome)
//! - Many forgotten beasts have a special attack, named at the end of their arrival text
//!   ("Beware its poisonous bite!", "Beware its deadly dust!", "Beware its webs!"):
//!   - a **poisonous bite or sting**: whenever one of its attacks wounds a creature, the venom
//!     takes hold;
//!   - a **breath** of deadly dust, noxious gas or toxic vapor blown at creatures a few tiles
//!     away, which affects everybody caught in the cloud;
//!   - **webs** shot at a target, which entangle it for a while.
//! - **Syndrome effects** from the wiki's list ("mild pain to complete and instantaneous
//!   necrosis or paralysis"): paralysis, unconsciousness, bleeding, pain, necrosis (rotting
//!   flesh) and death by respiratory failure. A paralyzed, unconscious or entangled creature
//!   cannot move or fight (soldiers on duty still finish off a helpless beast, and the beast
//!   kills helpless dwarves).
//! - Forgotten beasts are immune to their own syndrome; other hostiles in the cloud are not.
//!
//! # Estimates (not on the wiki)
//! - Odds per beast: no special attack 30 %, venom 35 %, breath 25 %, webs 10 %; the effect is
//!   uniform over the list (death only by venom, never in a breath cloud).
//! - Durations: paralysis 1 200-3 600 ticks, unconsciousness 600-1 800, entangled 100-300;
//!   bleeding adds a bleeding wound (4 blood per 10 ticks), pain a wound with pain 400 (most
//!   dwarves pass out), necrosis a severity-9 000 wound on a limb, respiratory failure kills
//!   600-1 800 ticks later.
//! - Breath range 2-8 tiles with a line of fire, the cloud covers the target tile and its 8
//!   neighbours, one breath every 600-1 000 ticks; webs range 2-10, every 300-500 ticks.
//! - Syndromes have no cure and no resistance roll (DF uses the creature's
//!   `SYN_IMMUNE`/size-dilution rules), fire breath is not modelled (no temperature).

use crate::combat::{wf, Role, WoundS};
use crate::rng::Rng;
use crate::{Kind, Sim, NONE};

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub(crate) enum Delivery {
    Venom,
    Breath,
    Web,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub(crate) enum Effect {
    Paralysis,
    Unconscious,
    Bleeding,
    Pain,
    Necrosis,
    Death,
    Entangle,
}

/// Special attack of one forgotten beast.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct BeastPower {
    pub delivery: Delivery,
    pub effect: Effect,
    pub next: u64,
    /// "poisonous bite", "deadly dust", ... for the announcements.
    #[serde(with = "crate::saves11::interned")] // [saves11]
    pub noun: crate::saves11::SStr,
}

impl Effect {
    fn label(self) -> &'static str {
        match self {
            Effect::Paralysis => "paralysis",
            Effect::Unconscious => "unconsciousness",
            Effect::Bleeding => "bleeding",
            Effect::Pain => "pain",
            Effect::Necrosis => "necrosis",
            Effect::Death => "respiratory failure",
            Effect::Entangle => "webs",
        }
    }
}

/// Roll a forgotten beast's special attack (None = only claws and teeth).
pub(crate) fn roll_power(r: &mut Rng, quadruped: bool) -> Option<BeastPower> {
    const VENOM: [Effect; 6] = [Effect::Paralysis, Effect::Unconscious, Effect::Bleeding, Effect::Pain, Effect::Necrosis, Effect::Death];
    const CLOUD: [Effect; 5] = [Effect::Paralysis, Effect::Unconscious, Effect::Bleeding, Effect::Pain, Effect::Necrosis];
    let roll = r.below(100);
    let (delivery, effect, noun) = match roll {
        0..=29 => return None,
        30..=64 => {
            let e = VENOM[r.below(VENOM.len() as u32) as usize];
            (Delivery::Venom, e, if quadruped && r.chance(1, 2) { "poisonous bite" } else { "poisonous sting" })
        }
        65..=89 => {
            let e = CLOUD[r.below(CLOUD.len() as u32) as usize];
            (Delivery::Breath, e, ["deadly dust", "noxious gas", "toxic vapor"][r.below(3) as usize])
        }
        _ => (Delivery::Web, Effect::Entangle, "webs"),
    };
    Some(BeastPower { delivery, effect, next: 0, noun })
}

impl Sim {
    fn power_of(&self, s: usize) -> Option<usize> {
        self.caverns.powers.iter().position(|p| p.0 == s as u32)
    }

    /// Text for the arrival announcement ("Beware its deadly dust!").
    pub(crate) fn beware_text(p: &BeastPower) -> String {
        format!("Beware its {}!", p.noun)
    }

    /// [combat hook] After a melee attack of `a` on `d`: a venomous beast's wound poisons.
    pub(crate) fn syndrome_after_attack(&mut self, a: usize, d: usize, wounds_before: u64, now: u64) {
        let Some(k) = self.power_of(a) else { return };
        let p = self.caverns.powers[k].1;
        if p.delivery != Delivery::Venom || self.combat.stats.wounds <= wounds_before {
            return;
        }
        self.apply_syndrome(d, a, p.effect, now);
    }

    /// [combat hook] A beast with a breath or web attack uses it on its target from a distance.
    /// True if it did (the beast's turn is over).
    pub(crate) fn syndrome_breath(&mut self, s: usize, now: u64) -> bool {
        let Some(k) = self.power_of(s) else { return false };
        let p = self.caverns.powers[k].1;
        if p.delivery == Delivery::Venom || now < p.next {
            return false;
        }
        let t = self.combat.f[s].target;
        if !self.alive(t) || !self.can_fight(s, now) {
            return false;
        }
        let t = t as usize;
        let (a, b) = (self.upos(s), self.upos(t));
        let cheb = (a.x as i32 - b.x as i32).abs().max((a.y as i32 - b.y as i32).abs());
        let range = if p.delivery == Delivery::Web { 10 } else { 8 };
        if cheb < 2 || cheb > range || (a.z as i32 - b.z as i32).abs() > 1 || !self.line_of_fire(a, b, 20) {
            return false;
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0xB4EA);
        let wait = if p.delivery == Delivery::Web { 300 + r.below(201) } else { 600 + r.below(401) } as u64;
        self.caverns.powers[k].1.next = now + wait;
        let name = self.uname(s);
        let tname = self.uname(t);
        let verb = match p.delivery {
            Delivery::Web => format!("{} shoots a web at {}!", crate::combat::cap_pub(&name), tname),
            _ => format!("{} breathes {} at {}!", crate::combat::cap_pub(&name), p.noun, tname),
        };
        self.combat.log_line(verb);
        // The cloud: the target and everybody next to it (the web only hits the target).
        let mut hit = vec![t];
        if p.delivery == Delivery::Breath {
            for &u in &self.combat.fort {
                let u = u as usize;
                if u == t || self.ai[u].kind == Kind::Dead {
                    continue;
                }
                let q = self.upos(u);
                if q.z == b.z && (q.x as i32 - b.x as i32).abs() <= 1 && (q.y as i32 - b.y as i32).abs() <= 1 {
                    hit.push(u);
                }
            }
        }
        for u in hit {
            self.apply_syndrome(u, s, p.effect, now);
        }
        self.ai[s].wake = now + 20;
        true
    }

    /// Apply one syndrome effect to `d` (from beast `src`).
    fn apply_syndrome(&mut self, d: usize, src: usize, e: Effect, now: u64) {
        if self.ai[d].kind == Kind::Dead || d == src || self.power_of(d).is_some() {
            return;
        }
        self.ensure_body(d);
        let b = self.combat.f[d].body;
        if b == u16::MAX {
            return;
        }
        let mut r = Rng::for_unit(self.seed, d, now, 0x5A1D);
        self.combat.stats.syndromes += 1;
        let name = crate::combat::cap_pub(&self.uname(d));
        let line = match e {
            Effect::Paralysis | Effect::Unconscious | Effect::Entangle => {
                let dur = match e {
                    Effect::Paralysis => 1_200 + r.below(2_401),
                    Effect::Unconscious => 600 + r.below(1_201),
                    _ => 100 + r.below(201),
                } as u64;
                let f = &mut self.combat.f[d];
                f.syn_until = f.syn_until.max(now + dur);
                f.uncon_until = f.uncon_until.max(now + dur);
                f.target = if f.role == Role::Hostile { f.target } else { NONE };
                match e {
                    Effect::Paralysis => format!("{name} is completely paralyzed!"),
                    Effect::Unconscious => format!("{name} loses consciousness!"),
                    _ => format!("{name} is caught in a web!"),
                }
            }
            Effect::Death => {
                let due = now + 600 + r.below(1_201) as u64;
                if !self.caverns.doomed.iter().any(|x| x.0 == d as u32) {
                    self.caverns.doomed.push((d as u32, due));
                }
                format!("{name} is having trouble breathing!")
            }
            Effect::Bleeding | Effect::Pain | Effect::Necrosis => {
                let body = &self.combat.bodies[b as usize];
                // A random outer part (necrosis: a limb if there is one).
                let parts: Vec<u16> = body
                    .targets
                    .iter()
                    .map(|t| t.0)
                    .filter(|&p| e != Effect::Necrosis || body.parts[p as usize].flags & crate::combat::bp::LIMB != 0)
                    .filter(|&p| self.combat.f[d].part_state.get(p as usize).map(|s| s & 1 == 0).unwrap_or(true))
                    .collect();
                if parts.is_empty() {
                    return;
                }
                let part = parts[r.below(parts.len() as u32) as usize];
                let layer = body.parts[part as usize].l0;
                let (flags, sev, bleed, pain) = match e {
                    Effect::Bleeding => (wf::BLEEDING | wf::CUT, 1_500, 4, 20),
                    Effect::Pain => (wf::BRUISED, 3_000, 0, 400),
                    _ => (wf::CUT | wf::BLEEDING, 9_000, 2, 100),
                };
                let pname = body.parts[part as usize].name.clone();
                let f = &mut self.combat.f[d];
                f.wounds.push(WoundS { part, layer, flags, sev, bleed, pain, age: 0 });
                f.bleed += bleed;
                self.list_wounded(d);
                if bleed > 0 {
                    self.list_bleeder(d);
                }
                self.sync_wounds(d);
                self.check_pain(d, now);
                match e {
                    Effect::Bleeding => format!("{name}'s {pname} starts to bleed!"),
                    Effect::Pain => format!("{name} is wracked by pain!"),
                    _ => format!("The flesh of {name}'s {pname} rots away!"),
                }
            }
        };
        self.combat.log_line(line);
        if self.ai[d].kind != Kind::Dead && self.combat.f[d].uncon_until > now {
            self.ai[d].wake = self.ai[d].wake.min(now + 1);
        }
        let _ = e.label();
    }

    /// [caverns hook] Every 100 ticks: respiratory failure kills.
    pub(crate) fn syndrome_tick(&mut self, now: u64) {
        let mut k = 0;
        while k < self.caverns.doomed.len() {
            let (s, due) = self.caverns.doomed[k];
            if due > now {
                k += 1;
                continue;
            }
            self.caverns.doomed.swap_remove(k);
            let s = s as usize;
            if self.ai[s].kind != Kind::Dead {
                self.combat_kill(s, now, "has suffocated");
            }
        }
    }

    /// Effect label of a beast's special attack, for reports (e.g. "deadly dust (paralysis)").
    pub(crate) fn beast_power_label(&self, s: usize) -> Option<String> {
        self.power_of(s).map(|k| {
            let p = self.caverns.powers[k].1;
            format!("{} ({})", p.noun, p.effect.label())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Pos, World};

    /// Surface arena: dwarf 1 at `dwarf`, a forgotten beast with `power` at `beast`.
    fn arena(dwarf: (i16, i16), beast: (i16, i16), power: Option<BeastPower>) -> (Sim, usize, usize) {
        let mut w = World::synthetic(64, 64, 20);
        let sz = (20i16 * 2 / 3).max(2);
        w.units.retain(|u| u.id == 1);
        w.units[0].pos = Pos::new(dwarf.0, dwarf.1, sz);
        let mut sim = Sim::new(w, 11);
        sim.set_invasions(false);
        let id = sim.spawn_forgotten_beast().expect("beast");
        let b = sim.unit_slot[&id];
        let q = sim.world.idx(Pos::new(beast.0, beast.1, sz)).unwrap() as u32;
        let now = sim.ticks;
        sim.move_to(b, q, now);
        sim.caverns.powers.clear();
        if let Some(p) = power {
            sim.caverns.powers.push((b as u32, p));
        }
        let d = sim.unit_slot[&1];
        (sim, d, b)
    }

    #[test]
    fn breath_paralyzes_from_a_distance() {
        let p = BeastPower { delivery: Delivery::Breath, effect: Effect::Paralysis, next: 0, noun: "deadly dust" };
        let (mut sim, d, _b) = arena((20, 30), (26, 30), Some(p));
        let mut hit = false;
        for _ in 0..40 {
            sim.run_ticks(5);
            if sim.combat.stats.syndromes > 0 {
                hit = true;
                break;
            }
        }
        assert!(hit, "{:?}", sim.combat_log());
        assert!(sim.combat.f[d].syn_until > sim.ticks + 1_000, "paralyzed for a long time");
        assert!(!sim.can_fight(d, sim.ticks));
        assert!(sim.combat_log().iter().any(|l| l.contains("breathes deadly dust")), "{:?}", sim.combat_log());
    }

    #[test]
    fn venomous_bite_takes_hold_on_a_wound() {
        let p = BeastPower { delivery: Delivery::Venom, effect: Effect::Bleeding, next: 0, noun: "poisonous bite" };
        let (mut sim, _d, _b) = arena((20, 30), (21, 30), Some(p));
        sim.run_ticks(600);
        assert!(sim.combat.stats.wounds > 0);
        assert!(sim.combat.stats.syndromes > 0, "{:?}", sim.combat_log());
        // Without a special attack nothing like that happens.
        let (mut sim, _d, _b) = arena((20, 30), (21, 30), None);
        sim.run_ticks(600);
        assert_eq!(sim.combat.stats.syndromes, 0);
    }

    #[test]
    fn respiratory_failure_kills_later() {
        let (mut sim, d, b) = arena((20, 30), (60, 60), None);
        // Keep the beast out of it.
        sim.ai[b].kind = Kind::Dead;
        let now = sim.ticks;
        sim.apply_syndrome(d, b, Effect::Death, now);
        assert_eq!(sim.caverns.doomed.len(), 1);
        assert_ne!(sim.ai[d].kind, Kind::Dead);
        let mut t = 0;
        while t < 2_000 && sim.ai[d].kind != Kind::Dead {
            sim.run_ticks(100);
            t += 100;
        }
        assert_eq!(sim.ai[d].kind, Kind::Dead, "died of respiratory failure");
    }

    #[test]
    fn beasts_get_special_attacks_by_the_odds() {
        let mut r = Rng::new(42);
        let mut n = [0u32; 4];
        for _ in 0..1000 {
            match roll_power(&mut r, true).map(|p| p.delivery) {
                None => n[0] += 1,
                Some(Delivery::Venom) => n[1] += 1,
                Some(Delivery::Breath) => n[2] += 1,
                Some(Delivery::Web) => n[3] += 1,
            }
        }
        assert!(n.iter().all(|&k| k > 50), "{n:?}");
    }
}
