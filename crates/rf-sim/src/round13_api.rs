//! [contract13] Round-13 player API: weather and climate, undead and necromancers, vampires
//! and werebeasts, megabeasts/titans, diplomacy, threats, unit curse markers, cart stop filters.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs that forward to `*_impl` / hook functions in the owning
//! modules (`weather`, `undead`, `vampires`, `megabeasts`, `diplomacy`, `minecarts`); the
//! module owners replace those bodies in their own files. New fields on the structs below are
//! fine (mark them with your tag); renaming or removing is not.
//!
//! Ownership: [weather13] `weather_*`, `temperature_*`, `snow_at`, `evil_weather`,
//! `set_weather_enabled`, `force_weather`; [undead13] `set_reanimation_enabled`,
//! `reanimation_enabled`, `spawn_necromancer_siege`, `undead_report`; [vampire13]
//! `set_vampires_enabled`, `set_werebeasts_enabled`, `spawn_vampire_migrant`,
//! `spawn_werebeast_attack`, `moon_phase`, `is_full_moon`, `accuse_vampire`,
//! `vampire_report`; [mega13] `megabeasts`, `spawn_megabeast`, `set_megabeasts_enabled`,
//! `megabeast_report`; [diplo13] `diplomacy_*`, `answer_demand`, `set_liaison_requests`,
//! `land_title`, `set_diplomacy_enabled`; [perf13] `set_stop_filter`, `stop_filter`.
//! Shared (each owner fills its own part through a hook): `threats`, `unit_curse`.

use crate::Sim;
use rf_core::Pos;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------ weather [weather13]

/// What falls from (or hangs in) the sky over a surface tile (wiki Weather).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherKind {
    #[default]
    Clear,
    Cloudy,
    Rain,
    Snow,
    Fog,
    /// Evil rain of the region (blood, syndrome rain; see [`EvilWeather`]).
    EvilRain,
    /// Evil cloud drifting over the map (husk/thrall clouds, syndrome clouds).
    EvilCloud,
}

impl WeatherKind {
    /// Short label for the top bar.
    pub fn label(self) -> &'static str {
        match self {
            WeatherKind::Clear => "Clear",
            WeatherKind::Cloudy => "Cloudy",
            WeatherKind::Rain => "Raining",
            WeatherKind::Snow => "Snowing",
            WeatherKind::Fog => "Fog",
            WeatherKind::EvilRain => "Evil rain",
            WeatherKind::EvilCloud => "Evil cloud",
        }
    }
}

/// Weather and temperature of the map right now (top bar).
#[derive(Clone, Debug, Default)]
pub struct WeatherInfo {
    pub kind: WeatherKind,
    /// Surface temperature in degrees Urist (10 000 = water freezes).
    pub temperature: i32,
    /// e.g. "Early Spring" (see [`Sim::season_label`]).
    pub season: String,
    /// Surface water is frozen (winter ice).
    pub frozen: bool,
    /// Snow lies on the surface.
    pub snow_cover: bool,
    /// Weather is simulated at all (world setting and player toggle).
    pub enabled: bool,
}

/// An evil cloud or rain in progress (wiki Evil weather). Consumed by [undead13].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EvilWeather {
    pub kind: WeatherKind,
    /// Name as DF shows it, e.g. "a cloud of ..." / "blood rain".
    pub name: String,
    /// Effect token: `HUSK`, `THRALL`, `SYNDROME`, `BLOOD` (only stains), "" = unknown.
    pub effect: String,
    /// Syndrome name for `SYNDROME` effects.
    pub syndrome: String,
    /// Affected surface tile centre and radius (clouds), or None = the whole surface (rain).
    pub center: Option<Pos>,
    pub radius: i32,
}

// ------------------------------------------------------------------ diplomacy [diplo13]

