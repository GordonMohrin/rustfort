//! [skills9] Experience per job, item quality from skill, military training, social and mental
//! skills (round 9, calibrated against Gordon's real fort, see docs/VALIDATION.md).
//!
//! # DF rules implemented (wiki v50: Experience, Skill, Item quality, Training, Sparring,
//! Demonstration, Situational awareness, Manager)
//! - **Experience per job** (w:Experience): most civilian skills give 30 XP per use **plus 30**
//!   when the product has quality levels (crafts, furniture, weapons, armor, clothing, containers,
//!   tools, cloth); decorations and generic items (bars, blocks, flour, thread, tanned hides)
//!   only the base 30. Mining, smoothing, carving fortifications and tracks, engraving and making
//!   stone blocks give only **10**. A work order the manager approves gives **100** Organization.
//! - **No level cap** (w:Skill): level n -> n+1 needs 500 + 100 n XP, without upper limit
//!   (Gordon's fort has Situational awareness up to 126 and Growing 150).
//! - **Item quality** depends on the maker's skill (w:Item quality): past a high skill a maker
//!   almost always makes at least exceptional items, and masterworks are capped at a 1/3 chance.
//! - **Military training** (w:Training): soldiers without a partner do individual drills that
//!   improve one skill at a time; two squadmates with at least Competent (3) Fighting spar,
//!   which trains every usable combat skill (weapon, Fighting, Wrestling, Discipline, Dodging,
//!   Armor, Shield, the unarmed strikes) and Situational awareness ("Observer"), which also grows
//!   by witnessing a fight nearby (w:Situational awareness). Demonstrations: a squadmate who is
//!   at least 2 levels better teaches a skill; the student gains the skill plus Student
//!   (KNOWLEDGE_ACQUISITION) and Concentration, the teacher gains Teacher (TEACHING).
//! - **Social skills** (w:Conversation): talking trains the conversation skills (Persuader,
//!   Comedian, Flatterer, Liar, Intimidator, Pacifier, Consoler, Judge of intent, Conversationalist):
//!   consoling a stressed partner, pacifying or intimidating in an argument.
//!
//! # Estimates (not on the wiki, fitted to Gordon's r4 -> r5 history)
//! - Quality roll: a floor of skill / 4 (the remainder as a chance), then up to 3 more levels,
//!   each with chance 0.55; a result of masterwork or better is a masterwork with chance 1/3,
//!   otherwise exceptional. At skill 2 that gives 45 / 25 / 14 / 17 % ordinary .. superior
//!   (Gordon's leather armor by skill 1-3 leatherworkers: 39 / 34 / 12 / 15 %), at 16 82 %
//!   exceptional and 18 % masterwork, at 20+ 2/3 exceptional and 1/3 masterwork.
//! - Training amounts per 100-tick round, in XP (fractions by chance): sparring SA 8, Fighting 3,
//!   weapon 2, Discipline 2, Wrestling 1, Armor 0.5, Dodging 0.4, Shield 0.4, grasp strike 0.4,
//!   stance strike 0.2, bite 0.2; witnessing a bout within 5 tiles SA 3; individual drill 5 XP
//!   to one skill (weapon 30 %, Fighting 40 %, Discipline 20 %, Armor / Dodging / Shield 10 %);
//!   demonstration 1 in 10 rounds: skill 3, Student 13, Concentration 13, teacher Teacher 13.
//!   Gordon's three constantly training squads gained 14.5k military XP per dwarf-year (6.1k
//!   SA, 2.6k Fighting, 1.8k axe, 1.7k Discipline).
//! - Conversation: the speaker gains `TALK_XP` (11) in one conversation skill per conversation; the
//!   skill is Consoler for a stressed partner, Pacifier / Intimidator in an argument, otherwise
//!   picked with the weights of Gordon's fort (Persuader 76, Comedian 41, Judge of intent 37,
//!   Intimidator 20, Pacifier 25, Consoler 20, Conversationalist 28, Flatterer 8, Liar 4).
//! - Reading a book: Student and Concentration 30 each.

