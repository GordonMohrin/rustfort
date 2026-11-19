//! [health15] M1 hospitals, M2 hospital supply chests and stocking, M3 what happens to the
//! supplies (thread sewn in, soap used up, items handed to the patient).

use super::{Bench, Hosp, Sup, NSUP, SUP_DEFAULT, SUP_UNITS};
use crate::path::Goals;
use crate::rng::Rng;
use crate::{labor, Kind, Sim, Task, NONE};
use rf_core::Pos;

/// Supply items per container building (estimate).
const CHEST_CAP: i32 = 20;
/// Open stocking hauls per hospital at most (estimate).
const MAX_STOCK_JOBS: usize = 4;
/// Soap units used per cleaning (estimate: one 150-unit bar per 10 cleanings).
const SOAP_PER_CLEAN: i32 = 15;

/// Supply kind of an item kind name (material checks are separate).
pub(crate) fn sup_of_kind(kind: &str) -> Option<Sup> {
    Some(match kind {
        "THREAD" => Sup::Thread,
        "CLOTH" => Sup::Cloth,
        "SPLINT" => Sup::Splint,
        "CRUTCH" => Sup::Crutch,
        "POWDER_MISC" => Sup::Powder,
        "BUCKET" => Sup::Bucket,
        "BAR" | "GLOB" => Sup::Soap,
        _ => return None,
    })
}

/// Desired items from an exported supply line (DF units -> items, rounded up).
pub(crate) fn want_from_units(sup: Sup, units: i32) -> i32 {
    let u = SUP_UNITS[sup as usize];
    (units.max(0) + u - 1) / u
}

fn sup_of_export_name(name: &str) -> Option<Sup> {
    let n = name.to_ascii_lowercase();
    Some(if n.contains("thread") {
        Sup::Thread
    } else if n.contains("cloth") {
        Sup::Cloth
    } else if n.contains("splint") {
        Sup::Splint
    } else if n.contains("crutch") {
        Sup::Crutch
    } else if n.contains("powder") || n.contains("plaster") {
        Sup::Powder
    } else if n.contains("bucket") {
        Sup::Bucket
    } else if n.contains("soap") {
        Sup::Soap
    } else {
        return None;
    })
}

fn inside(r: &[(i16, i16, i16, i16, i16)], p: Pos) -> bool {
    r.iter().any(|&(x1, y1, x2, y2, z)| p.z == z && p.x >= x1 && p.x <= x2 && p.y >= y1 && p.y <= y2)
}

impl Sim {
    /// M1: hospitals from hospital zones and v50 hospital locations (on meeting-area zones).
    /// Overwrites `combat.hospitals` (healing rate, bed choice) with the union of their rects.
    pub(crate) fn h15_rebuild_hospitals(&mut self) {
        let mut out: Vec<Hosp> = Vec::new();
        for l in &self.world.locations {
            if !l.kind.eq_ignore_ascii_case("Hospital") {
                continue;
            }
            let mut want = SUP_DEFAULT;
            for sl in &l.supplies {
                if let Some(k) = sup_of_export_name(&sl.item) {
                    want[k as usize] = want_from_units(k, sl.desired);
                }
            }
            out.push(Hosp { key: l.id, name: l.name.clone(), zones: l.zones.clone(), want, ..Default::default() });
        }
        for b in &self.world.buildings {
            if b.kind == "Civzone" && b.subtype.contains("Hospital") && !out.iter().any(|h| h.zones.contains(&b.id)) {
                out.push(Hosp { key: -b.id, name: if b.name.is_empty() { "Hospital".into() } else { b.name.clone() }, zones: vec![b.id], want: SUP_DEFAULT, ..Default::default() });
            }
        }
        out.sort_by_key(|h| h.key);
        let mut all = Vec::new();
        for h in out.iter_mut() {
            for &z in &h.zones {
                if let Some(b) = self.world.buildings.iter().find(|b| b.id == z && b.kind == "Civzone") {
                    h.rects.push((b.x1, b.y1, b.x2, b.y2, b.z));
                }
            }
            all.extend(h.rects.iter().copied());
            if let Some((_, w)) = self.h15.want_set.iter().find(|(k, _)| *k == h.key) {
                h.want = *w;
            }
            if let Some(old) = self.h15.hosp.iter().find(|o| o.key == h.key) {
                h.have = old.have;
            }
        }
        // Furniture inside the zones.
        let mut benches: Vec<Bench> = Vec::new();
        let mut chest_tiles = Vec::new();
        for b in &self.world.buildings {
            if !b.built || b.kind == "Civzone" {
                continue;
            }
            let p = b.center;
            let Some(h) = out.iter_mut().find(|h| inside(&h.rects, p)) else { continue };
            let Some(t) = self.world.idx(p) else { continue };
            match b.kind.as_str() {
                "Bed" => h.beds += 1,
                "Table" => h.tables += 1,
                "Box" | "Cabinet" => {
                    h.chests.push(t as u32);
                    chest_tiles.push(t as u32);
                }
                "TractionBench" => {
                    let old = self.h15.benches.iter().find(|x| x.building == b.id).copied();
                    benches.push(old.unwrap_or(Bench { building: b.id, tile: t as u32, occupant: NONE, placed: false, since: 0 }));
                }
                _ => {}
            }
        }
        chest_tiles.sort_unstable();
        chest_tiles.dedup();
        benches.sort_by_key(|b| b.building);
        // A bench that is gone while a patient lies in it: the bone rebreaks (Toady One).
        let gone: Vec<u32> = self.h15.benches.iter().filter(|o| o.placed && o.occupant != NONE && !benches.iter().any(|b| b.building == o.building)).map(|o| o.occupant).collect();
        self.h15.benches = benches;
        let now = self.ticks;
        for p in gone {
            self.h15_rebreak(p as usize, now);
        }
        all.sort_unstable();
        all.dedup();
        self.h15.rep.hospitals = out.len();
        self.h15.rep.benches = self.h15.benches.len();
        self.h15.hosp = out;
        self.h15.chest_tiles = chest_tiles;
        self.combat.hospitals = all; // [health15] location hospitals count (M1)
    }

