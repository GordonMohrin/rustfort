//! Military (round 3): squads on duty, orders, uniforms, training, civilians fleeing from
//! hostiles, burrows and the civilian alert.
//!
//! # DF rules implemented (wiki: Military, Squad, Schedule, Burrow)
//! - A squad is **on duty** when it has an order (`Train`, `Station x,y,z`, `Kill <id>`,
//!   `Patrol`) or a training schedule (`Constant training`: everybody; `Staggered training`:
//!   half the positions each month, alternating). `Off duty` / `Inactive` / empty = civilians.
//! - On-duty soldiers fetch their **uniform** (squad `uniform` entries `KIND:SUBTYPE`) from the
//!   fort's free items, then follow the order: Kill hunts the unit down, Station walks to the
//!   spot and holds it, Train trains in place (skill experience every 100 ticks). They engage
//!   every hostile that comes within sight (20 tiles) on their own. They still drink, eat and
//!   sleep (off duty for that moment).
//! - Soldiers also spot **lurking** (dormant) hostiles in sight (round 6). **Wounded** soldiers
//!   (broken bones, heavy damage, wounds the doctors can treat) go to bed like other patients
//!   when no able enemy is within 3 tiles, and return to duty once healed; resting soldiers are
//!   not pulled out of bed by hostiles in sight.
//! - **Civilians** flee from hostiles within 12 tiles; when cornered they fight back. With a
//!   civilian alert they go to the alert burrow and stay there.
//! - [r7] **Civilian alert work** (wiki Burrow: burrowed citizens "will only accept jobs from
//!   within their assigned burrow(s)"): inside the alert burrow civilians keep working, but
//!   only on jobs whose work tile and needed items lie inside the burrow (designations,
//!   constructions, workshops, farm plots, hauling from and to burrow tiles). Membership is a
//!   bitset over all map tiles, so the filter costs one bit test per candidate. Idle civilians
//!   mill about inside the burrow.
//!
//! # Estimates
//! - Training experience: 3 XP per 100 ticks for the weapon skill (or WRESTLING when unarmed),
//!   1 each for DODGING, ARMOR, MELEE_COMBAT, SHIELD (with a shield).
//! - [r7] Under an alert, meals, drinks and beds outside the burrow are still used (DF prefers
//!   the burrow's own); doctor, burial, depot and lever jobs are not taken under an alert.
//!   Hauling picks the nearest item inside the burrow over all wanted stockpile categories.
//! - Patrol routes are not simulated (treated as Train).

use crate::combat::{Role, Sub};
use crate::nav::MAX_NEIGHBORS;
use crate::path::Goals;
use crate::rng::Rng;
use crate::{Kind, Sim, Task, NONE};
use rf_core::Pos;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Duty {
    #[default]
    None,
    Train,
    /// Station at a tile index.
    Station(u32),
    /// Kill a unit (unit id).
    Kill(i32),
}

#[derive(Default)]
pub(crate) struct Military {
    pub alert: Option<i32>,
    pub alert_tiles: HashSet<u32>,
    pub alert_list: Vec<u32>,
    pub last_month: i32,
    /// [r7] All tiles of the alert burrow (walkable or not), one bit per map tile.
    pub alert_bits: Vec<u64>,
    /// [r7] Slot whose job search is running under the alert restriction (set by
    /// `combat_decide` right before the work searches of that unit).
    pub cur: Option<u32>,
}

fn parse_pos(s: &str) -> Option<Pos> {
    let v: Vec<i16> = s.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).filter_map(|t| t.parse().ok()).collect();
    (v.len() >= 3).then(|| Pos::new(v[0], v[1], v[2]))
}