use crate::rng::Rng;
use crate::Sim;

/// Base experience per use of a civilian skill (wiki).
pub(crate) const XP_USE: i32 = 30;
/// Extra experience when the product has quality levels (wiki).
pub(crate) const XP_QUALITY: i32 = 30;
/// Mining, smoothing, carving, engraving and stone blocks (wiki).
pub(crate) const XP_STONECUT: i32 = 10;
/// Organization for each work order the manager approves (wiki).
pub(crate) const XP_ORGANIZE: i32 = 100;
/// Conversation skill experience per conversation (estimate).
pub(crate) const TALK_XP: i32 = 11;

/// Does an item of this kind have quality levels? (Meals roll their own in `lifestyle.rs`.)
pub(crate) fn has_quality(kind: &str) -> bool {
    matches!(
        kind,
        "BED" | "CHAIR" | "TABLE" | "DOOR" | "CABINET" | "BOX" | "COFFIN" | "STATUE" | "ARMORSTAND" | "WEAPONRACK"
            | "SLAB" | "HATCH_COVER" | "FLOODGATE" | "GRATE" | "WINDOW" | "QUERN" | "MILLSTONE" | "TRACTION_BENCH"
            | "BOOKCASE" | "PEDESTAL" | "DISPLAY_CASE" | "ANVIL" | "BARREL" | "BIN" | "BAG" | "BUCKET" | "CAGE"
            | "ANIMALTRAP" | "FLASK" | "BACKPACK" | "QUIVER" | "AMULET" | "BRACELET" | "CROWN" | "EARRING"
            | "FIGURINE" | "RING" | "SCEPTER" | "GOBLET" | "TOY" | "TOTEM" | "INSTRUMENT" | "SHOES" | "GLOVES"
            | "PANTS" | "ARMOR" | "HELM" | "WEAPON" | "SHIELD" | "AMMO" | "TOOL" | "TRAPPARTS" | "TRAPCOMP"
            | "CHAIN" | "CLOTH" | "SPLINT" | "CRUTCH"
    )
}

/// Item quality 0..=5 from the maker's (effective) skill level (estimate, see module docs).
pub(crate) fn quality_roll(rating: i32, r: &mut Rng) -> u8 {
    let s = rating.clamp(0, 40) as u32;
    // Floor: a quarter of the skill (the remainder as a chance).
    let mut q = (s / 4) as i32;
    if r.below(4) < s % 4 {
        q += 1;
    }
    // Ladder: up to 3 more levels, each with chance 11 in 20.
    for _ in 0..3 {
        if !r.chance(11, 20) {
            break;
        }
        q += 1;
    }
    if q >= 5 {
        if r.chance(1, 3) {
            5
        } else {
            4
        }
    } else {
        q.max(0) as u8
    }
}

/// [metal12] Attributes that feed the quality roll of a skill: (physical, mental), primary first
/// (wiki Attribute: the skills each attribute affects; Kinesthetic sense "most skills involving
/// any movement"). The order inside a group is an ESTIMATE (the wiki lists no primaries).
pub(crate) fn quality_attrs(skill: &str) -> (&'static [&'static str], &'static [&'static str]) {
    const KIN: &[&str] = &["KINESTHETIC_SENSE"];
    const ART: &[&str] = &["CREATIVITY", "SPATIAL_SENSE", "KINESTHETIC_SENSE"];
    const SMITH: &[&str] = &["STRENGTH", "AGILITY", "ENDURANCE"];
    match skill {
        // Crafters: Agility + Creativity, Spatial sense, Kinesthetic sense.
        "STONECRAFT" | "WOODCRAFT" | "BONECARVE" | "CLOTHESMAKING" | "POTTERY" | "GLAZING" | "WAX_WORKING" => (&["AGILITY"], ART),
        "WEAVING" => (&[], ART),
        "LEATHERWORK" | "GLASSMAKER" => (SMITH, ART),
        "FORGE_WEAPON" | "FORGE_ARMOR" | "FORGE_FURNITURE" | "METALCRAFT" | "MASONRY" => (SMITH, KIN),
        "CARPENTRY" => (&["STRENGTH", "AGILITY"], KIN),
        "MECHANICS" => (SMITH, &["ANALYTICAL_ABILITY", "KINESTHETIC_SENSE"]),
        "BOWYER" | "ENGRAVE_STONE" | "CUTGEM" | "ENCRUSTGEM" => (&["AGILITY"], KIN),
        "COOK" => (&["AGILITY"], &["ANALYTICAL_ABILITY", "CREATIVITY", "KINESTHETIC_SENSE"]),
        _ => (&["AGILITY"], KIN),
    }
}