    /// Hospital index of a position.
    pub(crate) fn h15_hosp_at(&self, p: Pos) -> Option<usize> {
        self.h15.hosp.iter().position(|h| inside(&h.rects, p))
    }

    /// The item is a supply that the fort can use for this treatment (plaster powder, empty bucket).
    pub(crate) fn h15_supply_ok(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        match it.kind.as_str() {
            "POWDER_MISC" => self.world.material(it.mat).map(|m| m.token.ends_with("PLASTER")).unwrap_or(false),
            "BUCKET" => self.contents.get(&it.id).map(|v| v.is_empty()).unwrap_or(true),
            _ => true,
        }
    }

    fn h15_is_soap(&self, idx: usize) -> bool {
        self.world.material(self.world.items[idx].mat).map(|m| m.name.contains("soap") || m.token.contains("SOAP")).unwrap_or(false)
    }

    /// Supply kind of an item (with material checks), if it is a hospital supply.
    pub(crate) fn h15_sup_of(&self, idx: usize) -> Option<Sup> {
        let k = sup_of_kind(&self.world.items[idx].kind)?;
        let ok = match k {
            Sup::Powder | Sup::Bucket => self.h15_supply_ok(idx),
            Sup::Soap => self.h15_is_soap(idx),
            _ => true,
        };
        ok.then_some(k)
    }

    /// Distance bonus for supplies inside a hospital (doctors use the hospital chests first).
    pub(crate) fn h15_supply_bonus(&self, p: Pos) -> i32 {
        if !self.h15.hosp.is_empty() && self.h15_hosp_at(p).is_some() {
            10_000
        } else {
            0
        }
    }

    /// The item lies in a hospital chest (M2 earmark): stored, not for other jobs.
    pub(crate) fn h15_stored(&self, idx: usize) -> bool {
        if self.h15.chest_tiles.is_empty() {
            return false;
        }
        let it = &self.world.items[idx];
        let Some(t) = self.world.idx(it.pos) else { return false };
        self.h15.chest_tiles.binary_search(&(t as u32)).is_ok() && it.holder < 0 && self.h15_sup_of(idx).is_some()
    }

    /// Daily count reset (from `health_refresh_daily`).
    pub(crate) fn h15_count_reset(&mut self) {
        self.h15.cnt = Default::default();
        for h in self.h15.hosp.iter_mut() {
            h.have = [0; NSUP];
        }
        // Items on their way to a chest count as stocked, or the next pass would send another.
        for e in 0..self.h15.stock.len() {
            let (id, key, _) = self.h15.stock[e];
            let Some(&idx) = self.item_idx.get(&id) else { continue };
            let Some(k) = sup_of_kind(&self.world.items[idx as usize].kind) else { continue };
            if let Some(h) = self.h15.hosp.iter_mut().find(|h| h.key == key) {
                h.have[k as usize] += 1;
            }
        }
    }

