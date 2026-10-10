//! [society2] v50 locations: temples, taverns, libraries, guildhalls and hospitals, their
//! occupations, visitors and petitions.
//!
//! # DF rules implemented (wiki v50: Location, Temple, Tavern, Library, Guildhall, Performance,
//! Visitor, Petition, Mercenary, Scholar)
//! - A **location** is built on civzones (`World::locations`). Exports without locations get
//!   them from the fort's named meeting halls (Gordon's naming): "Tempel/Kapelle/Temple/Chapel/
//!   Shrine" = temple (dedicated to the deity whose name is in the zone name, "alle" = all
//!   deities), "Taverne/Tavern/Inn" = tavern, "Bibliothek/Library" = library, "Gilde/Guild" =
//!   guildhall, "Hospital/Heilung" = hospital. `Sim::create_location` adds one for a zone.
//! - **Occupations** (`Sim::assign_occupation`): `TAVERN_KEEPER`, `PERFORMER`, `SCHOLAR`
//!   (plus any DF name, kept as data). A citizen with an occupation goes to the location when it
//!   has no other job and works there.
//! - **Tavern keeper** (w:Tavern): serves drinks from the fort's stock to idle patrons in the
//!   tavern; the patron drinks without walking to the stockpile (`DrinkAlcohol`, `GoodDrink`).
//! - **Performers** (w:Performance): a performer in a tavern or temple performs ("Perform",
//!   `BeCreative`); citizens idle in the same location watch ("WatchPerform"): music and dance
//!   satisfy `MakeMerry`, poetry and storytelling `HearEloquence`.
//! - **Temples** (w:Temple): praying in a temple dedicated to the dwarf's own deity (or an
//!   all-deities temple) adds DF's "PrayedDedicatedTemple" thought on top of "Prayer".
//! - **Libraries** (w:Library, w:Scholar): idle citizens with an unmet `LearnSomething` /
//!   `ThinkAbstractly` read a written work kept in the library ("ReadBook"); scholars ponder
//!   ("PonderTopic") and write on a blank quire or scroll ("WriteBook"), which then becomes a
//!   written work of the fort.
//! - **Visitors** (w:Visitor): only come when the fort has a tavern, library or temple and stay
//!   under the `VISITOR_CAP` setting. Performers (bards, poets, dancers) and mercenaries / monster
//!   slayers come to taverns, scholars to libraries, pilgrims to temples. They walk in from the
//!   map edge, stay in their location and leave again.
//! - **Petitions** (w:Petition): visiting performers and scholars petition for residency,
//!   mercenaries and monster slayers for citizenship. `Sim::petitions` lists them,
//!   `Sim::answer_petition` accepts or denies. Accepted residents stay and take the matching
//!   occupation; new citizens join the fort. A petition nobody answers is denied
//!   ("DenyResident" / "DenyCitizen") and the visitor leaves.
//!
//! # Estimates (not on the wiki)
//! - Visitor rates: per season and location kind, tavern performers 2 in 3 (1-3 of them),
//!   mercenaries 1 in 4, scholars 1 in 2, pilgrims 1 in 3; arrival 1-20 days into the season;
//!   stay 1-3 months (pilgrims 3-14 days); petition after 7-21 days; unanswered petitions are
//!   denied after 28 days. 1 in 10 visitors is a thief (see `justice`).
//! - A tavern keeper serves one patron per 100 ticks; a scholar writes with a chance of 1 in 14
//!   per day when a blank quire/scroll exists; reading at most once a week per dwarf.

use crate::path::Goals;
use crate::rng::Rng;
use crate::social::Class;
use crate::{needs, AnnouncementKind, Kind, Sim, Task, Zone};
use rf_core::{Location, Occupation, Pos, Unit, UnitFlags, TICKS_PER_DAY, TICKS_PER_MONTH};

/// The round-8 society pass runs when `tick % S2_PERIOD == S2_PHASE`.
pub const S2_PERIOD: u64 = 100;
pub const S2_PHASE: u64 = 37;
const DAY: u64 = TICKS_PER_DAY as u64;
const MONTH: u64 = TICKS_PER_MONTH as u64;
/// Passes per day.
const WHEEL: u64 = DAY / S2_PERIOD;
/// Unanswered petitions are denied after this many ticks (estimate).
pub const PETITION_TIMEOUT: u64 = 28 * DAY;
/// Members a religion or guild needs in the fort before it petitions (estimate).
pub const LOCATION_PETITION_MEMBERS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocKind {
    Temple,
    Tavern,
    Library,
    Guildhall,
    Hospital,
    Other,
}

impl LocKind {
    pub fn of(kind: &str) -> LocKind {
        match kind.to_ascii_lowercase().as_str() {
            "temple" => LocKind::Temple,
            "tavern" | "inn" | "inntavern" => LocKind::Tavern,
            "library" => LocKind::Library,
            "guildhall" | "guild" => LocKind::Guildhall,
            "hospital" => LocKind::Hospital,
            _ => LocKind::Other,
        }
    }
}

/// Visitor role (DF visitor kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitorRole {
    Performer,
    Scholar,
    Mercenary,
    MonsterSlayer,
    Pilgrim,
}

impl VisitorRole {
    pub fn label(self) -> &'static str {
        match self {
            VisitorRole::Performer => "performer",
            VisitorRole::Scholar => "scholar",
            VisitorRole::Mercenary => "mercenary",
            VisitorRole::MonsterSlayer => "monster slayer",
            VisitorRole::Pilgrim => "pilgrim",
        }
    }
    fn profession(self, r: &mut Rng) -> &'static str {
        match self {
            VisitorRole::Performer => ["BARD", "POET", "DANCER"][r.below(3) as usize],
            VisitorRole::Scholar => ["SCHOLAR", "PHILOSOPHER", "HISTORIAN"][r.below(3) as usize],
            VisitorRole::Mercenary => "MERCENARY",
            VisitorRole::MonsterSlayer => "MONSTER_SLAYER",
            VisitorRole::Pilgrim => "PILGRIM",
        }
    }
}

/// A petition of a visitor (DF `agreement`: residency or citizenship).
#[derive(Clone, Debug, Default)]
pub struct Petition {
    pub id: i32,
    pub unit: i32,
    pub name: String,
    /// `Residency` or `Citizenship`.
    pub kind: String,
    /// Visitor role, e.g. `performer`.
    pub role: String,
    pub location: i32,
    pub tick: u64,
    /// `Pending`, `Accepted`, `Denied`, `Expired`.
    pub status: String,
}

