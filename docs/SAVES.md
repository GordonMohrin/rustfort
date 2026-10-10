# Save games (round 10, `[saves10]`)

## File format 2 (Rustfort 0.10)

```text
b"RFSAVE\x02\n"            magic + format version
u32 LE                     header length
msgpack(SaveHeader)        named fields: fort, date, seed, ..., format, Rustfort version,
                           body length + CRC-32, raw length, sim tick
zstd(msgpack(World))       MessagePack with named fields (struct maps), zstd level 3 + content checksum
```

- **Tolerant**: every rf-core struct has `#[serde(default)]`, so a field added later is filled
  with its default when an older save is loaded, and a field an older Rustfort does not know is
  skipped. Enum variants are stored by name. **Rule for new rf-core fields: give the struct (or
  the field) `#[serde(default)]`.** Never rename a field without `#[serde(alias = "old")]`.
- **Map tiles** are stored as byte columns (`rf_core::world::tile_columns`: `tt`, `mat`, `flags`,
  `liquid`, `desig`); a new `Tile` field becomes a new column, missing columns read as 0.
- **Integrity**: CRC-32 and length of the body in the header (checked before decompressing), the
  zstd content checksum, a read-back check of every written file before it replaces anything,
  `<name>.rfsave.bak` when a file is overwritten, and loading falls back to the `.bak` of a
  damaged file (with a note in the log). Truncated saves show as damaged in the load menu and
  "Continue" skips them.
- **Format 1** (bincode, Rustfort 0.9 and older) still loads: the frozen v0.9 types in the
  `rf-save-v09` crate decode it, serde converts it field by field into the current `World`
  (log note "converted from a Rustfort 0.9 save"); the next save writes format 2. Never change
  `rf-save-v09`.

Real fort (Trussroads, r5, 192x192x150, 352 units, 18k items) after 20,000 ticks:

| | bincode (reference) | format 2 |
|---|---|---|
| raw body | 51.1 MB | 56.4 MB |
| encode | 0.12 s | 0.10 s |
| file | | 1.9 MB |
| save total (encode, compress, fsync, read-back verify) | | 0.36 s (0.82 s on a loaded machine) |
| load (read, CRC, decompress, decode, reindex) | | 0.43 s |
| `Sim::new` after load | | 0.69 s |
| v0.9 save of the same world (1.8 MB): load + convert | | 0.93 s |

(Measured with `RUSTFORT_SAVEBENCH_TICKS=20000 rustfort savebench`, while other builds ran.)

`savebench` also checks that re-encoding a loaded world gives the same bytes ("lossless: true")
and that a v0.9 (format 1) file of the real fort converts to the identical world
("identical to the original: true").

## Simulation state (`World::sim_state`)

`Sim::world_for_save()` clones the world and fills `sim_state` (module name -> JSON);
`Sim::new` takes it out again, restores the tick count before the init passes and the modules
after them. A module that cannot read its entry starts fresh, as before.

| entry | what | after a reload |
|---|---|---|
| `core` | tick count, next item id, needs of non-citizens, dwarves asleep in bed | exact |
| `locations` | visitors (slot, role, stay, petition timer, thief, loot), arrivals, petitions, petition members, reading timers, location usage, report counters | exact |
| `moods` | mood counter, who had a mood, records, the active mood (workshop, demands, collected items with their reservation, step, insanity wait, work end, carried item) | exact |
| `industry` | melted metal fractions per smelter, items made, items melted | exact |
| `social` | breakdowns (tantrum / depression / oblivious) and their end, breakdown count, meal/drink variety, admired buildings, thought timers, eustress/distress counters, monthly stress, report counters | exact |
| `fluids` | mud tiles | exact |
| `combat` | raid groups (kind, members, camp, charge time, leader, revealed, retreat), hidden raiders, scheduled thieves/snatchers, per unit: role, hidden, raid, loot, carried/carrying unit, leave timer, building destroyer target, patrol waypoint, training spot, unconsciousness, sim wounds, blood, broken parts, squad duty | exact |
| constructions, designations | planned constructions are buildings, dig/smooth/engrave are tile flags | rebuilt from the world (worker and material are chosen again) |

"exact" = restoring and saving again gives the identical JSON (`examples/saveload.rs`).

Since round 11 these entries are the **fallback**: a save also holds the exact snapshot below,
and `sim_state` is used only when the snapshot cannot be (a save from a build with other sim
state, an edited world).

## Exact snapshot (`World::sim_exact`, round 11, `[saves11]`)