    /// Daily count of one free supply item (from `health_refresh_daily`'s item loop).
    pub(crate) fn h15_count_supply(&mut self, idx: usize) {
        let Some(k) = self.h15_sup_of(idx) else { return };
        match k {
            Sup::Crutch => self.h15.cnt.crutches += 1,
            Sup::Powder => self.h15.cnt.powder += 1,
            Sup::Bucket => self.h15.cnt.buckets += 1,
            _ => {}
        }
        if !self.h15.hosp.is_empty() {
            let p = self.world.items[idx].pos;
            if let Some(h) = self.h15_hosp_at(p) {
                self.h15.hosp[h].have[k as usize] += 1;
            }
        }
    }

    /// M2: every 400 ticks: queue stocking hauls for short hospitals with chests.
    pub(crate) fn h15_stock_pass(&mut self, now: u64) {
        // Finished or abandoned hauls leave the list. Hauls nobody took since the last pass are
        // planned again (a hauler may not reach them; they must not block the hospital).
        let mut k = 0;
        while k < self.h15.stock.len() {
            let (id, key, s) = self.h15.stock[k];
            let running = s != NONE && (s as usize) < self.ai.len() && matches!(self.ai[s as usize].task, Task::GoPickup(x, _) | Task::GoStore(x, _) if x == id);
            if running {
                k += 1;
                continue;
            }
            self.h15.stock.remove(k);
            if s == NONE {
                let sup = self.item_idx.get(&id).and_then(|&i| sup_of_kind(&self.world.items[i as usize].kind));
                if let (Some(sup), Some(h)) = (sup, self.h15.hosp.iter_mut().find(|h| h.key == key)) {
                    h.have[sup as usize] = (h.have[sup as usize] - 1).max(0);
                }
            }
        }
        if self.h15.chest_tiles.is_empty() {
            return;
        }
        for h in 0..self.h15.hosp.len() {
            if self.h15.hosp[h].chests.is_empty() {
                continue;
            }
            let key = self.h15.hosp[h].key;
            let open = self.h15.stock.iter().filter(|e| e.1 == key).count();
            if open >= MAX_STOCK_JOBS {
                continue;
            }
            let total: i32 = self.h15.hosp[h].have.iter().sum();
            if total >= CHEST_CAP * self.h15.hosp[h].chests.len() as i32 {
                continue;
            }
            let hp = &self.h15.hosp[h]; // [perf15]
            let mut need = [false; NSUP]; // [perf15] kinds below their wanted count
            for &k in super::SUPS.iter() { // [perf15]
                need[k as usize] = hp.have[k as usize] < hp.want[k as usize]; // [perf15]
            } // [perf15]
            if !need.iter().any(|&b| b) { // [perf15]
                continue; // [perf15]
            } // [perf15]
            self.h15_refresh_sup_ids(); // [perf15]
            let best = self.h15_find_stock_items(&need, h, now); // [perf15] one scan of the supply items
            for &k in super::SUPS.iter() {
                if !need[k as usize] { // [perf15]
                    continue;
                }
                if let Some(id) = best[k as usize] { // [perf15]
                    self.h15.hosp[h].have[k as usize] += 1;
                    self.h15.stock.push((id, key, NONE));
                    break; // one per hospital and pass
                }
            }
        }
    }

    /// [perf15] Bring `h15.sup_ids` up to date: a full scan once, then only the items created
    /// since (new items always get ids from `next_item_id`); ids of removed items are dropped.
    fn h15_refresh_sup_ids(&mut self) {
        while self.h15.soap_mat.len() < self.world.materials.len() {
            let m = &self.world.materials[self.h15.soap_mat.len()];
            let soap = m.name.contains("soap") || m.token.contains("SOAP"); // as `h15_is_soap`
            self.h15.soap_mat.push(soap);
        }
        if self.h15.sup_to == self.next_item_id {
            return;
        }
        if self.h15.sup_to == 0 {
            for it in &self.world.items {
                if let Some(k) = sup_of_kind(&it.kind) {
                    self.h15.sup_ids[k as usize].push(it.id);
                }
            }
        } else {
            for id in self.h15.sup_to..self.next_item_id {
                if let Some(&ix) = self.item_idx.get(&id) {
                    if let Some(k) = sup_of_kind(&self.world.items[ix as usize].kind) {
                        self.h15.sup_ids[k as usize].push(id);
                    }
                }
            }
        }
        self.h15.sup_to = self.next_item_id;
        let idx = &self.item_idx;
        for v in self.h15.sup_ids.iter_mut() {
            v.retain(|id| idx.contains_key(id));
        }
    }

