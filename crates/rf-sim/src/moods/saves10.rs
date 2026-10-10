//! [saves10] Save/load of strange moods: the mood counter, who already had a mood, the records
//! and the active mood with its progress (claimed workshop, collected materials, current step,
//! waiting time towards insanity, work end).

use super::*;
use crate::persist::{get, put};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "moods";

const ALL_ARTS: &[&[ArtKind]] = &[CRAFTS, FURNITURE, WOOD_FURNITURE, WEAPONS, ARMOR, CLOTHES, LEATHER_GOODS, BOWS, MECHS, GEMS, GLASS];
const ALL_MATS: [Mat; 12] =
    [Mat::Boulder, Mat::Log, Mat::Bar, Mat::CutGem, Mat::Block, Mat::RoughGem, Mat::Bone, Mat::Shell, Mat::Leather, Mat::Cloth, Mat::RawGlass, Mat::Body];
const ALL_KINDS: [MoodKind; 5] = [MoodKind::Fey, MoodKind::Secretive, MoodKind::Possessed, MoodKind::Macabre, MoodKind::Fell];

fn mat_code(m: Mat) -> u8 {
    ALL_MATS.iter().position(|&x| x == m).unwrap_or(0) as u8
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct RecordS {
    unit_id: i32,
    name: String,
    kind: String,
    skill: String,
    workshop: String,
    demands: Vec<String>,
    started: u64,
    ended: Option<u64>,
    outcome: String,
    artifact_id: i32,
    victim: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ActiveS {
    slot: usize,
    uid: i32,
    kind: String,
    skill: String,
    shop_bid: i32,
    site: u32,
    demands: Vec<u8>,
    all_demands: Vec<u8>,
    collected: Vec<i32>,
    /// `claim`, `hunt`, `fetch`, `carry`, `wait`, `work` with its argument.
    step: String,
    step_arg: i64,
    wait_since: Option<u64>,
    started: u64,
    needs: (i32, i32, i32),
    /// Artifact item kind, subtype and name.
    art: (String, String, String),
    record: usize,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct MoodsS {
    counter: i32,
    had: Vec<i32>,
    artifacts_made: u32,
    insane: u32,
    records: Vec<RecordS>,
    active: Option<ActiveS>,
}

impl Sim {
    pub(crate) fn moods_save(&self, m: &mut BTreeMap<String, String>) {
        let md = &self.moods;
        let active = md.active.as_ref().map(|a| {
            let (step, step_arg) = match a.step {
                Step::Claim => ("claim", 0),
                Step::Hunt(v) => ("hunt", v as i64),
                Step::Fetch(id) => ("fetch", id as i64),
                Step::Carry(id) => ("carry", id as i64),
                Step::Wait => ("wait", 0),
                Step::Work(t) => ("work", t as i64),
            };
            ActiveS {
                slot: a.slot,
                uid: a.uid,
                kind: a.kind.name().into(),
                skill: a.skill.into(),
                shop_bid: a.shop_bid,
                site: a.site,
                demands: a.demands.iter().map(|&d| mat_code(d)).collect(),
                all_demands: a.all_demands.iter().map(|&d| mat_code(d)).collect(),
                collected: a.collected.clone(),
                step: step.into(),
                step_arg,
                wait_since: a.wait_since,
                started: a.started,
                needs: a.needs,
                art: (a.art.0.into(), a.art.1.into(), a.art.2.into()),
                record: a.record,
            }
        });
        let s = MoodsS {
            counter: md.counter,
            had: md.had.clone(),
            artifacts_made: md.artifacts_made,
            insane: md.insane,
            records: md
                .records
                .iter()
                .map(|r| RecordS {
                    unit_id: r.unit_id,
                    name: r.name.clone(),
                    kind: r.kind.clone(),
                    skill: r.skill.clone(),
                    workshop: r.workshop.clone(),
                    demands: r.demands.clone(),
                    started: r.started,
                    ended: r.ended,
                    outcome: r.outcome.clone(),
                    artifact_id: r.artifact_id,
                    victim: r.victim.clone(),
                })
                .collect(),
            active,
        };
        put(m, KEY, &s);
    }

    pub(crate) fn moods_restore(&mut self, m: &BTreeMap<String, String>) {
        let Some(s) = get::<MoodsS>(m, KEY) else { return };
        self.moods.inited = true;
        self.moods.counter = s.counter;
        self.moods.had = s.had;
        self.moods.artifacts_made = s.artifacts_made;
        self.moods.insane = s.insane;
        self.moods.records = s
            .records
            .into_iter()
            .map(|r| MoodRecord {
                unit_id: r.unit_id,
                name: r.name,
                kind: r.kind,
                skill: r.skill,
                workshop: r.workshop,
                demands: r.demands,
                started: r.started,
                ended: r.ended,
                outcome: r.outcome,
                artifact_id: r.artifact_id,
                victim: r.victim,
            })
            .collect();
        self.moods.active = None;
        let Some(a) = s.active else { return };
        if a.slot >= self.ai.len() || self.ai[a.slot].id != a.uid || self.ai[a.slot].kind == Kind::Dead || self.moods.records.is_empty() {
            return;
        }
        let Some(kind) = ALL_KINDS.iter().copied().find(|k| k.name() == a.kind) else { return };
        let skill: &'static str = MOODABLE.iter().copied().find(|k| *k == a.skill).unwrap_or("STONECRAFT");
        let (shop_kind, shop_sub, shop_label, _, arts) = skill_spec(skill);
        let art = ALL_ARTS
            .iter()
            .flat_map(|l| l.iter())
            .copied()
            .find(|x| x.0 == a.art.0 && x.1 == a.art.1 && x.2 == a.art.2)
            .unwrap_or(arts[0]);
        let mat = |c: &u8| ALL_MATS.get(*c as usize).copied().unwrap_or(Mat::Boulder);
        let s_ = a.slot;
        let step = match a.step.as_str() {
            "hunt" => Step::Hunt(a.step_arg as u32),
            "fetch" => Step::Fetch(a.step_arg as i32),
            "carry" => Step::Carry(a.step_arg as i32),
            "wait" => Step::Wait,
            "work" => Step::Work(a.step_arg as u64),
            _ => Step::Claim,
        };
        // Reservations and the carried item are sim state: put them back.
        for &id in &a.collected {
            if let Some(&i) = self.item_idx.get(&id) {
                self.imeta[i as usize].reserved = self.world.items[i as usize].stack.max(1);
                self.haul_remove(id);
            }
        }
        let step = match step {
            Step::Fetch(id) => match self.item_idx.get(&id) {
                Some(&i) => {
                    self.imeta[i as usize].reserved += 1;
                    step
                }
                None => Step::Wait,
            },
            Step::Carry(id) => {
                if self.world.units[self.ai[s_].uidx].inventory.contains(&id) {
                    self.ai[s_].carrying = id;
                    step
                } else {
                    Step::Wait
                }
            }
            st => st,
        };
        self.moods.active = Some(Active {
            slot: a.slot,
            uid: a.uid,
            kind,
            skill,
            shop_kind,
            shop_sub,
            shop_label,
            shop_bid: a.shop_bid,
            site: a.site,
            demands: a.demands.iter().map(mat).collect(),
            all_demands: a.all_demands.iter().map(mat).collect(),
            collected: a.collected,
            step,
            wait_since: a.wait_since,
            started: a.started,
            needs: a.needs,
            art,
            record: a.record.min(self.moods.records.len().saturating_sub(1)),
        });
        self.ai[s_].task = Task::Mood;
        self.ai[s_].wake = self.ticks + 1;
    }
}