/// [metal12] DF's item quality roll (wiki "Skill", item quality formula), in quality points:
/// 1. effective level `level` (Dabbling 0 .. Legendary 15, uncapped, penalties applied);
/// 2. `rand(0..10) + rand(0..5 level / 2) + rand(0..5 level / 2)`;
/// 3./4. `rand(0..attr1) / 100 + rand(0..attr2) / 250 + rand(0..attr3) / 250` for the skill's
///    physical and mental attributes (`phys`, `ment`: current values, 0 = none);
/// 5. `points * focus / 100` (current / undistracted focus in percent);
/// 6. status penalties again (`penalty_pct`); 7. no curses; 8./9. +10 per matching item and
///    material preference (`prefs`);
/// 10. 0-21 ordinary, 22-29 well-crafted, 30-34 finely-crafted, 35-44 superior, 45-54
///    exceptional, 55+ masterwork with chance 1/3, else exceptional.
pub(crate) fn quality_points_roll(level: i32, phys: [i32; 3], ment: [i32; 3], focus_pct: i32, penalty_pct: i32, prefs: i32, r: &mut Rng) -> u8 {
    let pen = penalty_pct.clamp(0, 100);
    let lv = (level.max(0) * pen / 100) as u32;
    let half = lv * 5 / 2;
    let mut pts = r.below(10) as i64 + r.below(half) as i64 + r.below(half) as i64;
    let attr = |a: [i32; 3], r: &mut Rng| -> i64 {
        r.below(a[0].max(0) as u32) as i64 / 100 + r.below(a[1].max(0) as u32) as i64 / 250 + r.below(a[2].max(0) as u32) as i64 / 250
    };
    pts += attr(phys, r);
    pts += attr(ment, r);
    pts = pts * focus_pct.max(0) as i64 / 100;
    pts = pts * pen as i64 / 100;
    pts += 10 * prefs.clamp(0, 2) as i64;
    match pts {
        i64::MIN..=21 => 0,
        22..=29 => 1,
        30..=34 => 2,
        35..=44 => 3,
        45..=54 => 4,
        _ => {
            if r.chance(1, 3) {
                5
            } else {
                4
            }
        }
    }
}

/// Experience in tenths: the whole part plus one more with the remainder as a chance.
fn tenths(t: i32, r: &mut Rng) -> i32 {
    t / 10 + i32::from((r.below(10) as i32) < t % 10)
}

/// What a soldier found around its training spot (from `barracks_spar`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct TrainScene {
    /// A sparring partner next to it (both at least Competent in Fighting).
    pub partner: bool,
    /// Other soldiers training within 5 tiles (a bout to watch).
    pub watching: bool,
    /// A squadmate within 3 tiles who is at least 2 levels better in the drilled skill.
    pub teacher: Option<usize>,
    /// The skill a demonstration would teach: 0 weapon, 1 Fighting, 2 Discipline, 3 Dodging.
    pub demo_skill: u8,
}

/// Conversation skills with the weights seen in Gordon's fort (XP per dwarf-year r4 -> r5).
const TALK_SKILLS: [(&str, u32); 9] = [
    ("PERSUASION", 76),
    ("COMEDY", 41),
    ("JUDGING_INTENT", 37),
    ("CONVERSATION", 28),
    ("PACIFY", 25),
    ("INTIMIDATION", 20),
    ("CONSOLE", 20),
    ("FLATTERY", 8),
    ("LYING", 4),
];