    /// Free item of each needed supply kind outside every hospital (best material first for
    /// thread and cloth: wiki "doctors prefer the most expensive").
    /// [perf15] One scan of the items for all kinds (was one scan per kind): the pass takes at
    /// most one item per hospital and changes nothing between the kinds, so the result is the same.
    fn h15_find_stock_items(&self, need: &[bool; NSUP], h: usize, now: u64) -> [Option<i32>; NSUP] {
        let mut best: [Option<(i32, i32)>; NSUP] = [None; NSUP]; // [perf15]
        let chest = self.h15.hosp[h].chests[0] as usize;
        let cc = self.nav.comp_near(chest);
        if cc == 0 {
            return [None; NSUP]; // [perf15]
        }
        for &k in super::SUPS.iter() { // [perf15] only the cached items of the needed kinds
            if !need[k as usize] { // [perf15]
                continue; // [perf15]
            } // [perf15]
            for &id in &self.h15.sup_ids[k as usize] { // [perf15]
                let Some(&ix) = self.item_idx.get(&id) else { continue }; // [perf15]
                let idx = ix as usize; // [perf15]
                if self.world.items[idx].id != id { // [perf15]
                    continue; // [perf15]
                } // [perf15]
                let it = &self.world.items[idx];
                if !self.item_free(idx, now) { // [perf15]
                    continue; // [perf15]
                } // [perf15]
                let sup_ok = if k == Sup::Soap { self.h15.soap_mat.get(it.mat as usize).copied().unwrap_or(false) } else { self.h15_sup_of(idx) == Some(k) }; // [perf15] cached soap materials
                if !sup_ok { // [perf15]
                    continue;
                }
                if self.h15_hosp_at(it.pos).is_some() || self.h15.stock.iter().any(|e| e.0 == it.id) || self.world.idx(it.pos).map(|t| self.nav.comp_near(t) != cc).unwrap_or(true) {
                    continue; // inside a hospital already, planned, or out of reach of the chest
                }
                let v = if matches!(k, Sup::Thread | Sup::Cloth) { self.world.material(it.mat).map(|m| m.value).unwrap_or(0) } else { 0 };
                let key = (-v, it.id);
                let b = &mut best[k as usize]; // [perf15]
                if b.map(|b| key < b).unwrap_or(true) {
                    *b = Some(key); // [perf15]
                }
            } // [perf15]
        }
        best.map(|b| b.map(|b| b.1)) // [perf15]
    }

    /// M2 hook in `decide`: an idle hauler takes an open stocking haul.
    pub(crate) fn h15_decide(&mut self, s: usize, now: u64) -> bool {
        if self.h15.off || self.h15.stock.is_empty() || self.ai[s].labors & labor::HAUL_ITEM == 0 || self.ai[s].kind != Kind::Worker {
            return false;
        }
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        // The first open haul this hauler can reach (item and chest in its component).
        let mut pick = None;
        for (e, &(id, key, who)) in self.h15.stock.iter().enumerate() {
            if who != NONE {
                continue;
            }
            let reach = self.item_idx.get(&id).and_then(|&i| self.world.idx(self.world.items[i as usize].pos)).map(|t| self.nav.comp_near(t) == c).unwrap_or(true);
            let chest_ok = self.h15.hosp.iter().find(|h| h.key == key).and_then(|h| h.chests.get(id as usize % h.chests.len().max(1))).map(|&t| self.nav.comp_near(t as usize) == c).unwrap_or(true);
            if reach && chest_ok {
                pick = Some(e);
                break;
            }
        }
        let Some(e) = pick else { return false };
        let (id, key, _) = self.h15.stock[e];
        let Some(&idx) = self.item_idx.get(&id) else {
            self.h15.stock.remove(e);
            return false;
        };
        let idx = idx as usize;
        let Some(h) = self.h15.hosp.iter().position(|h| h.key == key) else {
            self.h15.stock.remove(e);
            return false;
        };
        if !self.item_free(idx, now) || self.h15.hosp[h].chests.is_empty() {
            self.h15.stock.remove(e);
            return false;
        }
        let Some(t) = self.world.idx(self.world.items[idx].pos) else { return false };
        let chest = self.h15.hosp[h].chests[(id as usize) % self.h15.hosp[h].chests.len()];
        if c == 0 || self.nav.comp_near(t) != c || self.nav.comp_near(chest as usize) != c {
            return false;
        }
        self.haul_remove(id);
        self.set_hauling(id, true);
        self.h15.stock[e].2 = s as u32;
        self.ai[s].task = Task::GoPickup(id, chest);
        let g = self.item_goals(id);
        self.request_path(s, g, now);
        true
    }

