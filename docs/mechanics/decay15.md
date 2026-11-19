# [decay15] Decay, rot, vermin and item wear: mechanics spec for v0.15

This is the implementer's only spec for the `[decay15]` topic. For each mechanic it gives the DF v50 rule with its
source, what Rustfort already has (file and function), the gap, and a concrete Rust design. Anything not stated
by the wiki or the raws is marked **ESTIMATE**. Where the reference fort's exports give numbers, they are in
section 1, and the estimates are fitted to them.

Sources:
- **w:** = dwarffortresswiki.org v50 pages, read on 2026-11-18: Miasma (the target of the "Rot" redirect), Food, Barrel,
  Large pot, Vermin, Hateable, Cat, Remains, Refuse, Corpse, Bone, Shell, Skin, Tanner, Wear, Stockpile,
  Standing orders, Hauling, Garbage dump, Animal trap, Kitchen, Pet, Thought, Tile attributes, Creature token,
  Material definition token, Using bins and barrels.
- **raws:** = `data/vanilla` in the local install (read only): `vanilla_materials/objects/material_template_default.txt`,
  `vanilla_creatures/objects/*.txt`, `vanilla_items/objects/item_tool.txt`.
- **export:** = the seven Trussroads exports in `export/tileshtulon*` (r1 to r7, years 273 to 280), read with
  throwaway Python scripts (not committed).

Conventions: 1 day = 1 200 ticks, 1 month = 33 600, 1 year = 403 200 (w:Food). "Exposed" means "not protected"
in the sense of M1.2.

---------------------------------------------------------------------------------------------------------------

## 0. Ownership and coordination

| Topic | Owner | What decay15 may do |
|---|---|---|
| Corpses rising, undead, necromancers | `[undead13]` (`undead.rs`) | Owns the corpse **decay stage**, and exposes it through `decay15_api` (`corpse_stage`). Never destroys a corpse the undead module has reserved (`imeta.reserved > 0`). Skeleton corpses stay as CORPSE items, so undead can raise skeletons. |
| Weather, temperature, freezing | `[weather13]` | Reads `temperature_at_impl` only if the heat or cold wear in M7.6 is ever done. Not in v0.15. |
| Minecarts and wheelbarrows | `[perf13]`/`[carts12]` | Reads only: an item inside a minecart is protected from rot (M1.2). |
| Miasma set and thoughts | `lifestyle.rs` (`[society2]`) | Calls in through new tagged functions only (M3.6). The `Life` struct and the "Miasma" thought stay where they are. |
| Clothing wear while worn | `lifestyle.rs` | Untouched. M7 adds the other kinds of wear (refuse piles, combat, value). |
| Butchering, tanning | `industry.rs` | Reads `flags.rotten` (already done) and adds tagged lines where a skin or body part is made (M2.7). |
| Food rot | `storage.rs::rot_pass` | Replaced by `decay15::decay_day` behind one tagged switch in `storage_pass` (M1.6). |

New files: `crates/rf-sim/src/decay15.rs` (logic and state), `crates/rf-sim/src/decay15/tests.rs`,
`crates/rf-sim/src/decay15_api.rs` (UI calls and report), `crates/rf-app/src/ui/decay15.rs` (panel). Mark every
line in a shared file with a trailing `// [decay15]`.

---------------------------------------------------------------------------------------------------------------

## 1. Calibration data from the reference fort

Method: an item's creation tick is estimated from its id (DF ids only grow). The tick is interpolated linearly
between the exports' `(max item id, export tick)` pairs, at 20 to 29 new ids per day. Each item is then tracked
across the seven exports. Locations: `cont` = in a container, `inv` = carried, `bld` = in a building, `pile` = on a
stockpile tile, `loose` = elsewhere on the ground. Ages are only good to a few days.

| Observation | Numbers | Used for |
|---|---|---|
| Loose CORPSE, fresh | 233 samples; age p10/median/p90/max = 1/6/18/97 days | Corpse rot start: about 14 days (keeps `CORPSE_ROT_DAYS` = 14) |
| CORPSE, rotten | ages 12, 23, 90 to 96 days | The rotten stage can last 70+ days |
| CORPSE, skeleton | youngest 48 days (p10 = 95) | Rotten to skeleton takes about 34 to 80 days: keep 56 |
| "Fresh" CORPSE items thousands of days old | "voracious cave crawler tooth", "giant cave spider mangled chitin" | **Items whose material does not ROT never rot**, including CORPSE items (M2.2) |
| REMAINS, rotten | 21 samples, all **bat** remains, ages 17 to 31 days (two at 60, one at 115) | Remains rot after about 14 days; rotten remains vanish within about 14 more days |
| REMAINS that never rot | olm, cave swallow and cap hopper remains, fresh up to 528 days | The rule is per species and unknown (MISSING, see M2.5) |
| REMAINS made per year | about 35 to 40 per year with 4 adult cats, 9 dogs and 1 falcon; almost all are in the caverns (z 46 to 75, while the fort is at z 130 to 144); **no rat remains at all** | Vermin kill rate: about 10 per cat per year (M6) |
| FOOD (meals), rotten | ages 12 to 19 days in r4 (some at the kitchen, some at the dump); 20 samples, median 29, max 67 | **Food rots at 12 days of exposure or less** (the current 28 is too slow) |
| Rotten food and corpses at "Bridge + Dump zone" | 8 meals, 2 corpses | The fort atom-smashes its garbage, so rot-away times cannot be measured from these |
| Fresh meals more than 20 days old on a kitchen tile | 11 (r4) | Exposure is not continuous (the meals are hauled in and out). Not a contradiction. |
| Wear of metal armor and clothing | ARMOR 13/152, GLOVES 13/237, HELM 7/221, PANTS 15/136, SHOES 16/260 worn (x or more) | Combat wear on metal armor: about 5 to 10 % after the sieges |
| Wear of weapons | 0 of 535 (473 metal, 62 wood) | Weapon wear is very rare against flesh |
| Wear of stone and wood furniture | 8 of 150 doors, 11 of 350 chairs, 7 of 209 beds | Building destroyers or traded goods. Out of scope. |
| Leather, silk and hair clothing not worn | 214 of 341 ARMOR pieces, 198 PANTS: worn and lying in piles | Shows the "refuse pile wear" and dropped worn clothing |
| Raw hides | 0 in every export (auto-tan) | No data |

**Exporter additions** that would turn the ESTIMATEs into measurements are listed in section 10. The most
important is `rot_timer`.

---------------------------------------------------------------------------------------------------------------

## M1. Food rot

### M1.1 What rots (DF rule)
- w:Miasma: "Prepared food will rot the same as meat, cheese, eggs, and prepared fish, but plants will wither
  instead of producing miasma." Milk, fat, tallow, flour, drinks, seeds and rock nut paste do **not** rot
  ("it seems that they do not").