impl Sim {
    /// [skills9] Experience for a finished workshop job: `XP_USE` (10 for stone blocks), plus
    /// `XP_QUALITY` and a quality roll for products with quality levels (items `first..`).
    pub(crate) fn skills_job_done(&mut self, s: usize, skill: &'static str, first: i32, now: u64) {
        let mut quality = false;
        if !skill.is_empty() {
            // [metal12] wiki item quality formula: skill level, the skill's attributes, focus.
            let (level, phys, ment) = {
                let u = &self.world.units[self.ai[s].uidx];
                let (pa, ma) = quality_attrs(skill);
                let get = |v: &[(String, i32)], names: &[&str]| {
                    let mut a = [0i32; 3];
                    for (k, n) in names.iter().take(3).enumerate() {
                        a[k] = v.iter().find(|(id, _)| id == n).map(|e| e.1).unwrap_or(0);
                    }
                    a
                };
                (u.skill(skill), get(&u.phys_attrs, pa), get(&u.ment_attrs, ma))
            };
            let focus = self.social_rating(s, 100);
            let mut r = Rng::for_unit(self.seed, s, now, 0x9A11);
            let mut masterworks: Vec<i32> = Vec::new(); // [calib11-social]
            for id in first..self.next_item_id {
                let Some(&i) = self.item_idx.get(&id) else { continue };
                let it = &mut self.world.items[i as usize];
                if !has_quality(&it.kind) || it.flags.artifact {
                    continue;
                }
                quality = true;
                let q = quality_points_roll(level, phys, ment, focus, 100, 0, &mut r); // [metal12]
                if it.value > 0 && q != it.quality {
                    let old = crate::trade::quality_mult(it.quality).max(1);
                    it.value = (it.value as i64 * crate::trade::quality_mult(q) / old) as i32;
                }
                it.quality = q;
                self.skills.made[q as usize] += 1;
                if q == 5 {
                    masterworks.push(id); // [calib11-social]
                }
            }
            for id in masterworks {
                self.skills_masterpiece(s, id, now); // [calib11-social]
            }
        }
        let base = if skill == "MASONRY" && (first..self.next_item_id).any(|id| self.item_idx.get(&id).map(|&i| self.world.items[i as usize].kind == "BLOCKS").unwrap_or(false)) {
            XP_STONECUT
        } else {
            XP_USE
        };
        if !skill.is_empty() {
            self.add_xp(s, skill, base + if quality { XP_QUALITY } else { 0 });
        }
    }