/// Location statistics for reports and the benchmark.
#[derive(Clone, Debug, Default)]
pub struct LocationReport {
    /// (location id, kind, name, citizen-days spent there).
    pub usage: Vec<(i32, String, String, u64)>,
    pub derived_locations: usize,
    pub drinks_served: u64,
    pub performances: u64,
    pub audience: u64,
    pub dedicated_prayers: u64,
    pub books_read: u64,
    pub books_written: u64,
    pub scholar_days: u64,
    /// (tick, role, location name) of each arriving visitor.
    pub visitors: Vec<(u64, String, String)>,
    pub visitors_left: u64,
    pub petitions: u64,
    pub accepted: u64,
    pub denied: u64,
}

/// Runtime view of one location.
#[derive(Clone, Debug)]
pub(crate) struct LocRt {
    /// Index into `World::locations`.
    pub li: usize,
    pub id: i32,
    pub kind: LocKind,
    pub deity: i32,
    /// Building indices of the location's civzones.
    pub zones: Vec<usize>,
    pub bbox: (i16, i16, i16, i16, i16, i16),
    /// Citizen passes inside (x `S2_PERIOD` ticks).
    pub passes: u64,
    /// Written works lying in the location (refreshed daily).
    pub books: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Visitor {
    pub slot: u32,
    pub unit: i32,
    pub role: VisitorRole,
    pub loc: usize,
    pub until: u64,
    pub petition_at: u64,
    pub leaving: bool,
    pub resident: bool,
    pub tries: u8,
    pub thief: bool,
    /// Item carried off by a thief (-1 none).
    pub loot: i32,
}

#[derive(Clone, Debug)]
struct Arrival {
    tick: u64,
    role: VisitorRole,
    loc: usize,
}

#[derive(Default)]
pub(crate) struct Locs {
    /// Round-8 society modules switched off (A/B runs; `Sim::set_society2_enabled`).
    pub off: bool,
    /// No visitors (`Sim::set_visitors_enabled(false)`).
    pub no_visitors: bool,
    ready: bool,
    key: (usize, usize),
    pub rt: Vec<LocRt>,
    pub visitors: Vec<Visitor>,
    arrivals: Vec<Arrival>,
    pub petitions: Vec<Petition>,
    next_petition: i32,
    last_season: i32,
    last_month: i32,
    /// Members (unit ids) of the religion / guild behind each temple / guildhall petition.
    members: Vec<(i32, Vec<i32>)>,
    /// Per-slot: next tick a citizen may read again.
    next_read: Vec<u64>,
    /// Blank quires / scrolls (item ids), refreshed daily.
    blanks: Vec<i32>,
    pub rep: LocationReport,
}

/// First word of a name, lowercased, ASCII letters only (deity matching). Zone names typed in
/// DF arrive with their non-ASCII letters mangled (CP437 view of UTF-8 bytes), so only the
/// ASCII letters are compared: "Rîsen" and its mangled form both give "rsen".
fn first_word(s: &str) -> String {
    ascii_key(s.split_whitespace().next().unwrap_or(""))
}

fn ascii_key(w: &str) -> String {
    w.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

impl Sim {
    // ------------------------------------------------------------------ shared helpers

    /// Living citizen worker (or a caged/jailed citizen) in slot `s`.
    pub(crate) fn s2_citizen(&self, s: usize) -> bool {
        let ai = &self.ai[s];
        let u = &self.world.units[ai.uidx];
        u.flags.citizen && !u.flags.dead && !u.flags.animal && ai.kind == Kind::Worker
    }

    /// Set a need to satisfied (focus 400); returns the lowest focus level before.
    pub(crate) fn s2_satisfy(&mut self, s: usize, need: &str) -> i32 {
        let u = &mut self.world.units[self.ai[s].uidx];
        let mut before = i32::MAX;
        for n in u.needs.iter_mut().filter(|n| n.id == need) {
            before = before.min(n.focus_level);
            n.focus_level = 400;
        }
        before
    }

    pub(crate) fn s2_need(&self, s: usize, need: &str) -> i32 {
        self.world.units[self.ai[s].uidx].needs.iter().filter(|n| n.id == need).map(|n| n.focus_level).min().unwrap_or(i32::MAX)
    }

    /// Slot of the living holder of a noble position (worker on the map).
    pub(crate) fn s2_noble_slot(&self, token: &str) -> Option<usize> {
        self.world
            .nobles
            .iter()
            .filter(|n| n.token == token && n.holder >= 0)
            .filter_map(|n| self.unit_slot.get(&n.holder).copied())
            .find(|&s| self.s2_citizen(s))
    }

    /// Idle (no job, not in a breakdown).
    pub(crate) fn s2_idle(&self, s: usize) -> bool {
        matches!(self.ai[s].task, Task::Idle | Task::Wander) && !self.social_no_work(s)
    }

    /// Consume one free unit of a drink (`drink`) or prepared/edible food and return its id.
    pub(crate) fn s2_take_consumable(&mut self, drink: bool) -> Option<i32> {
        let now = self.ticks;
        let list = if drink { &self.drinks } else { &self.foods };
        let mut pick = None;
        for &id in list.iter() {
            if let Some(&i) = self.item_idx.get(&id) {
                let i = i as usize;
                if self.item_free(i, now) && !self.world.items[i].flags.rotten {
                    pick = Some(id);
                    break;
                }
            }
        }
        pick
    }

    /// Path a unit to a random walkable tile of building `bi` (a zone). False if none reachable.
    pub(crate) fn s2_go_zone(&mut self, s: usize, bi: usize, now: u64, salt: u64) -> bool {
        let b = &self.world.buildings[bi];
        let (x1, y1, x2, y2, z) = (b.x1, b.y1, b.x2, b.y2, b.z);
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let mut r = Rng::for_unit(self.seed, s, now, salt);
        let w = (x2 - x1 + 1).max(1) as u32;
        let h = (y2 - y1 + 1).max(1) as u32;
        for _ in 0..12 {
            let p = Pos::new(x1 + r.below(w) as i16, y1 + r.below(h) as i16, z);
            if !self.world.buildings[bi].contains(p) {
                continue;
            }
            if let Some(t) = self.world.idx(p) {
                if self.nav.walkable(t) && self.nav.comp_root(t) == c {
                    self.ai[s].task = Task::Wander;
                    self.request_path(s, Goals::one(t as u32), now);
                    return true;
                }
            }
        }
        false
    }

    // ------------------------------------------------------------------ locations

    fn loc_ensure(&mut self) {
        if !self.locs.ready {
            self.locs.ready = true;
            if self.world.locations.is_empty() {
                self.derive_locations();
            }
            let now = self.ticks;
            self.locs.last_season = self.world.calendar.month() / 3;
            let _ = now;
        }
        let key = (self.world.locations.len(), self.world.buildings.len());
        if key != self.locs.key {
            self.locs.key = key;
            self.rebuild_loc_rt();
        }
        if self.locs.next_read.len() < self.ai.len() {
            self.locs.next_read.resize(self.ai.len(), 0);
        }
    }

    /// Locations from the fort's named meeting halls (exports before round 6 have none).
    fn derive_locations(&mut self) {
        let mut deities: Vec<(i32, String)> = Vec::new();
        for u in &self.world.units {
            if !u.flags.citizen {
                continue;
            }
            for n in u.needs.iter().filter(|n| n.id == "PrayOrMeditate" && n.deity_id >= 0) {
                if deities.iter().all(|d| d.0 != n.deity_id) {
                    if let Some(h) = self.world.hist_figs.iter().find(|h| h.id == n.deity_id) {
                        deities.push((n.deity_id, first_word(&h.name)));
                    }
                }
            }
        }
        deities.sort();
        let mut next = 1;
        let mut out = Vec::new();
        for b in &self.world.buildings {
            if b.kind != "Civzone" || !b.subtype.contains("Meeting") || b.name.is_empty() {
                continue;
            }
            let n = b.name.to_lowercase();
            let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
            let kind = if has(&["tempel", "temple", "kapelle", "chapel", "shrine", "schrein"]) {
                "Temple"
            } else if has(&["tavern", "inn ", "kneipe", "schenke", "wirtshaus"]) {
                "Tavern"
            } else if has(&["bibliothek", "library"]) {
                "Library"
            } else if has(&["gilde", "guild"]) {
                "Guildhall"
            } else if has(&["hospital", "heilung", "lazarett", "spital"]) {
                "Hospital"
            } else {
                continue;
            };
            let words: Vec<String> = n.split_whitespace().map(ascii_key).collect();
            let deity = if kind == "Temple" {
                deities.iter().find(|(_, w)| !w.is_empty() && words.iter().any(|x| x == w)).map(|d| d.0).unwrap_or(-1)
            } else {
                -1
            };
            let profession = if kind == "Guildhall" {
                match () {
                    _ if has(&["steinmetz", "mason"]) => "MASON",
                    _ if has(&["schmied", "smith"]) => "WEAPONSMITH",
                    _ if has(&["zimmer", "carpent", "tischler"]) => "CARPENTER",
                    _ if has(&["handwerk", "craft"]) => "CRAFTSMAN",
                    _ => "",
                }
            } else {
                ""
            };
            out.push(Location {
                id: next,
                kind: kind.into(),
                name: b.name.clone(),
                deity,
                profession: profession.into(),
                zones: vec![b.id],
                occupations: Vec::new(),
                allow_visitors: true,
            });
            next += 1;
        }
        self.locs.rep.derived_locations = out.len();
        // DF shows a citizen's occupation as the profession: give the derived locations the
        // tavern keepers, performers and scholars the export names.
        for u in &self.world.units {
            if !u.flags.citizen || u.flags.dead {
                continue;
            }
            let (want, occ) = match u.profession.as_str() {
                "TAVERN_KEEPER" => ("Tavern", "TAVERN_KEEPER"),
                "POET" | "BARD" | "DANCER" => ("Tavern", "PERFORMER"),
                "SCHOLAR" | "PHILOSOPHER" | "HISTORIAN" | "MATHEMATICIAN" | "ASTRONOMER" | "NATURALIST" | "CHEMIST" | "GEOGRAPHER" | "SCRIBE" => ("Library", "SCHOLAR"),
                _ => continue,
            };
            if let Some(l) = out.iter_mut().filter(|l| l.kind == want).min_by_key(|l| l.occupations.len()) {
                l.occupations.push(Occupation { kind: occ.into(), unit: u.id });
            }
        }
        self.world.locations = out;
    }

    /// Guild of a profession (DF guilds group related professions; grouping is an estimate).
    fn guild_of(profession: &str) -> Option<&'static str> {
        Some(match profession {
            "PLANTER" | "FARMER" | "THRESHER" | "HERBALIST" | "MILLER" | "CHEESE_MAKER" | "BREWER" | "COOK" | "BEEKEEPER" => "FARMER",
            "MASON" | "STONECUTTER" | "ENGRAVER" | "STONE_CARVER" | "STONECRAFTER" => "MASON",
            "MINER" => "MINER",
            "WOODCUTTER" | "CARPENTER" | "WOODCRAFTER" | "BOWYER" | "WOODWORKER" => "CARPENTER",
            "BLACKSMITH" | "ARMORER" | "WEAPONSMITH" | "METALSMITH" | "METALCRAFTER" | "FURNACE_OPERATOR" => "METALSMITH",
            "WEAVER" | "CLOTHIER" | "SHEARER" | "SPINNER" | "DYER" | "LEATHERWORKER" | "TANNER" => "CLOTHIER",
            "JEWELER" | "GEM_CUTTER" | "GEM_SETTER" | "BONE_CARVER" | "CRAFTSMAN" | "GLAZER" | "GLASSMAKER" | "POTTER" => "CRAFTSMAN",
            _ => return None,
        })
    }

    /// Monthly: religions and guilds with enough members in the fort petition for a temple /
    /// guildhall (w:Petition). Unanswered petitions anger the members.
    fn location_petitions_month(&mut self, now: u64) {
        let mut deity: std::collections::BTreeMap<i32, Vec<i32>> = Default::default();
        let mut guild: std::collections::BTreeMap<&'static str, Vec<i32>> = Default::default();
        for s in 0..self.ai.len() {
            if !self.s2_citizen(s) {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            if let Some(d) = u.needs.iter().find(|n| n.id == "PrayOrMeditate" && n.deity_id >= 0).map(|n| n.deity_id) {
                deity.entry(d).or_default().push(u.id);
            }
            if let Some(g) = Sim::guild_of(&u.profession) {
                guild.entry(g).or_default().push(u.id);
            }
        }
        let mut new: Vec<(String, String, Vec<i32>)> = Vec::new();
        for (d, members) in deity {
            if members.len() < LOCATION_PETITION_MEMBERS {
                continue;
            }
            let has = self.locs.rt.iter().any(|l| l.kind == LocKind::Temple && l.deity == d);
            let name = self.world.hist_figs.iter().find(|h| h.id == d).map(|h| h.name.clone()).unwrap_or_else(|| format!("deity {d}"));
            if !has {
                new.push(("Temple".into(), name, members));
            }
        }
        for (g, members) in guild {
            if members.len() < LOCATION_PETITION_MEMBERS {
                continue;
            }
            let has = self.locs.rt.iter().any(|l| l.kind == LocKind::Guildhall && self.world.locations[l.li].profession == g);
            if !has {
                new.push(("Guildhall".into(), g.to_string(), members));
            }
        }
        for (kind, role, members) in new {
            // At most one open petition per group; a new one a year after the last.
            let recent = self.locs.petitions.iter().any(|p| p.kind == kind && p.role == role && (p.status == "Pending" || now < p.tick + 12 * MONTH));
            if recent {
                continue;
            }
            self.locs.next_petition += 1;
            let id = self.locs.next_petition;
            let name = if kind == "Temple" { format!("the worshippers of {role}") } else { format!("the {} guild", role.to_lowercase()) };
            self.locs.petitions.push(Petition { id, unit: -1, name: name.clone(), kind: kind.clone(), role, location: -1, tick: now, status: "Pending".into() });
            self.locs.members.push((id, members));
            self.locs.rep.petitions += 1;
            self.announce(now, format!("{name} have petitioned for a {}.", kind.to_lowercase()), None, AnnouncementKind::Info);
        }
    }

    /// Answer a temple / guildhall petition: members are pleased or feel ignored.
    fn answer_location_petition(&mut self, pi: usize, accept: bool, expired: bool) {
        let now = self.ticks;
        let (id, kind) = (self.locs.petitions[pi].id, self.locs.petitions[pi].kind.clone());
        let members = self.locs.members.iter().find(|m| m.0 == id).map(|m| m.1.clone()).unwrap_or_default();
        let temple = kind == "Temple";
        let thought = match (accept, temple) {
            (true, true) => "AcceptedTemplePetition",
            (true, false) => "AcceptedGuildhallPetition",
            (false, true) => "IgnoredTemplePetition",
            (false, false) => "IgnoredGuildhallPetition",
        };
        for uid in members {
            if let Some(&s) = self.unit_slot.get(&uid) {
                if self.s2_citizen(s) {
                    if accept {
                        self.feel(s, thought, Class::Pos, 120, -1, 0, now);
                    } else {
                        self.feel(s, thought, Class::NegMild, 100, -1, 0, now);
                    }
                }
            }
        }
        if accept {
            self.locs.rep.accepted += 1;
        } else {
            self.locs.rep.denied += 1;
        }
        self.locs.petitions[pi].status = if accept {
            "Accepted".into()
        } else if expired {
            "Expired".into()
        } else {
            "Denied".into()
        };
    }

    fn rebuild_loc_rt(&mut self) {
        let old: Vec<(i32, u64)> = self.locs.rt.iter().map(|l| (l.id, l.passes)).collect();
        let mut rt = Vec::new();
        for (li, l) in self.world.locations.iter().enumerate() {
            let zones: Vec<usize> = l.zones.iter().filter_map(|&z| self.world.buildings.iter().position(|b| b.id == z)).collect();
            if zones.is_empty() {
                continue;
            }
            let mut bb = (i16::MAX, i16::MAX, i16::MIN, i16::MIN, i16::MAX, i16::MIN);
            for &bi in &zones {
                let b = &self.world.buildings[bi];
                bb = (bb.0.min(b.x1), bb.1.min(b.y1), bb.2.max(b.x2), bb.3.max(b.y2), bb.4.min(b.z), bb.5.max(b.z));
            }
            let passes = old.iter().find(|o| o.0 == l.id).map(|o| o.1).unwrap_or(0);
            rt.push(LocRt { li, id: l.id, kind: LocKind::of(&l.kind), deity: l.deity, zones, bbox: bb, passes, books: 0 });
        }
        for v in &mut self.locs.visitors {
            v.loc = v.loc.min(rt.len().saturating_sub(1));
        }
        self.locs.rt = rt;
    }

    /// Index into `locs.rt` of the location containing `p`.
    pub(crate) fn loc_at(&self, p: Pos) -> Option<usize> {
        self.locs.rt.iter().position(|l| {
            let b = l.bbox;
            p.x >= b.0 && p.y >= b.1 && p.x <= b.2 && p.y <= b.3 && p.z >= b.4 && p.z <= b.5 && l.zones.iter().any(|&bi| self.world.buildings[bi].contains(p))
        })
    }

    fn occupation_holder(&self, ri: usize, kind: &str) -> Vec<i32> {
        let li = self.locs.rt[ri].li;
        self.world.locations[li].occupations.iter().filter(|o| o.kind == kind && o.unit >= 0).map(|o| o.unit).collect()
    }

    /// Location index and occupation kind held by unit `id`.
    fn occupation_of(&self, id: i32) -> Option<(usize, &'static str)> {
        for (ri, l) in self.locs.rt.iter().enumerate() {
            for o in &self.world.locations[l.li].occupations {
                if o.unit == id {
                    let k = match o.kind.as_str() {
                        "TAVERN_KEEPER" => "TAVERN_KEEPER",
                        "PERFORMER" => "PERFORMER",
                        "SCHOLAR" => "SCHOLAR",
                        _ => continue,
                    };
                    return Some((ri, k));
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------ periodic pass

    /// [hook] Round-8 society pass (locations, justice, lifestyle), every `S2_PERIOD` ticks.
    pub(crate) fn society2_pass(&mut self, now: u64) {
        if self.locs.off {
            return;
        }
        self.loc_ensure();
        let phase = (now / S2_PERIOD) % WHEEL;
        let season = self.world.calendar.month() / 3;
        if season != self.locs.last_season {
            self.locs.last_season = season;
            self.visitors_season(now);
        }
        let month = self.world.calendar.year * 12 + self.world.calendar.month();
        if month != self.locs.last_month {
            if self.locs.last_month != 0 {
                self.location_petitions_month(now);
            }
            self.locs.last_month = month;
        }
        self.visitor_arrivals(now);
        self.locations_tick(now, phase == 0);
        self.petitions_tick(now);
        self.justice_tick(now, phase); // [justice]
        self.lifestyle_tick(now, phase); // [lifestyle]
    }

    fn locations_tick(&mut self, now: u64, daily: bool) {
        if self.locs.rt.is_empty() {
            return;
        }
        if daily {
            self.refresh_books();
        }
        // Who is where (citizens and visitors).
        let n = self.locs.rt.len();
        let mut inside: Vec<Vec<usize>> = vec![Vec::new(); n];
        for s in 0..self.ai.len() {
            let k = self.ai[s].kind;
            if k == Kind::Dead {
                continue;
            }
            let cit = self.s2_citizen(s);
            if !cit && !(k == Kind::Other && self.visitor_owns(s)) {
                continue;
            }
            let p = self.world.units[self.ai[s].uidx].pos;
            if let Some(ri) = self.loc_at(p) {
                inside[ri].push(s);
                if cit {
                    self.locs.rt[ri].passes += 1;
                }
            }
        }
        for ri in 0..n {
            if inside[ri].is_empty() {
                continue;
            }
            match self.locs.rt[ri].kind {
                LocKind::Tavern => self.tavern_serve(ri, &inside[ri], now),
                LocKind::Library => self.library_read(ri, &inside[ri], now),
                _ => {}
            }
            if daily {
                if matches!(self.locs.rt[ri].kind, LocKind::Tavern | LocKind::Temple) {
                    self.perform(ri, &inside[ri], now);
                }
                if self.locs.rt[ri].kind == LocKind::Library {
                    self.scholars_work(ri, &inside[ri], now);
                }
            }
        }
    }

    /// The tavern keeper serves one idle patron a drink (w:Tavern).
    fn tavern_serve(&mut self, ri: usize, inside: &[usize], now: u64) {
        let keepers = self.occupation_holder(ri, "TAVERN_KEEPER");
        let keeper = inside.iter().copied().find(|&s| keepers.contains(&self.ai[s].id) && self.s2_idle(s));
        let Some(k) = keeper else { return };
        let patron = inside.iter().copied().find(|&s| {
            s != k && self.s2_citizen(s) && self.s2_idle(s) && {
                let (th, _, _) = self.needs_now(&self.ai[s]);
                th >= needs::THIRST_CONSIDER || self.s2_need(s, "DrinkAlcohol") < 0
            }
        });
        let Some(p) = patron else { return };
        let Some(id) = self.s2_take_consumable(true) else { return };
        self.update_needs(p, now);
        self.ai[p].thirst = (self.ai[p].thirst - needs::DRINK_SATISFIES).max(0);
        self.social_drank(p, id, now);
        self.consume(id);
        self.s2_satisfy(k, "StayOccupied");
        self.s2_satisfy(k, "HelpSomebody");
        self.locs.rep.drinks_served += 1;
    }

    /// Performers in the location perform for the idle citizens there (w:Performance).
    fn perform(&mut self, ri: usize, inside: &[usize], now: u64) {
        let occ = self.occupation_holder(ri, "PERFORMER");
        let performers: Vec<usize> = inside
            .iter()
            .copied()
            .filter(|&s| {
                let id = self.ai[s].id;
                (occ.contains(&id) && self.s2_citizen(s) && self.s2_idle(s))
                    || self.locs.visitors.iter().any(|v| v.slot == s as u32 && v.role == VisitorRole::Performer && !v.leaving)
            })
            .collect();
        for &p in &performers {
            self.locs.rep.performances += 1;
            // Music / dance (MakeMerry) or poetry / story (HearEloquence), fixed per performer.
            let eloquent = self.ai[p].id % 3 == 2;
            if self.s2_citizen(p) {
                self.feel(p, "Perform", Class::PosMild, 80, -1, 0, now);
                self.s2_satisfy(p, "BeCreative");
                self.s2_satisfy(p, "StayOccupied");
            }
            for &a in inside {
                if a == p || !self.s2_citizen(a) || !self.s2_idle(a) || performers.contains(&a) {
                    continue;
                }
                self.locs.rep.audience += 1;
                self.feel(a, "WatchPerform", Class::Pos, 100, -1, 0, now);
                self.s2_satisfy(a, if eloquent { "HearEloquence" } else { "MakeMerry" });
            }
        }
    }

    fn refresh_books(&mut self) {
        let mut blanks = Vec::new();
        for l in &mut self.locs.rt {
            l.books = 0;
        }
        for (i, it) in self.world.items.iter().enumerate() {
            let written = it.kind == "BOOK" || (it.kind == "TOOL" && matches!(it.subtype.as_str(), "ITEM_TOOL_QUIRE" | "ITEM_TOOL_SCROLL"));
            if !written || self.imeta[i].dead {
                continue;
            }
            if it.kind == "TOOL" && it.name.is_empty() {
                if !it.flags.forbid && it.holder < 0 {
                    blanks.push(it.id);
                }
                continue;
            }
            if let Some(ri) = self.loc_at(it.pos) {
                self.locs.rt[ri].books += 1;
            }
        }
        self.locs.blanks = blanks;
    }

    /// Idle citizens with an unmet learning need read a book in the library (w:Library).
    fn library_read(&mut self, ri: usize, inside: &[usize], now: u64) {
        if self.locs.rt[ri].books == 0 {
            return;
        }
        for &s in inside {
            if !self.s2_citizen(s) || !self.s2_idle(s) || now < self.locs.next_read.get(s).copied().unwrap_or(0) {
                continue;
            }
            if self.s2_need(s, "LearnSomething").min(self.s2_need(s, "ThinkAbstractly")) >= 0 {
                continue;
            }
            self.s2_satisfy(s, "LearnSomething");
            self.s2_satisfy(s, "ThinkAbstractly");
            self.feel(s, "ReadBook", Class::Pos, 100, -1, 0, now);
            self.locs.rep.books_read += 1;
            if let Some(n) = self.locs.next_read.get_mut(s) {
                *n = now + 7 * DAY;
            }
        }
    }

    /// Scholars ponder and write in the library (w:Scholar).
    fn scholars_work(&mut self, ri: usize, inside: &[usize], now: u64) {
        let occ = self.occupation_holder(ri, "SCHOLAR");
        for &s in inside {
            let id = self.ai[s].id;
            let citizen = occ.contains(&id) && self.s2_citizen(s) && self.s2_idle(s);
            let visiting = self.locs.visitors.iter().any(|v| v.slot == s as u32 && v.role == VisitorRole::Scholar && !v.leaving);
            if !citizen && !visiting {
                continue;
            }
            self.locs.rep.scholar_days += 1;
            if citizen {
                self.s2_satisfy(s, "ThinkAbstractly");
                self.s2_satisfy(s, "LearnSomething");
                self.feel(s, "PonderTopic", Class::PosMild, 80, -1, 0, now);
            }
            let mut r = Rng::for_unit(self.seed, s, now, 0xB00C);
            if r.chance(1, 14) {
                if let Some(bid) = self.locs.blanks.pop() {
                    let name = self.world.units[self.ai[s].uidx].name.clone();
                    if let Some(&bi) = self.item_idx.get(&bid) {
                        let topics = ["On the Nature of Stone", "A History of the Fortress", "Treatise on Mathematics", "The Art of Brewing", "Of Gods and Mountains"];
                        let t = topics[r.below(topics.len() as u32) as usize];
                        self.world.items[bi as usize].name = format!("{t} by {name}");
                    }
                    if citizen {
                        self.feel(s, "WriteBook", Class::Pos, 150, -1, 0, now);
                    }
                    self.locs.rep.books_written += 1;
                    let p = self.world.units[self.ai[s].uidx].pos;
                    self.announce(now, format!("{name} has written a new work."), Some(p), AnnouncementKind::Info);
                }
            }
        }
    }

    /// [hook] From `social_day`: the unit just prayed in a temple room at `pos`.
    pub(crate) fn locations_prayed(&mut self, s: usize, pos: Pos, deity: i32, now: u64) {
        if self.locs.off {
            return;
        }
        self.loc_ensure();
        let Some(ri) = self.loc_at(pos) else { return };
        let l = &self.locs.rt[ri];
        if l.kind != LocKind::Temple {
            return;
        }
        if l.deity < 0 || l.deity == deity {
            self.locs.rep.dedicated_prayers += 1;
            self.feel(s, "PrayedDedicatedTemple", Class::Pos, 120, deity, 0, now);
        }
    }

    /// [hook] End of `decide` (no job found): citizens with an occupation go to their location.
    pub(crate) fn society2_idle(&mut self, s: usize, now: u64) -> bool {
        if self.locs.off || self.locs.rt.is_empty() || !self.s2_citizen(s) || self.social_no_work(s) {
            return false;
        }
        let id = self.ai[s].id;
        let Some((ri, _)) = self.occupation_of(id) else { return false };
        let p = self.world.units[self.ai[s].uidx].pos;
        if self.loc_at(p) == Some(ri) {
            let b = self.world.buildings[self.locs.rt[ri].zones[0]].clone();
            let mut r = Rng::for_unit(self.seed, s, now, 0x0CC);
            if r.chance(1, 3) {
                self.random_step(s, now, &mut r, Some(Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z }));
            }
            self.ai[s].wake = now + 30 + r.below(60) as u64;
            return true;
        }
        let bi = self.locs.rt[ri].zones[0];
        self.s2_go_zone(s, bi, now, 0x0CD)
    }

    // ------------------------------------------------------------------ visitors

    pub(crate) fn visitor_owns(&self, s: usize) -> bool {
        self.locs.visitors.iter().any(|v| v.slot == s as u32)
    }

    fn visitor_cap(&self) -> usize {
        self.world
            .settings
            .other
            .iter()
            .find_map(|o| o.strip_prefix("VISITOR_CAP=").and_then(|v| v.trim().parse::<usize>().ok()))
            .unwrap_or(100)
    }

    /// Plan this season's visitors (w:Visitor).
    fn visitors_season(&mut self, now: u64) {
        if self.locs.no_visitors || !self.has_civilization() {
            return;
        }
        let mut r = Rng::for_unit(self.seed, 0, now, 0x7151);
        let pick = |sim: &Sim, k: LocKind, r: &mut Rng| -> Option<usize> {
            let c: Vec<usize> = (0..sim.locs.rt.len()).filter(|&i| sim.locs.rt[i].kind == k && sim.world.locations[sim.locs.rt[i].li].allow_visitors).collect();
            (!c.is_empty()).then(|| c[r.below(c.len() as u32) as usize])
        };
        let mut plan: Vec<(VisitorRole, usize)> = Vec::new();
        if let Some(t) = pick(self, LocKind::Tavern, &mut r) {
            if r.chance(2, 3) {
                for _ in 0..1 + r.below(3) {
                    plan.push((VisitorRole::Performer, t));
                }
            }
            if r.chance(1, 4) {
                plan.push((if r.chance(1, 3) { VisitorRole::MonsterSlayer } else { VisitorRole::Mercenary }, t));
            }
        }
        if let Some(l) = pick(self, LocKind::Library, &mut r) {
            if r.chance(1, 2) {
                plan.push((VisitorRole::Scholar, l));
            }
        }
        if let Some(t) = pick(self, LocKind::Temple, &mut r) {
            if r.chance(1, 3) {
                plan.push((VisitorRole::Pilgrim, t));
            }
        }
        let room = self.visitor_cap().saturating_sub(self.locs.visitors.len() + self.locs.arrivals.len());
        for (role, loc) in plan.into_iter().take(room) {
            let tick = now + (1 + r.below(20) as u64) * DAY;
            self.locs.arrivals.push(Arrival { tick, role, loc });
        }
    }

    fn visitor_arrivals(&mut self, now: u64) {
        let mut k = 0;
        while k < self.locs.arrivals.len() {
            if self.locs.arrivals[k].tick > now {
                k += 1;
                continue;
            }
            let a = self.locs.arrivals.remove(k);
            if a.loc < self.locs.rt.len() {
                self.spawn_visitor(a.role, a.loc, now);
            }
        }
    }

    /// Spawn one visitor at a map edge connected to the fort. Returns the unit id.
    pub(crate) fn spawn_visitor(&mut self, role: VisitorRole, loc: usize, now: u64) -> Option<i32> {
        let tiles = self.society_edge_tiles(0);
        if tiles.is_empty() {
            return None;
        }
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let mut r = Rng::for_unit(self.seed, id as usize, now, 0x7152);
        let tile = tiles[r.below(tiles.len() as u32) as usize];
        // A people at peace with the fort (or its own civilization).
        let civs: Vec<(i32, String)> = self
            .world
            .entities
            .iter()
            .filter(|e| e.kind == "Civilization" && matches!(e.relation.as_str(), "Own" | "Peace") && matches!(e.race.as_str(), "DWARF" | "HUMAN" | "ELF"))
            .map(|e| (e.id, e.race.clone()))
            .collect();
        let (civ, race) = if civs.is_empty() { (-1, "DWARF".to_string()) } else { civs[r.below(civs.len() as u32) as usize].clone() };
        let names: Vec<&str> = self.world.hist_figs.iter().filter(|h| h.race == race).map(|h| h.name.as_str()).collect();
        let name = if names.len() >= 2 {
            let a = names[r.below(names.len() as u32) as usize].split_whitespace().next().unwrap_or("Urist").to_string();
            let b = names[r.below(names.len() as u32) as usize].split_whitespace().last().unwrap_or("Visitor").to_string();
            format!("{a} {b}")
        } else {
            format!("Visitor {id}")
        };
        let sex = r.below(2) as i8;
        let raw = self.world.creature_raws.iter().find(|c| c.token == race);
        let walk = raw.and_then(|c| c.castes.first()).map(|c| c.walk_speed).filter(|&w| w > 0).unwrap_or(900);
        let body = raw.and_then(|c| c.castes.first()).map(|c| c.body_size).filter(|&b| b > 0).unwrap_or(60_000);
        let prof = role.profession(&mut r);
        let p = self.world.pos_of(tile as usize);
        self.world.units.push(Unit {
            id,
            race: race.clone(),
            caste: if sex == 0 { "FEMALE".into() } else { "MALE".into() },
            name: name.clone(),
            profession: prof.into(),
            pos: p,
            flags: UnitFlags { visitor: true, ..Default::default() },
            civ_id: civ,
            hist_fig_id: -1,
            sex,
            age: 20.0 + r.below(80) as f32,
            squad_id: -1,
            squad_position: -1,
            invader_civ: -1,
            body_size: body,
            walk_speed: walk,
            blood: 5000,
            blood_max: 5000,
            ..Default::default()
        });
        self.init_units();
        let slot = self.world.units.len() - 1;
        let stay = match role {
            VisitorRole::Pilgrim => (3 + r.below(12) as u64) * DAY,
            _ => MONTH + r.below(2 * MONTH as u32) as u64,
        };
        let petition_at = match role {
            VisitorRole::Pilgrim => u64::MAX,
            _ => now + (7 + r.below(15) as u64) * DAY,
        };
        let thief = r.chance(1, 10);
        self.locs.visitors.push(Visitor { slot: slot as u32, unit: id, role, loc, until: now + stay, petition_at, leaving: false, resident: false, tries: 0, thief, loot: -1 });
        self.ai[slot].wake = now + 1;
        let lname = self.world.locations[self.locs.rt[loc].li].name.clone();
        self.locs.rep.visitors.push((now, role.label().into(), lname.clone()));
        let race_l = race.to_lowercase();
        self.announce(now, format!("{name}, a {race_l} {}, has come to visit {lname}.", role.label()), Some(p), AnnouncementKind::Info);
        Some(id)
    }

    /// [hook] Visitors (Kind::Other units owned by this module): walk to the location, stay,
    /// leave.
    pub(crate) fn visitor_think(&mut self, s: usize, now: u64) {
        let Some(vi) = self.locs.visitors.iter().position(|v| v.slot == s as u32) else { return };
        if self.world.units[self.ai[s].uidx].flags.dead {
            self.locs.visitors.remove(vi);
            return;
        }
        if self.ai[s].waiting {
            return;
        }
        if self.ai[s].path_i < self.ai[s].path.len() {
            self.step(s, now);
            return;
        }
        let here = self.ai[s].pos;
        let p = self.world.pos_of(here as usize);
        let (leaving, until, resident, loc) = {
            let v = &self.locs.visitors[vi];
            (v.leaving, v.until, v.resident, v.loc)
        };
        if !leaving && !resident && now >= until {
            self.locs.visitors[vi].leaving = true;
            self.locs.visitors[vi].tries = 0;
            self.thief_strike(vi, now); // [justice]
            self.ai[s].wake = now + 1;
            return;
        }
        if leaving {
            let at_edge = p.x == 0 || p.y == 0 || p.x as usize == self.nav.dx - 1 || p.y as usize == self.nav.dy - 1;
            if at_edge || self.locs.visitors[vi].tries > 5 {
                self.visitor_leave(vi, now);
                return;
            }
            self.locs.visitors[vi].tries += 1;
            let c = self.nav.comp_near(here as usize);
            let edges = self.society_edge_tiles(c);
            if edges.is_empty() {
                self.visitor_leave(vi, now);
                return;
            }
            let up = self.nav.xyz(here as usize);
            let e = *edges.iter().min_by_key(|&&t| (crate::manhattan(up, self.nav.xyz(t as usize)), t)).unwrap();
            self.request_path(s, Goals::one(e), now);
            if !self.ai[s].waiting && self.ai[s].path.is_empty() {
                self.ai[s].wake = now + 10;
            }
            return;
        }
        if loc >= self.locs.rt.len() {
            self.locs.visitors[vi].leaving = true;
            self.ai[s].wake = now + 1;
            return;
        }
        if self.loc_at(p) == Some(loc) {
            let b = self.world.buildings[self.locs.rt[loc].zones[0]].clone();
            let mut r = Rng::for_unit(self.seed, s, now, 0x7153);
            if r.chance(1, 3) {
                self.random_step(s, now, &mut r, Some(Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z }));
            }
            self.ai[s].wake = now + 50 + r.below(100) as u64;
            self.locs.visitors[vi].tries = 0;
            return;
        }
        let tries = {
            let v = &mut self.locs.visitors[vi];
            v.tries = v.tries.saturating_add(1);
            v.tries
        };
        let bi = self.locs.rt[loc].zones[0];
        if tries > 5 || !self.s2_go_zone(s, bi, now, 0x7154 + tries as u64) {
            if tries > 8 {
                // No way in: the visitor gives up and leaves.
                self.locs.visitors[vi].leaving = true;
            }
            self.ai[s].task = Task::Idle;
            self.ai[s].wake = now + 200;
        }
    }

    fn visitor_leave(&mut self, vi: usize, now: u64) {
        let v = self.locs.visitors.remove(vi);
        let s = v.slot as usize;
        if v.loot >= 0 {
            self.destroy_item(v.loot);
        }
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.clear();
        self.world.units[uidx].flags.inactive = true;
        self.cancel_task(s);
        self.ai[s].kind = Kind::Dead;
        self.ai[s].wake = u64::MAX;
        self.locs.rep.visitors_left += 1;
        let _ = now;
        for pe in &mut self.locs.petitions {
            if pe.unit == v.unit && pe.status == "Pending" {
                pe.status = "Expired".into();
            }
        }
    }

    // ------------------------------------------------------------------ petitions

    fn petitions_tick(&mut self, now: u64) {
        // New petitions.
        for vi in 0..self.locs.visitors.len() {
            let v = &self.locs.visitors[vi];
            if v.leaving || v.resident || now < v.petition_at {
                continue;
            }
            let kind = match v.role {
                VisitorRole::Performer | VisitorRole::Scholar => "Residency",
                VisitorRole::Mercenary | VisitorRole::MonsterSlayer => "Citizenship",
                VisitorRole::Pilgrim => continue,
            };
            self.locs.next_petition += 1;
            let (unit, role, loc) = (v.unit, v.role, v.loc);
            let name = self.world.unit(unit).map(|u| u.name.clone()).unwrap_or_default();
            let location = self.locs.rt.get(loc).map(|l| l.id).unwrap_or(-1);
            self.locs.petitions.push(Petition {
                id: self.locs.next_petition,
                unit,
                name: name.clone(),
                kind: kind.into(),
                role: role.label().into(),
                location,
                tick: now,
                status: "Pending".into(),
            });
            self.locs.visitors[vi].petition_at = u64::MAX;
            // The visitor waits for the answer.
            self.locs.visitors[vi].until = self.locs.visitors[vi].until.max(now + PETITION_TIMEOUT + DAY);
            self.locs.rep.petitions += 1;
            self.announce(now, format!("{name} has petitioned for {}.", kind.to_lowercase()), None, AnnouncementKind::Info);
        }
        // Unanswered petitions are denied.
        let expired: Vec<i32> = self.locs.petitions.iter().filter(|p| p.status == "Pending" && now >= p.tick + PETITION_TIMEOUT).map(|p| p.id).collect();
        for id in expired {
            if let Some(pi) = self.locs.petitions.iter().position(|p| p.id == id && p.unit < 0) {
                self.answer_location_petition(pi, false, true);
                continue;
            }
            self.answer_petition_impl(id, false);
            if let Some(p) = self.locs.petitions.iter_mut().find(|p| p.id == id) {
                p.status = "Expired".into();
            }
        }
    }

    pub(crate) fn answer_petition_impl(&mut self, id: i32, accept: bool) -> bool {
        let now = self.ticks;
        let Some(pi) = self.locs.petitions.iter().position(|p| p.id == id && p.status == "Pending") else { return false };
        let (unit, kind) = (self.locs.petitions[pi].unit, self.locs.petitions[pi].kind.clone());
        if unit < 0 {
            self.answer_location_petition(pi, accept, false);
            return true;
        }
        self.locs.petitions[pi].status = if accept { "Accepted".into() } else { "Denied".into() };
        let Some(vi) = self.locs.visitors.iter().position(|v| v.unit == unit) else { return true };
        let s = self.locs.visitors[vi].slot as usize;
        let name = self.world.units[self.ai[s].uidx].name.clone();
        if !accept {
            self.locs.rep.denied += 1;
            self.feel(s, if kind == "Citizenship" { "DenyCitizen" } else { "DenyResident" }, Class::NegMild, 100, -1, 0, now);
            self.locs.visitors[vi].until = now + DAY;
            return true;
        }
        self.locs.rep.accepted += 1;
        if kind == "Citizenship" {
            // Joins the fort: a citizen worker from now on.
            let v = self.locs.visitors.remove(vi);
            let uidx = self.ai[s].uidx;
            let civ = self.world.meta.civ_id;
            let u = &mut self.world.units[uidx];
            u.flags.visitor = false;
            u.flags.citizen = true;
            if civ >= 0 {
                u.civ_id = civ;
            }
            self.cancel_task(s);
            self.ai[s].kind = Kind::Worker;
            self.ai[s].last = now;
            self.ai[s].wake = now + 1;
            self.feel(s, "BecomeCitizen", Class::Pos, 200, -1, 0, now);
            let _ = v;
            self.announce(now, format!("{name} has become a citizen of the fortress."), None, AnnouncementKind::Info);
        } else {
            let v = &mut self.locs.visitors[vi];
            v.resident = true;
            let (role, loc) = (v.role, v.loc);
            self.world.units[self.ai[s].uidx].flags.resident = true;
            if let Some(l) = self.locs.rt.get(loc) {
                let occ = if role == VisitorRole::Scholar { "SCHOLAR" } else { "PERFORMER" };
                self.world.locations[l.li].occupations.push(Occupation { kind: occ.into(), unit });
            }
            self.feel(s, "BecomeResident", Class::Pos, 150, -1, 0, now);
            self.announce(now, format!("{name} has been granted residency."), None, AnnouncementKind::Info);
        }
        true
    }

    pub(crate) fn location_report_impl(&self) -> LocationReport {
        let mut r = self.locs.rep.clone();
        r.usage = self
            .locs
            .rt
            .iter()
            .map(|l| {
                let loc = &self.world.locations[l.li];
                (l.id, loc.kind.clone(), loc.name.clone(), l.passes * S2_PERIOD / DAY)
            })
            .collect();
        r
    }
}

// ---------------------------------------------------------------------- public API (round 8)

impl Sim {
    /// Make sure the round-8 society state exists (derived locations etc.).
    pub fn society2_ensure(&mut self) {
        self.loc_ensure();
    }

    /// Switch locations, visitors, justice and lifestyle on/off (default on; A/B runs).
    pub fn set_society2_enabled(&mut self, on: bool) {
        self.locs.off = !on;
    }

    /// Switch visitors on/off (default on).
    pub fn set_visitors_enabled(&mut self, on: bool) {
        self.locs.no_visitors = !on;
    }

    /// Visitor petitions (residency / citizenship), oldest first.
    pub fn petitions(&self) -> Vec<Petition> {
        self.locs.petitions.clone()
    }

    /// Accept or deny a pending petition. False if there is no such pending petition.
    pub fn answer_petition(&mut self, id: i32, accept: bool) -> bool {
        self.answer_petition_impl(id, accept)
    }

    /// Visitors on the map: (unit id, role, location id, resident).
    pub fn visitors(&self) -> Vec<(i32, VisitorRole, i32, bool)> {
        self.locs.visitors.iter().map(|v| (v.unit, v.role, self.locs.rt.get(v.loc).map(|l| l.id).unwrap_or(-1), v.resident)).collect()
    }

    /// Let a visitor of `role` arrive now for location `location_id` (UI/debug). Returns its id.
    pub fn spawn_visitor_for(&mut self, role: VisitorRole, location_id: i32) -> Option<i32> {
        self.loc_ensure();
        let loc = self.locs.rt.iter().position(|l| l.id == location_id)?;
        let now = self.ticks;
        self.spawn_visitor(role, loc, now)
    }

    /// Location of a position (location id), for the UI.
    pub fn location_at(&self, p: Pos) -> Option<i32> {
        self.loc_at(p).map(|ri| self.locs.rt[ri].id)
    }

    /// Headless / automation: the law enforcer convicts accused criminals himself (DF leaves
    /// this to the player's justice screen; default off).
    pub fn set_auto_convict(&mut self, on: bool) {
        self.justice.auto_convict = on;
    }

    /// Is the unit serving jail time?
    pub fn is_prisoner(&self, unit_id: i32) -> bool {
        self.justice_is_prisoner(unit_id)
    }

    pub fn location_report(&self) -> LocationReport {
        self.location_report_impl()
    }

    pub fn justice_report(&self) -> crate::JusticeReport {
        self.justice_report_impl()
    }

    pub fn lifestyle_report(&self) -> crate::LifestyleReport {
        self.lifestyle_report_impl()
    }

    /// Is there miasma on this tile right now?
    pub fn miasma_at(&self, p: Pos) -> bool {
        self.world.idx(p).map(|t| self.lifestyle_miasma_at(t as u32)).unwrap_or(false)
    }

    pub(crate) fn society2_hash(&self, mix: &mut dyn FnMut(u64)) {
        mix(self.world.crimes.len() as u64);
        mix(self.locs.visitors.len() as u64 | (self.locs.petitions.len() as u64) << 20 | (self.justice.prisoners.len() as u64) << 40);
        let l = &self.life.rep;
        mix(l.replacements ^ l.miasma_events << 16 ^ l.wear_steps << 32);
    }
}

#[cfg(test)]
pub(crate) mod tests;
