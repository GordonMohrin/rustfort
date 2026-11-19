# [nobles15] Nobles, room requirements, mandates, demands (mechanics spec for v0.15)

Implementer's spec. Every rule has its source: **raws** = vanilla `entity_default.txt`
(`ENTITY:MOUNTAIN`, DF 53.16 install, read only), **w:** = dwarffortresswiki.org page (v50/53.17
pages unless noted), **export** = the reference fort Tileshtulon/Trussroads (`export/tileshtulon`
= year 279, `export/tileshtulon-r6`/`-r7` = year 280 with `crimes.json`/`punishments.json`).
Classes: **OK** (stated by a source), **OBSERVED** (derived from export data), **ESTIMATE**
(our guess, constant must be named and documented), **MISSING** (deliberately not done).

Tag every added line in shared files with `// [nobles15]`. New code goes into:

| file | content |
|---|---|
| `crates/rf-sim/src/nobles15.rs` | state `Nobles15`, the passes, requirement evaluation, mandates, demands, bookkeeper, manager validation |
| `crates/rf-sim/src/nobles15/tests.rs` | unit tests (pattern of `justice/tests.rs`) |
| `crates/rf-sim/src/nobles15_api.rs` | public UI-facing API + report types |
| `crates/rf-app/src/ui/nobles15.rs` | requirement/mandate/demand panel (extends the round-4 nobles screen via one call) |

Out of scope (other agents): liaison meetings, elevation to barony/county/duchy, monarch
arrival (diplomacy [diplo13]); read the title only through `Sim::land_title()` (stub returns
`""`, later `"Barony"`/`"County"`/`"Duchy"`). Undead, vampires, megabeasts, weather, minecarts.

---

## 0. What exists today (read before changing anything)

| area | where | what it does | problem |
|---|---|---|---|
| positions | `rf_core::NoblePosition` (`society.rs`), `World::nobles` from `nobles.json` | token, name, holder, entity, mandate texts | the export also writes `mandate_max`, `demand_max`, `required_*`, `flags`, `responsibilities`, `precedence`, `mandates_detail` - serde drops them |
| elections | `population.rs` `elect`, `nobles_daily`, `nobles_yearly` | EL -> MAYOR at 50 citizens (token renamed in place), yearly re-election; score = social skills x10 + 20/friend, incumbent x1.5 | thought `BecomeNoble` instead of `Elected`/`Reelected`; no re-election announcement; old mayor's mandates survive a lost election |
| mandates | `population.rs` `Mandate`, `mandates_season`, `nobles_daily`, `check_export_bans`, `parse_mandate`, `MANDATE_ITEMS` | 1 per noble, 1 in 3 per season, deadline 3 months, fulfilment by `count_kind` delta over ALL items (O(items) per mandate per day), export ban by description substring | wrong limits, wrong deadline range, caravan goods count as made, no punishment numbers, MandateIgnored/Met mapping, EXPEDITION_LEADER issues none (correct) but MANDATE_MAX ignored |
| justice hook | `justice.rs` `justice_mandate_missed` | random citizen culprit, noble witness, crime `ProductionOrderViolation`/`ExportViolation`; sentence from `sentence(kind)` = 14 days or 5 strikes | DF uses the mandate's own punishment (strikes AND jail), auto-sentenced, PUNISHMENT_EXEMPTION ignored |
| room value | `social.rs` `Room`, `social_revalue_rooms`, `room_tile_value`, `room_value(zone_id)`, tiers 0/100/250/500/1000/1500/2500/10000 | Bedroom, Dormitory, DiningHall, Meeting* zones | `Office` and `Tomb` zones are `RoomKind::Other` -> no value; nobles get commoner thoughts |
| zones | `build_api.rs` `ZONE_KINDS` | player zone kinds | no `Office`; no API to assign a zone owner |
| trade | `trade.rs` `shown_value`, `broker_unit` | displayed value error (25 - 2 x APPRAISAL) % | skill 0 still shows values |
| work orders | `production.rs` (orders created `validated: true`, ~1033), `skills.rs` `skills_order_approved` (+100 ORGANIZATION per approval) | | no manager/office validation |
| stocks | rf-app `topbar.rs count_stocks` (live counts) | | no bookkeeper |
| thoughts | `social/captions.rs` has all DF names needed: `MandateIgnored`, `MandateDeadlineMet`, `MandateDeadlineMissed`, `Demands`, `Elected`, `Reelected`, `OfficeQuality`, `MeetingInBedroom`, `MeetingInDiningRoom`, `NoRooms`, `TombQuality`, `TombLack`, `RoomPretension`, `MoreChests`, `MoreCabinets`, `MoreWeaponRacks`, `MoreArmorStands`, `NoPunishment`, `DelayedPunishment`, `PunishmentDelayed`, `ImproperPunishment`, `ReceivedComplaint` (caption "[multiple]") | `Sim::feel(s, thought, Class, strength, sub, severity, now)` | |
| UI | `rf-app/src/ui/nobles.rs` | positions, appoint, mandate texts | no requirements/demands |

Time units: 1 day = 1 200 ticks, 1 month = 33 600, 1 season = 100 800, 1 year = 403 200
(`rf_core::TICKS_PER_*`).

---

## 1. Position data (requirements, limits, flags)

### DF rule (raws, OK)

Dwarven positions (`ENTITY:MOUNTAIN`). `-` = 0 / absent.

