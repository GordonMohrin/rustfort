# [health15] Hospital and medicine depth: mechanics analysis and implementation spec (target v0.15)

Status: analysis only (no code yet). This file is the implementer's spec. Every rule carries a
class: **OK** (stated by the wiki, the installed v50 raws, or measured in the reference export),
**DEV** (statement by the developer quoted on the bug tracker), **ESTIMATE** (our choice; DF's
number is not public). Nothing here comes from decompiling DF.

Sources used:

- Wiki (dwarffortresswiki.org): Health care (DF2014 page, v0.47.05), Crutch (v50), Cast, Splint,
  Plaster, Plaster powder, Traction bench (via search excerpts; the wiki rate-limited direct
  fetches), Surgeon, Crutch walker (v0.34), Organ, Wound (v0.31/v0.34), Ambusher (penalty model).
- Bug tracker: 4470 (traction patients never released, fixed 0.40.12), 194 (Apply cast buckets,
  fixed 0.31.22), 2627; Toady One's comment on overlapping fractures (quoted in the 4470/4350 threads).
- Raws of the local v50 install (read only): `reaction_other.txt` (MAKE_PLASTER_POWDER),
  `inorganic_other.txt` (PLASTER), `inorganic_stone_mineral.txt` (REACTION_CLASS:GYPSUM),
  `tissue_template_default.txt` (HEALING_RATE, SETTABLE, SPLINTABLE), `b_detail_plan_default.txt`,
  `body_default.txt` (spine parts), `creature_standard.txt` (DWARF gaits),
  `c_variation_default.txt` (gait variations), `entity_default.txt` (PERMITTED_REACTION).
- Reference export `export/tileshtulon` (measured, see each section).
- Rustfort code at the round-13 contract commit `c60e3a7`.

Ownership: this topic owns hospital supplies, fractures and immobilization, plaster, traction,
nerve damage and disabilities, crutches, treatment outcomes by skill, surgery dangers and per-tissue
healing. NOT ours: noble appointment of the chief medical dwarf (`nobles15`), item rot/decay
(`decay15`), fire (`fire15`), undead/vampires/weather/megabeasts/diplomacy (round 13), audio.

---

## 0. What Rustfort has today (inventory)

| Area | Where | What it does |
|---|---|---|
| Wound model | `combat.rs` `WoundS {part, layer, flags:u16, sev, bleed, pain, age}`, `wf::*` bits 1..64 | Cut/bruise/broken/severed/torn/bleeding/artery per (part, layer). |
| Treatment bits | `health.rs` `hf::*` 128..8192 (DIAGNOSED, SUTURED, DRESSED, SET, INFECTED, CLEANED, OPERATED) | Two bits left in `WoundS::flags` (16384, 32768). |
| Care jobs | `health.rs` `Treat`, `CareJob`, `CStep`, `health_pass` (every 100 ticks, phase 37), `health_decide`, `care_arrive`, `apply_treatment` | Diagnose -> Suture -> SetBone -> Surgery -> Dress (+soap = CLEANED), Water/Food feeding, Carry (recover wounded). One supply item per job. |
| Supplies | `health_refresh_daily` (phase 637 daily) | Counts free THREAD/CLOTH/SPLINT and "any soap" in the whole fort. Supplies are taken from anywhere (nearest). Used items are **destroyed**. |
| Healing | `combat.rs` `heal_pass` (every 100 ticks, phase 0) | Same rate for every tissue: rest 60 / hospital 100 / awake 15 severity per 100 ticks; broken bones heal only at rest. Treated wounds -20 more (health_pass), unset broken +30. |
| Hospitals | `combat.rs` `init_combat` | Rects of civzones whose `subtype` contains `Hospital`, collected **once at init**. |
| Standing | `health.rs` `cannot_stand`, `health_gait`, `CRAWL_FACTOR = 4` | Unset broken bone above/at a STANCE part, or a severed STANCE part -> crawl at 4x the walk delay. |
| Infection | `health.rs` `infection_daily` | Daily odds 1/100 uncleaned, 1/400 dressed, 1/4000 soap-cleaned; infected wound bleeds >= 2 and worsens +100 sev / 100 ticks; cure 1/10, 1/25, 1/50 per day. |
| Surgery | `apply_treatment(Surgery)` | Internal parts with sev >= 3000: sev * 2/3, never fails. |
| Paralysis | `combat.rs:1879` | A torn NERVOUS non-THOUGHT part (spine) sets `Fighter::paralyzed = true` (permanent). |
| Part state | `Fighter::part_state` bits 1 severed, 2 broken, **4 function lost (declared, unused)** | |
| Production | `production.rs:336-339` | Splints and crutches: carpenter (1 log), forge (1 bar). Kiln has no plaster. |
| Reactions | `industry/reactions.rs` `REACTIONS`, `Rg`, `Pr` | No GYPSUM reagent, no bag-product, no plaster. |
| Locations | `locations.rs` `derive_locations` | Derives a `Location{kind:"Hospital"}` from a *MeetingHall* civzone named "Hospital"/"Lazarett"/...; `rf_core::Location` has no `supplies`. |
| UI | `rf-app/src/ui/round5.rs:1036` shows SPLINT/CAST/IMMOBILIZED for broken wounds; `places.rs` hospital location; `nobles.rs` CHIEF_MEDICAL_DWARF -> DIAGNOSE | |

### 0.1 Defects found while analysing (fix first, they change results)

1. **v50 hospitals are not hospitals for healing/bed choice.** In v50 a hospital is a *location*
   on a "Meeting area" zone. The reference fort's hospital is `Civzone subtype "MeetingHall"` named
   "Hospital" (building 10634, 9x7 tiles, z 125). `combat.hospitals` only takes `subtype` containing
   "Hospital", so the reference fort has **no hospital** for `heal_pass` (rate 100) and
   `rescue_bed_for`. Zones/locations created at runtime are never added either. (OK, measured.)
2. **Sutures are thread left in the patient, not destroyed.** Reference export: the 26 dwarves
   who carry THREAD in their inventory are exactly the 26 dwarves with SUTURED wounds (72 thread items
   vs 78 sutured part-wounds; 0 of 61 dwarves without sutures carry thread). (OK, measured.)
   Bandages/splints are *not* kept: only 1 CLOTH/SPLINT/CRUTCH is held fort-wide.
3. **Nerve damage is dropped on import.** Reference export: 4 MOTOR_NERVE_SEVERED, 2
   SENSORY_NERVE_SEVERED wound entries, all on the MUSCLE layer of hands/arms (RH, LUA, RLA, LH,
   LUA, goblin RUA) and all carrying SCAR_CUT, so `load_wounds` files them as scars and the
   disability is lost. (OK, measured.)