/// A civilization or other entity as the diplomacy screen shows it.
#[derive(Clone, Debug, Default)]
pub struct DiplomacyEntity {
    pub id: i32,
    pub name: String,
    pub race: String,
    /// `Own`, `Peace`, `War`, `Unknown` (as `rf_core::EntityInfo::relation`).
    pub relation: String,
    /// -100 ..= 100 (see `rf_core::EntityInfo::attitude`).
    pub attitude: i32,
    pub trades: bool,
    /// Game year the current war started, None at peace.
    pub at_war_since: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeetingKind {
    /// Outpost liaison with the fort's own civilization's caravan (requests for next year).
    #[default]
    Liaison,
    /// Diplomat of another civilization (tree quotas, tribute, peace).
    Diplomat,
    /// The fort is elevated (barony, county, duchy).
    Elevation,
    /// A land holder (baron, count, duke) arrives.
    LandHolder,
}

/// A meeting with an envoy (wiki Outpost liaison, Diplomat).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meeting {
    pub id: i32,
    pub kind: MeetingKind,
    pub entity_id: i32,
    /// Envoy unit on the map, -1 if none.
    pub envoy: i32,
    /// Citizen meeting the envoy (noble), -1 if none yet.
    pub host: i32,
    /// `Waiting`, `InProgress`, `Done`.
    pub state: String,
    pub tick: u64,
    pub text: String,
    /// Demand ids raised in this meeting.
    pub demands: Vec<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DemandKind {
    /// Liaison asks what the fort wants next year / offers goods.
    #[default]
    Request,
    /// Elves: no more than `amount` trees felled per year.
    TreeQuota,
    /// Humans (or others): pay tribute worth `amount`.
    Tribute,
    /// Peace offer of a civilization at war.
    Peace,
}

/// A demand or offer awaiting the player's answer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Demand {
    pub id: i32,
    pub kind: DemandKind,
    pub entity_id: i32,
    pub text: String,
    pub amount: i64,
    /// Tick by which the demand must be met (0 = no deadline).
    pub due: u64,
    /// None = unanswered, Some(true) accepted, Some(false) refused.
    pub answer: Option<bool>,
}

// ------------------------------------------------------------------ threats (shared)

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatKind {
    /// [mega13] dragon, hydra, roc, bronze colossus.
    #[default]
    Megabeast,
    /// [mega13] ettin, cyclops, giant, minotaur ...
    SemiMegabeast,
    /// [mega13] generated titan.
    Titan,
    /// [mega13] forgotten beast in the caverns (existing `caverns` module).
    ForgottenBeast,
    /// [mega13] regular siege of a civilization at war (existing `invasion` module).
    Siege,
    /// [undead13] risen corpses on the map.
    UndeadHorde,
    /// [undead13] necromancer siege with undead.
    NecromancerSiege,
    /// [vampire13] werebeast on the map (transformed or attacking).
    Werebeast,
    /// [vampire13] unmasked vampire still free.
    Vampire,
}

/// One active threat (Threats panel, alerts).
#[derive(Clone, Debug, Default)]
pub struct Threat {
    pub kind: ThreatKind,
    pub name: String,
    /// Creature token, e.g. `DRAGON`, `ZOMBIE`, `WEREWOLF`.
    pub race: String,
    /// Unit ids taking part (alive).
    pub units: Vec<i32>,
    /// Where it is (leader / first unit), if on the map.
    pub pos: Option<Pos>,
    pub since: u64,
    /// Entity behind it (civ, necromancer tower), -1 if none.
    pub entity_id: i32,
}

// ------------------------------------------------------------------ unit markers (shared)

/// Curse / undead markers of a unit (unit sheet).
#[derive(Clone, Debug, Default)]
pub struct UnitCurse {
    /// [undead13] `rf_core::Unit::undead`.
    pub undead: bool,
    /// [undead13] Intelligent undead (keeps its mind, e.g. a necromancer's servant).
    pub intelligent_undead: bool,
    /// [undead13] Necromancer / husk / thrall (from `rf_core::Unit::curse`).
    pub necromancer: bool,
    pub husk: bool,
    pub thrall: bool,
    /// [vampire13] Is a vampire (true identity; the UI shows it only when unmasked).
    pub vampire: bool,
    /// [vampire13] Suspected by the justice investigation.
    pub vampire_suspected: bool,
    /// [vampire13] Convicted / revealed.
    pub vampire_unmasked: bool,
    /// [vampire13] Carries the werebeast curse; `were_race` = its other form.
    pub werebeast: bool,
    pub were_race: String,
    /// [vampire13] Currently in were-form.
    pub transformed: bool,
    /// [mega13] Is a megabeast / semi-megabeast / titan.
    pub megabeast: bool,
}

/// [undead13] Counters for tests and the bench.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UndeadReport {
    pub reanimated: u64,
    pub destroyed: u64,
    pub necro_sieges: u64,
    pub husks: u64,
    pub thralls: u64,
}

/// [vampire13] Counters for tests and the bench.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VampireReport {
    pub vampires_arrived: u64,
    pub feedings: u64,
    pub drained: u64,
    pub unmasked: u64,
    pub were_attacks: u64,
    pub transformations: u64,
    pub infected: u64,
}