impl Sim {
    /// Re-read squads, orders and schedules into the soldiers' duty state.
    pub(crate) fn refresh_military(&mut self) {
        let month = self.world.calendar.month();
        self.combat.mil.last_month = month;
        for f in &mut self.combat.f {
            f.duty = Duty::None;
            f.squad = -1;
        }
        let squads = self.world.squads.clone();
        for sq in &squads {
            let order = sq.orders.iter().find(|o| !o.trim().is_empty()).map(|o| o.trim().to_string()).unwrap_or_default();
            let sched = sq.schedule.to_ascii_lowercase();
            let base = if order.starts_with("Kill") {
                order.split_whitespace().nth(1).and_then(|t| t.parse().ok()).map(Duty::Kill).unwrap_or(Duty::Train)
            } else if order.starts_with("Station") {
                parse_pos(order.trim_start_matches("Station")).and_then(|p| self.world.idx(p)).map(|t| Duty::Station(t as u32)).unwrap_or(Duty::Train)
            } else if order.starts_with("Train") || order.starts_with("Patrol") {
                Duty::Train
            } else if sched.contains("constant") || sched.contains("ready") || sched == "active/training" {
                Duty::Train
            } else if sched.contains("staggered") || sched.contains("train") {
                // Staggered: half of the positions each month (DF minimum 5 of 10).
                Duty::Train
            } else {
                Duty::None
            };
            let staggered = order.is_empty() && sched.contains("staggered");
            for (pos, &uid) in sq.members.iter().enumerate() {
                if uid < 0 {
                    continue;
                }
                let Some(&s) = self.unit_slot.get(&uid) else { continue };
                if s >= self.combat.f.len() || self.ai[s].kind != Kind::Worker {
                    continue;
                }
                let duty = if staggered && (pos as i32 + month) % 2 == 1 { Duty::None } else { base };
                let f = &mut self.combat.f[s];
                f.duty = duty;
                f.squad = sq.id;
            }
        }
        let now = self.ticks;
        for s in 0..self.combat.f.len() {
            let (role, duty) = (self.combat.f[s].role, self.combat.f[s].duty);
            if role == Role::Soldier && duty == Duty::None {
                self.combat.f[s].role = Role::None;
                self.combat.f[s].target = NONE;
                if self.ai[s].task == Task::Combat {
                    self.ai[s].task = Task::Idle;
                    self.ai[s].path.clear();
                    if !self.ai[s].waiting {
                        self.ai[s].wake = self.ai[s].wake.min(now + 1);
                    }
                }
            } else if duty != Duty::None && matches!(role, Role::None | Role::Civilian | Role::Soldier) {
                let f = &mut self.combat.f[s];
                f.role = Role::Soldier;
                f.threat = NONE;
                if let Duty::Kill(_) = duty {
                    f.target = NONE;
                }
                self.engage_now(s, now);
            }
        }
    }

    /// Decide hook: soldiers on duty, civilians under alert and the badly wounded are run by
    /// the combat module. Drinking, eating and sleeping come first.
    pub(crate) fn combat_decide(&mut self, s: usize, now: u64) -> bool {
        self.combat.mil.cur = None; // [r7]
        if s >= self.combat.f.len() || self.needs_break(s) {
            return false;
        }
        if self.combat.f[s].uncon_until > now {
            self.ai[s].task = Task::Combat;
            self.ai[s].wake = self.combat.f[s].uncon_until;
            return true;
        }
        let role = self.combat.f[s].role;
        if role == Role::None {
            let f = &self.combat.f[s];
            // Wounded soldiers rest like everybody else until healed or treated (wiki Health
            // care: injured dwarves "go (or are brought) to recuperate"), then resume duty.
            let new = if f.duty != Duty::None && self.needs_rest(s) && !self.fight_nearby(s) {
                Role::Patient
            } else if f.duty != Duty::None {
                Role::Soldier
            } else if self.combat.mil.alert.is_some() && !self.combat.mil.alert_list.is_empty() {
                Role::Civilian
            } else if self.needs_rest(s) {
                Role::Patient
            } else {
                return false;
            };
            self.combat.f[s].role = new;
        }
        // [r7] Calm civilians inside the alert burrow work on burrow jobs (filtered searches).
        if self.alert_may_work(s) {
            self.combat.mil.cur = Some(s as u32);
            self.ai[s].task = Task::Idle;
            return false;
        }
        self.ai[s].task = Task::Combat;
        self.combat_think(s, now);
        true
    }

    /// [r7] Civilian under the alert, no threat, standing inside the burrow.
    fn alert_may_work(&self, s: usize) -> bool {
        let f = &self.combat.f[s];
        f.role == Role::Civilian && f.threat == NONE && f.duty == Duty::None && self.alert_on() && self.in_alert(self.ai[s].pos as usize)
    }

    fn alert_on(&self) -> bool {
        self.combat.mil.alert.is_some() && !self.combat.mil.alert_list.is_empty()
    }

    /// [r7] Is tile `t` part of the alert burrow?
    #[inline]
    pub(crate) fn in_alert(&self, t: usize) -> bool {
        self.combat.mil.alert_bits.get(t / 64).map(|w| (w >> (t % 64)) & 1 != 0).unwrap_or(false)
    }