`Sim` and every module struct derive `serde::Serialize`/`Deserialize`. `Sim::world_for_save`
puts a MessagePack snapshot (named fields) of the whole sim except the world into
`World::sim_exact` (one `bin` value of the save body, compressed with it). `Sim::new` restores
it instead of running the init passes: unit AI (tasks, paths, carried hauls, think-wheel
timers), the job lists and their spatial index, workshop/production jobs with claims and
progress, construction jobs, piles and reservations, the haul queue, the path cache, caravans,
animals (training, pregnancy, eggs), the health care queue, caverns and forgotten beasts,
population timers, justice and lifestyle counters, report counters, the navigation grid with its
walk components and landmark tables. RNG streams are derived from (seed, slot, tick), so there
is no generator state to save. Result: **run N, save, load, run M == run N+M (full
`state_hash`)**.

- **Encoding:** big per-tile tables of the navigation grid are little-endian byte columns; the
  landmark distances (`Nav::lm`, 16 per walkable tile) are stored column-wise as residuals of a
  left/above prediction (`saves11::lm_encode`, LOCO-I median predictor), so they compress to a
  fraction; the landmark index maps are rebuilt. `&'static str` fields are interned on load.
- **When the snapshot is used:** `version` matches, the world matches (`WorldCheck`: dims,
  unit/item/building counts, date, a hash of unit and item ids and item positions), table sizes
  match, and the **layout** matches: the set of (path, field name) pairs of the snapshot equals
  that of the restored sim encoded again, i.e. no field was filled with a default and none was
  ignored. A save from a Rustfort build that added or dropped sim state therefore never skips
  the init passes with a table missing; it falls back to `sim_state` (close continuation).
  `Sim::restore_mode()` says which way a sim was restored (`exact`, `close (reason)`, or empty).
- **Crashes:** a sim whose tick panicked (state possibly half-updated) writes no snapshot, so
  such a save restarts the AI like round 10 instead of replaying the crash (the automatic
  rescue save stores the bare world anyway). `RUSTFORT_CLOSE_LOAD=1` ignores snapshots on load
  (escape hatch if a save ever re-crashes on the same tick).
- **Rule for new sim state** (also in `ARCHITECTURE.md`): derive serde for it, or
  `#[serde(skip)]` for a pure cache that `Sim::after_exact_load` rebuilds; module structs get
  `#[serde(default)]`; `&'static str` fields are `crate::saves11::SStr` with
  `#[serde(with = "crate::saves11::interned")]`.
- **Tests:** `cargo test -p rf-sim --release --test saves11` (synthetic and bench worlds, four
  reloads in a row, the real fort N 100 / M 100 and N 2000 / M 1000, and the fallbacks); the
  chaos harness alternates exact reloads (flagged if the state hash changes) and bare-world
  reloads; `examples/saveload.rs` for long runs (`--sizes` prints the snapshot by part).

### Continuation (gold standard)

Round 11, r5 seed 1, through the save encoding (MessagePack round trip as in format 2):

| N | M | full state hash after N+M |
|---|---|---|
| 100 | 100 | identical |
| 20,000 | 20,000 | identical |
| 403,200 | 403,200 | identical |

Also identical: r6 seed 2 N 50,000 / M 50,000, r4 seed 3 N 100,000 / M 60,000, and through a
real save file (`rustfort savebench`, +2,000 ticks after the load).

Cost on the real fort after 20,000 ticks (`savebench`, measured while other builds and a chaos
run loaded the machine, so absolute times are pessimistic):

| | round 10 | round 11 |
|---|---|---|
| snapshot (raw, in the body) | | 45 MB (nav 35 MB, item index 8 MB) |
| snapshot compressed (zstd-3 alone) | | 1.5 MB (landmark tables 0.84 MB) |
| file | 1.9 MB | 3.4 MB |
| `world_for_save` + save total | 0.36 s | 0.30 s + 0.51 s |
| load + `Sim::new` | 0.43 s + 0.69 s | 0.30 s + 0.35 s (no init passes, no landmark build; the layout check is ~0.2 s of it) |

Round 10 (before the snapshot), for reference:
`cargo run -p rf-sim --release --example saveload -- --n 20000 --m 20000 --every 100` on r5:
right after the load the `ticks`, `units` (positions and needs of every unit), `tiles`, `items`,
`social` and `moods` parts of the state hash are identical; `prod`, `combat`, `fluids`,
`society`, `health`, `caverns`, `animals` and `society2` differ because their hashes include
per-run counters and job state that are not saved. The full state hash differs from the first
100 ticks on: **unit AI tasks, paths, job claims and workshop jobs in progress restart from Idle**
after a load (a dwarf walking to a job picks one again). After 20,000 more ticks both runs have
the same citizens alive (82), dead units (16), petitions and moods; item counts differ by under
0.5 %. One year, reload, one more year (`--n 403200 --m 403200`): 87 vs 90 citizens alive, 135 vs
121 dead units, 5 / 5 moods, 21 / 21 petitions, items within 0.5 %, i.e. inside the spread of
different seeds. So a reload continues the fort closely, not bit-identically.

Round 11 saves all of that (see the exact snapshot above).