    /// [calib11-social] Slot `s` just made the masterwork item `id` (wiki Item quality: each
    /// masterwork is a historical event): DF's announcement "<name> has created a masterpiece
    /// <item>!" (MASTERPIECE_CRAFTED, the line Gordon's gamelog shows for crafts, cloth,
    /// furniture, ammo and meals) and the maker's `MakeMasterwork` thought ("after producing a
    /// masterwork"; strength an estimate like the other thoughts).
    pub(crate) fn skills_masterpiece(&mut self, s: usize, id: i32, now: u64) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let it = &self.world.items[i as usize];
        let mut what = it.desc.trim_matches(|c: char| "\u{2261}\u{263c}+*-\u{ab}\u{bb}".contains(c)).trim().to_string();
        if what.is_empty() {
            what = it.kind.to_lowercase().replace('_', " ");
        }
        if it.stack > 1 {
            what = format!("{what} [{}]", it.stack);
        }
        let pos = it.pos;
        let name = self.world.units[self.ai[s].uidx].name.clone();
        self.skills.masterpieces += 1;
        self.announce(now, format!("{name} has created a masterpiece {what}!"), Some(pos), crate::AnnouncementKind::Info);
        self.feel(s, "MakeMasterwork", crate::social::Class::Pos, 150, -1, 0, now);
    }

    /// [skills9] A repeating work order starts a new period: the manager approves it again.
    pub(crate) fn skills_order_approved(&mut self) {
        self.skills.approvals += 1;
        if let Some(m) = self.s2_noble_slot("MANAGER") {
            self.add_xp(m, "ORGANIZATION", XP_ORGANIZE);
        }
    }

    /// [skills9] One 100-tick training round of a soldier at its spot (see module docs).
    pub(crate) fn skills_train_round(&mut self, s: usize, weapon: &str, scene: TrainScene, now: u64) {
        let mut r = Rng::for_unit(self.seed, s, now, 0x7A41);
        let shield = self.world.units[self.ai[s].uidx]
            .inventory
            .iter()
            .any(|id| self.item_idx.get(id).map(|&i| self.world.items[i as usize].kind == "SHIELD").unwrap_or(false));
        if scene.partner {
            self.skills.spar_rounds += 1;
            for (skill, t) in [
                ("SITUATIONAL_AWARENESS", 80),
                ("MELEE_COMBAT", 30),
                (weapon, 20),
                ("DISCIPLINE", 20),
                ("WRESTLING", 10),
                ("ARMOR", 5),
                ("DODGING", 4),
                ("GRASP_STRIKE", 4),
                ("STANCE_STRIKE", 2),
                ("BITE", 2),
            ] {
                let xp = tenths(t, &mut r);
                if xp > 0 {
                    self.add_xp(s, skill, xp);
                }
            }
            if shield {
                let xp = tenths(4, &mut r);
                if xp > 0 {
                    self.add_xp(s, "SHIELD", xp);
                }
            }
        } else {
            self.skills.drill_rounds += 1;
            // Individual drill: one skill at a time.
            let roll = r.below(100);
            let skill = match roll {
                0..=29 => weapon,
                30..=69 => "MELEE_COMBAT",
                70..=89 => "DISCIPLINE",
                90..=93 => "ARMOR",
                94..=96 => "DODGING",
                _ if shield => "SHIELD",
                _ => "DODGING",
            };
            self.add_xp(s, skill, 5);
            if scene.watching {
                self.add_xp(s, "SITUATIONAL_AWARENESS", 3);
            }
        }
        if let Some(t) = scene.teacher {
            if r.chance(1, 10) {
                self.skills.demonstrations += 1;
                self.combat.stats.demonstrations += 1;
                let demo = match scene.demo_skill {
                    0 => weapon,
                    1 => "MELEE_COMBAT",
                    2 => "DISCIPLINE",
                    _ => "DODGING",
                };
                self.add_xp(s, demo, 3);
                self.add_xp(s, "KNOWLEDGE_ACQUISITION", 13);
                self.add_xp(s, "CONCENTRATION", 13);
                self.add_xp(t, "TEACHING", 13);
            }
        }
    }

    /// [skills9] Speaker `s` talked with a partner (`code` = relationship code of `socialize`,
    /// -2 = grudge; `partner_stress` = the partner's stress).
    pub(crate) fn skills_talked(&mut self, s: usize, code: i32, partner_stress: i32, now: u64) {
        let mut r = Rng::for_unit(self.seed, s, now, 0x7A1C);
        let skill = if code == -2 {
            if r.chance(1, 2) {
                "PACIFY"
            } else {
                "INTIMIDATION"
            }
        } else if partner_stress >= 10_000 && r.chance(1, 2) {
            "CONSOLE"
        } else {
            let total: u32 = TALK_SKILLS.iter().map(|t| t.1).sum();
            let mut k = r.below(total);
            let mut pick = TALK_SKILLS[0].0;
            for (name, w) in TALK_SKILLS {
                if k < w {
                    pick = name;
                    break;
                }
                k -= w;
            }
            pick
        };
        self.skills.talks += 1;
        self.add_xp(s, skill, TALK_XP);
    }

    /// [skills9] A citizen read a book in the library.
    pub(crate) fn skills_read(&mut self, s: usize) {
        self.add_xp(s, "KNOWLEDGE_ACQUISITION", 30);
        self.add_xp(s, "CONCENTRATION", 30);
    }

    /// [skills9] Kill a unit now (scripted scenarios and experiments): corpse, announcement,
    /// grief and horror of the others as for any death. False if it is unknown or dead.
    pub fn kill_unit(&mut self, unit_id: i32, why: &str) -> bool {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return false };
        if self.ai[s].kind == crate::Kind::Dead || self.world.units[self.ai[s].uidx].flags.dead {
            return false;
        }
        let now = self.ticks;
        self.kill(s, now, why);
        true
    }

    /// [skills9] Counters of this module (quality of made items, training rounds, talks).
    pub fn skills_report(&self) -> SkillsReport {
        self.skills.clone()
    }
}