| token | office | bedroom | dining | tomb | boxes | cabinets | racks | stands | DEMAND_MAX | MANDATE_MAX | flags of interest |
|---|---|---|---|---|---|---|---|---|---|---|---|
| MONARCH | 10000 | 10000 | 10000 | 10000 | 10 | 5 | 5 | 5 | 10 | 5 | SLEEP_PRETENSION, PUNISHMENT_EXEMPTION, MENIAL_WORK_EXEMPTION(+_SPOUSE), SUCCESSION:BY_HEIR |
| DUKE | 2500 | 2500 | 2500 | 2500 | 5 | 3 | 3 | 3 | 5 | 3 | LAND_HOLDER:3, as monarch |
| COUNT | 1500 | 1500 | 1500 | 1500 | 3 | 2 | 2 | 2 | 3 | 2 | LAND_HOLDER:2, REPLACED_BY:DUKE |
| BARON | 500 | 500 | 500 | 500 | 2 | 1 | 1 | 1 | 2 | 1 | LAND_HOLDER:1, REPLACED_BY:COUNT |
| MAYOR | 500 | 500 | 500 | - | 2 | 1 | 1 | 1 | 2 | 1 | ELECTED, REQUIRES_POPULATION:50 (no pretension, no exemption) |
| GENERAL | 500 | 250 | 250 | 1 | 2 | 1 | 3 | 3 | 2 | - | SLEEP_PRETENSION, PUNISHMENT_EXEMPTION |
| LIEUTENANT | 100 | 100 | 100 | - | 1 | 1 | 2 | 2 | 1 | - | |
| CAPTAIN | 1 | 1 | 1 | - | 1 | 1 | 1 | 1 | - | - | |
| OUTPOST_LIAISON, DIPLOMAT | 1500 | 1500 | 1500 | - | 3 | 2 | 2 | 2 | 3 | - | SLEEP_PRETENSION, PUNISHMENT_EXEMPTION |
| CAPTAIN_OF_THE_GUARD | 250 | 250 | 250 | - | 1 | 1 | 1 | 1 | - | - | REQUIRES_POPULATION:50, REQUIRES_MARKET, SQUAD |
| SHERIFF | 100 | 100 | 100 | - | 1 | 1 | 1 | 1 | - | - | REPLACED_BY:CAPTAIN_OF_THE_GUARD |
| DUNGEON_MASTER | 250 | 250 | 250 | - | 1 | 1 | 1 | 1 | - | - | REQUIRES_POPULATION:50, APPOINTED_BY:MAYOR only |
| MANAGER | 1 | - | - | - | - | - | - | - | - | - | REQUIRES_MARKET; raws DESCRIPTION: at some population the manager must work in an office to validate orders |
| BOOKKEEPER | 1 | - | - | - | - | - | - | - | - | - | REQUIRES_MARKET |
| CHAMPION | - | - | - | - | - | - | - | - | 1 | - | PUNISHMENT_EXEMPTION, appointed by BARON/COUNT/DUKE |
| EXPEDITION_LEADER, BROKER, CHIEF_MEDICAL_DWARF, HAMMERER, MILITIA_*, MESSENGER | - | - | - | - | - | - | - | - | - | - | |

w:Noble (V50 body) gives the same table as tier names: 500 "Decent", 1500 "Great"/"Throne
Room"/"Mausoleum", 2500 "Grand"/"Opulent Throne Room"/"Grand Mausoleum", 10000 "Royal"; demand/
mandate maxima Baron 2/1, Count 3/2, Duke 5/3, Mayor 2/1, General 2/0, Diplomat/Liaison 3/0 (the
page says Monarch 9 demands, raws say 10: **use the raws**). w:Noble: "Immune dwarves aren't
punished for any crimes" (PUNISHMENT_EXEMPTION). w:Noble: the "lazy" (MENIAL_WORK_EXEMPTION)
flag is "Currently non-functional" -> **do not implement** menial exemption (MISSING by design).

The export has these numbers per assignment (`nobles.json` extras `mandate_max`, `demand_max`,
`required_office/bedroom/dining/tomb`, `required_boxes/cabinets/racks/stands`, `flags`,
`responsibilities`, `precedence`), verified identical to the raws above for Tileshtulon (e.g.
MAYOR 500/500/500/0, boxes 2, mandate_max 1, demand_max 2).

### Design

