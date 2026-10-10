# Contributing to Rustfort

Thanks for helping. Bug reports, fidelity corrections (with a wiki reference), performance work
and pull requests are all welcome. Please read this page first. The rules are short, but some of
them are strict.

## The rules that are not negotiable

1. **Clean-room.** The specification is the [Dwarf Fortress Wiki](https://dwarffortresswiki.org)
   (the v50 pages), the raws of your own installation, and behavior you observe in the running
   game (read-only, for example through DFHack). Do not decompile, disassemble or otherwise
   reverse-engineer the Dwarf Fortress executable, and do not paste code or data taken from it.
   Do not copy DF's text (descriptions, messages, captions) into the code either. The one
   existing exception is the table of DF thought captions in
   `crates/rf-sim/src/social/captions.rs`, taken from DFHack's df-structures (zlib license) and
   credited in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md); do not add new copied DF
   text. Small compiled-in tables of game facts (token names and numbers, such as reactions,
   default materials, armor or ores) are fine; name their source (wiki page or raws) in a
   comment.
2. **No DF files in the repository.** Do not commit raws, graphics, fonts, sounds or any other
   file from a DF installation, not even a single sprite sheet or raw file for a test.
   Rustfort reads them from the user's install at runtime (`rf_raws::df_dir()`).
3. **No exports, saves or screenshots.** Fort exports (`export/`), saves (`saves/`,
   `*.rfsave`), crash logs and screenshots stay out of git. `.gitignore` covers the usual places.
   Keep screenshots under `target/`. Tests that need a real fort read it from `RUSTFORT_EXPORT`
   and skip when there is none. The one exception is the set of curated README screenshots in
   `docs/screenshots/`: they are chosen by the maintainers only, show DF's Premium graphics
   (Bay 12 Games / Kitfox Games) and are excluded from the project's license
   ([THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)). Pull requests do not add or change them;
   attach screenshots to the issue or pull request instead.
4. **Determinism.** The simulation must give the same state hash with one thread and with many:

   ```powershell
   $env:RUSTFORT_EXPORT = "<abs path>\export\<a fort export>"
   $env:RAYON_NUM_THREADS = "1"; target\release\rustfort.exe bench --ticks 806400 --standdown
   Remove-Item Env:RAYON_NUM_THREADS; target\release\rustfort.exe bench --ticks 806400 --standdown
   ```

   Both runs print the same `state hash:` line. Without an export, use `bench --synthetic`, and
   run `cargo test -p rf-sim --release --test determinism_stress`. In practice:
   - no iteration over a `HashMap`/`HashSet` where the order can affect results (use `BTreeMap`,
     sorted vectors or ids);
   - random numbers come from the sim's seeded streams (seed, slot, tick), never from the
     clock or a global generator;
   - parallel phases only read, and their results are applied in a fixed order (by id).
5. **Simulation changes are deliberate.** A change that is not meant to change behavior
   (UI, docs, refactoring, speed) must keep the state hash of the 2-year bench unchanged. If a
   change does alter simulation results, say so in the pull request, explain why (wiki page,
   observation), and update [docs/FIDELITY.md](docs/FIDELITY.md).

## Save and export compatibility

- **New fields in `rf-core`** get `#[serde(default)]` (on the struct or the field), so older
  saves still load. Never rename a field without `#[serde(alias = "old_name")]`. Changes to
  rf-core are additive only.
- **New simulation state** must be serializable (it is part of the exact save snapshot), or
  `#[serde(skip)]` if it is a pure cache that `Sim::after_exact_load` rebuilds. Module structs
  get `#[serde(default)]`. A `&'static str` field is written as `crate::saves11::SStr` with
  `#[serde(with = "crate::saves11::interned")]`. Check with
  `cargo test -p rf-sim --release --test saves11`. Details: [docs/SAVES.md](docs/SAVES.md).
- **`crates/rf-save-v09` is frozen.** It holds the exact v0.9 save layout used to convert old
  saves. Never edit it.
- **The export format** ([docs/EXPORT_FORMAT.md](docs/EXPORT_FORMAT.md)): you may add fields,
  and document every one you add. Do not rename or remove fields. The exporter must stay
  read-only: it never pauses DF, never writes to game memory, and works in frame slices
  ([tools/dfhack/README.md](tools/dfhack/README.md)).

## Building and testing

You need the toolchain from [docs/INSTALL.md](docs/INSTALL.md) (Rust stable; on Windows the MSVC
toolchain and the Visual Studio C++ Build Tools).

```powershell
cargo build --release --workspace --bins --examples   # everything, including rustfort.exe
cargo test --release --workspace                      # all tests
```

- `--examples` alone does **not** rebuild `rustfort.exe`, so keep `--bins` in the command.
- Tests that need a real fort export run only when `RUSTFORT_EXPORT` points to one (or an
  export is found in `export/`). Otherwise they print a skip note and pass. If you have a fort
  export, run the tests with it before a pull request that touches the simulation or the
  importer.
- Useful tools: `rustfort bench` (speed and state hash), the rf-sim examples `chaos` (crash
  hunting), `saveload` (save/load continuation) and `validate` (against a later real export).
  See [README.md](README.md#headless-tools).
- Speed is a feature. Keep hot loops free of allocations, and measure with `rustfort bench`
  before and after.
- The app can be tested without touching the screen. `rustfort play --screenshot <png>
  --frames N` with `--ui` and `--script` renders a scripted run and exits
  ([crates/rf-app/README.md](crates/rf-app/README.md#screenshot-tests)).

## Commits and pull requests

- One topic per commit and per pull request.
- Commit messages are short, imperative and in English ("Add cart collisions", not "Added..."),
  with a body that explains why when that is not obvious.
- Files use LF line endings (`.gitattributes` normalizes them).
- Update the documentation that your change touches: [PROGRESS.md](PROGRESS.md) for
  user-visible changes, [docs/FIDELITY.md](docs/FIDELITY.md) for simulation rules (with the
  wiki page as `w:Page`), and [crates/rf-app/README.md](crates/rf-app/README.md) for screens
  and controls.
- By contributing you agree that your contribution is dual licensed under MIT OR Apache-2.0,
  like the rest of the project ([README.md](README.md#license)).

## Reporting bugs

Please include:

- what you did and what happened (a screenshot helps, but keep it out of the repository);
- the Rustfort version (`git rev-parse --short HEAD`), Windows or Linux, and your DF and DFHack
  versions;
- `saves\crash.log` if Rustfort crashed;
- for simulation bugs, a save (`saves\*.rfsave`) that shows the problem, if you are comfortable
  sharing it. A save contains your fort's data.