    /// [r7] Is unit `s` restricted to the alert burrow right now?
    #[inline]
    pub(crate) fn alert_restricted(&self, s: usize) -> bool {
        !self.combat.mil.alert_bits.is_empty()
            && self.combat.f.get(s).map(|f| f.role == Role::Civilian && f.duty == Duty::None).unwrap_or(false)
            && self.alert_on()
    }

    /// [r7] The cheap job filter: true if unit `s` may not take work at tile `t`.
    #[inline]
    pub(crate) fn alert_blocks(&self, s: usize, t: usize) -> bool {
        self.alert_restricted(s) && !self.in_alert(t)
    }

    /// [r7] Same filter for searches that do not know the unit (the item searches of the
    /// current unit's job search, see `Military::cur`).
    #[inline]
    pub(crate) fn alert_blocks_cur(&self, t: usize) -> bool {
        match self.combat.mil.cur {
            Some(s) => self.alert_blocks(s as usize, t),
            None => false,
        }
    }

    /// [r7] Hauling under the alert: the nearest waiting item inside the burrow of a wanted
    /// category (one scan over the haul list; only runs for restricted civilians).
    pub(crate) fn alert_haul_pick(&self, want: u32, up: (usize, usize, usize), c: u32, now: u64) -> Option<(crate::Cat, i32, usize)> {
        let mut best: Option<(i32, i32, usize, crate::Cat)> = None;
        for (k, &id) in self.haul.iter().enumerate() {
            let idx = self.haul_ix[k] as usize;
            let m = &self.imeta[idx];
            if want & m.cat.bit() == 0 || m.reserved > 0 || m.blocked_until > now || m.dead {
                continue;
            }
            let Some(t) = self.world.idx(self.world.items[idx].pos) else { continue };
            if !self.in_alert(t) {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(t));
            if best.map(|b| (d, id) >= (b.0, b.1)).unwrap_or(false) || self.nav.comp_near(t) != c {
                continue;
            }
            best = Some((d, id, t, m.cat));
        }
        best.map(|b| (b.3, b.1, b.2))
    }

    /// [r7] Idle civilians under the alert mill about inside the burrow instead of wandering.
    pub(crate) fn alert_mill(&mut self, s: usize, now: u64) -> bool {
        if !self.alert_restricted(s) || !self.in_alert(self.ai[s].pos as usize) {
            return false;
        }
        self.burrow_think(s, now);
        true
    }

    /// `Task::Combat` was cancelled (path failure, death): drop reservations and beds.
    pub(crate) fn combat_cancel(&mut self, s: usize) {
        self.ranged_cancel(s); // [ranged]
        self.release_rest(s);
        if let Some(f) = self.combat.f.get_mut(s) {
            if let Sub::Equip(id) = f.sub {
                f.sub = Sub::None;
                self.unreserve(id);
            }
            if let Some(f) = self.combat.f.get_mut(s) {
                f.repath_at = self.ticks + 60;
            }
        }
    }

    /// Walk towards `goal` (re-using the current path when it leads there).
    fn walk_to(&mut self, s: usize, goal: u32, now: u64) {
        let ai = &self.ai[s];
        if ai.path_i < ai.path.len() && *ai.path.last().unwrap() == goal {
            self.step(s, now);
        } else if self.combat.f[s].repath_at > now && ai.path.is_empty() {
            self.ai[s].wake = self.combat.f[s].repath_at;
        } else {
            self.request_path(s, Goals::one(goal), now);
        }
    }