/// [skills9] Counters: quality of made items (0..=5), training rounds, demonstrations,
/// conversations that trained a skill, manager approvals.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct SkillsReport {
    pub made: [u64; 6],
    pub spar_rounds: u64,
    pub drill_rounds: u64,
    pub demonstrations: u64,
    pub talks: u64,
    pub approvals: u64,
    /// Bad memories relived (`social_recall`).
    pub recalls: u64,
    /// [calib11-social] Masterworks made (each announced as "has created a masterpiece").
    pub masterpieces: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_quality_points_follow_skill_attributes_and_focus() {
        let mut r = Rng::new(11);
        let mut mean = |level: i32, phys: [i32; 3], ment: [i32; 3], focus: i32| {
            let mut h = [0u32; 6];
            for _ in 0..20_000 {
                h[quality_points_roll(level, phys, ment, focus, 100, 0, &mut r) as usize] += 1;
            }
            let m = h.iter().enumerate().map(|(q, &n)| q as f64 * n as f64).sum::<f64>() / 20_000.0;
            (m, h)
        };
        // A median dwarf crafter (Agility 900; Creativity 1250, Spatial 1500, Kinesthetic 1000).
        let craft = ([900, 0, 0], [1250, 1500, 1000]);
        // A median mason (Strength 1250, Agility 900, Endurance 1000; Kinesthetic 1000).
        let mason = ([1250, 900, 1000], [1000, 0, 0]);
        let (c0, _) = mean(0, craft.0, craft.1, 100);
        let (m0, _) = mean(0, mason.0, mason.1, 100);
        assert!(c0 > m0, "creative crafters score more points: {c0} vs {m0}");
        let (c5, _) = mean(5, craft.0, craft.1, 100);
        assert!(c5 > c0 + 1.0, "{c5} vs {c0}");
        let (cf, _) = mean(5, craft.0, craft.1, 60);
        assert!(cf < c5, "distraction lowers quality: {cf} vs {c5}");
        let (_, legend) = mean(20, craft.0, craft.1, 100);
        // 55+ points: masterwork with chance 1/3, else exceptional; a high skill "almost always"
        // makes at least exceptional items.
        assert!(legend[5] < 6_700 && legend[4] + legend[5] > 16_000, "{legend:?}");
    }

    #[test]
    fn quality_follows_skill_and_caps_masterworks() {
        let mut r = Rng::new(7);
        let mut hist = |rating: i32| {
            let mut h = [0u32; 6];
            for _ in 0..30_000 {
                h[quality_roll(rating, &mut r) as usize] += 1;
            }
            h
        };
        let novice = hist(0);
        assert!(novice[0] > 12_000 && novice[0] < 15_000 && novice[4] == 0, "{novice:?}");
        let skilled = hist(16);
        assert!(skilled[4] > 22_000 && skilled[5] > 4_000 && skilled[5] < 7_000 && skilled[3] == 0, "{skilled:?}");
        let legend = hist(30);
        // Masterworks at most 1 in 3, the rest exceptional.
        assert!(legend[5] < 10_600 && legend[5] > 9_400 && legend[3] == 0, "{legend:?}");
    }
}