4. **Crawl speed is not DF's.** DWARF raws: `STANDARD_BIPED_GAITS:900:...` (walk 900) and
   `STANDARD_CRAWLING_GAITS:2990:...` (crawl 2990) -> ratio 3.32, not 4. (OK, raws.)
5. **Bones heal as fast as skin.** Raws: SKIN/FAT/MUSCLE/organs `HEALING_RATE:100`, BONE/SHELL
   `HEALING_RATE:1000`; NERVE, BRAIN, CARTILAGE, NAIL, TOOTH, HORN, HOOF, HAIR, SPINE templates
   have none. The exporter already writes `healing_rate` per layer (0 = none; reference fort:
   12187 layers at 100, 4229 at 1000, 4188 at 0) but `rf_core::TissueLayerRaw` drops it. (OK.)
6. Soap is never used up (`health.soap` is a bool). (Gap; DF uses soap units, see M3.)
7. `infection_daily` computes `soap` and ignores it (`let _ = soap`). Harmless.

---

## M1. Hospital recognition (zone + location)

**DF rule (OK, wiki Health care/Hospital):** a hospital is a zone with the Hospital purpose; in
v50 it is a location assigned to a meeting-area zone. Beds, tables, traction benches and
containers must be inside the zone to count. Injured dwarves rest in hospital beds first. Healthy
tired dwarves may camp in a hospital (not modelled; ESTIMATE: skip).

**Rustfort now:** `combat.hospitals` from `subtype.contains("Hospital")` at init only.

**Gap:** location-based hospitals, runtime changes (new zone, `create_location`, zone removed).

**Design:**

- `health15.rs`: `fn h15_rebuild_hospitals(&mut self)` builds `Vec<Hosp>` from (a) civzones with
  subtype containing `Hospital` and (b) every `world.locations` entry with `kind == "Hospital"`,
  using its `zones` building ids. One `Hosp` per location (zone-only hospitals: one each, `loc = -1`).
  Sort by (loc id, first zone id) for determinism.
- It then **overwrites `self.combat.hospitals`** with the union of the rects (dedup). Mark the
  single line `self.combat.hospitals = ...; // [health15]`.
- Call it: at `init_health` (after `init_combat`), daily at phase 667, from
  `create_location`/zone build/remove API paths (one `// [health15]` line each, or simply a dirty
  flag `h15.hosp_dirty = true` set there and consumed at the next 100-tick pass).
- Keep `in_hospital` in combat unchanged (it reads `combat.hospitals`).

**Hash:** changes for the reference fort (patients now heal at the hospital rate). Expected.

**Tests:** `meeting_zone_location_is_hospital`, `runtime_hospital_zone_is_used`.

---

## M2. Hospital supplies and chests (stocking)

**DF rule:**

- (OK, wiki DF2014 Health care) The hospital requisitions **thread, cloth, splints, crutches,
  plaster powder, buckets, soap**; desired amounts are adjustable per item; containers (chests,
  "a small hospital can manage with 2, a large one may need up to 8") must be in the zone.
  Unit conversions shown by the screen: 1 thread = 15 000, 1 cloth = 10 000, 1 soap bar = 150,
  1 plaster powder = 150 (PRODUCT_DIMENSION:150 in MAKE_PLASTER_POWDER confirms 150).
- (OK) Doctors **can and will use supplies from anywhere**; the chests earmark supplies for medical
  use. They prefer the most expensive thread/cloth, including adamantine.
- (OK, exporter) DF stores per-location `contents.desired_<x>` and `count_<x>` for
  goblets, instruments, paper, splints, thread, cloth, crutches, powder, buckets, soap, copies;
  `tools/dfhack/rustfort-export.lua` already exports them as `locations[].supplies[]`
  (`{item, desired, count}`), but `rf_core::Location` has no field, so the importer drops them.
- (ESTIMATE, from memory of the v0.47 hospital screen; verify against an export with a hospital
  location) defaults: thread 75 000 (5), cloth 50 000 (5), splints 5, crutches 5, powder 750 (5),
  buckets 2, soap 750 (5).
- (ESTIMATE) The stocking job is DF's "Store item in hospital", a hauling job; we use the labor
  `HAUL_ITEM` (`labor::HAUL_ITEM`).
- (ESTIMATE) Chest capacity: 20 supply items per container building (`Box`; also `Cabinet` if
  present in the zone). Bags of powder count as 1 item each.

**Rustfort now:** nothing stocked; supply counts are fort-wide.

**Design (module `health15/supplies.rs`):**

```rust
#[repr(u8)] #[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sup { Thread, Cloth, Splint, Crutch, Powder, Bucket, Soap }   // NSUP = 7
pub const SUP_UNITS: [i32; 7] = [15_000, 10_000, 1, 1, 150, 1, 150];   // DF screen units per item
pub const SUP_DEFAULT: [i32; 7] = [5, 5, 5, 5, 5, 2, 5];               // items, ESTIMATE

#[derive(Serialize, Deserialize, Clone, Default)] #[serde(default)]
pub(crate) struct Hosp {
    pub loc: i32,                       // location id, -1 = plain zone
    pub zones: Vec<i32>,                // civzone building ids
    pub rects: Vec<(i16, i16, i16, i16, i16)>,
    pub chests: Vec<(i32 /*building id*/, i32 /*container item id or -1*/, u32 /*tile*/)>,
    pub tables: Vec<u32>,               // tiles of Table buildings in the zone (surgery)
    pub want: [i32; 7],                 // desired items (player-set; init from export or default)
    #[serde(skip)] pub have: [i32; 7],  // rebuilt daily / after load
}
#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) struct StockJob { pub item: i32, pub hosp: u16, pub chest: u16, pub hauler: u32, pub step: u8 }
```

- `want` for a location: `supplies[].desired / SUP_UNITS` (round up) when exported, else
  `SUP_DEFAULT`. Player edits via API (`set_hospital_supply`).