    pub(crate) fn soldier_think(&mut self, s: usize, now: u64) {
        let duty = self.combat.f[s].duty;
        if duty == Duty::None {
            self.release(s, now);
            return;
        }
        // A soldier leaves even an ongoing fight once thirst/hunger reach DF's job-cancel
        // thresholds; otherwise an unkillable foe makes the whole squad die of thirst.
        let critical = {
            let ai = &self.ai[s];
            ai.thirst >= crate::needs::THIRST_CANCEL || ai.hunger >= crate::needs::HUNGER_CANCEL
        };
        if critical && self.needs_break(s) {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        }
        if self.pursue(s, now) {
            return;
        }
        if self.needs_break(s) {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        }
        // Wounded and no enemy at hand: go to bed (a Kill order still waits for recovery).
        if self.needs_rest(s) && !self.fight_nearby(s) {
            self.combat.f[s].role = Role::Patient;
            self.combat.f[s].target = NONE;
            self.combat.stats.soldiers_rested += 1;
            self.patient_think(s, now);
            return;
        }
        if let Duty::Kill(uid) = duty {
            match self.unit_slot.get(&uid).copied() {
                Some(t) if self.alive(t as u32) && !self.world.units[self.ai[t].uidx].flags.inactive => {
                    self.combat.f[s].target = t as u32;
                    if self.pursue(s, now) {
                        return;
                    }
                    // Unreachable: wait.
                    self.ai[s].wake = now + 100;
                    return;
                }
                _ => {
                    // Target dead: the order is done.
                    let sq = self.combat.f[s].squad;
                    if let Some(q) = self.world.squads.iter_mut().find(|q| q.id == sq) {
                        q.orders.retain(|o| !o.starts_with("Kill"));
                    }
                    self.refresh_military();
                    if self.combat.f[s].role != Role::Soldier {
                        if self.ai[s].task == Task::Combat {
                            self.release(s, now);
                        }
                        return;
                    }
                }
            }
        }
        // Uniform.
        if let Sub::Equip(id) = self.combat.f[s].sub {
            if self.item_reachable_here(s, id) {
                self.unreserve(id);
                self.equip(s, id);
                self.combat.f[s].sub = Sub::None;
            } else if self.item_idx.get(&id).map(|&i| self.world.items[i as usize].holder >= 0 || self.imeta[i as usize].dead).unwrap_or(true) {
                self.unreserve(id);
                self.combat.f[s].sub = Sub::None;
            } else {
                let ai = &self.ai[s];
                if ai.path_i < ai.path.len() {
                    self.step(s, now);
                } else {
                    let g = self.item_goals(id);
                    self.request_path(s, g, now);
                }
                return;
            }
        }
        if self.ranged_soldier(s, now) {
            return; // [ranged] quiver, ammo, archery practice
        }
        if now >= self.combat.f[s].equip_at {
            self.combat.f[s].equip_at = now + rf_core::TICKS_PER_DAY as u64;
            if let Some(id) = self.find_uniform_item(s) {
                self.reserve(id);
                self.combat.f[s].sub = Sub::Equip(id);
                let g = self.item_goals(id);
                self.request_path(s, g, now);
                return;
            }
        }
        match self.combat.f[s].duty {
            Duty::Station(t) => {
                let here = self.upos(s);
                if here.dist(self.world.pos_of(t as usize)) > 1 && self.nav.comp_near(t as usize) == self.nav.comp_near(self.ai[s].pos as usize) {
                    self.walk_to(s, t, now);
                } else {
                    self.ai[s].wake = now + 50;
                }
            }
            _ => {
                if now >= self.combat.f[s].train_at {
                    self.combat.f[s].train_at = now + 100;
                    self.train_xp(s);
                }
                self.ai[s].wake = now + 50;
            }
        }
    }

    fn train_xp(&mut self, s: usize) {
        let skill = match self.wielded(s) {
            Some((_, w)) => self.combat.weapons[w as usize].skill.clone(),
            None => "WRESTLING".to_string(),
        };
        self.add_xp(s, &skill, 3);
        self.add_xp(s, "DODGING", 1);
        self.add_xp(s, "ARMOR", 1);
        self.add_xp(s, "MELEE_COMBAT", 1);
        if self.world.units[self.ai[s].uidx].inventory.iter().any(|id| self.item_idx.get(id).map(|&i| self.world.items[i as usize].kind == "SHIELD").unwrap_or(false)) {
            self.add_xp(s, "SHIELD", 1);
        }
    }

