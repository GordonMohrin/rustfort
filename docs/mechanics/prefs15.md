# [prefs15] Preferences, item quality and item value: mechanics spec (target v0.15)

This is the implementer's spec for tag `[prefs15]`. For each mechanic it gives the DF v50 rule, the
source, what Rustfort has now (file / function), the gap and a Rust design. Anything not stated
on the wiki or measured in the reference export is marked **ESTIMATE**.

Sources:
- `w:` = dwarffortresswiki.org, v50 main namespace pages, read on 2026-11-18 with `action=raw`:
  Preferences, Item quality (+ `Item quality/Table`), Item value, Skill (item quality formula,
  skill penalties), Strange mood, Legendary artifact, Thought, Personality need, Personality facet,
  Zone (Quality and value), Engraving, Decoration, Wealth, Pet, Kitchen, Mandate, Demand,
  Hateable, Shape.
- `raws:` = `data/vanilla` of the local install (read only). Counts used below:
  `inorganic_stone_layer` 25, `inorganic_stone_mineral` 58, `inorganic_stone_soil` 21,
  `inorganic_stone_gem` 127, `inorganic_metal` 26, `inorganic_other` 8 objects;
  `descriptor_shape_standard` 43 shapes, `descriptor_color_standard` 136 colors;
  22 creatures with `[VERMIN_HATEABLE]`.
- `export:` = the reference fort export `export/tileshtulon` (10.10.2026, 87 citizens). Numbers
  measured with throw-away scripts (not committed): item values, artifact values, thought
  severities and ages.
- DFHack structure names (`unit_preference`, `unit_preference_type`) are from df-structures as
  remembered by the analyst: **verify** field names with a read-only `printall` in the probe
  before shipping the exporter change (section 2.2).

---------------------------------------------------------------------------------------------

## 0. Summary of findings (what is wrong today)

| # | Finding | Effect |
|---|---|---|
| F1 | Units have no preferences at all (`rf_core::Unit` has no field, exporter does not export `soul.preferences`). | every preference effect missing; the "+10 per preference" term of the quality roll is always 0 (`skills.rs` `quality_points_roll(..., prefs = 0, ...)`) |
| F2 | `quality_points_roll` uses `Rng::below(n)` = `0..n-1`, the wiki's `rand(0..N)` is **inclusive** (`0..=N`). | every roll is 1-3 points too low (mean quality at skill 2: 0.49 instead of 0.65) |
| F3 | `trade::quality_mult` = 1/2/3/4/5/12/120 is the **pre-v50** table. v50 items use x1 / x1.1 / x1.2 / x4/3 / x1.5 / x2 / x20 **plus a flat bonus** +0/+3/+6/+10/+15/+30/+300 (`w:Item quality/Table`, `w:Item value`, `w:Zone`). Verified on the export: the large majority of items fit `floor(base x mat x mult) + bonus` exactly (e.g. 878/878 boulders, 166/209 beds, 229/301 tables; the rest are decorated or floor-rounded). Only engravings still use the old table (`w:Engraving`, `w:Zone`). | item, trade, created-wealth (siege triggers!), room and meal values far too high for quality goods (a masterwork chair: Rustfort 5x1x12 = 60, DF 10x1x2+30 = 50; an artifact wood amulet: Rustfort 1 200 + 2 x decorations, DF 500 + 500 per wood decoration) |
| F4 | `trade::base_value` differs from the wiki / export for many kinds (furniture 5 instead of 10, statue 25, window 25, mechanism 30, blocks 3 -> 5, boulder/log 1 -> 3, cut gem 10 -> 20, tanned hide 10 -> 5, shoes/gloves 10 -> 5, cheese 10, traction bench 20, ...). | same as F3 |
| F5 | Mood artifacts ignore preferences (item type, metal, cloth, glass, shell) and use the wrong value (`10 x mat x 120 + 2 x decorations`). DF: `floor(base x mat x 20) + 300` + each decoration at artifact quality. | artifacts too valuable, wrong kinds |
| F6 | `moods.rs` `MOODABLE` uses `BONE_CARVE`, `CUT_GEM`, `ENCRUST_GEM`; DF's `job_skill` names (export) are `BONECARVE`, `CUTGEM`, `ENCRUSTGEM`. Bone carvers and jewelers are never picked for their skill. (`skills.rs` `quality_attrs` already uses the right names.) | wrong mood skill for bone carvers / gem cutters / setters |
| F7 | Admiring furniture: Rustfort needs quality >= 2/3 and sets severity = quality; DF admires regardless of quality, the happiness depends only on the item's **value in the dwarf's eyes** (preferences multiply it). In the export the `AdmireBuilding` severity *is* that value (10, 14, 18, 23, 30, 50 = an ordinary..masterwork 10-value chair; 80..551 for valuable / liked ones). | wrong admire thoughts, no preference effect |
| F8 | `SatisfiedAtWork` is random by skill (`social_work`); DF gives it only for work with a liked material, item or animal, never when CHEER_PROPENSITY <= 24. | too many happy thoughts for everybody, none tied to preferences |
| F9 | `GoodDrink` needs drink quality >= 1 (drinks have none, so never), `GoodMeal` fires on every meal. DF: GoodDrink only for a liked booze, GoodMeal only if the meal matches a preference or is worth >= 20; level by value 5/25/100/200/400. `SameFood`/`SameBooze` exempt liked food. | meal/drink thoughts not like DF |
| F10 | Needs `AcquireObject` and `BeExtravagant` are never satisfied (no hook); `AdmireArt` only by quality furniture. | permanent distraction for greedy / immodest dwarves |
| F11 | Noble mandates pick from a fixed list (`population.rs` `MANDATE_ITEMS`); DF: only items the noble likes, none without an item preference. | wrong mandates |
| F12 | Rooms: preferences multiply furniture value and double engravings in a liked stone **in the eyes of the dwarf** (`w:Preferences`, `w:Engraving`). Display furniture contents count only if liked (`w:Zone`). | bedroom/dining thoughts too low for owners with matching tastes |

The v0.12 gap "craft quality below DF" (crafts mean 1.8 vs DF 2.6): F2 + F1 are the two levers
the wiki gives. Expected effect (simulated with the wiki formula, crafter with 1000 in Agility,
Creativity, Spatial and Kinesthetic sense, focus 100 %):

| level | now (exclusive rand, no pref) | F2 fixed | + 1 preference | + 2 preferences |
|---|---|---|---|---|
| 0 | 0.18 | 0.21 | 1.25 | 2.68 |
| 2 | 0.49 | 0.65 | 1.97 | 3.23 |
| 5 | 1.36 | 1.57 | 2.85 | 3.77 |
| 8 | 2.39 | 2.54 | 3.52 | 4.07 |
| 15 | 3.58 | 3.65 | 4.07 | 4.25 |