    /// M3: on a successful cleaning use soap units; a bar lasts 150 units. False if no soap left.
    pub(crate) fn h15_use_soap(&mut self) -> bool {
        if self.h15.soap_units >= SOAP_PER_CLEAN {
            self.h15.soap_units -= SOAP_PER_CLEAN;
            return true;
        }
        let now = self.ticks;
        let mut best: Option<(i32, i32)> = None;
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            if !matches!(it.kind.as_str(), "BAR" | "GLOB") || !self.item_free(idx, now) || !self.h15_is_soap(idx) {
                continue;
            }
            let key = (-self.h15_supply_bonus(it.pos), it.id);
            if best.map(|b| key < b).unwrap_or(true) {
                best = Some(key);
            }
        }
        let Some((_, id)) = best else {
            self.health.soap = false;
            return false;
        };
        self.consume(id);
        self.h15.rep.soap_bars_used += 1;
        self.h15.soap_units += 150 - SOAP_PER_CLEAN;
        true
    }

    /// M3: the supply item moves from the doctor to the patient (sewn in or strapped on).
    pub(crate) fn h15_hand_over(&mut self, d: usize, p: usize, item: i32) {
        let du = self.ai[d].uidx;
        self.world.units[du].inventory.retain(|&x| x != item);
        if self.ai[d].carrying == item {
            self.ai[d].carrying = -1;
        }
        let Some(&i) = self.item_idx.get(&item) else { return };
        let i = i as usize;
        let (pid, pu, pos) = (self.ai[p].id, self.ai[p].uidx, self.upos(p));
        {
            let it = &mut self.world.items[i];
            it.holder = pid;
            it.container = -1;
            it.pos = pos;
            it.flags.in_inventory = true;
            it.flags.on_ground = false;
            it.flags.in_container = false;
            it.flags.in_job = false;
        }
        self.imeta[i].reserved = 0;
        self.imeta[i].hauling = false;
        self.haul_remove(item);
        if !self.world.units[pu].inventory.contains(&item) {
            self.world.units[pu].inventory.push(item);
        }
    }

    /// A held item is put down where the unit stands.
    pub(crate) fn h15_put_down(&mut self, s: usize, item: i32) {
        let uidx = self.ai[s].uidx;
        if !self.world.units[uidx].inventory.contains(&item) {
            return;
        }
        let here = self.ai[s].pos;
        self.drop_item(s, item, here);
    }

    /// Water reachable for a cast (bucket filled at a source).
    pub(crate) fn h15_water_near(&self, s: usize) -> bool {
        !self.water.is_empty() && self.nearest_water(s).is_some()
    }

    /// Doctor walks to a patient: path goals.
    #[allow(dead_code)]
    pub(crate) fn h15_goals_one(&self, t: u32) -> Goals {
        Goals::one(t)
    }

    /// Player: desired items of a supply kind in a hospital (key from `hospitals()`).
    pub(crate) fn h15_set_want(&mut self, key: i32, k: Sup, items: i32) {
        let items = items.clamp(0, 100);
        match self.h15.want_set.iter_mut().find(|(x, _)| *x == key) {
            Some((_, w)) => w[k as usize] = items,
            None => {
                let mut w = self.h15.hosp.iter().find(|h| h.key == key).map(|h| h.want).unwrap_or(SUP_DEFAULT);
                w[k as usize] = items;
                self.h15.want_set.push((key, w));
                self.h15.want_set.sort_by_key(|e| e.0);
            }
        }
        if let Some(h) = self.h15.hosp.iter_mut().find(|h| h.key == key) {
            h.want[k as usize] = items;
        }
    }

    /// Deterministic roll for a patient.
    pub(crate) fn h15_rng(&self, s: usize, now: u64, salt: u64) -> Rng {
        Rng::for_unit(self.seed, s, now, salt)
    }
}