/// [mega13] A megabeast, semi-megabeast or titan that came (alive or slain).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MegabeastInfo {
    pub unit_id: i32,
    pub name: String,
    pub race: String,
    pub kind: ThreatKind,
    pub arrived: u64,
    pub alive: bool,
    pub kills: u32,
    /// Unit that dealt the killing blow, -1 if alive or unknown.
    pub slain_by: i32,
    /// Caught in a cage trap (still alive, no longer a threat). // [mega13]
    pub caught: bool, // [mega13]
}

/// [mega13] Counters for tests and the bench.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MegabeastReport {
    pub attacks: u64,
    pub slain: u64,
    pub fire_breaths: u64,
    pub buildings_destroyed: u64,
}

/// [diplo13] Counters for tests and the bench.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiplomacyReport {
    pub meetings: u64,
    pub demands: u64,
    pub accepted: u64,
    pub refused: u64,
    pub wars_declared: u64,
    pub peace_made: u64,
    pub elevations: u64,
}

/// [weather13] Counters for tests and the bench.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WeatherReport {
    pub rain_ticks: u64,
    pub snow_ticks: u64,
    pub fog_ticks: u64,
    pub evil_ticks: u64,
    pub freezes: u64,
    pub thaws: u64,
    pub frozen_deaths: u64,
}

impl Sim {
    // -------------------------------------------------------------- weather [weather13]

    /// Weather, temperature and season now.
    pub fn weather_summary(&self) -> WeatherInfo {
        self.weather_summary_impl()
    }

    /// Weather over a tile (underground / inside tiles: `Clear`).
    pub fn weather_at(&self, p: Pos) -> WeatherKind {
        self.weather_at_impl(p)
    }

    /// Temperature of a tile in degrees Urist.
    pub fn temperature_at(&self, p: Pos) -> i32 {
        self.temperature_at_impl(p)
    }

    /// Snow lies on the tile.
    pub fn snow_at(&self, p: Pos) -> bool {
        self.snow_at_impl(p)
    }

    /// The evil cloud / rain in progress, if any.
    pub fn evil_weather(&self) -> Option<EvilWeather> {
        self.evil_weather_impl()
    }

    /// Player toggle (default: the world's `settings.weather`).
    pub fn set_weather_enabled(&mut self, on: bool) {
        self.set_weather_enabled_impl(on)
    }

    /// Debug/tests: force a weather for `ticks` ticks.
    pub fn force_weather(&mut self, kind: WeatherKind, ticks: u64) {
        self.force_weather_impl(kind, ticks)
    }

    pub fn weather_report(&self) -> WeatherReport {
        self.weather_report_impl()
    }

    /// "Early Spring", "Mid Summer", "Late Winter" ... (DF top bar). [contract13] final.
    pub fn season_label(&self) -> String {
        let m = self.world.calendar.month().clamp(0, 11);
        let part = ["Early", "Mid", "Late"][(m % 3) as usize];
        format!("{part} {}", self.world.calendar.season_name())
    }

    /// Degrees Urist to degrees Celsius (wiki Temperature: U = F + 9968). [contract13] final.
    pub fn urist_to_celsius(u: i32) -> i32 {
        ((u - 10_000) * 5).div_euclid(9)
    }

    // -------------------------------------------------------------- undead [undead13]

    /// Corpses rising in reanimating regions (default: the world's `climate.reanimating`).
    pub fn set_reanimation_enabled(&mut self, on: bool) {
        self.set_reanimation_enabled_impl(on)
    }

    pub fn reanimation_enabled(&self) -> bool {
        self.reanimation_enabled_impl()
    }

    /// Debug/tests: a necromancer siege with `n` undead; returns the units spawned.
    pub fn spawn_necromancer_siege(&mut self, n: usize) -> usize {
        self.spawn_necromancer_siege_impl(n)
    }

    pub fn undead_report(&self) -> UndeadReport {
        self.undead_report_impl()
    }

    // -------------------------------------------------------------- vampires [vampire13]

    pub fn set_vampires_enabled(&mut self, on: bool) {
        self.set_vampires_enabled_impl(on)
    }

    pub fn set_werebeasts_enabled(&mut self, on: bool) {
        self.set_werebeasts_enabled_impl(on)
    }

    /// Debug/tests: a vampire arrives among the next migrants (or now); its unit id.
    pub fn spawn_vampire_migrant(&mut self) -> Option<i32> {
        self.spawn_vampire_migrant_impl()
    }

    /// Debug/tests: a werebeast attacks; its unit id.
    pub fn spawn_werebeast_attack(&mut self) -> Option<i32> {
        self.spawn_werebeast_attack_impl()
    }

    /// Moon phase 0..=27 (0 = new moon, 14 = full moon).
    pub fn moon_phase(&self) -> u8 {
        self.moon_phase_impl()
    }