(The wiki's "one preference ~ four skill levels" matches: level 0 + 1 pref ~ level 5.) A material
preference only matches if the crafted material is the dwarf's one liked stone/metal/gem/wood...,
so the fort-wide gain depends on the real preference lists: **measure it with exported
preferences** (validation step in section 9) before tuning anything else.

---------------------------------------------------------------------------------------------

## 1. Preferences: what a unit likes (data model)

### 1.1 DF rule (w:Preferences)

Every dwarf (citizens, migrants, visitors, merchants...) has **at least four** preferences:
exactly one stone-like material, one metal, one gem and one alcohol; everything else by chance.
List with chances (each rolled independently):

Materials (`LikeMaterial`):
| what | chance |
|---|---|
| one stone, ore, pottery material **or** glaze | 1 (exactly one) |
| one metal | 1 (exactly one) |
| one gem | 1 (exactly one) |
| wood | 1/5 |
| glass (green, clear or crystal) | 1/7 |
| leather | 1/3 |
| horn | 1/7 |
| pearl | 1/7 (never happens in practice, wiki) |
| ivory | 1/7 |
| decorative (coral or amber) | 1/15 |
| bone | 1/7 |
| shell | 1/14 |
| silk | 1/14 |
| yarn | 1/14 |
| plant cloth | 1/7 |
| paper or parchment | 1/7 |

Other:
| what | DF type | chance |
|---|---|---|
| a color | `LikeColor` | 1/2 |
| a shape | `LikeShape` | 1/5 |
| weapon (4/5) or ammo (1/5), 1 % exotic | `LikeItem` | 1/3 |
| armor piece, 1 % exotic | `LikeItem` | 1/3 |
| clothing (incl. backpack, quiver), 1 % exotic | `LikeItem` | 1/7 |
| furniture: door, floodgate, bed, chair, window, cage, barrel, table, coffin, statue, box, armor stand, weapon rack, cabinet, bin, hatch cover, grate, quern, millstone, traction bench, slab | `LikeItem` | 1/3 |
| craft: figurine, amulet, scepter, crown, ring, earring, bracelet, large gem | `LikeItem` | 1/3 |
| misc: catapult parts, ballista parts, siege ammo, trap component, coins, anvil, totem (20 %), chain, flask, goblet, bucket, animal trap, instrument, toy, splint, crutch, tool (1 % exotic) | `LikeItem` | 1/3 |
| a domestic animal | `LikeCreature` | 1/2 |
| a non-domestic creature with `[PREFSTRING]` (not own race, wagons, werebeasts, forgotten beasts, titans, angels, experiments, unique demons) | `LikeCreature` | 1/2 |
| a tree | `LikeTree` | 1/50 |
| a plant | `LikePlant` | 1/10 |
| poetic / musical / dance form | `LikePoeticForm` ... | 1/2 each |
| food: meat 1/2, fish 1/3, cheese 1/10, edible plant 1/7, **alcohol 1 (exactly one)**, extract 1/7, mill powder 1/5, seed 1/7, plant growth 1/7 | `LikeFood` | as listed |
| one hateable vermin (`[VERMIN_HATEABLE]`) not already liked | `HateCreature` | 1 (always) |

Exported preferences of real units replace all of this (section 2).

### 1.2 Rustfort now

Nothing. `rf_core::Unit` (entities.rs) has facets, needs, values, emotions, but no preferences.

### 1.3 Design: rf-core (additive)

`crates/rf-core/src/entities.rs`, new struct + one new `Unit` field (mark every line `// [prefs15]`):

```rust
/// [prefs15] One DF unit preference (DFHack `unit_preference`), wiki "Preferences".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UnitPreference {
    /// DF `unit_preference_type` name: `LikeMaterial`, `LikeCreature`, `LikeFood`,
    /// `HateCreature`, `LikeItem`, `LikePlant`, `LikeTree`, `LikeColor`, `LikeShape`,
    /// `LikePoeticForm`, `LikeMusicalForm`, `LikeDanceForm`.
    pub kind: String,
    /// LikeMaterial / LikeFood: palette index of the material (`NO_MAT` if not in the palette).
    pub mat: u16,
    /// LikeMaterial / LikeFood: DF material token (`INORGANIC:GRANITE`, `PLANT:MUSHROOM_HELMET_PLUMP:DRINK`).
    pub mat_token: String,
    /// LikeMaterial: DF `matter_state` name (`Solid`, `Liquid`...), "" if unknown.
    pub mat_state: String,
    /// LikeItem / LikeFood: DF `item_type` name (`WEAPON`, `CHAIR`, `DRINK`, `MEAT`...).
    pub item_type: String,
    /// LikeItem: subtype token (`ITEM_WEAPON_AXE_BATTLE`), "" = any subtype.
    pub item_subtype: String,
    /// LikeCreature / HateCreature: creature token (`CAT`); LikePlant / LikeTree: plant token.
    pub token: String,
    /// LikeColor / LikeShape: descriptor token (`AMBER`, `CIRCLE`); forms: the form id as text.
    pub descriptor: String,
    /// DF `active` flag (false = shown but ignored by DF).
    pub active: bool,
    /// Generated by Rustfort (export had none), see `prefs::derive_preferences`.
    pub derived: bool,
}
```

`Unit` gets: `pub preferences: Vec<crate::UnitPreference>, // [prefs15]` (re-export in
rf-core `lib.rs` with `// [prefs15]`). `#[serde(default)]` is already on `Unit`, old exports and
saves load with an empty list. `rf-import` needs no change: `units.jsonl` deserializes straight into
`Unit` (crates/rf-import/src/lib.rs:723).

Optional (P5, for decorations and art defacement, section 6/7):
`Item::decorations: Vec<ItemDecoration>` (`{ kind: String, mat: u16, quality: u8 }`) and
`Item::maker: i32` with `#[serde(default = "...")]` returning -1. Not needed for P1-P4.

---------------------------------------------------------------------------------------------

## 2. Exporter and import

### 2.1 DF data

DFHack: `unit.status.current_soul.preferences` = vector of `unit_preference` (df-structures):
`type` (`df.unit_preference_type`), a **union** `item_type` / `creature_id` / `color_id` /
`shape_id` / `plant_id` / `poetic_form_id` / `musical_form_id` / `dance_form_id`, then
`item_subtype`, `mattype`, `matindex`, `mat_state`, `active`, `prefstring_seed`.
**Verify the names read-only first** (one `dfhack-run lua` line with `printall` on one dwarf's
`current_soul.preferences[0]`); some DFHack versions keep `active` inside `flags`. Read only the
union member that belongs to the type (the others are garbage).

### 2.2 Exporter change (tools/dfhack/rustfort-export.lua, additive, read only)

Next to `values_json` (line ~840) add `prefs_json(soul)` and call it in the unit writer
(line ~1121) inside `pcall` like the others; key `'preferences', jarr(prefs)` in `r3kv` or a new
`r15kv` block. Per entry:

```lua
local t = df.unit_preference_type[p.type] or tostring(p.type)
kv = { 'kind', ja(t), 'active', jb(try(function() return p.active end) ~= false) }
if t == 'LikeMaterial' or t == 'LikeFood' then
  kv += 'mat', ji(palidx(p.mattype, p.matindex)), 'mat_token', ja(mat_token(p.mattype, p.matindex)),
        'mat_state', ja(df.matter_state[p.mat_state] or '')
  if t == 'LikeFood' then kv += 'item_type', ja(df.item_type[p.item_type] or '') end
elseif t == 'LikeItem' then
  kv += 'item_type', ja(df.item_type[p.item_type]),
        'item_subtype', ja(p.item_subtype >= 0 and (dfhack.items.getSubtypeDef(p.item_type, p.item_subtype) or {}).id or '')
elseif t == 'LikeCreature' or t == 'HateCreature' then
  kv += 'token', ja(df.creature_raw.find(p.creature_id).creature_id)
elseif t == 'LikePlant' or t == 'LikeTree' then
  kv += 'token', ja(df.plant_raw.find(p.plant_id).id)
elseif t == 'LikeColor' then kv += 'descriptor', ja(df.descriptor_color.find(p.color_id).id)
elseif t == 'LikeShape' then kv += 'descriptor', ja(df.descriptor_shape.find(p.shape_id).id)
else kv += 'descriptor', ja(tostring(p.poetic_form_id)) end   -- forms: the id
```

`palidx` adds the material to the palette (it must run before the materials are written, as the
units already do for their inventory). Add `unit_preferences` to the round counters and the
manifest counts like `unit_values`. Document in `docs/EXPORT_FORMAT.md` under a new
"Additions for round 15" heading: `units.jsonl: preferences` with every key above.

`creature_raws.json` already carries flags (`COMMON_DOMESTIC`, `PET`, `LARGE_PREDATOR`,
`VERMIN_HATEABLE` seen in the export); make sure `PREFSTRING` presence is exported as a flag too
(additive: `'HAS_PREFSTRING'` when `#caste.prefstring > 0` or the raw has one; ESTIMATE where DF
keeps it) for derived creature preferences.

### 2.3 Derived preferences when the export has none (old exports, sim-born units)

Module `crates/rf-sim/src/prefs.rs`, `pub(crate) fn derive_preferences(world: &World, u: &Unit) -> Vec<UnitPreference>`.

- Who: every unit with `!flags.animal` whose race is `DWARF` (or any race with
  `CAN_LEARN`/intelligent, ESTIMATE: only dwarves need them in fort mode) **and** an empty
  `preferences` list. A dwarf always has >= 4, so empty means "not generated yet".
- When: in `Sim::init_units` (lib.rs, runs at start and whenever `units.len()` changes: migrants,
  births) through a hook `self.prefs_init_units()` `// [prefs15]`. Results are written into
  `Unit::preferences` with `derived: true` (so they are saved with the world and shown in the UI).
- RNG: **not** the sim's seed (the user asked for "from personality and unit id"; it also keeps
  the same preferences for the same unit across sim seeds): `Rng::new(splitmix(0x9F5E_15 ^
  unit.id as u64 ^ facet_hash))` where `facet_hash` folds the unit's facets in export order
  (`splitmix(h ^ value)` per facet). Deterministic, thread-independent.