1. **rf-core (additive, `#[serde(default)]`)**: add to `NoblePosition` exactly the export key
   names so `read_json("nobles.json")` fills them with no importer change:
   `mandate_max: i32, demand_max: i32, required_office: i32, required_bedroom: i32,
   required_dining: i32, required_tomb: i32, required_boxes: i32, required_cabinets: i32,
   required_racks: i32, required_stands: i32, flags: Vec<String>, responsibilities: Vec<String>,
   precedence: i32` and `mandates_detail: Vec<MandateDetail>` (new rf-core struct, fields =
   the exporter's: `text, mode, item_type, item_subtype, mat, amount_total, amount_remaining,
   timeout_counter, timeout_limit, punishment_hammerstrikes, punishment_prison_time`). Struct
   literals of `NoblePosition` (population.rs `appoint_impl`, tests, `rf-sim/examples/society*.rs`,
   rf-import) need `..Default::default()` with `// [nobles15]`. Also add
   `punishment_hammerstrikes`/`punishment_prison_time` to `rf_import::MandateInfo` (additive) and
   document both in `docs/EXPORT_FORMAT.md` (already exported, only newly read).
2. **Fallback table** `nobles15::VANILLA: &[PosReq]` = the table above (numbers only, no DF
   text). `fn pos_req(&NoblePosition) -> PosReq`: export values if any `required_*`/`*_max`
   field is non-zero or `flags` non-empty, else the table by token (BARONESS/COUNTESS/DUCHESS/
   KING/QUEEN map to the gender-neutral token). Precedence fallback: raws PRECEDENCE
   (MONARCH 1, DUKE 20, COUNT 30, BARON 40, GENERAL 50, MAYOR 60, CHAMPION 65, DIPLOMAT 70,
   LIAISON 80, LIEUTENANT 100, CAPTAIN_OF_THE_GUARD 105, DUNGEON_MASTER 108, EL 110,
   MILITIA_COMMANDER 120, SHERIFF 130, HAMMERER 150, MANAGER 160, CMD 165, BROKER 170,
   BOOKKEEPER 180, CAPTAIN 190, MILITIA_CAPTAIN 200, MESSENGER 250; lower = higher rank).
3. A unit holding several positions uses the **maximum** of each requirement and the **sum** of
   demand/mandate maxima? -> ESTIMATE: maximum per field (DF evaluates per position; the
   reference mayor is also the EL, whose values are 0, so max == mayor). Cache per holder:
   `Nobles15::holders: Vec<HolderRt>` rebuilt when `World::nobles` changes (cheap: compare a
   hash of (token, holder) pairs once per pass).

---

## 2. Rooms of a noble and room value

### DF rule

- v50 rooms are zones assigned to a unit: Bedroom, Office (civzone type `Office`, the old
  "study"), DiningHall, Tomb (export: civzone subtypes `Bedroom`, `Office`, `DiningHall`,
  `Tomb`; owner = `assigned_unit_id`). OK (export: Tileshtulon has 1 Office zone, owned by the
  manager, named "Manager-Buero").
- Tier thresholds (w:Room / w:Zone quality and value; same numbers as `social.rs`): 0 meager,
  100 modest, 250 ordinary, 500 decent, 1000 fine, 1500 great, 2500 grand, 10000 royal. Names:
  bedroom "Meager Quarters" .. "Royal Bedroom"; office "Meager Office", "Modest Office",
  "Office", "Decent Office", "Splendid Office", "Throne Room", "Opulent Throne Room", "Royal
  Throne Room"; dining "... Dining Room" / "Great/Grand/Royal Dining Hall"; tomb "Grave"?,
  "Burial Chamber"?, ..., "Mausoleum" (1500), "Grand Mausoleum" (2500), "Royal Mausoleum".
  OK for thresholds and the names quoted on w:Noble; the low tomb names are ESTIMATE (UI text
  only).
- A requirement is met when an assigned room of that kind has value >= the required value; an
  office requirement of 1 means "any office". OK (w:Noble: "fulfilled or pending").
- Furniture counts (boxes = built chests `Box`, cabinets `Cabinet`, racks `Weaponrack`, stands
  `Armorstand`) count built furniture inside the noble's assigned rooms. ESTIMATE which rooms
  (DF2014: the bedroom; we count bedroom + office, the union, each building once).
- Room value = the existing `social.rs` value (furniture + floor + bordering walls + engravings).
  Do not re-implement it: extract the per-room loop body of `social_revalue_rooms` into
  `pub(crate) fn zone_value(&self, b: &Building) -> (i32 /*furniture*/, i32 /*total*/)` (pure,
  `&self`) or call it on a temporary `Room`; mark the refactor lines `// [nobles15]`. Must not
  change existing room values (hash-neutral).

### Gap / design

- Add `"Office"` to `build_api::ZONE_KINDS` (append at the end -> existing indices unchanged).
- New API `Sim::assign_zone(zone_id, unit_id)` (`nobles15_api.rs`; `unit_id` -1 = free): sets
  `Building::owner`, then `self.init_social()`-equivalent invalidation (`social.known_buildings
  = usize::MAX` or the existing rebuild trigger) so bedroom beds get the owner. Validate zone
  kind in {Bedroom, Office, DiningHall, Tomb}. DF allows one owner per zone and several zones per
  unit (ESTIMATE: we allow it; the requirement uses the best room of each kind).
- `Nobles15::rooms: Vec<NobleRoom { zone: i32, owner: i32, kind: u8, value: i32, furn: [u8; 4] }>`
  (`#[serde(skip)]` cache, rebuilt in `after_exact_load` and when `world.buildings.len()` or
  `detail_epoch()` changes, at most once per nobles pass). Only zones whose owner holds a
  position with any requirement are valued -> a handful of zones, cheap.
- `RequirementStatus { kind, required, best_value, tier_name, state: None|Missing|Low|Met }` per
  holder; colours as DF: red = a required room missing, yellow = present but too low or a
  furniture count short, green = all met (w:Noble menu "fulfilled or pending"; colour rule from
  the older w:Nobles screen: ESTIMATE that v50 uses the same).

---

## 3. Room thoughts of nobles (the only real "unmet requirement" effect)

### Evidence (OBSERVED, export year 279)

The mayor (unit 13937, REQUIRED_BEDROOM/DINING/OFFICE 500, boxes 2, cabinets 1, racks 1,
stands 1) owns only a bedroom (zone 11412, 3x3: bed, table, chair, 1 cabinet, 1 chest; no rack,
no stand), **no office, no own dining room**. His ~200 thoughts of year 279 contain:
`BedroomQuality` severity **-1** (x2), `DiningQuality` severity **3** (commoners eating in the
same hall: severity 5, 129 times), `NoDining` (x1), `MeetingInDiningRoom` (x2, ticks 14 263 and
87 284), `ReceivedComplaint` (x2), `MandateIgnored` (x1, tick 28 520), `DelayedPunishment` (x1).
**No** `MoreChests`/`MoreWeaponRacks`/`MoreArmorStands`, `NoRooms`, `Demands`,
`RoomPretension` in either export (r7: 0 of 21 863 emotions).

Conclusions:
- DF 53.16 does **not** emit the `More*` furniture thoughts for an unmet mayor (2 shortfalls,
  one year) -> furniture counts are status only. `More*` thoughts: MISSING by design (constant
  `MORE_FURNITURE_THOUGHTS = false`, keep the code path for a later calibration).
- Room-quality thoughts of a noble are **shifted by the requirement**. With the round-8 finding
  that DF's room thought severity is the tier index (FIDELITY "Known gaps" 1: 54/97 exact), the
  grand/great hall that commoners rate 5 is rated 3 by the mayor (required tier 3 = decent):
  shift 2. His modest bedroom (tier 1, ESTIMATE until measured) rated -1: shift 2. Hypothesis
  **H1** (OBSERVED, 2 points): `severity = tier - max(0, req_tier - 1)`, where `req_tier` = tier
  index of the required value (500 -> 3, 1500 -> 5, 2500 -> 6, 10000 -> 7, 100/250 -> 1/2),
  valence negative when `tier < req_tier`, positive when `tier >= req_tier`. Commoners and
  positions without that requirement: shift 0 (unchanged behaviour).
  Test H1 in a unit test against the real export: `room_value(11412)` must give tier 1 for -1
  to hold; if Rustfort values it at tier 2, document the residual (the r8 spread is one tier).
- The mayor eats in the common hall and gets a (shifted) `DiningQuality`, so a missing own
  dining room is **not** a separate thought. `NoDining` stays the existing no-table rule.

### Design

- Hook: in `social.rs` at the `BedroomQuality` (~line 1195) and `DiningQuality` (~1104) sites,
  one tagged line `let tier = tier - self.nobles15_room_shift(s, RoomKindReq::Bedroom);` plus
  sign handling inside a helper `nobles15_room_feel(s, thought, tier, now) -> bool` that emits
  the noble variant (strength: negative 60 + 30 x deficit, positive as existing; ESTIMATE) and
  returns true so the commoner path is skipped. Behaviour change only for holders with a
  requirement -> hash changes on forts with a mayor/sheriff/etc. (the reference fort: yes).
  `nobles15_room_shift` is O(1): `HolderRt` per slot (`Vec<u8>` indexed by slot, 0 = none).
- **Office meetings** (OK by the thought names + OBSERVED rate): a holder with responsibility
  `MEET_WORKERS` (EL, mayor) holds meetings: petitions (`locations.rs` petition answered: one
  tagged hook `self.nobles15_meeting(noble_slot, now)`) and complaints. Where: own office ->
  `OfficeQuality` (severity = shifted tier as above); else own dining room or the hall he eats in
  -> `MeetingInDiningRoom`; else own bedroom -> `MeetingInBedroom`; else `NoRooms`. Complaints
  (OBSERVED 2/year for ~150 citizens): each day, 1 in `COMPLAINT_IN` (ESTIMATE 50 000 / citizens
  so ~2 per year at 150) a stressed citizen (stress >= 25 000) visits: mayor gets
  `ReceivedComplaint` + a meeting thought. No walking in v0.15 (MISSING: the visitor's trip);
  the meeting is resolved instantly at the pass.
- **Tombs** (OK by caption "after gaining another year"): holders with `required_tomb > 0` (and
  only they, ESTIMATE) think on their birthday: own tomb zone -> `TombQuality` (shifted tier),
  none -> `TombLack`. Birthday = `Unit::age` crossing an integer (check in the daily pass; store
  `last_age_int` per holder slot in `HolderRt`).
- **Sleep pretension** (raws token, w:Noble silent; ESTIMATE behaviour): holder with
  `SLEEP_PRETENSION` sleeping in his own bedroom: if any citizen of lower precedence (no position
  or larger precedence number) owns a bedroom with a higher tier -> `RoomPretension` (Neg,
  strength 100, at most once per 7 days per holder). The vanilla mayor has no pretension, so the
  reference fort never shows it (consistent). Cache `best_commoner_bedroom_tier` per nobles pass.

---

## 4. Mandates

### DF rules

| rule | source | class |
|---|---|---|
| Kinds: production ("Make N <items>") and export ban ("Ban export of <items>"); export text `Guild: ...` exists (guild mandates) | w:Mandate, export `mandates_detail.mode` = Make/Export/Guild | OK; Guild MISSING (no guild agreements) |
| Who: positions with MANDATE_MAX > 0 (mayor 1, baron 1, count 2, duke 3, monarch 5); "Starting nobles such as the expedition leader will not make mandates" | raws, w:Mandate | OK |
| Only nobles "with a preference for a specific item" mandate it | w:Mandate | OK rule, data MISSING (see preferences) |
| Cooldown: no new mandate or demand "if one was made recently"; gap set by the difficulty setting `MANDATE_PERIOD` (default 1000), `DEMAND_PERIOD` (500) | w:Mandate; export `settings.json` other: `MANDATE_PERIOD=1000`, `DEMAND_PERIOD=500` | OK rule / ESTIMATE unit |
| Issue blocked if the noble is dead, off map, caged, insane or in a mood, incapacitated, unconscious/stunned/winded/nauseous/feverish/bleeding, not a fort member | w:Mandate | OK |
| Production deadline 3-6 months | w:Mandate | OK |
| Export: `timeout_limit` 10 762 with `timeout_counter` 5 552 | export (mayor, "Make 3 spears") | OBSERVED: counter units are 10 ticks (see punishments below), so 107 620 ticks = 3.2 months, inside 3-6 months |
| Requested goods by type, not material; caravan goods do not count | w:Mandate | OK |
| Met: noble happy thought | w:Mandate | OK; thought `MandateDeadlineMet` (caption "to have a mandate deadline met") |
| Missed: noble unhappy thought + one or more dwarves sentenced for "violation of production order"; the dwarf "tends to have a related skill, but random dwarves can be chosen" | w:Mandate | OK |
| The miss thought is `MandateIgnored`, the crime has the same tick, victim = the noble, culprit unrelated (bookkeeper blamed for spears) | export: mayor `MandateIgnored` at 279/28 520; crime 1544 `ProductionOrderViolation` site_tick 28 520, victim 13937, criminal 14254 (bookkeeper) | OBSERVED |
| No dwarf can be sentenced (all nobles) or punished (no officer) -> another unhappy thought | w:Mandate | OK; thought `NoPunishment` ("that nobody could be punished for a failure") |
| Sentence = the mandate's punishment: hammerstrikes 1-5 uniform AND jail 1-4 months | export r7: 60 crimes, strikes {1:14,2:7,3:15,4:10,5:14}, prison_time {1:17,2:11,3:17,4:15}, all 60 `sentenced`+`discovered`, discovered at the crime tick, no witnesses | OBSERVED (ranges), crimes are auto-sentenced |
| prison_time unit = months | export r6: crime 1544 "Hammerstrikes 1, Jail 3", running punishment `prison_counter` 10 080 with 0 strikes left = 10 080 x 10 ticks = 100 800 = 3 months | OBSERVED |
| Order: strikes first, then jail | same record (strikes already 0, jail pending, `time_to_assign` 2 592) | OBSERVED |
| Gaps between a noble's violations >= 1.17 seasons (46 violations of one noble over 48 years, median ~3.3 seasons) | export r7 | OBSERVED (fits deadline 3.2-6 months + cooldown) |
| Export ban lasts about half a year, then ends; trading a banned item is a crime for each hauler who brought it to the depot, committed when the item leaves the map; items in bins: no crime, noble unhappy (nobody punished); melting is fine; banned items purple in the trade window | w:Mandate | OK |
| All mandates of a noble end when he dies; a mayor voted out loses his mandates at once | w:Mandate | OK |
| Announcements: "<name>, <position> has mandated the construction of certain goods." / "... has imposed a ban on certain exports." | DF announcements from memory | ESTIMATE wording (keep short, our own text is fine) |

### Design

```rust
// nobles15.rs
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct MandateRt {
    pub id: i32,
    pub noble: i32,            // unit id of the issuer
    pub position: String,      // token
    pub export_ban: bool,      // false = production
    pub item_type: String,     // DF item type (`WEAPON`, `CHAIR`, ...)
    pub item_subtype: String,  // `ITEM_WEAPON_SPEAR` or ""
    pub mat: String,           // only for display/legacy; matching ignores it (w:Mandate)
    pub amount_total: i32,
    pub amount_remaining: i32,
    pub issued: u64,
    pub deadline: u64,         // tick
    pub strikes: u8,           // 1..=5
    pub jail_months: u8,       // 1..=4
    pub text: String,          // "Make 3 spears" / "Ban export of spears"
}
pub(crate) struct Nobles15 {
    pub mandates: Vec<MandateRt>, pub next_id: i32,
    pub last_issue: Vec<(i32 /*unit*/, u64)>,   // shared mandate/demand cooldown
    pub demands: Vec<DemandRt>, pub bk: Bookkeeper, pub rep: NoblesReport,
    pub crime_sentence: Vec<(i32 /*crime id*/, u8, u8)>, // for justice
    #[serde(skip)] pub holders: Vec<HolderRt>, #[serde(skip)] pub rooms: Vec<NobleRoom>, ...
}
```

- **Take-over from population.rs** (tagged edits): `mandates_season` returns early and the
  mandate block of `nobles_daily` is skipped when `self.n15.active` (always true; keep the old code
  for `set_society_enabled(false)` symmetry). `pop_init`'s mandate import moves to
  `nobles15_init`: prefer `NoblePosition::mandates_detail` (exact counter/limit/punishment:
  `deadline = now + (timeout_limit - timeout_counter) * 10`), else `parse_mandate(text)` with
  deadline now + rand(3..6 months), strikes/jail random. Old saves: convert `Pop::mandates` once in
  `after_exact_load`/init (`Pop.mandates` stays in the struct for decoding, then is emptied).
  `World::nobles[k].mandates` texts are kept in sync for the round-4 UI.
- **Issue** (`nobles15_pass`, period 100, phase 23 — free per `tick_inner`): for each holder slot
  with `mandate_max > 0`, alive, on map, citizen, not caged/insane/mood/unconscious (reuse
  existing ai flags; `moods` active, `health` unconscious), active mandates < max, and no own
  mandate or demand issued within the cooldown: chance 1 in `MANDATE_PERIOD` (setting,
  default 1000) per pass. ESTIMATE: per 100-tick pass gives a mean wait of 100 000 ticks ~ one
  season, matching the observed >= 1.17-season gaps with a 3.2-month deadline. Cooldown after
  any mandate/demand: `MANDATE_COOLDOWN` = 1 month (ESTIMATE). RNG:
  `Rng::for_unit(self.seed, slot, now, 0x4E15_0001)`.
  Kind: 2 in 3 production, 1 in 3 export ban (ESTIMATE; the 60 r7 crimes are all production
  violations, no ExportViolation, but bans only become crimes when broken).
  Item: from the noble's preferred item types (see section 9); fallback: the existing
  `MANDATE_ITEMS` list extended with `WEAPON:ITEM_WEAPON_SPEAR` etc., picked by RNG; production
  only for item types some workshop of `build_api::OPTIONS`/reactions can make (ESTIMATE, DF bug
  944 says impossible demands exist, so do not over-filter). Amount 1..=5 (ESTIMATE; observed 3).
  Deadline: production `now + 10 * (10_080 + rand(10_081))` ticks = 3..6 months (OK); export ban
  6 months (OK "about half a year"). Punishment: strikes `1 + rand(5)`, jail `1 + rand(4)` (OBSERVED).
- **Fulfilment** (production): hook in `production.rs push_item` next to `war_item_made`
  (`self.nobles15_item_made(&it); // [nobles15]`): if the item was made by a citizen (same test as
  `war_item_made`: value 0 before valuation, holder citizen or none, not caravan: caravan items are
  created via trade code with `it.value != 0`/`merchant` flag; verify) and kind/subtype matches an
  active production mandate -> `amount_remaining -= stack.max(1)`; at 0 -> met. O(#mandates) per
  created item (<= 11) -> negligible. No more O(items) `count_kind` scans.
- **Deadline / met** (same pass): met -> `MandateDeadlineMet` (PosMild 100) on the noble,
  announcement Info. Missed production -> `MandateIgnored` (Neg 200) + crime(s):
  culprit choice: candidates = adult citizens on map, not the noble, not holding any
  PUNISHMENT_EXEMPTION position; weight 4 if they have the item's skill > 0 (WEAPONSMITH for
  weapons, CARPENTRY for wooden furniture... table `item_skill(item_type)`), else 1 (ESTIMATE of
  "tends to have a related skill"). One culprit (ESTIMATE; "one or more"). No candidate ->
  `NoPunishment` (Neg 150) on the noble.
  Crime: call `justice_mandate_missed` replacement `justice_crime(kind, culprit, noble, true, now)`
  then record `crime_sentence.push((id, strikes, jail_months))`, set the crime accused + convicted
  immediately (auto-sentenced, OBSERVED) by calling `self.convict(id, culprit)` path, tagged.
- **Justice integration** (one tagged block in `justice.rs` before `let (days, strikes) =
  sentence(&kind)`): `if let Some((st, jm)) = self.nobles15_sentence(crime_id) { strikes = st;
  days = jm * 28 }` and apply **both** (strikes by the hammerer if any, else skip strikes; then
  jail if a restraint is free, else beating). The noble thinks `DelayedPunishment` (Neg 80) if the
  punishment has not started 7 days (ESTIMATE; DF `time_to_assign` 2 592 x 10 = 21.6 days left in
  the r6 record) after conviction, the culprit `PunishmentDelayed` (PosMild); if it can never be
  applied (no hammer and no jail and no enforcer) -> `ImproperPunishment` on the noble.
  Jail months -> 28 days each (1 DF month).
- **Export bans**: replace `check_export_bans` (called from `trade_impl`) by
  `nobles15_items_traded(&[(item_id)])`: match by item type/subtype of each item and of items
  inside containers; a banned item directly offered -> crime `ExportViolation` for the hauler who
  brought it (needs `ItemMeta.last_hauler` or the depot-haul job's unit; if unknown: no crime,
  `NoPunishment` on the noble, OK per w: "items in bins: no crime, noble unhappy"); banned item
  inside a bin -> only `MandateIgnored` + `NoPunishment`. ESTIMATE simplification: "when the item
  leaves the map" = at trade commit. Trade UI: `TradeStatus` gets `banned: Vec<i32>` (round4 API
  struct is shared: add a new fn `Sim::banned_goods() -> Vec<i32>` in `nobles15_api.rs` instead).
  Ban expiry -> removed silently (ESTIMATE: DF "the noble ends them"), no thought.
- **Death / election loss**: noble dead or off map -> drop his mandates and demands. Mayor
  replaced by election -> drop the old mayor's mandates (OK); replaced by player nomination ->
  keep (bug 3047: OK to imitate or not; ESTIMATE: imitate).
- **Report**: `NoblesReport { mandates_issued, met, missed, export_bans, bans_broken,
  sentences: Vec<(u64, String)>, demands_*, meetings, ... }`; keep `PopulationReport.mandates_*`
  counters updated too (bench prints them).

Tests: `mandate_limits_follow_raws` (mayor 1, count 2, EL 0), `deadline_in_three_to_six_months`
(1 000 seeds), `crafted_items_fulfil_caravan_items_do_not`, `missed_mandate_sentences_both`
(hammerer + chain: strikes then jail, `prison` 28 x months days), `exempt_positions_never_blamed`,
`no_candidate_gives_no_punishment_thought`, `export_ban_hauler_blamed_and_bin_unblamed`,
`imported_mandate_keeps_counter` (Tileshtulon: deadline = (10 762 - 5 552) x 10 ticks after start),
`mandates_dropped_on_death_and_lost_election`, plus a determinism test (threads 1 vs default).

---

## 5. Demands

### DF rules (w:Demand page is v0.47 content migrated to 53.17: treat as best available)

- "Always for furniture"; each demand names item + material + room ("green glass window in
  dining room"); all three must match; doors/windows count in the room's bordering walls if the
  zone covers them; duplicates need one piece. OK (w:Demand).
- Nobles demand furniture they prefer, often in a preferred material. OK.
- Number limited by DEMAND_MAX (raws). Frequency: difficulty `DEMAND_PERIOD` (default 500) and the
  shared "recently made a mandate or demand" cooldown (w:Mandate). OK rule / ESTIMATE unit.
- "About half a year" to fulfil; after "about 3 years" the noble has "forgotten" the demand
  (announcement). OK (w:Demand).
- Fulfilled -> happy thought; unfulfilled -> "angered at the state of demands" (thought `Demands`,
  caption "considering the state of demands"); no dwarf is punished. OK.
- Status colours: grey none, green fulfilled, yellow time left, orange running out, red overdue.
  OK (w:Demand); "orange" threshold ESTIMATE = last month.
- OBSERVED: the reference mayor (DEMAND_MAX 2) shows **no** `Demands` thought in year 279 and none
  in 21 863 r7 emotions -> demands are rare in 53.16 (or need assigned rooms of the right kind; the
  mayor lacked office and dining room but had a bedroom).

### Design

```rust
pub(crate) struct DemandRt { id: i32, noble: i32, building: String /*Cabinet, Box, Weaponrack,
  Armorstand, Statue, Table, Chair, Door, Bed, Window*/, mat: String /*material token, "" = any*/,
  room: u8 /*Bedroom, Office, DiningHall*/, issued: u64, due: u64 /*issued + 6 months*/,
  forget: u64 /*issued + 3 years*/, met: bool, next_thought: u64 }
```

- Issue: only for a room kind the holder **owns** (ESTIMATE; explains the observed silence for a
  mayor without office/dining room only partially), chance 1 in `DEMAND_PERIOD` per **daily**
  check (ESTIMATE: mean 500 days; the per-100-tick reading would give a demand every ~42 days
  and many `Demands` thoughts, contradicting the export), shared cooldown with mandates.
  Building kind from `furniture_kind` list; material from preferences else the fort's most common
  furniture material (ESTIMATE).
- Check (daily, O(#demands x furniture in room)): a built building of that kind (and material if
  set) whose center lies inside the room (doors/windows: on a wall tile adjacent to the room) ->
  met: `Demands` PosMild 100 once, demand removed after 1 day (UI shows green). Overdue (now >
  due): `Demands` Neg 120 every 7 days (ESTIMATE). `now > forget`: removed, announcement
  "<name> has forgotten a demand." (ESTIMATE wording), no more thoughts.
- Tests: `demand_met_by_furniture_in_room`, `wrong_material_or_room_does_not_count`,
  `overdue_demand_angers_weekly`, `forgotten_after_three_years`, `demand_max_respected`.

---

## 6. Mayor, sheriff and other positions

### DF rules

- "Once the fortress population reaches 50, a mayor will be automatically elected (replacing the
  expedition leader)" (w:Noble V50); MAYOR `REQUIRES_POPULATION:50`, `ELECTED`; EL
  `REPLACED_BY:MAYOR` (raws). OK; existing.
- Re-election yearly (w:Mayor, existing FIDELITY "OK"); the winner rule is not documented
  (ESTIMATE: existing score). Thoughts: winner `Elected` (new) / `Reelected` (incumbent wins),
  captions exist. Announcement for re-election: ESTIMATE "<name> has been re-elected mayor.".
- Bug 8129: new mayors may take a while to appear (w:Noble): not imitated.
- SHERIFF `REPLACED_BY:CAPTAIN_OF_THE_GUARD`; captain `REQUIRES_POPULATION:50` (raws). Export:
  SHERIFF has `HAS_BEEN_REPLACED` and is inactive in Tileshtulon. OK.
- DUNGEON_MASTER `REQUIRES_POPULATION:50`, appointed by the mayor only (not the EL). OK (raws).
- REQUIRES_MARKET positions (manager, bookkeeper, broker, CMD, hammerer, champion, captain of the
  guard, dungeon master) carry `HAS_MET_MARKET_REQ` in the export; the trigger is not documented
  (difficulty `ECONOMY_*_TRIGGER` 20 pop / 5 000 created / 500 traded ESTIMATE). Rustfort forts
  are imported mature: do not gate (MISSING by design).
- Land holders: BARON/COUNT/DUKE come with the land title (diplomacy). Here: when
  `land_title()` is non-empty and no BARON/COUNT/DUKE entry of the fort's site exists, do nothing
  (the diplomacy agent creates/fills it); requirement/mandate/demand code treats any fort holder
  of those tokens like any other position. Positions of the **civilization** that hold no unit on
  the map (export: king, barons elsewhere) are ignored (holder not in `unit_slot`).

### Design (tagged edits in `population.rs`)

- `elect`: pass `incumbent`; feel `Elected` (Pos 200) if changed, `Reelected` (PosMild 120) if
  the incumbent won a yearly election (currently silent); keep `BecomeNoble` for appointments.
  Lost election -> `nobles15_drop_holder(old_id)` (mandates, demands).
- `nobles_daily`: at citizens >= 50, if a SHERIFF entry exists and no CAPTAIN_OF_THE_GUARD, add
  the captain position (`NoblePosition` with raws values) and move the sheriff's holder to it
  (ESTIMATE: DF leaves the new position vacant? Unknown -> carry the holder over so justice keeps
  working; document). `s2_noble_slot("SHERIFF")` fallback in `law_enforcer` already handles both.
- UI (`ui/nobles15.rs`): `filled_by` needs DUNGEON_MASTER (by mayor), "requires 50 citizens"
  note for MAYOR/CAPTAIN_OF_THE_GUARD/DUNGEON_MASTER when below.

---

## 7. Bookkeeper precision

### DF rules (w:Bookkeeper, header v53.17)

- Accuracy level 1-5 chosen on the nobles screen next to the bookkeeper. OK.
- Level 1 works without an office; higher levels require (at least) a meager office. OK.
- Precision affects only the accuracy of item and zone **counts**; item and zone **values**
  depend on the broker's appraisal skill. OK.
- Skill RECORD_KEEPING (profession clerk); attributes Analytical Ability, Memory, Focus. OK.
- The initial full stock list takes a long time; maintaining a level afterwards takes only a bit.
  OK (no numbers).
- How counts look per level: not on the wiki. ESTIMATE (DF2014 memory): low levels show rounded
  or ranged counts.

### Design

- State `Bookkeeper { level: u8 /*1..=5 target*/, reached: u8, work: u32, records:
  Vec<(String /*item type*/, i32)>, recorded_at: u64 }` (saved).
- The bookkeeper (holder slot of BOOKKEEPER) gets a low-priority idle task "update records":
  walk to his office (level >= 2) or stay anywhere (level 1), work in 100-tick units. Work needed
  (ESTIMATE): initial `level x 2 days` of work, maintenance `level x 2 hours` per day (2 h =
  100 ticks per level). Speed x (1 + RECORD_KEEPING / 10) ESTIMATE; 30 xp RECORD_KEEPING per work
  unit (ESTIMATE, cf. 100 ORGANIZATION per approved order in skills.rs). Hook: `society_decide`
  (population.rs, which already gives burial/broker duties) one tagged call
  `if self.nobles15_decide(s, now) { return true }`. If this touches the job loop too much,
  v0.15 can resolve work statistically in the pass (no walking) - document as ESTIMATE.
- When the work for the target is done: `reached = level`, snapshot `records` = counts per item
  type (one O(items) scan, at most once per day). Displayed count (`Sim::stock_count(kind) ->
  StockCount { shown: String, exact: i32 }`): level 5 exact; 4 nearest 5; 3 nearest 10; 2 nearest
  50 shown "~N"; 1 order of magnitude "<10", "10+", "100+", "1000+" (ESTIMATE). Without a
  bookkeeper or before the first record: "?" (ESTIMATE). rf-app `count_stocks` (top bar) stays
  live (it is the alert bar, not the stocks screen).
- Tests: `level1_needs_no_office`, `level3_waits_for_office`, `counts_round_by_level`,
  `snapshot_is_stale_until_next_update`.

---

## 8. Manager validation and broker appraisal

### Manager

- raws DESCRIPTION: "Once your fortress reaches a certain population, the manager must work in an
  office to validate work orders." Population: not on the wiki (Manager page could not be read,
  HTTP 429). ESTIMATE `MANAGER_OFFICE_POP = 20` (first `ECONOMY_POP_TRIGGER`). OK that approving
  gives ORGANIZATION (skills.rs already: 100 xp, wiki).
- Design: orders created by the player (`production.rs ~1033`, `build_api.rs ~305`) start
  `validated: false` when citizens >= 20 (tagged one-liners using `self.nobles15_order_needs_validation()`),
  `true` below (unchanged). Production skips `!validated` orders (one tagged `continue` where orders
  become jobs). The manager validates one order per `VALIDATE_TICKS = 200 / (1 + ORGANIZATION/5)`
  (ESTIMATE) of office work (same task mechanism as the bookkeeper; no office -> nothing
  validated, announcement once "The manager needs an office to validate work orders." ESTIMATE).
  No manager -> orders stay pending (ESTIMATE; DF requires a manager for work orders).
  Imported orders keep their exported `validated` flag -> reference bench hash unchanged unless
  new orders are created. Call `skills_order_approved` on validation.
- Tests: `orders_need_manager_office_above_twenty`, `small_fort_orders_auto_valid`.

### Broker

- raws DESCRIPTION: "The broker uses their appraisal skill to allow you to see the value of items
  in the fortress. They are also the default negotiator with visiting merchants." w:Bookkeeper:
  "item and zone values depend on the broker's appraiser skill". OK.
- Existing: displayed trade values deviate by up to (25 - 2 x APPRAISAL) % (ESTIMATE, FIDELITY).
  Gap: with no broker or APPRAISAL 0 the values are unknown (ESTIMATE: DF shows "???"). Design:
  `nobles15_api::appraised_value(item_id) -> Option<i64>` = `None` without broker skill, else the
  existing `shown_value`; zone values in the UI (room panel, requirement panel) go through the
  same function (true values stay in the sim; this is display only, hash-neutral). One tagged line
  in `trade.rs shown_value` callers is NOT needed: the TradeOffer keeps numbers for the merchants;
  the UI decides what to show via `appraised_value`.
- MISSING: haggling by NEGOTIATION/JUDGING_INTENT (not in v50 trading as far as known).

---

## 9. Data gaps (export)

- **Preferences** (w:Mandate: only nobles with item preferences mandate them; w:Demand: preferred
  furniture/material): units.jsonl has no preferences. Additive exporter change (read-only Lua):
  per unit `preferences: [{"type": "LikeItem"|"LikeMaterial"|..., "item_type", "item_subtype",
  "mat"}]` from `unit.status.current_soul.preferences`; rf-core `Unit::preferences:
  Vec<Preference>` `#[serde(default)]` (additive); document in EXPORT_FORMAT.md. Fallback without
  it: hash-chosen item from the list (current behaviour), documented ESTIMATE.
- **Demands** are not exported (DF keeps them in the unit's `specific_refs`/`plotinfo`? unknown):
  start empty (MISSING import).
- Room owner already exported (`owner`), zone subtypes include `Office`/`Tomb`.

---

## 10. Hooks, scheduling, saves, performance

| hook | where (tag `// [nobles15]`) | when |
|---|---|---|
| `nobles15_pass(now)` | `tick_inner`, `if now % 100 == NOBLES_PHASE (23)` (check no other module uses 23; weather 41, undead 59, vamp 83, S2 37, pop 1, social 50, moods 73) | issue mandates, deadlines, holder cache refresh |
| `nobles15_day(now)` | inside the pass when `now / DAY` changes | demands, birthdays/tombs, complaints, bookkeeper snapshot, sheriff replacement |
| `nobles15_item_made(&it)` | `production.rs push_item` | mandate fulfilment |
| `nobles15_items_traded(..)` | `trade.rs trade_impl` (replaces `check_export_bans`) | export bans |
| `nobles15_sentence(crime)` | `justice.rs` sentencing | mandate punishments |
| `nobles15_room_feel(..)` | `social.rs` bedroom/dining thoughts | noble room thoughts |
| `nobles15_meeting(..)` | `locations.rs` petition answered | office meetings |
| `nobles15_decide(s, now)` | `population.rs society_decide` | bookkeeper/manager office work |
| `nobles15_drop_holder(id)` | `population.rs elect`, death check | end mandates/demands |

- **Saves**: `Sim` field `n15: nobles15::Nobles15` (`#[serde(default)]`), registered like the
  round-13 skeletons: `saves11.rs exact_parts` `("nobles15", enc(&self.n15)), // [nobles15]`,
  `persist.rs` `v.push(("nobles15", fnv(|mix| self.nobles15_hash(mix))))`, `state_hash` mix call in
  lib.rs next to `weather_hash`; caches (`holders`, `rooms`) `#[serde(skip)]` and rebuilt in
  `after_exact_load` (tagged line). Document the part in docs/SAVES.md. `saveload` example must
  stay IDENTICAL.
- **Determinism**: all lists are `Vec` in creation order; no HashMap iteration; RNG only
  `Rng::for_unit(self.seed, slot, now, salt)` with fixed salts `0x4E15_00xx`; the pass is serial.
- **Perf**: the pass touches only holders (<= ~30 entries) and their zones; per-unit work is the
  O(1) `room_shift` lookup in the social sleep/meal path (a `Vec<u8>` by slot). The old daily
  `count_kind` scans (O(items) per mandate) disappear -> net speed-up. Bookkeeper snapshot
  O(items) at most daily. Expect bench within noise; measure `bench --ticks 806400 --standdown`.
- **Expected hash change**: yes on the reference fort (mayor's shifted room thoughts, new mandate
  timing and sentences). State this in the commit and FIDELITY section.

---

## 11. Prioritized implementation list

1. rf-core `NoblePosition` extra fields + `MandateDetail` + fallback table; `HolderRt` cache;
   `Nobles15` state with save/hash registration (skeleton first, hash-neutral commit).
2. Mandates rewrite: limits (MANDATE_MAX), cooldown (MANDATE_PERIOD), 3-6 month deadlines,
   import of exported counters, item-made hook, `MandateDeadlineMet` / `MandateIgnored`, culprit
   choice with skill weighting and exemptions, auto-sentence with the mandate's strikes + jail
   months (justice hook), `NoPunishment` / `DelayedPunishment`.
3. Export bans by item type with hauler blame, bins, half-year expiry; `banned_goods()` for the
   trade UI.
4. Rooms: `Office` zone kind, `assign_zone` API, `zone_value` refactor, requirement status per
   holder (rooms + furniture counts), nobles requirement panel in `ui/nobles15.rs`.
5. Room thoughts H1 (shifted bedroom/dining severity), office meetings (`OfficeQuality` /
   `MeetingInDiningRoom` / `MeetingInBedroom` / `NoRooms`), complaints, tomb birthday thoughts,
   sleep pretension.
6. Elections: `Elected`/`Reelected`, mandates dropped on lost election; sheriff -> captain of the
   guard at 50; dungeon master by mayor.
7. Demands (furniture + material + room, half-year due, weekly anger, 3-year forget).
8. Manager office validation above 20 citizens; broker `appraised_value` (None without skill).
9. Bookkeeper precision levels 1-5, office for >= 2, stale stock records with rounding.
10. Exporter: unit preferences (additive), then preference-driven mandate/demand items.
11. docs/FIDELITY.md section `[nobles15]` (table rule / Rustfort / DF source / class) and
    EXPORT_FORMAT/SAVES updates.

Open questions to settle by measurement: H1 (mayor bedroom tier in Rustfort vs DF's -1); demand
frequency (unit of DEMAND_PERIOD); manager population threshold; bookkeeper display rounding.