    /// Nearest free item for a missing uniform piece of soldier `s`.
    fn find_uniform_item(&mut self, s: usize) -> Option<i32> {
        let sq = self.combat.f[s].squad;
        let uniform = self.world.squads.iter().find(|q| q.id == sq).map(|q| q.uniform.clone()).unwrap_or_default();
        if uniform.is_empty() {
            return None;
        }
        let uidx = self.ai[s].uidx;
        let inv: Vec<(String, String)> = self.world.units[uidx]
            .inventory
            .iter()
            .filter_map(|id| self.item_idx.get(id).map(|&i| &self.world.items[i as usize]))
            .map(|it| (it.kind.clone(), it.subtype.clone()))
            .collect();
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return None;
        }
        let up = self.nav.xyz(pos);
        for entry in &uniform {
            // `KIND`, `KIND:SUBTYPE`, `KIND@category` or `KIND:SUBTYPE@MATERIAL` (export format).
            let head = entry.split('@').next().unwrap_or("");
            let mut parts = head.splitn(2, ':');
            let kind = parts.next().unwrap_or("").to_string();
            let sub = parts.next().unwrap_or("").to_string();
            if kind.is_empty() {
                continue;
            }
            if inv.iter().any(|(k, st)| *k == kind && (sub.is_empty() || *st == sub)) {
                continue;
            }
            let mut best = (i32::MAX, -1);
            for (i, it) in self.world.items.iter().enumerate() {
                if it.kind != kind || (!sub.is_empty() && it.subtype != sub) {
                    continue;
                }
                let m = &self.imeta[i];
                let f = &it.flags;
                if m.dead || m.reserved > 0 || m.hauling || it.holder >= 0 || it.container >= 0 || it.building >= 0 || f.forbid || f.in_building || f.construction || f.in_job {
                    continue;
                }
                let Some(t) = self.world.idx(it.pos) else { continue };
                let d = crate::manhattan(up, self.nav.xyz(t));
                if d < best.0 && self.nav.comp_near(t) == c {
                    best = (d, it.id);
                }
            }
            if best.1 >= 0 {
                return Some(best.1);
            }
        }
        None
    }

    /// Put an item into a unit's inventory (worn or wielded).
    pub(crate) fn equip(&mut self, s: usize, id: i32) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let i = i as usize;
        let uid = self.ai[s].id;
        let loose = self.world.items[i].flags.on_ground && self.world.items[i].container < 0;
        let pos = self.world.items[i].pos;
        if loose {
            if let Some(t) = self.world.idx(pos) {
                self.pile_occ_add(t as u32, -1);
            }
        }
        self.haul_remove(id);
        let it = &mut self.world.items[i];
        it.holder = uid;
        it.flags.on_ground = false;
        it.flags.in_inventory = true;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(id);
    }

    pub(crate) fn civilian_think(&mut self, s: usize, now: u64) {
        let thr = self.combat.f[s].threat;
        if thr != NONE {
            let gone = !self.alive(thr) || self.combat.f[thr as usize].role == Role::Leave || {
                let (a, b) = (self.upos(s), self.upos(thr as usize));
                let cheb = (a.x as i32 - b.x as i32).abs().max((a.y as i32 - b.y as i32).abs());
                cheb > 25 || (a.z as i32 - b.z as i32).abs() > 3
            };
            if gone && now >= self.combat.f[s].calm_since + 100 {
                self.combat.f[s].threat = NONE;
            }
        }
        let alert = self.combat.mil.alert.is_some() && !self.combat.mil.alert_list.is_empty();
        if self.combat.f[s].threat == NONE {
            if alert && self.alert_may_work(s) {
                // [r7] Inside the burrow: back to (burrow-filtered) work.
                self.ai[s].task = Task::Idle;
                self.ai[s].path.clear();
                self.ai[s].path_i = 0;
                self.decide(s, now);
            } else if alert {
                self.burrow_think(s, now);
            } else {
                self.release(s, now);
            }
            return;
        }
        if alert && !self.combat.mil.alert_tiles.contains(&self.ai[s].pos) && self.burrow_goal(s).is_some() {
            self.burrow_think(s, now);
            return;
        }
        self.flee_step(s, now);
    }

    fn burrow_goal(&self, s: usize) -> Option<u32> {
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, NONE);
        for &t in &self.combat.mil.alert_list {
            let d = crate::manhattan(up, self.nav.xyz(t as usize));
            if d < best.0 && self.nav.walkable(t as usize) && self.nav.comp_root(t as usize) == c {
                best = (d, t);
            }
        }
        (best.1 != NONE).then_some(best.1)
    }

    fn burrow_think(&mut self, s: usize, now: u64) {
        let here = self.ai[s].pos;
        if self.combat.mil.alert_tiles.contains(&here) {
            // Inside: mill about within the burrow.
            let mut r = Rng::for_unit(self.seed, s, now, 0xB0);
            let mut nb = [0u32; MAX_NEIGHBORS];
            let n = self.nav.neighbors(here as usize, &mut nb);
            if n > 0 && r.chance(1, 3) {
                let q = nb[r.below(n as u32) as usize];
                if self.combat.mil.alert_tiles.contains(&q) {
                    self.move_to(s, q, now);
                    self.ai[s].wake += 20 + r.below(40) as u64;
                    return;
                }
            }
            self.ai[s].wake = now + 30 + r.below(30) as u64;
            return;
        }
        let ai = &self.ai[s];
        if ai.path_i < ai.path.len() && self.combat.mil.alert_tiles.contains(ai.path.last().unwrap()) {
            self.step(s, now);
            return;
        }
        match self.burrow_goal(s) {
            Some(g) => self.walk_to(s, g, now),
            None => {
                // Burrow unreachable: behave normally.
                self.ai[s].wake = now + 100;
                if self.combat.f[s].threat == NONE {
                    self.release(s, now);
                }
            }
        }
    }

    /// One step away from the threat; fight back when cornered.
    fn flee_step(&mut self, s: usize, now: u64) {
        let thr = self.combat.f[s].threat as usize;
        let tp = self.nav.xyz(self.ai[thr].pos as usize);
        let here = self.ai[s].pos as usize;
        let d0 = crate::manhattan(self.nav.xyz(here), tp);
        let mut nb = [0u32; MAX_NEIGHBORS];
        let n = self.nav.neighbors(here, &mut nb);
        let mut best = (d0, NONE);
        for &q in &nb[..n] {
            let d = crate::manhattan(self.nav.xyz(q as usize), tp);
            if (d, std::cmp::Reverse(q)) > (best.0, std::cmp::Reverse(best.1)) {
                best = (d, q);
            }
        }
        if best.1 != NONE && best.0 > d0 {
            self.move_to(s, best.1, now);
            return;
        }
        // Cornered.
        if self.upos(s).dist(self.upos(thr)) <= 1 && self.can_fight(s, now) {
            self.combat.f[s].target = thr as u32;
            if now >= self.combat.f[s].next_atk {
                let killed = self.attack(s, thr, now);
                self.combat.f[s].next_atk = now + 10;
                if killed {
                    self.combat.f[s].threat = NONE;
                }
            }
            self.combat.f[s].target = NONE;
            self.ai[s].wake = self.combat.f[s].next_atk.max(now + 1);
            return;
        }
        if best.1 != NONE && best.0 == d0 {
            self.move_to(s, best.1, now);
            return;
        }
        self.ai[s].wake = now + 10;
    }

    /// Civilian alert on/off (restrict civilians to a burrow).
    pub(crate) fn military_set_alert(&mut self, burrow_id: Option<i32>) {
        self.combat.mil.alert = burrow_id;
        self.combat.mil.alert_tiles.clear();
        self.combat.mil.alert_list.clear();
        self.combat.mil.alert_bits.clear(); // [r7]
        self.combat.mil.cur = None; // [r7]
        if let Some(b) = burrow_id.and_then(|id| self.world.burrows.iter().find(|b| b.id == id)) {
            let mut tiles: Vec<u32> = b.tiles.iter().filter_map(|&p| self.world.idx(p)).filter(|&i| self.nav.walkable(i)).map(|i| i as u32).collect();
            tiles.sort_unstable();
            tiles.dedup();
            self.combat.mil.alert_tiles = tiles.iter().copied().collect();
            self.combat.mil.alert_list = tiles;
            // [r7] Bitset of every burrow tile (jobs may target walls, e.g. digging).
            let mut bits = vec![0u64; self.world.tiles.len().div_ceil(64)];
            for &p in &b.tiles {
                if let Some(i) = self.world.idx(p) {
                    bits[i / 64] |= 1 << (i % 64);
                }
            }
            self.combat.mil.alert_bits = bits;
        }
        let on = self.combat.mil.alert.is_some() && !self.combat.mil.alert_list.is_empty();
        let now = self.ticks;
        for s in 0..self.combat.f.len() {
            if self.ai[s].kind != Kind::Worker || self.combat.f[s].duty != Duty::None {
                continue;
            }
            let f = &mut self.combat.f[s];
            if on && matches!(f.role, Role::None) {
                f.role = Role::Civilian;
                f.threat = NONE;
                self.engage_now(s, now);
            } else if !on && f.role == Role::Civilian && f.threat == NONE && self.ai[s].task == Task::Combat {
                f.role = Role::None;
                self.ai[s].task = Task::Idle;
                self.ai[s].path.clear();
                if !self.ai[s].waiting {
                    self.ai[s].wake = now + 1;
                }
            }
        }
    }

    /// Monthly refresh (staggered training rotates).
    pub(crate) fn military_monthly(&mut self) {
        let m = self.world.calendar.month();
        if m != self.combat.mil.last_month {
            self.refresh_military();
        }
    }
}