- **Earmarked set** (serde skip, rebuilt in `after_exact_load` and daily): item ids inside a
  hospital chest (`contents[chest item id]`) of a supply kind. Keep it as a sorted `Vec<i32>`
  (binary search) or a bit in a dense table keyed by item index. Earmarked items:
  - are skipped by production reagent search and by the general hauler (one guard each:
    `if self.h15_earmarked(id) { continue } // [health15]` in the production reagent picker and in
    storage's haul candidate loop), so they are not turned into clothing or re-stockpiled;
  - are preferred by doctors: in `care_find_supply` subtract 10 000 from the distance of an
    earmarked candidate (mark the line).
- **Stock pass** every 400 ticks at `now % 400 == 247`: for each hospital with chests and
  `have[k] < want[k]` and no open StockJob for k: pick the nearest free (`item_free`, not
  earmarked, not in a hospital already) item of kind k by the same rules as `care_find_supply`
  (thread/cloth: highest material value first, OK per wiki "most expensive"); buckets must be empty;
  soap = BAR/GLOB with a soap material (reuse the existing soap test); powder = POWDER_MISC whose
  material token is `PLASTER` (in its bag: take the bag). Push one `StockJob` (max 4 open per
  hospital, ESTIMATE).
- **Claim**: in `decide`, after `health_decide` returns false: `h15_decide(s, now)` (one hook line),
  workers with `HAUL_ITEM`. Uses a new task variant `Task::Med15` (append to `enum Task`,
  `// [health15]`), dispatched in the arrive/wake match and in `cancel_task` (one line each).
- Steps: GoItem -> pick up (`mood_take`) -> GoChest -> put into the chest container exactly like
  `storage::store_drop` does (container + `contents` update) -> done, `have[k] += 1`.
- Cancel: release reservation, drop item (same as `care_drop_item`).

**Perf:** stock pass is O(open jobs + hospitals) plus one item scan only when a hospital is short
(scan the existing per-kind item lists if present; else one `world.items` scan per 400 ticks while
short, which stops once stocked). Daily `have` recount: fold into `health_refresh_daily`'s existing
item loop (extend the `interesting` match with `CRUTCH | POWDER_MISC | BUCKET` and call
`self.h15_count_supply(idx)` - no second scan).

**Data:** rf-core `Location.supplies: Vec<LocationSupply>` (`#[serde(default)]`,
`LocationSupply { item: String, desired: i32, count: i32 }`); importer reads the existing export
field (it is already in the JSON). `docs/EXPORT_FORMAT.md`: note that `supplies` is now imported.

**Tests:** `hospital_chest_is_stocked_to_desired`, `desired_from_export_units` (75 000 thread -> 5),
`earmarked_cloth_not_used_by_clothier`, `doctor_prefers_hospital_chest`, `no_chest_no_stocking`.

---

## M3. What happens to the supplies (consumption fidelity)

**DF rules:**

| Supply | Use | Fate | Class |
|---|---|---|---|
| Thread | Suture | Stays **sewn into** the patient for good (with the scar). | OK (measured, see 0.1.2) |
| Cloth | Dressing, cleaning, under a cast, suturing ("uses thread and cloth") | Bandage strapped on; removed when the wound has healed. | OK use (wiki); fate ESTIMATE (reference: 1 held item fort-wide) |
| Splint | Immobilize a fracture | Strapped on; removed when the bone has healed. | OK use; fate ESTIMATE |
| Plaster powder | Apply cast (with bucket + water + cloth) | Becomes an orthopedic cast item of PLASTER that hardens to GYPSUM (`HARDENS_WITH_WATER:INORGANIC:GYPSUM`); removed when healed. Bag stays. | OK raws/wiki; fate ESTIMATE |
| Bucket | Carries water for cast and cleaning | Not consumed. | OK |
| Soap | Cleaning (sterilizes) | 150 units per bar. | OK units; per-use amount ESTIMATE: 15 units -> one bar per 10 cleanings |
| Crutch | Given to a patient who cannot walk | Held until no longer needed, then dropped. | OK give (wiki Crutch); drop ESTIMATE |

**Rustfort now:** every supply item is destroyed in `apply_treatment`.

**Design:**

- `apply_treatment` (`health.rs`) Suture branch: instead of `destroy_item`, call
  `self.h15_attach(p, item, Attach::Sewn)` (one line + `// [health15]`): split one unit off the
  stack if `stack > 1` (`create_item` copy with stack 1), set `holder = patient id`,
  `flags.in_inventory = true`, push into `Unit::inventory`. Kept forever (OK). Hash change.
- Cloth (Dress), Splint (Immobilize), Cast: `Attach::Strapped(part)`; recorded in
  `Disab/med` per unit (`attached: Vec<(item, part, kind)>`). In the 100-tick pass, when no active
  wound on `part` still needs it (no wound with sev > 0 on that part and its BROKEN wound gone),
  detach: cloth -> destroy (soiled; ESTIMATE), splint -> drop at the patient's tile (haulable),
  cast -> destroy (ESTIMATE: casts are cut off).
- Soap: `h15.soap_units: i32` (serialized). On each successful cleaning subtract 15 (ESTIMATE); when
  it would go below 0, destroy one free soap item (prefer earmarked) and add 150. `health.soap`
  remains the "any soap" gate.
- Exporter (additive, optional): inventory modes per item (`SewnInto`, `Strapped`, `Weapon`) so
  imported splints/casts/crutches are recognized; without it, recognise by item kind held by a
  patient (SPLINT/CRUTCH/ORTHOPEDIC_CAST) - good enough.

**Tests:** `suture_thread_stays_in_patient`, `splint_removed_when_bone_healed`,
`soap_bar_used_up_after_ten_cleanings`.

---

## M4. Fracture classes

**DF rule:**

- (OK, exporter/DF flags) DF wound layers carry `NEEDS_SETTING`, `COMPOUND_FRACTURE`,
  `OVERLAPPING_FRACTURE` besides `BROKEN` (`df.wound_damage_flags1/2`, exported as uppercase names).
- (OK, wiki DF2014 Health care) Bone doctors set bones and immobilize them with splints or casts;
  "multiple overlapping and compound fractures require a surgeon"; traction benches immobilize
  "complex or overlapping fractures"; a wiki talk page says benches are only used for the rare
  overlapping fractures.
- (DEV, Toady One on the tracker) "when an overlapping fracture isn't in a bench, it rebreaks fully,
  so he'd never heal."
- (OK, raws) Only tissues with `SETTABLE`/`SPLINTABLE` (BONE, SHELL) can be set/splinted.

**Rustfort now:** only `wf::BROKEN` (+ `hf::SET`).

**Design:** a new per-wound extension word (zero cost; struct stays 24 bytes):

```rust
// combat.rs, WoundS
#[serde(default)] pub mx: u16, // [health15] medical extension bits (health15::mx)
```

Update the 7 literal constructions (`combat.rs` 1307, 2052, 2079, 3329; `syndrome.rs` 226;
`barracks.rs` 406; `combat/saves10.rs` 296) with `mx: 0 // [health15]`.

```rust
pub mod mx {
    pub const SEEN: u16 = 1;          // classified by health15
    pub const NEEDS_SET: u16 = 2;     // displaced: must be set before immobilizing
    pub const COMPOUND: u16 = 4;      // bone through skin: surgery first
    pub const OVERLAP: u16 = 8;       // overlapping: traction bench
    pub const SPLINT: u16 = 16;
    pub const CAST: u16 = 32;
    pub const TRACTION: u16 = 64;
    pub const IMMOB: u16 = SPLINT | CAST | TRACTION;
    pub const MOTOR: u16 = 128;       // motor nerve severed (permanent)
    pub const SENSORY: u16 = 256;     // sensory nerve severed (permanent)
    pub const CLEAN_FAIL: u16 = 512;  // cleaning attempted without success
    pub const SURG_FAIL: u16 = 1024;  // botched surgery (bleeds, infection risk)
}
```

Classification (`h15_classify`, in the 100-tick pass, only wounds without `SEEN`; no change in
combat's hot `strike`):

- BROKEN on a layer whose tissue is settable (bone/shell; `LayerT.rigid` and tissue flags; add
  `LayerT.settable: bool` `#[serde(default)]` filled from the tissue flags `SETTABLE`):
  - `NEEDS_SET` if `sev >= 5_000` (ESTIMATE: displaced fractures).
  - `COMPOUND` if the same part has an open (CUT|TORN) wound on an outer non-rigid layer whose
    `age` is within 100 ticks of the fracture (same blow) (ESTIMATE, matches DF's meaning "bone
    through the skin"); deterministic.
  - `OVERLAP` if not COMPOUND, `sev >= 8_000` and `Rng::for_unit(seed, s, now, 0x4815).chance(1, 4)`
    (ESTIMATE; DF calls them rare).
- Imported wounds: map flag names `NEEDS_SETTING`, `COMPOUND_FRACTURE`, `OVERLAPPING_FRACTURE`,
  `MOTOR_NERVE_SEVERED`, `SENSORY_NERVE_SEVERED` and set SEEN (no random roll on import). Imported
  `SET`/`SPLINT`/`CAST` -> clear NEEDS_SET, set SPLINT (or CAST if the patient holds an
  ORTHOPEDIC_CAST item). Do this in `init_health`'s loop (it already maps names) with `[health15]`
  lines, plus a scar pass for nerves (M9).

**Tests:** `compound_when_skin_cut_with_break`, `overlap_roll_deterministic_per_seed`,
`imported_fracture_flags_mapped`.

---

## M5. Setting and immobilizing (splints)

**DF rule:**

- (OK) Bone doctor (labor BONE_SETTING, skill SET_BONE) sets bones and immobilizes them; splint from
  1 log (carpenter) or 1 bar (forge; the Crutch page says 3 bars for a metal crutch - not ours).
- (OK) Once splinted the dwarf can leave the hospital (wounds already cleaned, sutured, dressed).
- (ESTIMATE) Setting itself uses no item; immobilizing uses one splint (or a cast, M6, or traction,
  M7). Without any splint/cast possible, DF falls back to cloth? Rustfort did "thread and cloth for
  minor fractures"; keep **cloth as a sling only for non-STANCE minor fractures** (sev < 5 000).

**Rustfort now:** `SetBone` consumes a splint (or cloth) and sets `hf::SET`, which also counts as
immobilized.

**Design:**

- `Treat::supply(SetBone)` returns `None` (`// [health15]`); `next_treat` asks for SetBone only if a
  BROKEN wound has `mx::NEEDS_SET` (or, for imported wounds without SEEN, the old rule).
  `apply_treatment(SetBone)` clears `NEEDS_SET` and sets `hf::SET` (with the outcome roll, M11).
- New treatments in health15 (own job list, task `Task::Med15`): `Immobilize` (splint, or cloth
  sling per above), `ApplyCast` (M6), `PlaceInTraction` (M7), `BringCrutch` (M10). Order per
  patient, after `health.rs` has nothing to do for it (`next_treat == None`):
  `Traction (OVERLAP, set)` > `Cast or Splint (BROKEN, set or not NEEDS_SET, not IMMOB, not
  COMPOUND-unoperated)` > `BringCrutch`.
- Healing: a broken settable wound heals only when `!NEEDS_SET && IMMOB` (or `hf::SET` for legacy
  imports); otherwise rate 0 and the existing `+30` worsening stays (ESTIMATE, unchanged).
- Standing: `cannot_stand` (health.rs) uses `hf::SET` today; change the predicate to
  `BROKEN && !(SET && IMMOB)` for crawling, and an immobilized broken STANCE limb counts as
  **function lost until healed** -> crutch need (M10). (Wiki Crutch: "short-term injuries" make
  hospital staff assign crutches.)
- Durations (ESTIMATE, scaled by skill like the others): Immobilize 200 ticks.

**Tests:** `set_bone_uses_no_splint_immobilize_does`, `unimmobilized_fracture_does_not_heal`,
`splinted_leg_needs_crutch_to_walk`.

---

## M6. Plaster powder and casts

**DF rules (OK, raws `reaction_other.txt`, `inorganic_*.txt`; wiki Plaster/Cast):**

- Reaction `MAKE_PLASTER_POWDER` at the **kiln** (also magma kiln: Rustfort's magma rule already
  maps plain jobs to magma variants), button `CUSTOM_SHIFT_P`: reagent A = 1 BOULDER of a material
  with `REACTION_CLASS:GYPSUM` (vanilla: GYPSUM, ALABASTER, SELENITE, SATINSPAR; **not** ANHYDRITE,
  not TALC), reagent B = 1 empty BAG (`[EMPTY][PRESERVE_REAGENT]`); product 1 `POWDER_MISC` of
  `INORGANIC:PLASTER`, `PRODUCT_DIMENSION:150`, put into bag B; `[FUEL]`; skill SMELT
  (labor Furnace operating). Entity `PERMITTED_REACTION:MAKE_PLASTER_POWDER` (dwarves have it).
- PLASTER: stone template, density 2787, value 3, `HARDENS_WITH_WATER:INORGANIC:GYPSUM`,
  `NO_STONE_STOCKPILE`. Embark cost 3 per unit incl. a free bag (wiki).
- Apply cast (job "Apply Cast"; bone doctor): needs plaster powder, a **bucket** with water from a
  water source, and **cloth**. Casts do the same job as splints (immobilize).

**Rustfort now:** kiln does pottery and pearlash only; no plaster.

**Design:**

- `industry/reactions.rs` (shared, mark lines): `Rg::Gypsum` (boulder whose material tags contain
  `RC:GYPSUM`; the palette already carries `RC:*` tags, see `stockpile_ed.rs:107`),
  `Rg::EmptyBag` (preserved, receives product), `Pr::Powder("PLASTER")` (POWDER_MISC, material
  looked up by token `PLASTER`; if the palette lacks it, add a MaterialInfo `INORGANIC:PLASTER`
  with value 3 at load, like other synthesized materials). Table row:
  `Reaction { key: "MAKE_PLASTER_POWDER", reagents: &[Gypsum, EmptyBag], product: Pr::Powder("PLASTER") }`.
  `production.rs` recipe row for the kiln ("Make plaster powder", labor SMELT, fuel, 300 ticks
  ESTIMATE). Extend `reaction_table_matches_raws` to also read `reaction_other.txt` for this key.
- `ApplyCast` job steps: GoItem(powder; take the bag) -> GoItem(empty bucket) -> GoWater (nearest
  water tile, `nearest_water`) -> GoItem(cloth) -> GoPatient -> Work 400 ticks (ESTIMATE). On
  success: powder item destroyed, bag dropped empty at the patient (haulable), bucket dropped,
  cloth consumed, a new item `ORTHOPEDIC_CAST` (subtype "", material GYPSUM, ESTIMATE: DF names
  the item "cast") created and attached `Strapped(part)`; wound gets `mx::CAST`.
  If any step fails (no water reachable, item gone) -> cancel and let the splint path run.
- Choice (ESTIMATE): cast when powder, an empty bucket, cloth and reachable water all exist;
  splint otherwise (wiki: splints are the easier alternative). Both immobilize identically
  (ESTIMATE; DF does not document a difference).

**Tests:** `kiln_makes_plaster_from_gypsum_into_bag`, `anhydrite_is_not_gypsum`,
`cast_applied_with_powder_bucket_water_cloth`, `cast_falls_back_to_splint_without_powder`.

---

## M7. Traction benches

**DF rules:**

- (OK) Built from 1 table + 1 mechanism + 1 rope or chain; material = the table's; component quality
  irrelevant. Must stand in the hospital zone. **One patient at a time.** Treatment can take a long
  time ("weeks or months").
- (OK/DEV) Used for overlapping fractures; outside the bench an overlapping fracture rebreaks fully
  and never heals.
- (OK, bug 4470, fixed 0.40.12) Healed patients are released (do not replicate the old bug).

**Rustfort now:** buildings `TractionBench` exist (reference fort: 6) but do nothing.

**Design:**

```rust
#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) struct Bench { pub building: i32, pub tile: u32, pub hosp: u16, pub occupant: u32 /*slot or NONE*/, pub since: u64 }
```

- Benches rebuilt with the hospitals (M1), keeping occupants by building id.
- `PlaceInTraction` job (bone doctor, ESTIMATE labor BONE_SETTING): for a patient with an
  `OVERLAP` wound that is set (`!NEEDS_SET`) and not TRACTION: reserve a free bench in a reachable
  hospital, doctor walks to the patient, **carries** it (reuse `carry_start` / `carry_drop` from
  the rescue code) to the bench tile, lays it down (`Sub::Resting(-1)`, position = bench tile),
  sets `mx::TRACTION` on all its OVERLAP wounds, `bench.occupant = slot`. Work 300 ticks
  (ESTIMATE) after arrival.
- While in traction: `health_bedridden` / `health_keep_in_bed` return true (hook line
  `|| self.h15_in_traction(s)`) so caretakers bring water/food (existing FEED logic).
- Heal: OVERLAP wounds heal at the bone rate (M8) only with TRACTION; without it rate 0 (DEV).
- Leaving early (patient moved by combat/fleeing, bench deconstructed, bench destroyed): every
  OVERLAP wound gets `sev = 10_000`, TRACTION cleared, announcement "X's bone rebreaks" (DEV).
- Release: when the unit has no OVERLAP wound left -> `occupant = NONE`, patient resumes normal
  rest/AI.
- No bench in any hospital: the patient is set and splinted but the OVERLAP wound does not heal
  (DEV); UI shows "needs traction bench".

**Tests:** `overlap_without_bench_never_heals`, `overlap_patient_placed_in_traction_and_released`,
`bench_removed_rebreaks_bone`, `one_patient_per_bench`.

---

## M8. Healing rates per tissue

**DF rules (OK, raws):** `HEALING_RATE` per tissue: lower heals faster ("time between healing
increments"); SKIN, FAT, MUSCLE, EYE, LUNG, HEART, LIVER, GUT, STOMACH, GIZZARD, PANCREAS, SPLEEN,
KIDNEY, SCALE, CHITIN, SPONGE = 100; BONE, SHELL = 1000; NERVE, BRAIN, CARTILAGE, NAIL, CLAW, TALON,
TOOTH, IVORY, HORN, HOOF, HAIR/whisker templates, FEATHER, SPINE, FLAME: none. (OK, wiki Wound
v0.34) nerve damage and lost limbs are permanent; (OK, wiki v0.31 Wound) giving NERVE_TEMPLATE a
HEALING_RATE makes spinal nerve damage heal - i.e. "no HEALING_RATE" = does not heal.
Hair/nail regrowth is a separate DF mechanic (ESTIMATE: treat hair/whisker wounds as healing at 100
so they leave the active list).

**Rustfort now:** one rate for all tissues.

**Design:**

- rf-core `TissueLayerRaw.healing_rate: i32` `#[serde(default)]` (exporter already writes it;
  0 = none). `LayerT.heal: u16` `#[serde(default)]` filled at body build: exported value, else a
  fallback by tissue name (BONE/SHELL 1000; NERVE/BRAIN/CARTILAGE/NAIL/CLAW/TALON/TOOTH/IVORY/HORN/HOOF 0;
  everything else 100); HAIR-like (`LayerT.hair`) forced to 100.
- `heal_pass` (combat.rs): multiply the per-100-tick rate by `100 / heal` (`heal == 0` -> 0); one
  marked line using a tiny `#[inline] fn h15_heal_scale(lt: &LayerT, rate: i32) -> i32`. Bones:
  rest 6 / hospital 10 per 100 ticks -> 10 000 severity in 100 000 ticks (~83 days, ~2.8 months)
  in a hospital (ESTIMATE, consistent with "weeks or months").
- Non-healing wounds (heal 0) are converted once bleeding has stopped and they are treated (or
  `age > TICKS_PER_MONTH`) into **permanent damage**: removed from `f.wounds`, recorded in
  `Disab` (M9) and pushed into `f.scars` as an rf_core::Wound so `Unit::wounds` keeps them (DF shows
  them forever). Without this they would keep the unit in `combat.wounded` forever (perf).
- Partial-wound per-part layer lookup is O(1): `bodies[b].layers[w.layer]` (part-level wounds with
  `layer == u16::MAX` use 100).

**Hash:** changes (bones now heal ~10x slower). Expected.

**Tests:** `bone_heals_ten_times_slower_than_muscle`, `nerve_wound_never_heals_becomes_permanent`.

---

## M9. Nerve damage and permanent disabilities

**DF rules:**

- (OK, wiki Organ/Wound) Motor nerves actuate the muscles that move a limb: a severed motor nerve
  paralyses that part and the parts connected beyond it; a leg with motor nerve damage means the
  dwarf never stands on it again. Sensory nerve damage removes pain from the part (and may weaken
  attacks with it). Nerve damage is permanent; lost limbs are permanent.
- (OK, measured) DF records these as `MOTOR_NERVE_SEVERED` / `SENSORY_NERVE_SEVERED` on the
  **MUSCLE layer** wound of a limb part; they persist on scars.
- (OK, wiki) Spinal fractures do not paralyse; torn spinal nervous tissue does. (Rustfort already
  sets `paralyzed` for a torn NERVOUS part; keep.) Spinal nerve injuries get **no crutches**.
- (ESTIMATE) Chance a deep cut severs a nerve: an edged (CUT|TORN) MUSCLE wound with `sev >= 6_000`
  on a part with LIMB, GRASP or STANCE flags: motor 1/6, sensory 1/6, independent rolls
  (`Rng::for_unit(seed, s, now, 0x4E52)`). Reference fort: 6 nerve flags among 111 EDGED_DAMAGE
  entries (~5 %) - the 1/6-on-deep-cuts rule gives a similar overall share; re-check with
  `validate`.
- (ESTIMATE) Upper-spine (neck) paralysis = bedridden forever (needs feeding); lower spine = cannot
  stand, crawls, no crutch.

**Rustfort now:** limb nerves not modelled; imported nerve flags lost (0.1.3).

**Design (module `health15/disab.rs`):**

```rust
#[derive(Serialize, Deserialize, Clone, Default)] #[serde(default)]
pub(crate) struct Disab {
    pub slot: u32,
    pub motor: Vec<u16>,      // parts with a severed motor nerve (sorted, dedup)
    pub sensory: Vec<u16>,
    pub perm: Vec<(u16, u16)>,// (part, layer) of non-healing permanent damage (M8)
    pub crutch: i32,          // held crutch item id, -1
    pub walk: u8,             // 0 normal, 1 crutch, 2 crawl, 3 bedridden
    pub need_crutch: bool,
    pub last_pos: u32,        // for crutch XP
    pub attached: Vec<(i32, u16, u8)>, // M3 strapped items (item, part, kind)
}
// Health15 { dis: Vec<Disab> sorted by slot }  (few entries; binary search by slot)
```

- Classification (M4 pass) sets `mx::MOTOR/SENSORY` on the muscle wound and records the part in
  `Disab`. **Import**: `init_health` also scans `f.scars` (the rf_core wounds) for the two nerve
  flags and fills `Disab` (fixes 0.1.3).
- Effect, re-applied after every `heal_pass` for listed units and once after load: for each motor
  part, set `part_state |= 4` on the part and all `children` (DFS, small). (`heal_pass` only clears
  bit 2, so bit 4 survives; re-apply anyway after `ensure_body`.)
- Consumers of bit 4 (one guard each, `// [health15]`): `cannot_stand` (a STANCE part with 1|4 ->
  cannot stand unless crutch, M10), climbing's grasp test (`st & 3` -> `st & 7`), combat weapon/
  shield hand choice if it tests GRASP parts (implementer: grep `bp::GRASP`), `health_info`.
- Sensory: set `pain = 0` on every wound of that part and its children at classification and for
  new wounds there (pain decays multiplicatively in `heal_pass`, so 0 stays 0).
- Lost parts (`part_state & 1`, SEVERED stump wounds already persist) feed the same crutch logic.

**Tests:** `imported_motor_nerve_scar_disables_hand`, `motor_nerve_in_leg_needs_crutch`,
`sensory_nerve_part_feels_no_pain`, `nerve_roll_deterministic`.

---

## M10. Crutches and crutch walking

**DF rules:**

- (OK, wiki Crutch v50) Patients who lost the use of their legs from short-term injuries, permanent
  motor nerve damage or amputation are assigned crutches by hospital staff; dwarves with spinal
  nerve injuries do not get crutches. Crutches are finished goods (carpenter 1 log, forge).
- (OK, wiki Crutch walker v0.34) Skill CRUTCH_WALK reduces the movement penalty; at Legendary+5 the
  movement is no longer impaired; it does not apply to swimming, climbing, crawling.
- (OK, raws) Dwarf crawl gait 2990 vs walk 900 (ratio 3.32).
- (ESTIMATE) Base crutch penalty: move delay x2 at skill 0, falling linearly 5 % of the base per
  rating level (same shape as the wiki's Ambusher rule) to x1 at rating 20:
  `delay = walk * (40 - r) / 20`, `r = social_rating(CRUTCH_WALK) in 0..=20`.
- (ESTIMATE) A crutch needs one functional GRASP part (not severed, not motor-dead) and at least one
  functional STANCE part; otherwise the unit crawls.
- (ESTIMATE) Bring crutch job: labor BONE_SETTING (bone doctor), 20 ticks after arrival; the crutch
  is held (inventory) until `need_crutch` is false for 1 day, then dropped where the unit stands.
- (ESTIMATE) XP: +10 CRUTCH_WALK per 100-tick pass in which the unit moved while on a crutch
  (reference fort's average dwarf has 70.5 xp in CRUTCH_WALK, `docs/VALIDATION.md`).

**Rustfort now:** crawl at `CRAWL_FACTOR = 4`; no crutches.

**Design:**

- `need_crutch(s)` = any STANCE part with `part_state & (1 | 4)` or an immobilized broken STANCE
  limb (M5), AND not `paralyzed` AND crutch-capable (above).
- `CRAWL_FACTOR` -> exact ratio: `crawl = walk * 2990 / 900` for dwarves; better: rf-core
  `CasteRaw.crawl_speed: i32` `#[serde(default)]` (exporter: additive, the caste's CRAWL gait
  full-speed value) and fall back to `walk * 2990 / 900`. Update the test
  `without_rescuers_the_patient_crawls` to the new value (`[health15]`).
- `health_gait` (health.rs) is called for wounded units only; disabled-but-healed units must keep
  their gait: the 100-tick health15 pass runs `h15_gait(s)` for every `Disab` entry, setting
  `ai.speed` to walk / crutch / crawl delay when the mode or the rating changed. `health_gait`
  calls `h15_gait` first and returns if it handled the unit (one line).
- Wounded units that only need a crutch are not bedridden: they may leave bed after treatment
  (DF: splinted dwarves leave the hospital).
- `BringCrutch` claims from hospital chests first (M2), else anywhere.

**Tests:** `lost_foot_gets_crutch_and_walks_slow` (speed 1800 at rating 0),
`legendary_crutch_walker_not_slowed` (900 at rating 20), `no_crutch_crawls_at_gait_ratio` (2990),
`spinal_paralysis_gets_no_crutch`, `crutch_dropped_after_recovery`.

---

## M11. Treatment outcome by skill

**DF rules:**

- (OK, wiki) Diagnosis skill changes only the time taken, not the result.
- (OK, wiki Surgeon, marked [Verify]) Surgery skill relates to success rate and post-operation
  infection.
- (OK, wiki) Surgery not performed on a table (patient in bed) has its skill rolls cut in half.
- (Steam discussion, ESTIMATE) Wound dressing skill (cleaning with soap) lowers infection.
- No other success numbers are public -> all below are ESTIMATE.

**Design (module `health15/outcome.rs`):** one pure function used by `apply_treatment`
(hook: `let ok = self.h15_outcome(d, p, t, now); // [health15]` before the branch; on failure the
branch applies the failure effect instead).

```rust
/// Success chance in percent for rating r (0..=20, `social_rating`), deterministic roll
/// `Rng::for_unit(seed, patient, now, 0x0C0E ^ treat as u64)`.
pub(crate) fn success_pct(t: Treat15, r: i32) -> u32
```

| Treatment | Success % (ESTIMATE) | Failure effect (ESTIMATE) |
|---|---|---|
| Diagnose | 100 | - (time only, OK) |
| Suture | 70 + 3/2 r (max 100) | thread still sewn in (used), wound stays unsutured, bleeding halved; retried |
| Set bone | 60 + 2 r | no item used; +500 sev (pain); retried |
| Immobilize / cast / traction placing | 100 | - |
| Dress (+clean with soap) | dress 100; clean 60 + 2 r | `CLEAN_FAIL`: DRESSED but not CLEANED; soap still used |
| Surgery | 50 + 5/2 r, with r halved if not on a table | `SURG_FAIL`: sev +1 500, bleed +3, infection 1/3; success: sev * 2/3 (existing), infection 1/(20 + r) |

"On a table" (ESTIMATE): the patient rests inside a hospital that has at least one `Table`
building (`Hosp.tables`). DF moves patients to tables; we do not move them (documented gap).

**Tests:** `success_pct` table test; `legendary_surgeon_beats_novice` (Monte-Carlo over seeds);
`surgery_in_hospital_without_table_uses_half_skill`.

---

## M12. Surgery dangers and indications

**DF rules (OK unless marked):** surgeons repair internal organ damage, excise necrotic tissue,
treat serious muscle/bone injuries (incl. compound fractures), and treat infected or rotten wounds.
The patient must be at rest or the surgeon cancels. Several surgeons may work at one table.

**Design:** extend `next_treat`'s Surgery condition (health.rs, marked line) with:
`mx::COMPOUND && !OPERATED` (before SetBone) and `INFECTED && age >= 7 days (ESTIMATE) &&
!OPERATED`. In `apply_treatment(Surgery)` (marked call into health15): COMPOUND -> clear COMPOUND,
keep NEEDS_SET; infected wound -> on success clear INFECTED (excision), sev -= 1 000; on failure
see M11. Rest requirement: already `in_care`. Necrosis/rot of wounds: MISSING (coordinate with
`decay15` if it models rot; not part of this round).

**Tests:** `compound_fracture_needs_surgery_before_setting`, `surgery_clears_infection_on_success`.

---

## M13. Cleaning and infection (refinement only)

**DF rules (OK):** every open wound can become infected; causes: never cleaned, stagnant or muddy
water, cleaning without soap, bad luck; infection = constant internal bleeding that a healthy dwarf
can outpace; soap is the only sterilizer; cleaning also uses cloth and water. No numbers public.

**Design:** keep `infection_daily`'s ESTIMATE odds; only: CLEANED requires a successful clean roll
(M11), soap is consumed (M3). Water quality: MISSING (no stagnant/fresh distinction; note in
FIDELITY). Reference-fort note (OK, measured): 13 exported wounds carry INFECTION on scars aged up
to 9.7 M ticks - these are dormant and must stay scars (current `load_wounds` does this; add a test
so nobody "fixes" it into a lethal infection).

**Tests:** `imported_infected_scars_stay_dormant`.

---

## M14. Diagnosis and the chief medical dwarf

**DF rules (OK, wiki DF2014):** no treatment without diagnosis; diagnostician skill only changes
the duration. The chief medical dwarf enables the fortress health screen and per-unit health
summaries and has **no other in-game function**; his effect on diagnosis job creation is
unverified. Severity tiers shown after diagnosis: NONE, MINOR, INHIBITED, FUNCTION LOSS, BROKEN,
MISSING.

**Design:** no sim change (hash unchanged). API `chief_medical_dwarf() -> Option<i32>` reads the
noble assignment (owned by `nobles15`; read only). UI (rf-app `ui/health15.rs`): the fort health
overview lists patients with the DF tier per part:
`MISSING` = part_state&1, `BROKEN` = BROKEN wound, `FUNCTION LOSS` = part_state&4,
`INHIBITED` = sev >= 5 000 or motor/sensory, `MINOR` otherwise; shown only for diagnosed wounds;
without a chief medical dwarf show a one-line note "DF hides this screen without a chief medical
dwarf" (UI choice, ESTIMATE) instead of hiding it.

---

## M15. Rest and pain (no change, cross-checks)

- Rest: hospital beds first (OK, existing); hospital rate applies once M1 lands.
- Pain: existing `pain_threshold`/unconsciousness; M9 zeroes pain on sensory-dead parts.
- Bone pain receptors (raws: BONE `PAIN_RECEPTORS:50`, soft tissue 5) are already in `LayerT.pain`.

---

## 1. Module layout and hooks

New files (all `[health15]`):

- `crates/rf-sim/src/health15/mod.rs` - `Health15` state, pass functions, hash, after-load rebuild.
- `crates/rf-sim/src/health15/supplies.rs` - M1, M2, M3.
- `crates/rf-sim/src/health15/ortho.rs` - M4, M5, M6, M7, M8 helpers.
- `crates/rf-sim/src/health15/disab.rs` - M9, M10.
- `crates/rf-sim/src/health15/outcome.rs` - M11, M12.
- `crates/rf-sim/src/health15_api.rs` - public API.
- `crates/rf-app/src/ui/health15.rs` - hospital panel and unit medical tab.
- `crates/rf-sim/tests/health15.rs` - integration tests.

```rust
#[derive(Serialize, Deserialize, Default)] #[serde(default)]
pub(crate) struct Health15 {
    pub off: bool,
    pub hosp: Vec<Hosp>,
    pub benches: Vec<Bench>,
    pub stock: Vec<StockJob>,
    pub jobs: Vec<MedJob>,      // Immobilize, ApplyCast, PlaceInTraction, BringCrutch
    pub dis: Vec<Disab>,        // sorted by slot
    pub soap_units: i32,
    pub rep: Health15Report,    // counters + events (cap 400 like HealthReport)
    #[serde(skip)] pub earmarked: Vec<i32>,
    #[serde(skip)] pub hosp_dirty: bool,
    #[serde(skip)] pub open: usize,
}
#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) struct MedJob { pub patient: u32, pub kind: Med, pub doctor: u32, pub items: [i32; 4], pub step: u8, pub until: u64, pub target: u32 }
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Med { Immobilize, ApplyCast, PlaceInTraction, BringCrutch }
```

`Sim` field: `h15: health15::Health15, // [health15]` (+ default in `Sim::new`).

| Hook | Where (shared file) | When |
|---|---|---|
| `h15_tick(now)` | `lib.rs` `tick_inner`, right after `self.health_tick(now)` | 100-tick pass at `now % 100 == 47` (classify, gait, traction, detach, job list); stock pass `now % 400 == 247`; daily `now % TICKS_PER_DAY == 667` (hospital rebuild, have recount, earmarks). Free phases checked: 0, 1, 11, 37, 41, 50, 59, 61, 73, 83 are taken; daily 611, 637, 733, 857 taken. |
| `h15_decide(s, now) -> bool` | `lib.rs` `decide`, after `health_decide` | worker idle |
| `Task::Med15` arrive/cancel | `lib.rs` think/arrive match, `cancel_task` | |
| `h15_hash(mix)` | `lib.rs` `state_hash`, `persist.rs` (`v.push(("health15", ...))`) | |
| encode | `saves11.rs` list `("health15", enc(&self.h15))` | exact snapshot |
| `h15_after_load()` | `saves11.rs` `after_exact_load` | rebuild earmarks, `have`, re-apply part_state bit 4 |
| `mx` field | `combat.rs` `WoundS` + 7 literals | |
| heal scale | `combat.rs` `heal_pass` rate line | |
| `LayerT.heal/settable` | `combat.rs` body build from `TissueLayerRaw` | |
| hospitals overwrite | `combat.rs` none (health15 writes `combat.hospitals`) | |
| SetBone supply None, Surgery indications, sutures sewn in, outcome roll, `cannot_stand`, `health_gait`, `health_bedridden`, `health_keep_in_bed`, `care_find_supply` earmark bonus, `init_health` flag mapping, `pub(crate) med` | `health.rs` | |
| plaster | `industry/reactions.rs`, `production.rs` | |
| earmark guards | production reagent picker, storage haul candidates | |

Saves: everything in `Health15` except `#[serde(skip)]` caches goes into the exact snapshot.
`hosp` is serialized because `want` is player-editable. Old saves load with defaults (serde) and
`h15_after_load` rebuilds hospitals with `want` defaults.

## 2. rf-core / export additions (additive, `#[serde(default)]`)

- `TissueLayerRaw.healing_rate: i32` (already in the export JSON).
- `Location.supplies: Vec<LocationSupply { item, desired, count }>` (already in the export JSON).
- `CasteRaw.crawl_speed: i32` - exporter: caste CRAWL gait full speed (new extra field).
- Optional exporter extras on units: `inventory_modes: [[item_id, mode]]` (SewnInto/Strapped/...),
  `health_flags` (`unit.health.flags` names) - document in `docs/EXPORT_FORMAT.md`; read-only.

## 3. Public API (`health15_api.rs`)

`hospitals() -> Vec<HospitalInfo {loc, name, zones, chests, want: [i32;7], have: [i32;7], benches, tables}>`,
`set_hospital_supply(loc_or_zone: i32, sup: Sup, items: i32)`,
`unit_medical(unit_id) -> UnitMedical {fractures: Vec<(part, class, immobilization)>, motor: Vec<String>, sensory: Vec<String>, missing: Vec<String>, crutch: Option<i32>, walk: Walk, in_traction: bool, tiers: Vec<(part, &'static str)>}`,
`health15_report() -> Health15Report`, `set_health15_enabled(bool)`, `chief_medical_dwarf()`,
test helpers `h15_break_bone(unit, part_token, class)`, `h15_sever_nerve(unit, part_token, motor)`.

## 4. Performance notes

- All per-unit work runs only for `combat.wounded` (already a small list) plus `h15.dis` (units with
  permanent damage; tens at most) every 100 ticks; no work for healthy units.
- No allocation in the 100-tick loops: iterate indices, reuse a `Vec<u16>` scratch in `Health15`
  (`#[serde(skip)]`) for part DFS.
- No new world item scans: supply counting piggybacks on `health_refresh_daily`'s loop; stocking
  scans only while a hospital is short, every 400 ticks.
- `heal_pass` adds one table lookup per wound.
- `WoundS` size unchanged (24 bytes).
- Bench check: `rustfort bench --ticks 806400 --standdown` within 5 %; the state hash changes (M1,
  M3, M5, M8, M10) - state why in the commit.

## 5. Determinism

RNG only via `Rng::for_unit(self.seed, slot, now, SALT)` with salts 0x4815 (overlap), 0x4E52
(nerves), 0x0C0E ^ kind (outcomes). Job/hospital lists are Vecs in sorted order; no HashMap
iteration; earmark list sorted. All passes serial.

## 6. Prioritized implementation list

1. **M1 hospital recognition** (bug fix; small, big effect on the reference fort).
2. **M8 per-tissue healing + permanent damage conversion** (raws-exact; prerequisite for nerves and
   fracture timing).
3. **M9 nerve damage + import of nerve scars, M10 crutches + exact crawl ratio** (fixes lost data;
   visible disabilities).
4. **M4/M5 fracture classes, set vs immobilize, splints** (WoundS `mx`, job list, `Task::Med15`).
5. **M3 consumption fidelity** (sutures sewn in, strapped splints removed, soap units).
6. **M2 hospital supply chests and stocking** (+ `Location.supplies` import, earmark guards).
7. **M11/M12 outcomes by skill, surgery dangers and indications** (compound fractures, excision).
8. **M6 plaster powder + casts** (kiln reaction, multi-item job).
9. **M7 traction benches** (depends on 4, carry code).
10. **M14 UI health overview / M13 tests**, FIDELITY section `[health15]`.

## 7. Draft FIDELITY rows (to append under a `## [health15] ...` heading)

| Item | Class | Notes |
|---|---|---|
| v50 hospital locations on meeting zones count as hospitals | FIXED | wiki Hospital; reference fort zone 10634 |
| Sutures stay sewn into the patient | OK | measured: thread carriers == sutured dwarves |
| Hospital supplies (7 kinds, chests, earmark, unit conversions) | OK/ESTIMATE | defaults ESTIMATE |
| Plaster powder (kiln, gypsum-class boulder + bag, fuel, SMELT) | OK | raws `MAKE_PLASTER_POWDER` |
| Cast needs powder + bucket + water + cloth | OK | wiki Cast; duration ESTIMATE |
| Overlapping fractures need a traction bench, rebreak outside | OK/DEV | Toady One |
| Compound fractures need surgery | OK | wiki DF2014 Health care |
| Per-tissue HEALING_RATE (bone 10x slower, nerves never) | OK | raws |
| Motor/sensory nerve damage permanent, imported from scars | OK | odds ESTIMATE |
| Crutches for leg function loss; none for spinal injury | OK | wiki Crutch |
| Crutch penalty x2 -> x1 at Legendary+5; crawl 2990/900 | OK shape / ESTIMATE base | wiki Crutch walker, raws |
| Treatment success by skill; surgery off-table halves skill | ESTIMATE / OK | wiki Surgeon |
| Chief medical dwarf: screen only | OK | wiki DF2014 |
| Water quality for cleaning, wound necrosis, moving patients to tables | MISSING | |