- Personality: the wiki ties no preference to facets, so facets only seed the RNG. ESTIMATE.
- Draw order (fixed, as listed in 1.1): stone, metal, gem, wood..., color, shape, items,
  creatures, tree, plant, forms, foods, alcohol, hated vermin.
- Pools (deterministic, sorted by palette index or token):
  - materials: `world.materials` filtered by the class (stone = `is_stone && !is_gem` incl. ores
    and soils with ceramic use; metal = `is_metal && !is_ore`; gem = `is_gem`; wood = `is_wood`;
    glass = `is_glass`; leather / horn / ivory / bone / shell / silk / yarn / plant cloth by token
    suffix `:LEATHER`, `:HORN`, `:IVORY`/`:TOOTH`, `:BONE`, `:SHELL`, `:SILK`, `:YARN`/`:HAIR`,
    plant `:THREAD`; coral/amber by token).
  - **World-size correction (ESTIMATE, keeps DF's odds that the liked material exists here):** DF
    picks from all world materials, the palette only holds what the fort has seen. Draw
    `k = below(max(N_world, n_palette))`; if `k < n_palette` take palette entry `k`, else store a
    preference with `mat = NO_MAT`, `mat_token = "?"` (never matches, shown as "something
    unknown"). `N_world` constants from the raws counts: stone 83 (layer 25 + mineral 58) + 3
    ceramics, metal 26, gem 127, wood = number of `PLANT` raws with `TREE` in `world.plant_raws`
    (else 60), others = palette size (no correction).
  - creatures: `world.creature_raws` (domestic = `COMMON_DOMESTIC` or `PET`; wild = the rest with
    a prefstring flag; exclude own race, `MEGABEAST`, `TITAN`, `UNIQUE_DEMON`, `FEATURE_BEAST`).
  - items: the kind lists in 1.1; weapon/armor/clothing subtypes from `world.weapon_raws` /
    `armor_raws` (non-exotic = present in the fort civ's entity, ESTIMATE: all exported raws).
  - colors 136, shapes 43: store `descriptor` as `COLOR#n` / `SHAPE#n` (ESTIMATE: tokens are not
    in the export; they only matter for dyes and images, which Rustfort does not model).
  - foods: drink materials in the palette (`:DRINK` tokens) for alcohol; meat/fish/cheese/plant by
    token class.
  - hated vermin: creature raws with `VERMIN_HATEABLE` (22 in vanilla raws).
- Cost: once per unit, small Vecs; never in a hot loop.

### 2.4 Runtime cache (perf)

`Prefs` module state on `Sim` (`prefs: prefs::Prefs, // [prefs15]`), with a
`#[serde(skip)]` per-slot cache rebuilt in `init_units` and `after_exact_load`:

```rust
#[derive(Default, Clone)]
pub(crate) struct PrefSet {        // per unit slot
    mats: Vec<u16>,                // liked palette materials (LikeMaterial, active), sorted, dedup
    foods: Vec<u16>,               // liked food/drink materials, sorted
    items: Vec<(u16, u16)>,        // (kind id, subtype id) interned; subtype u16::MAX = any
    creatures: Vec<u32>,           // interned creature tokens (liked)
    hated: Vec<u32>,
    cheer: i32,                    // CHEER_PROPENSITY (for the "didn't feel anything at work" rule)
}
```

Interning: a `BTreeMap<String, u16>` for item kind/subtype strings built once (deterministic).
Lookups are `binary_search` on tiny sorted Vecs; no allocation per query.
`after_exact_load` currently is an empty fn in `saves11.rs:438`: add one line
`self.prefs_rebuild(); // [prefs15]`.

---------------------------------------------------------------------------------------------

## 3. Item quality from skill and preferences

### 3.1 DF rule (w:Skill "item quality formula", w:Item quality)

1. effective level = skill level (Dabbling 0, Novice 1 ... Legendary 15, uncapped) with status
   penalties;
2. `points = rand(0..=10) + rand(0..=5L/2) + rand(0..=5L/2)` (inclusive; integer `5L/2`);
3. `+ rand(0..=phys1)/100 + rand(0..=phys2)/250 + rand(0..=phys3)/250` for the skill's physical
   attributes;
4. same for the mental attributes;
5. `points = points * current_focus / undistracted_focus`;
6. status penalties again on the points;
7. luck curse/blessing (not modelled);
8. **+10 if the product matches an item preference** of the maker;
9. **+10 if the product matches a material preference** of the maker;
10. 0-21 ordinary, 22-29 well-crafted, 30-34 finely-crafted, 35-44 superior, 45-54
    exceptional, 55+ masterwork with chance 1/3, else exceptional.

`SKILL_ROLL_RANGE:11:5` is the default (11 = size of the flat roll, 5 = the level factor).
Penalties (w:Skill): nausea, winded, stunned, dizzy, fever -50 %; blind, extreme pain -75 %;
tired -25 %, over-exerted -25 % twice, exhausted three times; dehydrated, starving, very drowsy
-50 %; compounded multiplicatively. Preferences do not change the work speed (wiki says nothing).

### 3.2 Rustfort now

`skills.rs` `quality_points_roll` implements 1-10 with exclusive `below` (F2) and `prefs = 0`
from `skills_job_done` (F1). Meals use the old `quality_roll` estimate (`lifestyle.rs`
`lifestyle_meal_cooked`), engravings `construct.rs` `roll_quality` (`cx_finish_engrave`).

### 3.3 Design

- Fix F2 in place in `skills.rs` (one function, lines marked `// [prefs15]`):
  `r.below(11)`, `r.below(half + 1)`, attributes `r.below(a + 1)` (with `a.max(0)`).
- `skills_job_done`: per product item, `let prefs = self.prefs_quality_bonus(s, idx);` where
  `prefs_quality_bonus` (prefs.rs) returns `item_match as i32 + mat_match as i32`:
  - item match: the item's `(kind, subtype)` equals a LikeItem `(item_type, item_subtype)`;
    subtype "" in the preference matches any subtype; items whose DF type differs from the
    Rustfort kind (`TRAPPARTS` = mechanism, `SMALLGEM`, `GEM`) are compared by DF item_type name,
    which Rustfort already uses for `Item::kind`.
  - material match: the item's `mat` is in `PrefSet::mats` (exact palette index; DF compares
    mattype/matindex). ESTIMATE: mat_state is ignored for solid products.
  - Call it once per product, before the roll (the roll is per item already).
- Engravings (`construct.rs` `cx_finish_engrave`): switch to `quality_points_roll` with the
  ENGRAVE_STONE attributes and `prefs = mat_match` (the engraved stone). ESTIMATE (the wiki
  says "most other crafting skills" use the formula; engravings are not named). Hash change.
- Meals (`lifestyle_meal_cooked`): DF rolls each ingredient and the meal; Rustfort has no
  ingredient list. Keep one roll per meal but switch to `quality_points_roll` with COOK
  attributes (`quality_attrs("COOK")` exists) and `prefs = mat_match` on the meal's material.
  ESTIMATE. P3.
- Focus: `social_rating(s, 100)` is already `current/undistracted` in percent. OK.
- Perf: one extra binary search per produced item. Nothing per tick.

Tests (`crates/rf-sim/src/prefs/tests.rs` or `prefs_tests.rs`):
- `quality_roll_inclusive_bounds`: at level 0, no attributes, focus 100, 100k rolls: points
  reach 10 (needs the inclusive roll) and never exceed 10; at prefs 2 the minimum is 20.
- `preference_is_worth_about_four_levels`: mean quality level 0 + 1 pref within 0.35 of level 5
  without pref (table in section 0).
- `crafter_with_liked_material_makes_better_goods`: two identical dwarves, one likes granite;
  200 granite crafts each -> mean quality differs by >= 0.8.
- determinism: same products and qualities with 1 and N threads (covered by the bench hash).

---------------------------------------------------------------------------------------------

## 4. Item value, quality multipliers and decorations

### 4.1 DF rule

`w:Item value`: final value = `base(kind) x material value x quality multiplier`, rounded down,
`+ quality bonus`, `+` the value of every decoration. Quality (w:Item quality/Table):

| quality | multiplier | bonus |
|---|---|---|
| ordinary | 1 | 0 |
| well-crafted | 1.1 | 3 |
| finely-crafted | 1.2 | 6 |
| superior | 4/3 | 10 |
| exceptional | 1.5 | 15 |
| masterful | 2 | 30 |
| artifact | 20 | 300 |

Use integer math: `v = base * mat * NUM[q] / DEN[q] + BONUS[q]` with NUM/DEN
`1/1, 11/10, 6/5, 4/3, 3/2, 2/1, 20/1`. The export confirms the floor before the bonus
(a superior wood chair: `floor(10 x 4/3) + 10 = 23`; a ≡chalk mug≡: `10 x 2 x 1.5 + 15 = 45`).
Stacks: value per unit x stack (ammo, coins and decorations on ammo: `w:Item value` note 3).

Base values (w:Item value, **checked against the export**; brackets = export count that fits):
| base | kinds |
|---|---|
| 0 | corpses, body parts, remains |
| 1 | glob, seeds, drink (per unit), powder, liquid, branches; ammo; prepared meal (+ ingredients, see below) |
| 2 | meat, fish, egg, plant, plant growth |
| 3 | log (WOOD) [77/77], boulder [878/878] |
| 5 | bar [176], blocks [92], tanned hide [94], sheet; shoes and gloves made in pairs (socks, shoes, boots, gloves, gauntlets) |
| 6 | thread [2525], rough gem [346] |
| 7 | cloth (plus its quality) |
| 10 | cheese, honeycomb; and every quality item not listed elsewhere: furniture (bed, chair, table, door, cabinet, box, coffin, armor stand, weapon rack, bin, barrel, bucket, cage, animal trap, floodgate, grate, hatch cover, slab, quern, millstone? see below, pedestal, display case, bookcase), crafts (amulet, bracelet, crown, earring, figurine, ring, scepter, goblet, toy, totem, large gem), weapon, shield, armor/helm/pants not in pairs, quiver, backpack, bag, flask, chain, anvil, splint, crutch, trap component, tools |
| 20 | cut gem [91/91], quire/scroll, siege ammo, traction bench |
| 25 | window, statue [32 exact of 69, rest floor-rounded] |
| 30 | mechanism (TRAPPARTS) [147/218], ballista / catapult parts |
| 50 | cauldron, instrument main part, minecart, parchment sheet, stepladder, wheelbarrow |

(The export's single millstone fits 250 = 25 x 10: treat MILLSTONE as 25, ESTIMATE.) Clothing of
cloth/leather carries its cloth as a built-in decoration (`w:Item value` note 2), which is why
cloth socks/shirts show 25-30 in the export. Imported items keep their exported `value`.

Decorations (w:Decoration): each = `floor(B x mat x mult) + bonus` with `B = 20` for cut gems
(not glass/stone), else 10, at the decoration's own quality. Artifacts' decorations are made at
artifact quality: the export's mood artifacts are all `floor(base x mat x 20) + 300 + n x
(floor(B x mat_d x 20) + 300)` (every value is a multiple of 100 for material value 1; a wood
amulet with one decoration = 1 000; a maple chair with three = 2 000). **OK (measured)**.

Meals (w:Item value, Prepared meal): `(1 x main material x mult + bonus) + average over
ingredients of (ingredient base x material x mult)`, all x stack. Rustfort has no ingredients:
ESTIMATE `value = (floor(mult) + bonus + 2) x stack` per meal unit.

Engravings and room tiles keep the **old** multipliers 1/2/3/4/5/12 (w:Engraving, w:Zone). Room
furniture uses the item value above.

### 4.2 Rustfort now

`trade.rs` `base_value` (approximate, F4), `quality_mult` (old table, F3); used by
`item_trade_value` (trade.rs:297), caravan goods (trade.rs:994), `warcal.rs` `war_item_made`
(created wealth for siege triggers, line 252), `skills_job_done` (rescales `it.value`),
`lifestyle_meal_cooked` (meal value), `moods.rs` `finish_mood` (artifact value). Engravings in
`social.rs` `room_tile_value` (old table, correct).

### 4.3 Design

New module `crates/rf-sim/src/item_value.rs` `// [prefs15]` (own file, pure functions, no state):

```rust
pub(crate) const Q_NUM: [i64; 7] = [1, 11, 6, 4, 3, 2, 20];
pub(crate) const Q_DEN: [i64; 7] = [1, 10, 5, 3, 2, 1, 1];
pub(crate) const Q_BONUS: [i64; 7] = [0, 3, 6, 10, 15, 30, 300];
/// Old (pre-v50) multipliers, still used by engravings and room tiles.
pub(crate) const Q_OLD: [i64; 7] = [1, 2, 3, 4, 5, 12, 120];
pub fn base_value_v50(kind: &str, subtype: &str) -> i64;          // table 4.1
pub fn quality_value(base: i64, mat_value: i64, q: u8) -> i64 {   // per unit
    base * mat_value.max(1) * Q_NUM[q] / Q_DEN[q] + Q_BONUS[q]
}
pub fn decoration_value(gem: bool, mat_value: i64, q: u8) -> i64;
```

Switch-over in shared files (each changed line `// [prefs15]`):
- `trade::quality_mult` stays for engravings only (rename nothing; add a doc line), all item
  value computations call `item_value::quality_value(item_value::base_value_v50(..), mv, q) *
  stack`.
- `skills_job_done`: recompute `it.value` from scratch with the new formula instead of rescaling
  (rescaling by a ratio is wrong once a flat bonus exists).
- `war_item_made` (warcal): same formula. **This changes created wealth and therefore siege and
  migrant triggers** (smaller wealth for quality goods, larger for raw blocks/logs/boulders): say
  so in the commit and FIDELITY section; re-run `validate` r4 -> r5 / r5 -> r6.
- `trade.rs` caravan goods and `item_trade_value` fallback: same formula.
- Artifacts (moods `finish_mood`): `quality_value(base, mv, 6)` + per decoration
  `decoration_value(is_gem, mv_d, 6)`.
- Meals: ESTIMATE formula above.

Tests:
- `value_matches_df_export_samples`: synthetic items: superior wood chair = 23, ≡chalk mug≡
  (mat 2) = 45, masterwork wood chair = 50, artifact wood amulet without decorations = 500,
  with one wood decoration = 1 000, gold (30) statue ordinary = 750.
- `export_item_values_fit` (runs only with `RUSTFORT_EXPORT`): for every imported item with
  `value > 0`, quality < 6, kind in {BOULDER, WOOD, BAR, BLOCKS, ROUGH, SMALLGEM, THREAD,
  SKIN_TANNED, CHAIR, TABLE, BED, DOOR, CABINET, BOX, RING, AMULET, CROWN, WEAPON}: the formula
  equals DF's value for >= 75 % (decorated items are the rest).

Perf: value math is per created item, nothing per tick.

---------------------------------------------------------------------------------------------

## 5. Thoughts and needs driven by preferences and value

All thoughts go through `Sim::feel(s, thought, class, strength, sub, severity, now)`
(social.rs:918); needs through `satisfy(s, need) -> previous focus level` (social.rs:1021).
New logic lives in `prefs.rs`; the shared call sites get one-line hooks.

### 5.1 Value "in the eyes of" a dwarf

DF (w:Preferences "Room value considerations"): for furniture each preference match
**multiplies the whole item value**: likes the item type (+1), each decoration material (+1),
an image's shape (+1), maker's race (+1), dye color (+1) -> value x (1 + matches); the wiki's
example: type + two decoration materials + shape = x5. Material preference for the base material
counts like a decoration material (ESTIMATE: wiki says "preferences for materials or item types
will increase the value of furniture"). Smoothed/engraved floors and walls of a liked stone or gem
are worth more: an engraving in a liked stone counts **double** (w:Engraving); ESTIMATE: the tile
value too. Display furniture contents count for room value only if the dwarf likes them
(w:Zone).

`prefs.rs`: `pub(crate) fn perceived_value(&self, s: usize, item_idx: usize) -> i64`
= `value x (1 + item_match + base_mat_match + decoration matches)`. Only base material and item
type exist without decorations (P5 adds decorations).

### 5.2 Admiring furniture (AdmireBuilding / AdmireOwnBuilding / ...Arranged...)

DF:
- A dwarf passing a built piece of furniture or architecture notices it ("seeing a/near (own)
  (very fine/splendid/wonderful/completely sublime) [building]", w:Thought, Amenities).
- How much it pleases depends only on its value in the dwarf's eyes; quality has no influence
  on **how often** furniture is admired (w:Item quality, "Admiring furniture").
- Export: `AdmireBuilding` severity = that value (most common 10, 14, 18, 23, 30, 50, i.e. a
  10-value item at every quality; up to 551), `subthought` = DF `building_type` (0 chair, 1 bed,
  2 table, 8 door, 15 statue, ...), emotions PLEASURE / INTEREST (PRIDE for own), strength 0.
- Rate (export, emotions younger than 28 days): AdmireBuilding 156 + Arranged 65 + Own 1 = 222
  for 87 citizens = **2.55 per citizen per 28 days** (a lower bound: DF merges repeats).
- "Tastefully arranged" = the building stands in a sculpture garden zone (w:Thought links
  Sculpture garden). Rustfort has no garden zones: MISSING (keep plain AdmireBuilding).
- Engravings: w:Personality need says AdmireArt is met by "exposure to artful furniture,
  artistic crafts, or engravings"; no admire thought for engravings is documented: they satisfy
  the need only (ESTIMATE).

Rustfort now: `social.rs` ~1450: every 3 days, chance 1/2, furniture with quality >= 2 (>= 3
without ART), within 2 tiles, same z; strength `50 + 25 q`, severity = q.

Design (replace the block, the old lines stay in place behind `if !self.prefs.on`):
- Candidate: furniture (`social.furn`) within 2 tiles on the same z, max by
  `(perceived_value, bid)` (deterministic). No quality gate.
- Cadence: keep the daily social think (each citizen once per day via the 12-slot wheel); chance
  to admire `ADMIRE_NUM/ADMIRE_DEN` per day when a candidate exists, `next_admire` cooldown 1
  day. Calibrate so the 2-year bench gives ~2.5 admire thoughts per citizen per 28 days on r5
  (ESTIMATE start 1 in 4).
- Thought: `AdmireOwnBuilding` if the room's owner is the dwarf, else `AdmireBuilding`;
  `severity = perceived_value.min(i32::MAX)`, `sub = building kind id` (keep `fu.bid` if the UI
  needs the building; DF stores the building type), class by value tier (ESTIMATE thresholds
  for the four captions: < 50 "very fine" PosMild, < 250 "splendid" PosMild, < 1 000
  "wonderful" Pos, else "completely sublime" PosStrong); strength `50 + 25 x tier`.
- `describe_emotion` (social.rs ~1953) maps severity to the caption word by those tiers instead
  of quality (two lines, `// [prefs15]`).
- Satisfy `AdmireArt` on every admire; additionally once a day if an engraving of quality >= 1
  lies within 2 tiles on the same z (`cx.engr_at` lookup of the 5x5 square; 25 hash lookups per
  citizen per day, fine) ESTIMATE.
- `Furn` (social.rs:478) gets `value: i32` and `mat: u16`, `item_kind: SStr` of its main item
  (`#[serde(default)]`, `// [prefs15]`) filled where `furn` is built (social.rs ~740).

Tests: `admire_severity_is_perceived_value` (a dwarf liking oak next to an ordinary oak chair
gets severity 20, a dwarf without the preference 10); `ordinary_furniture_is_admired`;
`admire_rate_calibrated` (synthetic fort, 28 days, 0.5..5 admire thoughts per dwarf).

### 5.3 Satisfied at work

DF (w:Thought, Job satisfaction): "at work" when the dwarf worked with materials, items or
animals it prefers. Personality facet CHEER_PROPENSITY 0-24: never ("didn't feel anything at
work", w:Personality facet). Export: 62 SatisfiedAtWork in the last 28 days for 87 citizens
(0.71 per citizen per 28 days), emotion SATISFACTION, severity 0.

Rustfort now: `social_work` gives it with chance `(2 + skill)/60` on every XP gain.

Design: remove the random rule (behind `prefs.on`), add hook
`self.prefs_work(s, mat, kind, creature_token_id, now)` `// [prefs15]` at:
- `skills_job_done` (product kind/subtype + material),
- dig / chop completion (`construct.rs` / dig code: tile material for mining, tree wood for
  woodcutting),
- construction / smoothing / engraving (`cx_finish_*`: the material),
- animal jobs (butcher, tame/train, shear, milk; `animals.rs`: creature token),
- cooking / brewing (food material).
Rule: if CHEER_PROPENSITY >= 25 and (material or item or creature is liked) -> `feel(s,
"SatisfiedAtWork", PosMild, 60, -1, 0, now)` at most once per day per dwarf (ESTIMATE cooldown,
to stay near DF's 0.7 per 28 days for a fort whose stone mostly is not liked).

Tests: `liked_material_gives_work_satisfaction`, `cheerless_dwarf_feels_nothing_at_work`,
`no_satisfaction_without_preference`.

### 5.4 Food and drink

DF (w:Thought, Nourishment; w:Personality need; w:Kitchen):
- `GoodDrink` only for booze the dwarf likes; level by the drink's value (same 5 tiers).
- `GoodMeal` "if the meal matches a preference or is worth at least 20"; level by value:
  thresholds 5 / 25 / 100 / 200 / 400 -> severity 1..5 ("pretty decent / fine / wonderful /
  truly decadent / legendary"). Prepared meals count as every ingredient for preferences;
  booze cooked into a meal counts as a liked food.
- `EatGoodMeal` need: eating a preferred food (or a preferred alcohol cooked in) or a meal worth
  over 20.
- `SameFood` / `SameBooze`: the same food / booze 4 times in a row; liked foods are exempt;
  meals count as their main ingredient.
- Export: GoodMeal 36 (severity 1:17, 2:7, 3:6, 4:4, 5:2), GoodDrink 3 (severity 1), SameBooze 67.

Rustfort now (`social.rs` `social_drank` / `social_ate`): GoodDrink if quality >= 1, GoodMeal on
every meal, severity = quality, SameX after 3 in a row (the history keeps 2).

Design (`prefs.rs` helpers, call sites in social.rs marked):
- `liked_food(s, mat) -> bool` (binary search in `PrefSet::foods`; meals: the meal's `mat` =
  main ingredient, ESTIMATE since ingredients are not tracked).
- Drink: `if liked { feel GoodDrink, severity = value_tier(value_per_unit) }`;
  `SameBooze` only if `!liked`. Note DF needs **4** in a row: extend `Psy::drinks/meals` to 3
  previous entries (`[u16; 3]`, `#[serde(default)]`; saves10/saves11 compatible) ESTIMATE-free.
- Meal: `if liked || value_per_unit >= 20 { feel GoodMeal, severity = value_tier; satisfy
  EatGoodMeal }`; `value_tier`: `<5 -> 0 (no thought)`, `5..25 -> 1`, `25..100 -> 2`,
  `100..200 -> 3`, `200..400 -> 4`, `>= 400 -> 5`.
- Strengths/classes: keep the existing ones by severity.

Tests: `liked_booze_gives_good_drink`, `unliked_cheap_meal_gives_no_good_meal`,
`liked_food_is_exempt_from_same_food`.

### 5.5 Caged and hated creatures, pets

DF: `NearCaged` "after being near to a * in a cage" when passing a caged creature the dwarf
likes (zoo); `NearCagedHated` / vermin thoughts for hated vermin (w:Thought, w:Hateable);
`SeeAnimal` need: "viewing favorite animal in cage"; pets: a dwarf may adopt a liked animal even
if it is not available for adoption (w:Pet).

Rustfort: no NearCaged; vermin are not simulated; pet adoption in `animals.rs` (1/20 per day near
a stray, ESTIMATE).

Design (P4):
- In the daily social think: caged units (`flags.caged`) within 3 tiles same z whose race token
  is in `PrefSet::creatures` -> `feel NearCaged PosMild 80` + satisfy `SeeAnimal`; hated ->
  `NearCagedHated NegMild 80`. Use a per-pass list of caged creatures built once per social pass
  (sorted by unit id), not a scan of all units per dwarf.
- `animals.rs` adoption: a stray whose race the citizen likes doubles the chance (ESTIMATE) and
  ignores "available for adoption". One-line hook `// [prefs15]`.

### 5.6 Acquire something, be extravagant

DF (w:Personality need): `AcquireObject` is met when the dwarf performs "Store item in
stockpile", "Store item in bin" or "Bring item to depot" with an eligible trade good (finished
goods, clothing), not by equipment pickup; some dwarves only want liked items (ESTIMATE: the
wiki's "picky" remark is not a rule). Thought `AcquiredItem` "after a satisfying acquisition"
(export: 22, 8 in 28 days, PLEASURE). `BeExtravagant`: "wear any item with a quality
modifier".

Rustfort: neither need is satisfied anywhere (F10).

Design:
- `lib.rs` `drop_item`, after a successful drop at a pile or bin and in the trade depot move
  (`trade.rs`), hook `self.prefs_hauled(s, id, now)` `// [prefs15]`: if the hauler is a citizen
  with an `AcquireObject` need, the item kind is a trade good (crafts, furniture, weapons, armor,
  clothing, tools, `skills::has_quality(kind)`), satisfy; if the need was below 0 before, `feel
  AcquiredItem PosMild 60`. ESTIMATE: the thought only when the need was unmet (matches the
  export's low count).
- `lifestyle.rs` WearItem (line ~333): add `self.satisfy(s, "BeExtravagant")` when `q >= 1`
  `// [prefs15]` (needs `satisfy` to be `pub(crate)`; it is `fn` in `impl Sim` in social.rs, add
  `pub(crate)` with the tag).

Tests: `storing_finished_goods_satisfies_acquire_object`, `wearing_quality_clothes_satisfies_be_extravagant`.

### 5.7 Rooms

DF: room value thoughts (BedroomQuality, DiningQuality, OfficeQuality) use the room's value;
preference multipliers apply to furniture "for creatures who like it" (w:Preferences). Exact
order of operations unknown: ESTIMATE: `room value for dwarf = sum(tile values, liked stone
engravings x2) + sum(perceived_value(furniture))`.

Rustfort: `social_revalue_rooms` computes one value per room (social.rs ~800).

Design (P4, cheap): keep `Room::value` as the neutral value; at the moment a thought is produced
(`BedroomQuality` in the sleep hook, `DiningQuality` in `social_ate`), add the owner's / eater's
bonus = `sum over furniture in the room of value x matches` + `sum over engraved tiles in a
liked stone of the engraving value` (precompute per room a small list `(mat, value)` of
furniture and engravings; `Vec<(u16, i32)>` built in `social_revalue_rooms`, `#[serde(skip)]`).
The tier is taken from `value + bonus`. One pass over a room's list per thought, a few dozen
entries: fine.

---------------------------------------------------------------------------------------------

## 6. Strange moods and preferences

### 6.1 DF rules (w:Strange mood)

- Mood skill = highest moodable skill (Rustfort OK), **but the skill ids must be DF's**:
  `BONECARVE`, `CUTGEM`, `ENCRUSTGEM` (F6). The other names in `MOODABLE` (`ENGRAVE_STONE`,
  `CARVE_STONE`, `CUT_STONE`, `STONECRAFT`, ...) appear in the export as they are.
- **Artifact item type:** "if a dwarf has a personality preference for a particular item type
  ... and that thing is an available choice given the dwarf's profession, they are guaranteed to
  create an object of that type (if multiple preferences match, one will be randomly selected)."
  Otherwise random from the skill's list. Lists (w:Strange mood "Artifacts created"):
  - Armorsmith: each [METAL] armor (mail shirt, breastplate, leggings, greaves, gauntlet, low
    boot, high boot, cap, helm), each shield (shield, buckler).
  - Bone carver (bone): [BARRED] leggings, greaves, gauntlet, helm; shield, buckler; instrument,
    toy, door, bed, chair, table, statue, chest, bin, armor stand, weapon rack, cabinet, coffin,
    floodgate, hatch cover, grate, chain, cage, animal trap, figurine, amulet, scepter, crown,
    ring, earring, bracelet, any weapon, any trap component (†).
  - Bone carver (shell): [SCALED] leggings, gauntlet, helm; figurine, amulet, crown, ring,
    earring, bracelet, chain, cage, animal trap, instrument, toy.
  - Bowyer: each ranged weapon (crossbow).
  - Carpenter: door, bed, chair, table, statue, chest, bin, armor stand, weapon rack, cabinet,
    coffin, floodgate, hatch cover, grate, cage, barrel, bucket, animal trap, splint, crutch.
  - Clothier / Weaver: each [SOFT] clothing item (dress, shirt, tunic, toga, vest, robe, coat,
    cloak, trousers, loincloth, braies, glove, mitten, sock, shoe, cap, hood), bag, rope.
  - Engraver / Stone crafter / Stonecutter / Stone carver: figurine, amulet, scepter, crown,
    ring, earring, bracelet, goblet, instrument, toy.
  - Fell / Leatherworker / Tanner: each [LEATHER] item (dress ... hood, armor, leggings, low/high
    boot), shield, buckler, bag, backpack, quiver, instrument.
  - Gem cutter / Gem setter / Glassmaker: perfect gem (‡), door, bed, chair, table, statue, box,
    armor stand, weapon rack, cabinet, coffin, floodgate, hatch cover, grate, figurine, amulet,
    scepter, crown, ring, earring, bracelet, chain, flask, goblet, cage, barrel, bucket, animal
    trap, window, instrument, toy.
  - Macabre (vermin remains): amulet, bracelet, earring.
  - Mason / Miner: door, bed, chair, table, statue, quern, millstone, coffer, armor stand,
    weapon rack, cabinet, coffin, floodgate, hatch cover, grate.
  - Mechanic: mechanism.
  - Metal crafter: figurine, amulet, scepter, crown, ring, earring, bracelet, chain, flask,
    goblet, instrument, toy.
  - Metalsmith: door, bed, chair, table, statue, chest, armor stand, weapon rack, cabinet,
    anvil, coffin, floodgate, hatch cover, grate, cage, barrel, bucket, animal trap, pipe section
    (†), splint, crutch.
  - Weaponsmith: each melee weapon (battle axe, war hammer, short sword, spear, mace), any trap
    component (†).
  - Wood crafter: figurine, amulet, scepter, crown, ring, earring, bracelet, goblet, instrument,
    toy.
  - † weight x 0.1; ‡ only at mood start, 50 %; "any" = one entry with a random subtype, "each"
    = one entry per subtype, non-exotic only (v53.16).
  - The item type is decided when the mood **ends** (unless preferences force it).
- **Base material demands** (decided at mood start):
  - metalworkers: adamantine/divine metal if available, else a **preferred metal if the fort
    has ever smelted bars of it** (not traded, not melted), else any metal;
  - weavers/clothiers: adamantine cloth if available, else the generic class (silk / plant /
    yarn) of the **first** cloth preference, else a random class;
  - glassmakers: preferred glass if produced (green always counts as produced), else a random
    produced glass;
  - bone carvers: shells if they like a shell, else bones;
  - stoneworkers: boulders, woodworkers: logs, leatherworkers: leather, gem workers: rough gems
    (no preference rule on the wiki).
- **Decorations**: random from logs, metal bars, small (cut) gems, rock blocks, rough gems,
  boulders, bones, leather, cloth (plant/silk/yarn), raw glass (only types produced); never the
  same type as the primary material; miners, engravers, masons and stonecrafters never request
  rock blocks; macabre: 50 % per decoration to be remains or bones instead; gem cutters/setters:
  50 % to gather a single rough gem and nothing else and make a perfect gem.
- Artifact value: section 4.1 (`floor(base x mat x 20) + 300` + decorations at artifact quality).

### 6.2 Rustfort now

`moods.rs`: `start_mood` picks skill (with F6 names), demands base 1-3 + 0-7 decorations from
`DECOR` (may equal the primary type), artifact kind random from short lists (`CRAFTS`,
`FURNITURE`, ...) chosen at mood start; `finish_mood` value `10 x mv x 120 + 2 x decorations`.
Bone carvers use shells with chance 1/3 (estimate).

### 6.3 Design

moods.rs is a shared existing module: keep the changes small and tagged, put the new logic in
`prefs.rs` / a new `crates/rf-sim/src/mood_prefs.rs`:
- F6: fix the three names in `MOODABLE` and `skill_spec` (`// [prefs15]`).
- `mood_prefs::artifact_kinds(skill, base: Mat) -> &'static [ArtEntry]` with the wiki lists
  above (`ArtEntry { kind, subtype ("" = any), noun, weight_tenths }`); keep `ArtKind` tuples
  for the existing code path.
- `start_mood`: after the skill is known, `let forced = self.prefs_artifact_kind(s, entries)`:
  the LikeItem preferences that match an entry (kind + subtype or any), pick one at random with
  the mood RNG; store it in `Active::art`. Without a match keep the current random pick (DF
  picks at the end; Rustfort picks at the start: ESTIMATE-equivalent because nothing observable
  changes before the end, but note the save/reload remark of the wiki is not reproduced).
- Base material: a new optional `Active::want_mat: u16` (`#[serde(default = NO_MAT)]`,
  `// [prefs15]`) set at start: metal -> first liked metal in `PrefSet::mats` that is in
  `industry`'s smelted set (needs a small `BTreeSet<u16>` of metals smelted by the fort, filled
  in `industry` smelt completion with one hooked line; imported bars do not count) else any;
  cloth -> class of the first liked cloth (token suffix `:SILK` / `:YARN`|`:HAIR`|`:WOOL` /
  `:THREAD` plant); glass -> liked glass if produced; bone carver -> `Mat::Shell` iff a shell is
  liked (replaces the 1/3 estimate). `mood_matches` checks `want_mat` / class for the **first**
  demand only.
- Decorations: draw from `DECOR` minus the base type, minus `Block` for miners/engravers/
  masons/stonecrafters, minus `RawGlass` if the fort never made glass, macabre 50 % -> `Bone`;
  jewelers 50 % single rough gem and no decorations (artifact = large gem `GEM`).
- Value: section 4.3.
- Determinism: mood RNG streams are already per slot and tick.

Tests: `mood_uses_liked_item_type` (a carpenter who likes floodgates always makes a floodgate
over 20 forced moods), `metalworker_demands_liked_smelted_metal`,
`bone_carver_who_likes_shells_demands_shells`, `decorations_never_repeat_base_type`,
`bonecarve_skill_is_moodable` (F6), `artifact_value_v50`.

---------------------------------------------------------------------------------------------

## 7. Other preference effects (lower priority)

| Mechanic | DF rule (source) | Rustfort | Design |
|---|---|---|---|
| Noble mandates | nobles only mandate (or ban for export) items they like; no item preference -> no mandates (w:Mandate) | `population.rs` `MANDATE_ITEMS` fixed list | hook in the mandate issue: candidates = the noble's LikeItem kinds that the fort can make (intersection with `MANDATE_ITEMS` kinds + any furniture/craft kind with a recipe); empty -> no mandate. `// [prefs15]` one call |
| Noble room demands | a noble demands furniture he likes, usually in a liked material (w:Demand) | not modelled (the round-13 `Demand` type is diplomacy) | MISSING; expose `prefs15_api::unit_likes(...)` for a later owner |
| Art images | preferences (and hated vermin) appear more often in the dwarf's artwork (w:Preferences, w:Shape) | engravings have no image | MISSING (cosmetic) |
| Art defacement | masterwork made by the dwarf lost/destroyed/stolen -> `ArtDefacement` (less if many masterworks) (w:Item quality) | no `Item::maker` | P5: add `Item::maker` (default -1), set in `skills_job_done`, hook `destroy_item` for quality 5 |
| Trading | merchant decoration multipliers by civ (w:Decoration) | no decorations | P5 with decorations |
| Hated vermin thoughts | `NearVermin` / `AnnoyedVermin` / `PesteredVermin` (w:Thought, w:Hateable) | no vermin | MISSING |
| Eating a liked creature | `EatLikedCreature` when starving (w:Thought) | no vermin eating | MISSING |

---------------------------------------------------------------------------------------------

## 8. Module layout, hooks, phases, saves

New files (own topic, no other agent edits them):
- `crates/rf-sim/src/prefs.rs`: `Prefs` state, `PrefSet` cache, derive, matching, perceived
  value, all thought/need helpers (`prefs_work`, `prefs_hauled`, `prefs_quality_bonus`,
  `prefs_artifact_kind`, `liked_food`, `value_tier`), `prefs_hash`, tests module.
- `crates/rf-sim/src/item_value.rs`: v50 value tables and functions.
- `crates/rf-sim/src/mood_prefs.rs`: artifact lists and demand rules.
- `crates/rf-sim/src/prefs15_api.rs`: UI-facing API (below).
- `crates/rf-app/src/ui/prefs15.rs`: "Preferences" sub-tab of the unit sheet (psyche.rs has the
  personality screen: add one tab button there `// [prefs15]`).

```rust
// prefs.rs
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Prefs {
    pub on: bool,                       // default true via Sim::new (off = old behaviour, for A/B)
    pub smelted: BTreeSet<u16>,         // metals the fort has smelted (mood demands)
    pub rep: PrefsReport,               // counters: quality bonus hits, work satisfaction, admire, ...
    #[serde(skip)] pub sets: Vec<PrefSet>,      // per slot, rebuilt
    #[serde(skip)] pub intern: BTreeMap<String, u16>,
}
```

`Sim` field `prefs: prefs::Prefs, // [prefs15]`, initialised in `Sim::new` (lib.rs ~860),
`self.prefs_hash(&mut mix); // [prefs15]` in `state_hash` (lib.rs ~1312; mix `smelted` and
counters only), part list entry `("prefs", enc(&self.prefs))` in `saves11.rs` `exact_parts`
(~line 497) like `weather`, and `self.prefs_rebuild()` in `after_exact_load`. Preferences
themselves live in `World::units` (already in the snapshot).

Tick phases: **no new periodic pass is needed.** Everything is event-driven (job done, eat,
drink, haul, wear, mood start/finish) or runs inside the existing daily social think (admire,
caged creatures). If a periodic pass is ever needed, use `now % 100 == 67` (free phase; used:
0, 1, 37, 41, 50, 59, 73, 83 and day phases 611, 733, 857).

`init_units` hook: `self.prefs_init_units(); // [prefs15]` (derive missing preferences, rebuild
`sets`). Units added later (migrants: population.rs; births) pass through `init_units` because
`world.units.len() != ai.len()`.

Determinism: preference generation uses its own `Rng::new(...)` seeded from unit id + facets;
all other randomness uses existing per-slot streams (`Rng::for_unit(seed, s, now, salt)`) with
new salts (`0x9F15_01..`). No HashMap iteration (use BTreeMap / sorted Vecs). Nothing runs in
parallel phases.

`prefs15_api.rs` (pub, for the UI and later owners):
```rust
pub struct PrefLine { pub kind: String, pub text: String, pub derived: bool }
impl Sim {
    pub fn unit_preferences(&self, unit_id: i32) -> Vec<PrefLine>;   // "likes granite", "likes battle axes for their ..." (own wording, no DF prefstrings)
    pub fn unit_likes_item(&self, unit_id: i32, item_id: i32) -> u8;   // number of matches
    pub fn item_value_v50(&self, item_id: i32) -> i64;
    pub fn prefs_report(&self) -> PrefsReport;
    pub fn set_prefs_enabled(&mut self, on: bool);
}
```
UI: list grouped like DF (materials, items, creatures, food, dislikes), "(estimated)" marker for
derived ones; screenshot test with `rustfort play --screenshot target/prefs15.png --frames N`.

---------------------------------------------------------------------------------------------

## 9. Validation and expected hash changes

- Every P1-P4 step changes the state hash (thoughts, stress, values, wealth -> siege size).
  Commit messages and the FIDELITY section must say so.
- Checks after each step: `bench --ticks 806400 --standdown` with `RAYON_NUM_THREADS=1` and
  default: same hash; `saveload` IDENTICAL; ticks/s within 5 %.
- `validate` r4 -> r5 (examples/validate) targets:
  - crafts mean quality: DF 2.6 (export now: crafts among all items 146/245/153/210/310/103
    ordinary..masterwork = mean 2.5); report before/after F2 and after exported preferences
    (needs a re-export with round-15 preferences; with derived preferences the effect is
    smaller and is an ESTIMATE);
  - admire thoughts 2.5 per citizen per 28 days, SatisfiedAtWork 0.7, GoodMeal 0.3 (25/87),
    AcquiredItem 0.1 (8/87);
  - created wealth vs `settings.json` wealth (round 9 export) after F3/F4.
- FIDELITY.md: a new section at the end `## [prefs15] Preferences, quality and value` with one
  row per mechanic above: rule, `w:` page, status OK / ESTIMATE / MISSING.

---------------------------------------------------------------------------------------------

## 10. Prioritized implementation list

1. **P1 Quality roll fix + preference data**: F2 (inclusive rand); rf-core `UnitPreference` +
   `Unit::preferences`; exporter `preferences` + EXPORT_FORMAT; `prefs.rs` cache, derived
   preferences, `prefs_quality_bonus` wired into `skills_job_done` (and engravings). Tests 3.3.
2. **P2 v50 item value**: `item_value.rs` (quality multipliers + flat bonus, base table checked
   against the export), switch skills/warcal/trade/lifestyle/moods to it. Tests 4.3. Re-validate
   wealth-driven sieges/migrants.
3. **P3 Thoughts and needs**: admire by perceived value (5.2), SatisfiedAtWork by preference +
   cheer rule (5.3), food/drink (5.4), AcquireObject + BeExtravagant (5.6). Calibrate rates.
4. **P4 Moods**: F6 skill names, preference-forced artifact type, preference base materials,
   decoration rules, artifact value (6.3). Then rooms in the owner's eyes (5.7), caged liked
   creatures and pets (5.5), noble mandates from preferences (7).
5. **P5 Optional**: item decorations (rf-core field, exporter `improvements`, decoration jobs at
   jeweler / metalsmith / craftsdwarf), `Item::maker` and ArtDefacement, UI polish.
