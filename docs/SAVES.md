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

### Continuation (gold standard)

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

Not saved yet (next steps for exact continuation): the job lists (`Sim::jobs`, production
workshop jobs), unit tasks/paths/carried hauls, caravans in progress (`trade`), animal module
state (training, pregnancy timers kept in the sim), health care queue, cavern/beast timers,
population timers (migrant waves), lifestyle/justice counters.