- raws: the `[ROTS]` material token (w:Material definition token: "will rot if not stockpiled appropriately.
  Currently only affects food and refuse"). These templates have ROTS: SKIN, FAT, MUSCLE, SINEW, EYE, NERVE, BRAIN,
  LUNG, HEART, LIVER, GUT, STOMACH, GIZZARD, PANCREAS, SPLEEN, KIDNEY, BLOOD, ICHOR, GOO, SLIME, PUS, TALLOW, MILK,
  CREATURE_CHEESE, STRUCTURAL_PLANT, LEAF, FRUIT, BUD, MUSHROOM, FLOWER, PLANT_EXTRACT, CREATURE_EXTRACT,
  EGG_WHITE, EGG_YOLK. These do **not**: BONE, CARTILAGE, HAIR, FEATHER, SCALE, NAIL, TOOTH, HORN, HOOF, PEARL,
  SHELL, CHITIN, LEATHER, PARCHMENT, SILK, SEED, PLANT_POWDER, PLANT_ALCOHOL, SOAP, all stone and metal.
- The wiki and the raws disagree on FAT, TALLOW and MILK (they carry ROTS, but "it seems that they do not") and on
  eggs ("do not appear to go rotten", w:Nest box). Follow the wiki: the observed behaviour wins. This is OK and
  is what Rustfort already does.
- **Withering**: a rotten PLANT or PLANT_GROWTH is "withered". It makes no miasma and becomes refuse (w:Refuse:
  "withered plants").

**Rustfort today**: `storage.rs::perishable` = MEAT, FISH, FISH_RAW, FOOD, CHEESE, PLANT, PLANT_GROWTH. That
matches the rule. **Keep it** as the item-kind test, and add the material test of M2.2 only for CORPSE,
CORPSEPIECE and REMAINS.

### M1.2 Protection (DF rule, w:Miasma)
Food does not rot "in a stockpile, a trade depot, the embark wagon, a minecart, when used as a reaction component,
or while being carried by a dwarf, caravan pack animal, or wagon, (or eggs in a nest box)". "It does not matter
if the food is in a container; a barrel or large pot full of meat left in a corridor will rot." "Raw fish will
rot no matter where they're stored."

**Rustfort today**: `storage.rs::rot_safe` covers stockpile tiles (`pile_at`), carried (`holder >= 0`), in a job,
in a building (`in_building`, `building >= 0`, the container's holder or building), and being hauled. Raw fish
is never safe. Containers outside piles do not protect.

**Gaps**:
1. **Trade depot** (FIDELITY "MISSING"): an item on a tile of a built `TradeDepot` building, or held by the
   depot (`building == depot id`), is safe. Build a sorted `Vec<u32>` of depot tiles once a day.
2. **Minecart**: an item whose container is a `TOOL` with subtype `ITEM_TOOL_MINECART` is safe, wherever the
   cart is.
3. **Wheelbarrow**: not on the wiki. ESTIMATE: safe while it is being pushed (its holder is set), not safe when
   parked.
4. **Workshop tiles are not safe.** Products lie loose on the shop's tiles, and w:Kitchen says meals left in the
   kitchen rot. Check that `in_building`/`building` are set only for items **inside** a building (components and
   furniture contents), never for loose products on a shop tile. If they are set for loose products, test
   `flags.in_job` instead.
5. A "**reaction component**" is an item reserved for a job (`in_job`, `imeta.reserved`). It is already covered.

### M1.3 Rot timer (ESTIMATE, export section 1)
- DF keeps a per-item rot counter (the DFHack field name is `rot_timer`). The wiki gives no thresholds.
- **Proposal**: the counter rises by 1 for every day the item is exposed and keeps its value while protected
  (it does not reset; DF's counter does not reset either). This matches Rustfort's `store.rot`.
  - `FOOD_ROT_DAYS = 12` (was 28). The export shows meals rotten 12 days after they were cooked.
  - `FISH_RAW_ROT_DAYS = 12` (raw fish is always exposed, so this is its whole life).
  - `ROT_AWAY_DAYS = 28` after it went rotten, then the item is destroyed. The export cannot measure this
    (atom smasher). Keep the current value.
  - Optional jitter: `+ splitmix(id) % 4` days, so a stack of meals does not rot on one tick. Deterministic.
- This is a **behaviour change**: food rots faster. Reason: export section 1. It changes the bench hash.

### M1.4 What happens when an item rots
1. `flags.rotten = true`, and the item is removed from `self.foods` (as today).
2. **New**: change its category to `Cat::Refuse` (`set_item_cat(idx, Cat::Refuse)`), so refuse haulers carry it to
   a refuse pile or the dump (w:Refuse: "rotten meat, rotten meals"; w:Hauling: "haul rotting food"). Today it
   stays in `Cat::Food`, sits in the food piles and blocks them.
3. The description gets the DF prefix "rotten " (meals, meat, cheese, fish) or "withered " (plants and plant
   growths). The UI shows this.
4. A rotten food item that generates miasma (M3.1) lying on a subterranean tile becomes a miasma source (M3).
5. Eating rotten food gives "AteRotten" (`social.rs` handles it already). Rotten items are already excluded from
   cooking, brewing and eating (`flags.rotten` checks in production.rs, social.rs and lib.rs).
6. Masterwork meals: when a masterwork meal (quality 5) rots or is eaten by vermin, the cook gets the
   "masterwork destroyed" thought (w:Kitchen: "Prepared meals can also trigger masterwork destruction if they
   are eaten by vermin"). Look for an existing masterwork-lost hook in `skills`. If there is none, record it as
   MISSING.

### M1.5 Data
- Kinds: as in `perishable`. Material ROTS/miasma: the table in M2.2, keyed by the last token of the material
  name.
- Depot tiles: `world.buildings` with kind `TradeDepot`.

### M1.6 Design
```rust
// decay15.rs
pub(crate) const DECAY_DAY_PHASE: u64 = 531;    // now % TICKS_PER_DAY; unused by other modules, before lifestyle miasma (637)
pub(crate) const DECAY_PERIOD: u64 = 100;
pub(crate) const DECAY_PHASE: u64 = 91;           // unused at %100 (taken: 0,1,11,37,41,50,59,61,73,83)

#[derive(Serialize, Deserialize, Default)] #[serde(default)]
pub(crate) struct Decay {
    pub enabled: Option<bool>,                 // None = on; Sim::set_decay_enabled
    pub rot: BTreeMap<i32, RotState>,          // item id -> state, only for items that can rot (M1, M2)
    pub scanned_to: i32,                       // items with id >= this have not been classified yet
    pub vermin: Vec<Vermin>,                   // M5
    pub next_vermin_id: u32,
    pub hunters: Vec<HunterState>,             // M6, per animal slot
    pub wear_pts: BTreeMap<i32, u32>,          // M7, item id -> wear points below the next level
    pub rep: DecayReport,
    #[serde(skip)] pub depot_tiles: Vec<u32>,  // sorted, rebuilt daily and in after_exact_load
    #[serde(skip)] pub miasma_src: Vec<u32>,   // sorted, deduplicated; from the last decay_day
}
#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub(crate) struct RotState { pub days: u16, pub stage: u8 /*0 fresh,1 rotten,2 skeletal/withered-gone*/, pub class: u8 /*RotClass*/ }
```
- `decay_day(now)` runs at `now % DAY == DECAY_DAY_PHASE`, called from `tick_inner` right after the society2 pass
  line (one tagged `if`).
  1. **Classify new items**: walk `world.items` **backwards** from the end while `id >= scanned_to` (new items
     are appended in id order). Add those that can rot to `rot`, then set `scanned_to = next_item_id`. On the
     first run (and after a load of an older save) do one full scan. No per-day full scan.
  2. For each `(id, st)` in `rot` (in BTreeMap order, which is deterministic): look up `item_idx`. Drop the entry
     if the item is gone. Skip it if `imeta.dead`. Then run `rot_safe_ex(idx)` (M1.2: the old `rot_safe` plus
     depot, minecart and wheelbarrow) for food, or `true` exposure for corpses (M2). Advance `days` and apply
     the stage transitions, collecting them into small reused `Vec`s (no allocation per item; keep the buffers
     in `#[serde(skip)]` fields).
  3. Apply the transitions in id order: set rotten, move to refuse, change the description, turn into
     bones/shells (M2), destroy.
  4. Rebuild `miasma_src` (M3).
- `storage.rs::storage_pass`: `if !self.decay_on() { self.rot_pass(); } // [decay15]`. Old saves: in
  `decay_day`, if `rot` is empty and `store.rot` is not, copy `store.rot` into `rot` (days and stage from
  `flags.rotten`) and clear `store.rot`. Report counters: `DecayReport { rotted_food, withered, rotted_away, ... }`.
  `storage_report` keeps its `rotted`/`rotted_away` fields (tagged lines add decay's counters).
- **Perf**: the reference fort has about 2 200 organic items (r6: 2 662) → about 3 000 BTreeMap visits a day.
  That is nothing next to the per-tick work. The old `rot_pass` walked all 17 000 items every day, so this is
  a small speedup. The miasma rebuild reuses the same walk (M3), so `lifestyle::miasma_day` no longer walks all
  items either.

### M1.7 Tests (`decay15/tests.rs`, synthetic worlds as in `lifestyle/tests.rs`)
- `meal_on_floor_rots_after_12_days_and_becomes_refuse`: 11 days fresh, day 12 rotten, `Cat::Refuse`, desc
  "rotten …", not in `foods`.
- `meal_in_barrel_outside_pile_rots` / `meal_in_barrel_on_pile_keeps`.
- `raw_fish_rots_even_on_pile`.
- `food_at_trade_depot_does_not_rot`, `food_in_minecart_does_not_rot`.
- `seeds_drinks_fat_milk_eggs_never_rot`.
- `withered_plant_makes_no_miasma`.
- `rot_timer_survives_exact_save` (saveload of the `rot` map; `saves11` round trip).
- `old_store_rot_migrates`.

---------------------------------------------------------------------------------------------------------------

## M2. Corpses, body parts, remains, hides: decay stages

### M2.1 DF rule (w:Corpse, w:Remains, w:Bone, w:Shell, w:Skin, w:Tanner, w:Miasma)
- A corpse goes "X corpse" → "rotten X corpse" → "X skeleton" (this stage is final). With body parts removed
  before death it is "mutilated corpse" → "partial skeleton". Destroyed parts give a "mangled corpse".
- Corpses and body parts rot **"just over time"**: being on a stockpile does not protect them (w:Miasma, "Rotten
  items"). Rotting corpses in refuse/corpse piles make miasma underground (w:Stockpile: "If placed indoors,
  decaying bodies will generate miasma").
- Severed parts "will decay into a type of bone, depending on the part". A severed body part "allowed to fully
  rot … may leave behind a stack of usable bones" (w:Bone).
- Vermin remains: "remains" → "rotten remains" → vanish. "Unlike corpses, remains do not cause miasma or leave
  bones." "Certain remains may rot away, depending on what they are composed of."
- Shelled vermin caught by fishing (turtle, oyster, mussel) give a shell when processed **or "if left to rot
  long enough"** (w:Shell).
- Raw hides (skins, CORPSEPIECE of SKIN material) "rot fairly soon" (w:Tanner). "Fresh raw hide" and "rotten raw
  hide" are refuse pile options (w:Skin). Only fresh hides can be tanned. Scales and chitin are not hides.
- Entombed corpses (coffin in a tomb) are not hauled and do not count as unburied, but they still rot inside
  the coffin (the export has 43 skeletons in buildings). They make no miasma (they are in a building).
- Rotting pets and friends give unhappy thoughts until they are buried (w:Stockpile, w:Thought). That is
  `population.rs`/`social.rs` and is out of scope.
- Animal trainers get bad thoughts when their bonded animal rots (w:Corpse). MISSING, P3.

### M2.2 Material rule (raws, export)
An item decays only if its material **ROTS**. CORPSE items made only of tooth, chitin or bone stay "fresh"
forever (export: cave crawler teeth, spider chitin). Rustfort has no ROTS flag (`MaterialInfo.tags` holds only
reaction classes), so derive it from the material token's last segment, using a compiled table taken from
`material_template_default.txt` (game facts with a named source are allowed):

| last token segment (e.g. `CREATURE:COW:MUSCLE`) | ROTS | GENERATES_MIASMA |
|---|---|---|
| MUSCLE, FAT, BRAIN, LUNG, HEART, LIVER, GUT, STOMACH, GIZZARD, PANCREAS, SPLEEN, KIDNEY, MILK, CHEESE | yes | yes |
| SKIN, SINEW, EYE, NERVE, BLOOD, ICHOR, GOO, SLIME, PUS, TALLOW, EGG_WHITE, EGG_YOLK, EXTRACT | yes | no |
| plant STRUCTURAL, LEAF, FRUIT, BUD, MUSHROOM, FLOWER (all `PLANT:*` food mats) | yes | no |
| BONE, CARTILAGE, HAIR, WOOL, FEATHER, SCALE, NAIL, TOOTH, IVORY, HORN, HOOF, PEARL, SHELL, CHITIN, LEATHER, PARCHMENT, SILK | no | no |

- Corpse materials: Rustfort's `corpse_mat` picks `CREATURE:<race>:SKIN` or the first creature material. A
  **whole corpse** (`CORPSE` whose description does not name a single non-rotting part, i.e. not "tooth",
  "chitin", "bone", "skull", "shell", "horn", "hoof", "nail") counts as ROTS + MIASMA when the creature has a
  MUSCLE or FAT material in `world.materials` (`CREATURE:<race>:MUSCLE`). Otherwise it does not rot. ESTIMATE:
  DF decides from its body components; this is the closest rule that uses only exported data.
- Body part `CORPSEPIECE`: test the material (the table above) **and** the description words (skin, hide → SKIN;
  bone, skull → BONE).
- Remains: see M2.5.

### M2.3 Timers (ESTIMATE, fitted to the export)
| Item | fresh → rotten | rotten → final | final |
|---|---|---|---|
| CORPSE (whole, rotting materials) | 14 days | +56 days | skeleton (desc "X skeleton", or "partial skeleton" if "mutilated"); `flags.rotten = false` (DF shows no "rotten" on skeletons); never decays further |
| CORPSEPIECE flesh part (limb, organ, "X's <part>", meat-like) | 14 days | +28 days | bones (M2.4), or destroyed if the part has no bone |
| CORPSEPIECE skin/hide | 14 days ("fairly soon") | +28 days | destroyed (skin has no bone) |
| REMAINS (species that rot) | 14 days | +14 days | destroyed |
| FISH_RAW of a shelled creature | 12 days (M1.3) | +28 days | 1 shell CORPSEPIECE ("X shell") |
| FISH_RAW of a creature without a shell, MEAT, FISH, FOOD, CHEESE | 12 days | +28 days | destroyed |

The timers keep `CORPSE_ROT_DAYS` (14) and `CORPSE_MIASMA_DAYS` (56) from lifestyle.rs, which the export
supports (section 1). Make lifestyle import them from decay15, or the other way round, so there is only one
pair of constants (tagged line).

### M2.4 Transitions (design)
- **Corpse to rotten**: `flags.rotten = true`; desc `"rotten " + desc` (for named corpses: `"<Name>'s rotten corpse"`).
  Undead stay untouched: reanimated units are units, not items.
- **Rotten corpse to skeleton**: `flags.rotten = false`; desc: replace "corpse" with "skeleton" ("mutilated
  corpse" → "partial skeleton"); keep `mat`. `industry::F_BUTCHER` already treats a desc containing "skeleton"
  as bones and skull only. `lifestyle::desc_skeletal` already stops miasma for it.
- **Body part to bones**: destroy the part and create, on the same tile, `CORPSEPIECE` with
  `mat = CREATURE:<race>:BONE` (look it up in `world.materials`; skip if missing), desc "<race> bone", stack
  ESTIMATE = 1 for hands, feet, toes and fingers, 2 for limbs (a desc with "arm", "leg", "tail"), 3 for "upper
  body"/"lower body". A desc with "head" gives a "<race> skull" (stack 1). The new items go through
  `create_item`, so their `imeta.cat` must be set to `Cat::Refuse` (M4).
- **Raw fish to shell**: the creature has a `CREATURE:<race>:SHELL` material → create "X shell"
  (`CORPSEPIECE`, the SHELL material, stack 1). Vanilla shell vermin (raws): pond turtle, mussel, oyster,
  nautilus, anomalocaris, wiwaxia.
- **Rotten to gone**: `destroy_item(id)` unless `imeta.reserved > 0` (a job or undead claim). Then retry the next
  day.
- **Description changes**: change `world.items[idx].desc` only. It is not part of the state hash unless the
  hash already covers it. Also do not rely on the description for the stage: `RotState.stage` is the truth, and
  `desc` is only for display. The skeleton test in industry/lifestyle reads `desc`, so keep it in sync.

### M2.5 Remains (MISSING rule)
In the export only bat remains ever rotted; olm, cave swallow and cap hopper remains stayed fresh for hundreds of
days. The raws give no token that separates them (all are VERMIN_GROUNDER and have SKIN, FAT and MUSCLE). Until
`rot_timer` is exported (section 10):
- ESTIMATE rule: remains rot if the species has a `CREATURE:<race>:HAIR` material (mammals: bat, rat, hamster…).
  Otherwise they keep forever. This reproduces all 76 remains in the export (bat has hair; olm and cap hopper
  remove HAIR in their raws; cave swallow has feathers). Mark it ESTIMATE in FIDELITY.
- Remains make no miasma (wiki) and leave nothing.

### M2.6 Rustfort today
- `lifestyle::miasma_day`: counts `corpse_age` days only for corpses made in this run
  (`id >= first_new_item`), never sets `flags.rotten`, never changes the description and never makes
  skeletons or bones. Imported corpses are rotting only if the export already said so.
- `storage::rot_pass` ignores corpses, body parts and remains.
- Skins never rot (`industry::is_tannable` checks `!flags.rotten`, but nothing ever sets it).
- Remains (`animals.rs` cats, "rat remains") never rot and never vanish.

### M2.7 Hooks
- `industry.rs` F_BUTCHER: nothing. New skins and bones are found by the "classify new items" scan (M1.6).
- `combat.rs:2088` (severed part): nothing. The scan finds it.
- `undead.rs`: should call `decay15_api::corpse_stage(id)`. Tell `[undead13]` in the commit message or PR notes.

### M2.8 Tests
- `corpse_rots_at_14_and_is_skeleton_at_70` (desc, flags, butcher gives only bones/skull afterwards).
- `teeth_and_chitin_corpses_never_rot`.
- `corpse_on_refuse_pile_still_rots` (piles do not protect corpses).
- `severed_arm_rots_to_two_bones`, `severed_head_rots_to_skull`.
- `raw_hide_rots_and_cannot_be_tanned` (the tan job refuses it; the rotten hide is refuse; it vanishes after 42 days;
  no miasma).
- `turtle_raw_fish_rots_to_shell`.
- `bat_remains_vanish_olm_remains_stay`.
- `reserved_rotten_item_is_not_destroyed`.

---------------------------------------------------------------------------------------------------------------

## M3. Miasma sources and spread

### M3.1 Sources (DF rule)
- w:Miasma: miasma comes from rotting items on **Subterranean** tiles; "whether a tile is Inside or Outside or
  Light or Dark makes no difference". w:Stockpile and w:Refuse: tiles that are "Light Above Ground" (even when
  roofed later, so Inside) make none.
- Only materials with **GENERATES_MIASMA** produce it (raws, M2.2 table): rotting skin, sinew, plants and eggs do
  not. Remains do not (w:Remains).
- An item rots "from time to time" while rotten, so the source is a **rotten** item (stage 1). A skeleton is not
  a source.
- Necrotic body parts of living dwarves produce miasma "from time to time" (syndromes). MISSING, P3.
- Rotten items in a container or a building (coffin) are not sources (Rustfort's current `loose` test; ESTIMATE,
  the wiki does not say).

### M3.2 Spread (DF rule, w:Miasma)
- It spreads N, S, E, W, **up and down**, never diagonally.
- It passes grates, bars, **open** doors and hatches, and never closed ones. In DF a door is closed unless a
  creature is in it. When a dwarf walks through a door next to miasma there is "only a chance it will spread
  through".
- "Once miasma has been generated … it will spread for a while, eventually disappearing. This is true even if
  the original source has been removed." So it is a lingering cloud, not an aura.
- Water: rotten items "submerged in 7/7 water" make none.

### M3.3 Rustfort today (`lifestyle::miasma_day`, daily phase 6 of the society2 wheel)
- Sources: rotten corpses, parts and food (all kinds, also remains, eggs, plants: wrong), loose, not `OUTSIDE`.
- Spread: a 2-tile orthogonal BFS on the **same z** over `nav.walkable` tiles that are not OUTSIDE. Doors count
  as walkable, so miasma passes closed doors. The set is rebuilt from nothing each day (no lingering).
- Thoughts: "Miasma" at most every 2 days per dwarf. Calibrated to the export's 30 to 60 a month.

### M3.4 Gaps
1. The source test should use `SUBTERRANEAN`, not `!OUTSIDE` (roofed surface buildings, channelled-then-floored
   refuse rooms). **Behaviour change.**
2. Only GENERATES_MIASMA materials (no withered plants, skins, remains or eggs).
3. Blocked by closed doors and hatches (`Tile` has a door or hatch building that no unit is standing in).
4. Up and down spread through open space (stairs, ramps, open floor above: the tile above is not a floor or wall).
5. Lingering: a cloud that dissipates (ESTIMATE below).
6. Not under 7/7 water.

### M3.5 Proposed model (ESTIMATE where noted)
- Keep the daily rebuild, but make the set the union of the **sources' reach** and the **last 2 days' clouds**:
  `miasma = bfs(sources, range 2) ∪ miasma_prev` with `miasma_prev` aged out after `MIASMA_LINGER_DAYS = 2`
  (ESTIMATE). Implement it as `BTreeMap<u32 tile, u8 days_left>` in decay15 (`#[serde(default)]`, saved).
- BFS neighbours: the 4 orthogonal tiles plus z±1 when the vertical move is open (use `nav` up/down walkability:
  stairs or ramps, or an open tile above with no floor). Skip tiles with a closed door or hatch: a door tile
  blocks unless `unit_at(tile)` is occupied on the daily rebuild tick (deterministic "chance": occupied now).
  Skip tiles with water depth 7.
- Keep the 2-tile range (calibrated).
- The thought rule is unchanged.
- The calibration must be checked again: the "Miasma" thoughts per month on the 2-year bench should stay at 30
  to 60. If doors cut it too much, raise the range to 3 (ESTIMATE).

### M3.6 Hook into lifestyle (tagged lines only)
- In `lifestyle::miasma_day`: at the top, `if self.decay_on() { self.life.miasma = self.decay_miasma_set(); /*
  stats */ return; } // [decay15]`. The old body stays for `decay off`.
- `decay15::decay_miasma_set(&mut self) -> HashSet<u32>` builds the set from `decay.miasma_src` (made in
  `decay_day`). Society2 runs at `now % 100 == 37` with lifestyle `phase = (now / 100) % 12`
  (`locations.rs`, `WHEEL = 12`), so `miasma_day` (phase 6) runs at day tick 637. `DECAY_DAY_PHASE = 531` runs
  before it on the same day, so the miasma uses the same day's sources.
- The `Life.corpse_age` map becomes unused when decay is on. Leave it in place for save compatibility.

### M3.7 Tests
- `rotten_meat_in_roofed_surface_building_makes_no_miasma` (Inside but not subterranean).
- `miasma_does_not_pass_closed_door`, `miasma_passes_open_door_with_dwarf_in_it`.
- `miasma_spreads_up_stairs`.
- `miasma_lingers_two_days_after_source_removed`.
- `rotting_hide_and_withered_plants_make_no_miasma`.
- Keep `lifestyle/tests.rs::rotting_corpses_indoors_make_miasma` and `..._outdoors_make_no_miasma` passing (the
  synthetic world must set SUBTERRANEAN on indoor tiles; check how the test map is built).

---------------------------------------------------------------------------------------------------------------

## M4. Refuse: classification, hauling, standing orders

### M4.1 DF rule (w:Refuse, w:Stockpile, w:Standing orders, w:Hauling, w:Corpse)
- **Refuse pile** takes corpses and body parts of **non-sapient, non-tame** creatures (and of undead), bones,
  skulls, shells, teeth, horns, hair/wool, raw hides (fresh and rotten), anything rotten, withered plants,
  damaged furniture, **tattered (XX) clothing** and vermin remains.
- **Corpse pile** (category `corpses`) takes the corpses of sapient and tame creatures. Those are hauled with
  the Burial labor (corpses) or Refuse hauling (w:Corpse: "either refuse hauling or burial, depending on the
  corpse type").
- Hauling labors: Refuse hauling "will haul rotting food, and non-dwarf bodyparts to refuse stockpiles" and does
  the dumping. Burial hauls dwarf corpses to coffins (Rustfort `population.rs`).
- **Standing orders** (w:Standing orders, defaults: gather refuse on, outdoor refuse off, all "save"):
  - Workers gather refuse (on/off: refuse is ignored completely when off).
  - Workers gather outdoor refuse (off by default: refuse on Outside tiles is ignored, dumped items too).
  - Workers ignore outdoor vermin remains (only with outdoor on).
  - Save or dump, each separately: corpses (and parts, ivory, horn, nervous tissue, cartilage), skulls, bones,
    shells, skins (also chitin and scales), hair/wool, other (remains, rotten products, withered plants, damaged
    furniture, tattered clothes). "Dump" sends the type to a garbage dump zone, even items already in a pile.
- Refuse piles **damage some items**: clothes and armor "quite quickly", bones and shells "at a much slower pace",
  faster with vermin (w:Bone, w:Stockpile). See M7.3.
- Bins are not used in refuse piles (w:Storage).

### M4.2 Rustfort today
- `items.rs::Cat::of_kind`: CORPSE and CORPSEPIECE → `Cat::Corpses` (HAUL_BODY); REMAINS and VERMIN → `Cat::Refuse`
  (HAUL_REFUSE). So bones, skulls, skins, shells and severed parts go to **corpse** piles with the Burial-style
  labor. They sit in corpse piles. A refuse pile only gets remains.
- Rotten food stays `Cat::Food`; tattered clothes stay `Cat::Armor`.
- Haul choice: `perf::haul_pick` picks the nearest best candidate over all wanted categories. No priority by
  category; outdoor refuse is not separated.
- Pile filters (`refuse/bones/...`, `fresh_raw_hide`) are imported but the sim does not apply them (it matches
  categories only).
- Dump flag `flags.dump` and garbage dumps exist (`construct.rs`).

### M4.3 Design
1. **Category function** `decay15::refuse_cat(idx) -> Option<Cat>`, applied in the classify scan (M1.6) and at
   every stage change:
   - CORPSE of a sapient (has a name, or "'s corpse", or a race with `INTELLIGENT`/`CAN_LEARN`, as
     `industry::is_butcherable` tests) or of a tame animal (`unit.flags.tame` at death; ESTIMATE from the desc
     if the unit is gone) → `Cat::Corpses`. Everything else among CORPSE/CORPSEPIECE → `Cat::Refuse`.
   - Rotten or withered food → `Cat::Refuse`.
   - Clothing or armor with `wear >= 3` (XX) and **not worn** → `Cat::Refuse` (w:Stockpile: "any XXdamaged itemsXX
     will be moved to the refuse stockpile").
   - Use `set_item_cat` (existing; it requeues the haul).
   - **Behaviour change**: bones and skulls move from corpse piles to refuse piles. Check that the reference
     fort has refuse piles (yes: 'pile:refuse' tiles in the export). The `Cat::of_kind` default is unchanged
     for imports; decay15 reclassifies at its first full scan, which is deterministic.
2. **Pile filters** for refuse: apply `StockpileSettings::filter_allows("refuse", group, token)` with the groups
   `corpses`, `body_parts`, `skulls`, `bones`, `hair`, `shells`, `teeth`, `horns` (token = creature) and the
   options `fresh_raw_hide`/`rotten_raw_hide`. This needs a per-pile accept hook in the hauling target search.
   **Shared code** (storage/lib haul target). Keep it to one tagged call
   `self.decay_pile_accepts(pile, idx)` where the target pile is chosen, only for `Cat::Refuse` items. P2.
3. **Standing orders** (new, `decay15::RefuseOrders`, saved, defaults as in DF):
   `gather: bool = true, outdoor: bool = false, ignore_outdoor_remains: bool = false,
   dump: u8 bitmask {CORPSES, SKULLS, BONES, SHELLS, SKINS, HAIR, OTHER} = 0`.
   - `gather == false`: `Cat::Refuse` items are not added to the haul list (tagged guard in `haul_add`, or keep
     them with `imeta.blocked_until = u64::MAX`). Prefer one guard in `haulable` (tag), since `rebuild_haul` and
     `haul_add` both call it.
   - `outdoor == false`: a refuse item on an OUTSIDE tile is not haulable.
   - `dump` bit set: decay15 sets `flags.dump = true` on matching refuse items (daily); construct.rs then dumps them.
   - API: `Sim::refuse_orders()`, `Sim::set_refuse_orders(RefuseOrders)`. Not exported yet (section 10).
4. **Priority**: DF has no category priority in hauling (w:Hauling: "To ensure that high-priority hauling tasks
   (like food) … employ a large number of haulers"). Rustfort's nearest-first pick is OK. Do **not** add a
   priority. FIDELITY: OK.

### M4.4 Tests
- `bones_and_skins_go_to_refuse_pile_not_corpse_pile`.
- `dwarf_corpse_stays_corpse_category_goblin_corpse_too` (sapient) / `cow_corpse_is_refuse`.
- `rotten_meal_is_hauled_to_refuse_pile`.
- `tattered_unworn_clothing_becomes_refuse`.
- `outdoor_refuse_is_ignored_by_default`, `gather_refuse_off_stops_refuse_hauling`.
- `dump_bones_order_marks_bones_for_dumping`.
- `refuse_pile_bones_filter_rejects_skulls` (P2).

---------------------------------------------------------------------------------------------------------------

## M5. Vermin

### M5.1 DF rules (w:Vermin, w:Creature token, raws)
- Vermin are creatures under about 2 kg. They cannot be fought, do not trigger traps (except animal traps), do
  not breed but "spawn" in their biome, and some are inexhaustible.
- Tokens (raws counts in vanilla):
  - `VERMIN_EATER`: "will attempt to eat exposed food". Rat, hamster, lizard, large roach, demon rat, fluffy
    wambler, two-legged rhino lizard (7 creatures).
  - `PENETRATEPOWER:n` (caste): "controls the ability of vermin to find a way into containers when they are
    eating food from your stockpiles". Rat 2, hamster 2, large roach 2, rhino lizard 2, lizard 1, wambler 1,
    demon rat 3.
  - `VERMIN_ROTTER`: "attracted to rotting stuff and loose food left in the open and cause unhappy thoughts to
    dwarves who encounter them" (fly, acorn fly, blood gnat, creepy crawler, knuckle worm).
  - `VERMIN_GROUNDER` (93 creatures), `VERMIN_SOIL` (appear near dirt or mud, dug up by ROOT_AROUND poultry),
    `VERMIN_SOIL_COLONY` (one tile cluster, ants), `VERMIN_FISH` (in water, fishing: Rustfort fishing has it),
    `VERMIN_MICRO` (swarms), `VERMIN_NOROAM`, `VERMIN_NOTRAP`, `VERMIN_HATEABLE` (22 creatures).
  - Each sub-region keeps up to 7 creatures per list (GROUNDER, SOIL, SOIL_COLONY, …), the nearest valid ones
    (w:Creature token, sub-region wildlife lists).
  - `POPULATION_NUMBER` (rat 250:500) and `FREQUENCY` (rat 100, creepy crawler 10, knuckle worm 25).
  - `BIOME` and `UNDERGROUND_DEPTH` (olm, cap hopper 1:2 = cavern layers 1-2).
- Where they show up: "usually encountered near food and refuse stockpiles" and "active garbage dumps seem to
  increase the likelihood" (w:Garbage dump). Poultry "root around, which can attract vermin" (w:Pet).
- Eating: "Food will never spoil while in a stockpile, although it may attract and be eaten by vermin"
  (w:Stockpile). "Metal barrels are supposedly better for food stockpiles because they resist vermin" (w:Using
  bins and barrels). Large pot: material matters for "vermin resistance" (w:Large pot). Bones, skulls and shells in a refuse pile
  "with high vermin levels … will randomly disappear" (w:Stockpile, w:Shell).
- Thoughts (captions in `social/captions.rs`): "NearVermin" (near a hated vermin), "AnnoyedVermin" (shared a
  tile with an annoying vermin), "PesteredVermin", plus "EatVermin" when starving (w:Food: at 75 000 hunger the
  dwarf "begins hunting for vermin").
- Vermin are drawn as creatures that blink in and out of view.

### M5.2 Rustfort today
- Fishing uses vermin fish (`social.rs` fishing, `caverns.rs` reads `VERMIN*` flags for cavern fish).
- Cats: `animals.rs:1583`. Every adult `HUNTS_VERMIN` animal has a 1/14 chance a day to make "rat remains" at its
  own tile. There are no vermin, no food eating, no thoughts.

### M5.3 Gaps
No vermin entities, no species choice, no food eating, no container resistance, no rotter flies near rot, no
hateable thoughts, no starving dwarves eating vermin.

### M5.4 Design (ESTIMATE everywhere except the tokens)
**Species pool** (built once, `#[serde(skip)]`, rebuilt in `after_exact_load`): from the creature raws in
`world` (the creature list Rustfort imports, with flags). Take creatures with VERMIN_GROUNDER or VERMIN_EATER
or VERMIN_ROTTER, excluding VERMIN_FISH-only species. Split them:
- `surface`: no `UNDERGROUND_DEPTH`; biome check if `World::climate` is known (`[weather13]` fills it), else
  keep only those with `BIOME:NOT_FREEZING`/`ANY_LAND`/any temperate biome (ESTIMATE).
- `cavern[k]`: `UNDERGROUND_DEPTH` covering cavern layer k (`caverns.rs` knows the layers).
- Keep at most 7 per list, sorted by token (deterministic stand-in for "nearest"), weighted by `FREQUENCY`.
- Calibrate against the export: Trussroads shows bat, cave swallow, olm and cap hopper in the caverns and **no
  rat**, so its surface list probably has no VERMIN_EATER. Give `set_vermin_species(list)` for tests.

**State**:
```rust
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Vermin { pub id: u32, pub sp: u16, pub tile: u32, pub home: u32 /*pile or -1*/, pub born: u64,
                          pub next_meal: u64, pub kind: u8 /*EATER, ROTTER, ROAMER*/ }
```
Cap: `MAX_VERMIN = 24` live at once on the map (ESTIMATE). They are not units (no `ai` slot, no pathing). They
are a sparse list, so the per-tick cost is zero.

**Spawning** (in `decay_pass` every `DECAY_PERIOD`, one spawn roll per **day** per candidate site, staggered by
site index % 12 so a pass handles 1/12 of the sites):
- Sites:
  - (a) food stockpiles holding exposed food → eaters. "Exposed" here means any food item not in a container
    that resists the species (M5.5).
  - (b) refuse piles and garbage dumps → eaters and roamers; dumps count double (w:Garbage dump).
  - (c) tiles with a rotten item (M3 sources, also outdoors) → rotters.
  - (d) cavern floor tiles (via `caverns.rs` revealed cavern list) → roamers of the cavern list. These give the
    export's bats and swallows.
- Chance per site and day: `p = 1/40` (a, b), `1/20` (c), `1/30` per revealed cavern layer (d), ESTIMATE.
  Calibration target: with 4 cats, about 35 to 40 remains a year (section 1), nearly all from (d) on Trussroads.
- A spawned vermin lives `VERMIN_LIFE_DAYS = 14` (ESTIMATE). Then it leaves (despawns) unless killed. Roamers
  move to a random walkable tile within 8 of their site once a day (no pathing, `nav.walkable` only).

**Eating** (eaters only): once per day, at `next_meal`, choose the nearest exposed food item within the site's
tiles (or 4 tiles), in id order. Eat **one unit** of its stack (`stack -= 1`; at 0 `destroy_item`), count
`rep.vermin_ate`, and if the item was a masterwork meal (quality 5) give the cook "masterwork destroyed"
(M1.4.6). Bones, skulls and shells in a refuse pile: eaters and roamers there destroy one such item every
`VERMIN_BONE_DAYS = 7` days (ESTIMATE, "randomly disappear").

**Thoughts** (in the same pass, only for citizens on a vermin's tile or next to it):
- `VERMIN_HATEABLE` species that the dwarf hates. Rustfort preferences: check whether
  `Unit.preferences` holds dislikes (`HATE`/`dislike` creature entries from the export). If not, ESTIMATE: a
  dwarf hates a hateable species with 1/10 chance by `splitmix(unit id ^ species)`. Effect: "NearVermin", at
  most once per 7 days per dwarf.
- Rotters (flies) on a dwarf's tile: "AnnoyedVermin" at most once per 7 days (ESTIMATE).
- Use `self.feel(s, "...", Class::NegMild, …)` as lifestyle does.

**Starving dwarves eat vermin** (w:Food): when a citizen's hunger counter is at 75 000 or more and there is no
reachable food, they "eat" the nearest vermin within 20 tiles, which resets hunger by 50 000 as an Eat would,
gives "EatVermin" and removes the vermin. This needs a tagged hook in the hunger decision (`needs`/`lib.rs`
eat). P3.

**Animal traps and "Capture a live land animal"**: out of scope for v0.15 (MISSING).

### M5.5 Container resistance (ESTIMATE)
`resist(container)`: no container 0; wood, plant, leather, cloth, bone or shell container 1; stone, ceramic or
glass container 2; metal container 3. An eater can reach the food when `PENETRATEPOWER > resist`. So rats (2) get
into wooden barrels but not into stone pots; lizards and wamblers (1) only eat loose food; the demon rat (3)
gets into everything but metal. Source: the token description plus "metal barrels resist vermin". The exact
comparison is not on the wiki.

### M5.6 Tests
- `rat_eats_loose_food_on_pile`, `rat_cannot_open_stone_pot_but_opens_wooden_barrel`,
  `lizard_only_eats_loose_food`.
- `no_eater_species_means_no_food_eaten` (Trussroads-like pool).
- `flies_spawn_at_rotting_items` (rotters).
- `hated_vermin_gives_near_vermin_thought_once_a_week`.
- `vermin_state_round_trips_exact_save`.
- Determinism: `RAYON_NUM_THREADS=1` vs default on the bench (all of the above is serial).

---------------------------------------------------------------------------------------------------------------

## M6. Vermin hunters (cats, falcons; dogs do not)

### M6.1 DF rules (w:Cat, w:Remains, w:Creature token, raws)
- `HUNTS_VERMIN`: the creature "hunts and kills nearby vermin, randomly walking between places with food laying
  on the ground or in stockpiles, to check for possible [VERMIN_EATER] vermin, but they'll kill any other vermin
  too". In vanilla only the **cat** has it (raws: 1 creature). `DIVE_HUNTS_VERMIN` on a tame creature acts the
  same: the **peregrine falcon** (w:Vermin: "Vermin can be hunted by cats and peregrine falcons").
- **Dogs do not hunt vermin** (no token). The task's "cats/dogs" is a misunderstanding. Document it as OK, not
  MISSING.
- `RETURNS_VERMIN_KILLS_TO_OWNER`: "If it kills a vermin creature and has an owner, it carries the remains in its
  mouth and drops them at their feet" (cats; w:Cat: "the dwarf will then dismiss the cat, at which point the cat
  will drop the remains").
- A cat pastured over a food pile "will keep it free of troublesome vermin" (w:Cat).
- Cats do not need food (w:Cat). Remains are refuse (w:Remains).

### M6.2 Rustfort today
`animals.rs:1580-1588`: `species.vermin` = HUNTS_VERMIN; adult; 1/14 a day → "rat remains" at the cat's tile. No
movement toward food, no link to real vermin, no falcon, no owner delivery. It gives about 96 remains a year
for 4 cats, against the export's 35 to 40, and rats where the export has none.

### M6.3 Design
- Replace the roll (tagged guard `if !self.decay_on()` around the old 3 lines, so `decay off` keeps the old hash).
- `HunterState { target_vermin: u32, next_patrol: u64 }` per animal slot (decay-owned `Vec`, resized like
  `animals.st`).
- **Patrol** (ESTIMATE): an idle adult hunter with no owner errand has a 1/4 chance a day to pick a food
  stockpile or a vermin site as its wander goal. Use the existing wander task (`Task::Wander` with a goal tile).
  The animals module owns movement, so this needs one tagged call
  `self.decay_hunter_goal(s) -> Option<u32>` where animals choose a wander goal.
- **Kill**: in `decay_pass`, for each hunter (slot order) and each live vermin within Chebyshev distance 2 on the
  same z: kill chance 1/2 per pass (ESTIMATE). On a kill: remove the vermin and make a `REMAINS` item
  (`mat` = `animal_mat(<species token>, "REMAINS", "<name> remains")`, desc "<name> remains") at the vermin's
  tile; if the hunter has an owner (`species.adopts` and the owner pet link) and RETURNS_VERMIN_KILLS_TO_OWNER
  (cats), put it at the owner's tile instead. Count `rep.vermin_killed` (keep `animals.rep.vermin_killed` in
  step for the old report, tagged).
- Falcons: `DIVE_HUNTS_VERMIN` only when tame. Add the flag to `Species` (tagged field) or keep a decay-side
  `Vec<bool>` built from the raws.
- Calibration: with the cavern roamers of M5.4(d) and patrols that rarely go there, the yearly remains should
  come out at 35 to 40 on the 2-year bench. Tune the spawn chance (d), not the kill chance.

### M6.4 Tests
- `cat_kills_rat_near_food_pile_and_remains_appear`.
- `owned_cat_brings_remains_to_owner`.
- `dog_does_not_hunt_vermin`.
- `tame_falcon_hunts_wild_falcon_does_not`.
- `no_vermin_no_remains` (no more remains from nothing).

---------------------------------------------------------------------------------------------------------------

## M7. Item wear

### M7.1 DF rule (w:Wear)
- Levels: none → x → X → XX → destroyed. "Every 806400 'points' of wear will increase the item's damage level
  one step." 806 400 = 2 years of ticks. A burning item gets 10 points a tick and "will be totally consumed
  after exactly 322560 game ticks", which is 3 225 600 points = **4 levels**. This confirms destruction after
  XX, at 4 × 806 400.
- Value multiplier by level: 100 %, 75 %, 50 %, 25 %.
- "Clothing that is being worn will wear out over time … regardless of material". Invaders' clothing too.
- "All improvable items made from creature or plant materials (cloth, silk, leather, yarn, wood) … gaining one
  level of wear every 100 years", even in stockpiles. This is irrelevant within a 2-year bench: MISSING, P4.
- "**Armor does not wear out over time, unless put in a refuse stockpile.** Combat damage can inflict wear."
  Refuse piles degrade clothes and armor "quite quickly", and bones and shells "at a much slower pace" (w:Bone).
- Combat: "For weapons, the chances of suffering wear depend on the material properties of the body tissues or
  armor encountered during an attack, while armor suffers wear depending on the material properties of the body
  part/weapon hitting it … relative fracture, yield, and possibly strain-at-yield, impact or shear depending on
  the type of attack". Example: a leather helm wears fast from dog bites; bone and shell helms wear slowly.
- Above HEATDAM_POINT or below COLDDAM_POINT: accelerated wear (needs temperature: P4, `[weather13]`).
- Artifacts never wear (wear stays 0).
- Building destroyers wear doors and hatch covers (invasion topic, out of scope).
- Cages do not prevent wear.

### M7.2 Rustfort today
- `lifestyle::wear_day`: worn **non-metal, non-stone** clothing: +1 level every `WEAR_DAYS = 336` days (ESTIMATE,
  calibrated on the export's wear mix). After level 3 the item falls apart ("RottedClothing"). Metal is excluded,
  which contradicts "regardless of material". The calibration found the export's metal clothing mostly unworn,
  so leave it. Not in scope: lifestyle owns it.
- No other wear anywhere: weapons, armor in combat, refuse piles, value.
- `Item.wear: u8` exists (rf-core) and is exported by DF (`getWear`).

### M7.3 Refuse-pile wear (ESTIMATE)
- Items on a refuse-pile tile (or in a garbage dump zone, ESTIMATE: no) that are ARMOR, HELM, GLOVES, SHOES, PANTS,
  SHIELD (any material) or clothing: +`REFUSE_WEAR_PTS_DAY = 806_400 / 56` points a day, i.e. one level per 8
  weeks, destroyed after about 7.5 months ("quite quickly").
- Bones, skulls and shells on a refuse pile: one level per 336 days (`806_400 / 336` a day, "much slower").
  At XX + 1 they are destroyed. Vermin eat them faster (M5.4).
- State: `Decay.wear_pts: BTreeMap<i32, u32>`, only for items on refuse tiles. Visit them in the daily pass:
  reuse the refuse-pile tile set; walk the item list of those tiles only (the `pile_occ`/`pile_at` index), never
  all items.
- Interaction with M4: tattered (XX) clothing becomes `Cat::Refuse` and goes to the refuse pile, where it then
  falls apart within about 8 weeks. That is DF's garbage cycle.

### M7.4 Combat wear (ESTIMATE, fitted to the export: metal armor 5 to 10 % worn, weapons 0 %)
Hook: `combat.rs::strike`, after the armor layer loop decides deflect or penetrate (around lines 1713 to 1775).
One tagged call `self.decay_combat_wear(a_weapon_id, armor_hits: &[(item_id, deflected)], wmech, layer mats, r)`.
Use the Blow's `r: &mut Rng` (an existing seeded stream), so no new RNG stream is needed. **Check** that the
number of draws from `r` does not change other results: draw from a **separate** stream
`Rng::for_unit(seed, a, now, 0xDEC1)` instead, so combat outcomes stay as they were and only the wear differs.
- **Armor**: each armor piece hit that **deflects or absorbs** the blow:
  - edged attack: `ratio = attacker material shear_fracture / armor material shear_yield`;
  - blunt attack: `ratio = momentum-based force / armor impact_yield` (use the same quantities as the blunt
    deflection test `2 * Sw * IYw < A * Da`: `ratio = (A * Da) / (2 * Sw * IYw)` of the armor);
  - wear chance `p = clamp(ratio / 4, 0, 0.25)` (ESTIMATE), adding `806_400 / 4` points (a quarter level).
  - Teeth and claws against leather: bites use the attacker's TOOTH/NAIL material from `combat.bodies`. Leather
    shear yield is low, so `ratio` is high and the leather helm wears out fast, as the wiki example says.
- **Weapon**: on each hit that touches a layer (armor or tissue) whose `shear_fracture` (edged) or
  `impact_fracture` (blunt) is **above** the weapon's yield: `p = 0.05` (ESTIMATE), a quarter level. Against
  flesh (low fracture) this never fires, which matches 0 worn weapons in the export.
- Wear points live in `Decay.wear_pts`; when they pass 806 400, set `item.wear += 1`. At wear 4 destroy the item
  (unequip it first; the armor pieces come from `armor_over`'s item list). Announce "X's Y has been destroyed"?
  DF wording not known: no announcement (MISSING).
- Artifacts (`flags.artifact` or quality 6): no wear.
- Training (sparring) uses the same strike path: check whether sparring blows run through `strike`. If they
  do, spar blows wear armor too, which DF does as well (ESTIMATE).

### M7.5 Value
`item.value` is DF's base value at export time. Multiply by `WEAR_VALUE_PCT = [100, 75, 50, 25]` at the moment
the level changes (`value = value * pct[new] / pct[old]`), so trade and wealth see it. One function,
`decay15::apply_wear_level(idx, new_level)`. lifestyle's `wear_day` should call it too (one tagged line), so
clothing value drops as well.

### M7.6 Heat and cold wear, burning, 100-year wear
MISSING; needs fire and temperature (`[weather13]`). Constants for later: 10 points a tick when burning.

### M7.7 Tests
- `wear_value_drops_25_percent_per_level`.
- `xx_armor_on_refuse_pile_falls_apart_in_8_weeks`.
- `leather_helm_wears_from_dog_bites_steel_helm_does_not` (synthetic 200 bites).
- `steel_sword_on_flesh_never_wears`, `copper_axe_on_steel_armor_wears`.
- `artifact_never_wears`.
- `combat_outcome_unchanged_by_wear_stream` (compare kills and wounds with wear on/off on a fixed seed).

---------------------------------------------------------------------------------------------------------------

## 9. Architecture summary

### 9.1 Files
- `crates/rf-sim/src/decay15.rs`: `Decay` state, constants, `decay_day`, `decay_pass`, the classify scan, rot,
  corpse stages, miasma sources and cloud, refuse category and orders, vermin, hunters, wear. If it grows past
  about 1 500 lines, split it into `decay15/{rot.rs, vermin.rs, wear.rs}`.
- `crates/rf-sim/src/decay15/tests.rs`.
- `crates/rf-sim/src/decay15_api.rs`:
  ```rust
  pub enum CorpseStage { Fresh, Rotten, Skeleton, NotRotting }
  pub struct DecayReport { rotted_food, withered, rotted_away, corpses_rotted, skeletons, parts_to_bones,
      shells_from_fish, hides_rotted, miasma_sources_max, vermin_spawned, vermin_ate, vermin_killed,
      remains_made, wear_levels_combat, wear_levels_refuse, items_worn_out }
  pub struct VerminInfo { pub race: String, pub name: String, pub pos: Pos, pub eater: bool, pub hateable: bool }
  pub struct RefuseOrders { gather, outdoor, ignore_outdoor_remains, dump_corpses, dump_skulls, dump_bones,
      dump_shells, dump_skins, dump_hair, dump_other }
  impl Sim {
      pub fn set_decay_enabled(&mut self, on: bool); pub fn decay_enabled(&self) -> bool;
      pub fn decay_report(&self) -> DecayReport;
      pub fn corpse_stage(&self, item_id: i32) -> CorpseStage;      // for [undead13] and the UI
      pub fn rot_days(&self, item_id: i32) -> Option<(u16 days, u16 limit)>; // item sheet "rots in N days"
      pub fn vermin(&self) -> Vec<VerminInfo>;                     // map overlay
      pub fn refuse_orders(&self) -> RefuseOrders; pub fn set_refuse_orders(&mut self, o: RefuseOrders);
      pub fn miasma_tiles(&self) -> Vec<Pos>;                      // overlay
  }
  ```
- `crates/rf-app/src/ui/decay15.rs`: a "Refuse" standing-orders panel (DF layout: toggles plus save/dump per
  type), an item tooltip line ("rotten", "rots in N days", wear x/X/XX), a vermin and miasma map overlay
  (drawn only when switched on), and report rows in the fort statistics. Headless check:
  `rustfort play --screenshot target/decay15.png --frames N`.

### 9.2 Shared-file edits (each tagged `// [decay15]`)
| File | Edit |
|---|---|
| `lib.rs` | `pub mod decay15; pub mod decay15_api;`, `pub use` of the API types; `decay: decay15::Decay` field and its `Default`; in `tick_inner`: `if now % DAY == decay15::DECAY_DAY_PHASE { self.decay_day(now); }` and `if now % decay15::DECAY_PERIOD == decay15::DECAY_PHASE { self.decay_pass(now); }` (next to the contract13 hooks, before the society2 pass); `self.decay_hash(&mut mix)` in `state_hash`; `haulable`: `&& self.decay_haul_ok(idx)` (refuse orders). |
| `saves11.rs` | `("decay", enc(&self.decay))` in `exact_parts`; `after_exact_load`: `self.decay_after_load();` (rebuild the depot tiles, species pool and miasma sources). |
| `persist.rs` | `v.push(("decay", fnv(|mix| self.decay_hash(mix))));` |
| `storage.rs` | `storage_pass`: `if !self.decay_on() { self.rot_pass(); }`; `storage_report`: add decay's rotted counters. |
| `lifestyle.rs` | `miasma_day`: early return to `decay_miasma_set` (M3.6); `wear_day`: `apply_wear_level` for value (M7.5); one constant pair (M2.3). |
| `animals.rs` | the cat roll behind `!self.decay_on()`; the wander goal hook (M6.3); optional `dive_hunts` species flag. |
| `combat.rs` | one call in `strike` (M7.4). |
| `industry.rs` | none needed (the scan finds products). |
| `rf-app` main/ui mod | `mod decay15;` and the menu entry. |
| `docs/FIDELITY.md` | new section `## Decay, rot, vermin and wear (round 14, [decay15])` at the end. |
| `docs/SAVES.md` | one line: the `decay` part of the snapshot. |

### 9.3 Tick placement and cost
| Hook | When | Work |
|---|---|---|
| `decay_day` | `now % 1200 == 531` | classify new items (tail scan), rot step over `rot` (about 3 000), refuse orders, refuse-pile wear (refuse tiles only), miasma sources, vermin spawn and despawn, depot tiles |
| `decay_pass` | `now % 100 == 91` | vermin eating (due vermin only), hunters near vermin (hunter slots × live vermin, both small), thoughts for citizens near vermin (only when vermin exist) |
| `decay_combat_wear` | per strike | a few multiplies per armor piece |

No allocation in per-unit loops: keep the scratch `Vec`s in `#[serde(skip)]` fields and `clear()` them. No
HashMap iteration: use `BTreeMap`/sorted `Vec` for anything that is walked. The 2-year bench must stay within 5 %
(the target is faster, since `rot_pass` and the miasma walk over all items go away).

### 9.4 Determinism and hash
- All randomness: `Rng::for_unit(self.seed, slot_or_site, now, 0xDEC0 + k)` with distinct salts per use (spawn,
  species pick, kill, eat target, wear).
- `decay_hash`: mix `rot.len()`, for each entry `(id, days, stage)` (BTreeMap order), `vermin` (id, sp, tile),
  `wear_pts` entries, and the report counters.
- Expected hash changes (state why in the commit message): faster food rot, corpse/part stages, refuse
  reclassification, subterranean miasma with doors, vermin and cats, combat wear.
- `decay off` (`set_decay_enabled(false)`) must reproduce the round-13 bench hash exactly. Test:
  `decay_off_keeps_contract13_hash` on the synthetic bench.

---------------------------------------------------------------------------------------------------------------

## 10. Data needed (rf-core, import, export)

rf-core: **no new fields required**. Use `Item.wear`, `ItemFlags.rotten/dump`, `MaterialInfo.token` and the
mechanical properties, and `tile_flags::SUBTERRANEAN`. Optional (additive, `#[serde(default)]`):
`MaterialInfo.rots: Option<bool>`, `MaterialInfo.miasma: Option<bool>`, filled by the exporter when present;
otherwise use the table in M2.2.

Exporter additions (additive, read-only, documented in `docs/EXPORT_FORMAT.md`; all inside `try()` so missing
fields do nothing):
1. `items.jsonl`: `rot_timer` for items that have it (corpses, body parts, remains, meat, fish, food, cheese,
   plants). DFHack exposes it on the organic item structs. Also `wear_timer` (the points below the next wear
   level). This turns M1.3, M2.3, M2.5 and M7.3 into measurements; then recalibrate on two exports a few days
   apart (as r6 and r7: 1.6 days apart).
2. `materials.json`: `rots`, `generates_miasma` from `material.flags.ROTS/GENERATES_MIASMA`.
3. `settings.json` `other`: refuse standing orders (DFHack globals `standing_orders_gather_refuse`,
   `_gather_refuse_outside`, `_gather_vermin_remains`, `_dump_corpses`, `_dump_skulls`, `_dump_bones`,
   `_dump_shells`, `_dump_skins`, `_dump_hair`, `_dump_other`). Check the names with `rustfort-probe.lua`
   first. Import them into `RefuseOrders`.
4. `vermin.json`: `df.global.world.vermin.all` → race, caste, pos, amount, flags (`is_colony`, `is_roaming`).
   This is the only way to calibrate M5.4 (which species are on the map, and how many).
5. `creature_raws.json` already carries the creature flags. Make sure VERMIN_EATER, VERMIN_ROTTER,
   VERMIN_GROUNDER, VERMIN_SOIL, VERMIN_HATEABLE, HUNTS_VERMIN, DIVE_HUNTS_VERMIN, RETURNS_VERMIN_KILLS_TO_OWNER,
   PENETRATEPOWER (caste), UNDERGROUND_DEPTH and FREQUENCY are among them. If not, add them (additive).

---------------------------------------------------------------------------------------------------------------

## 11. Prioritized implementation list

| P | Item | Section | Behaviour change | Risk |
|---|---|---|---|---|
| P0 | Module skeleton, state, save and hash registration, enable switch, `decay off` keeps the contract13 hash | 9 | no | low |
| P1 | Food rot moved into decay15: tail scan, depot and minecart protection, rotten → refuse, 12-day timer | M1 | yes | low |
| P1 | Corpse, part, hide and remains stages (rotten, skeleton, bones/skull, shell, vanish), material ROTS rule | M2 | yes | medium (undead interplay: API `corpse_stage`) |
| P1 | Miasma sources from decay: GENERATES_MIASMA only, SUBTERRANEAN, closed doors block, up/down, 2-day linger | M3 | yes | medium (recheck the 30 to 60 Miasma thoughts a month) |
| P1 | Refuse classification (bones, skins, parts, non-sapient corpses, rotten food, XX clothing → refuse), standing orders (gather/outdoor/dump) | M4 | yes | medium (hauling load) |
| P2 | Vermin species pool, spawning at food/refuse/rot/caverns, eating with container resistance | M5 | yes | medium |
| P2 | Cats and falcons hunt real vermin, remains to the owner; old roll removed | M6 | yes | low |
| P2 | Wear value multiplier; refuse-pile wear | M7.3, M7.5 | yes | low |
| P2 | Refuse pile filters (bones/skulls/… and hide options) | M4.3.2 | yes | medium (shared haul code) |
| P3 | Combat wear for armor and weapons on a separate RNG stream | M7.4 | yes | medium (perf in `strike`) |
| P3 | Hateable and rotter vermin thoughts; starving dwarves eat vermin; masterwork meal lost to rot or vermin | M5.4, M1.4 | yes | low |
| P3 | UI: refuse orders panel, rot and wear in tooltips, vermin and miasma overlay | 9.1 | no | low |
| P3 | Exporter additions (rot_timer, wear_timer, material flags, standing orders, vermin) and recalibration | 10 | maybe | low |
| P4 | Heat, cold and fire wear, 100-year organic wear, building-destroyer wear, animal traps | M7.6 | yes | depends on others |

---------------------------------------------------------------------------------------------------------------

## 12. FIDELITY.md rows to add (template)

| rule | Rustfort [decay15] | DF (source) | class |
|---|---|---|---|
| What rots | food kinds (meat, fish, meals, cheese, plants); corpses, parts, remains only of ROTS materials | w:Miasma, raws `[ROTS]` | OK |
| Never rot | seeds, drinks, flour, fat, tallow, milk, eggs | w:Miasma ("it seems that they do not"), w:Nest box | OK |
| Protection | stockpile, trade depot, minecart, carried, in a job or building; containers do not protect; raw fish never | w:Miasma | OK (wheelbarrow ESTIMATE) |
| Food rot timer | 12 days exposed, rots away 28 days later | not on the wiki; export: meals rotten at 12 days | ESTIMATE |
| Corpse stages | 14 days to rotten, +56 to skeleton; partial skeletons | w:Corpse; export ages | ESTIMATE (timers) |
| Body parts | rot to bones or a skull; skins rot and vanish; shelled raw fish rot to a shell | w:Bone, w:Shell, w:Tanner | OK (counts ESTIMATE) |
| Remains | rot and vanish for haired species; other species keep | w:Remains; export (only bats rot) | ESTIMATE |
| Miasma sources | rotten GENERATES_MIASMA items on Subterranean tiles | w:Miasma, raws | OK |
| Miasma spread | orthogonal and up/down, not through closed doors or hatches, lingers 2 days, range 2 | w:Miasma | OK (range and linger ESTIMATE) |
| Refuse | non-sapient corpses, parts, bones, skulls, shells, hides, rotten food, withered plants, XX clothing → refuse piles; standing orders | w:Refuse, w:Standing orders | OK |
| Hauling priority | nearest first, no category priority | w:Hauling | OK |
| Vermin | species from raws, spawn near food, refuse, rot and in caverns; eaters eat 1 unit a day; container resistance by material vs PENETRATEPOWER | w:Vermin, w:Creature token | ESTIMATE (rates, resistance) |
| Vermin hunters | cats (HUNTS_VERMIN), tame falcons (DIVE_HUNTS_VERMIN); dogs do not; remains to the owner | raws, w:Cat | OK (rates ESTIMATE) |
| Wear levels | 806 400 points per level, destroyed after 4; value 100/75/50/25 % | w:Wear | OK |
| Refuse-pile wear | armor and clothing 1 level per 56 days, bones and shells per 336 days | w:Wear, w:Bone ("quite quickly", "much slower") | ESTIMATE |
| Combat wear | armor by attacker material vs armor yield, weapons only against harder materials | w:Wear | ESTIMATE (export: metal armor 5 to 10 %, weapons 0 %) |
| Heat, cold, fire wear; 100-year organic wear | not simulated | w:Wear | MISSING |