    pub fn is_full_moon(&self) -> bool {
        self.moon_phase() == 14
    }

    /// Accuse a unit of vampirism through justice; true if it was a vampire (unmasked).
    pub fn accuse_vampire(&mut self, unit_id: i32) -> bool {
        self.accuse_vampire_impl(unit_id)
    }

    pub fn vampire_report(&self) -> VampireReport {
        self.vampire_report_impl()
    }

    // -------------------------------------------------------------- megabeasts [mega13]

    /// Megabeasts, semi-megabeasts and titans that came (alive and slain).
    pub fn megabeasts(&self) -> Vec<MegabeastInfo> {
        self.megabeasts_impl()
    }

    /// Debug/tests: a megabeast of `race` (e.g. `DRAGON`, "" = pick one) attacks; its unit id.
    pub fn spawn_megabeast(&mut self, race: &str) -> Option<i32> {
        self.spawn_megabeast_impl(race)
    }

    pub fn set_megabeasts_enabled(&mut self, on: bool) {
        self.set_megabeasts_enabled_impl(on)
    }

    pub fn megabeast_report(&self) -> MegabeastReport {
        self.megabeast_report_impl()
    }

    // -------------------------------------------------------------- diplomacy [diplo13]

    pub fn diplomacy_entities(&self) -> Vec<DiplomacyEntity> {
        self.diplomacy_entities_impl()
    }

    pub fn diplomacy_meetings(&self) -> Vec<Meeting> {
        self.diplomacy_meetings_impl()
    }

    /// Demands and offers (answered ones included; filter on `answer`).
    pub fn diplomacy_demands(&self) -> Vec<Demand> {
        self.diplomacy_demands_impl()
    }

    /// Accept or refuse a demand; false if there is no such open demand.
    pub fn answer_demand(&mut self, demand: i32, accept: bool) -> bool {
        self.answer_demand_impl(demand, accept)
    }

    /// Requests for next year's caravan of an entity (item kinds as DF-like text).
    pub fn set_liaison_requests(&mut self, entity: i32, requests: &[&str]) {
        self.set_liaison_requests_impl(entity, requests)
    }

    /// "" (outpost), "Barony", "County", "Duchy".
    pub fn land_title(&self) -> &'static str {
        self.land_title_impl()
    }

    pub fn set_diplomacy_enabled(&mut self, on: bool) {
        self.set_diplomacy_enabled_impl(on)
    }

    pub fn diplomacy_report(&self) -> DiplomacyReport {
        self.diplomacy_report_impl()
    }

    // -------------------------------------------------------------- shared

    /// Active threats for the Threats panel and alerts. Each module appends its own.
    pub fn threats(&self) -> Vec<Threat> {
        let mut v = Vec::new();
        self.megabeast_threats(&mut v); // [mega13] megabeasts, titans, forgotten beasts, sieges
        self.undead_threats(&mut v); // [undead13] undead hordes, necromancer sieges
        self.vampire_threats(&mut v); // [vampire13] werebeasts, unmasked vampires
        v
    }

    /// Curse / undead markers of a unit (None: no such unit).
    pub fn unit_curse(&self, unit_id: i32) -> Option<UnitCurse> {
        let u = self.world.units.iter().find(|u| u.id == unit_id)?;
        let mut f = UnitCurse {
            undead: u.undead,
            necromancer: u.curse == "NECROMANCER",
            husk: u.curse == "HUSK",
            thrall: u.curse == "THRALL",
            vampire: u.curse == "VAMPIRE",
            werebeast: u.curse == "WEREBEAST",
            were_race: u.curse_race.clone(),
            ..Default::default()
        };
        self.undead_unit_flags(unit_id, &mut f); // [undead13]
        self.vampire_unit_flags(unit_id, &mut f); // [vampire13]
        self.megabeast_unit_flags(unit_id, &mut f); // [mega13]
        Some(f)
    }

    // -------------------------------------------------------------- minecarts [perf13]

    /// Item filter of a route stop (stockpile filter syntax; empty = everything).
    pub fn set_stop_filter(&mut self, route: i32, stop: usize, filter: &[&str]) {
        if let Some(s) = self.world.haul_routes.iter_mut().find(|r| r.id == route).and_then(|r| r.stops.get_mut(stop)) {
            s.filter = filter.iter().map(|f| f.to_string()).collect();
        }
    }

    pub fn stop_filter(&self, route: i32, stop: usize) -> Vec<String> {
        self.world.haul_routes.iter().find(|r| r.id == route).and_then(|r| r.stops.get(stop)).map(|s| s.filter.clone()).unwrap_or_default()
    }
}
