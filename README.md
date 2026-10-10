# Rustfort

A fast re-implementation of the Dwarf Fortress fortress mode in Rust. It is meant for personal use and continues a real DF save.

- Vision and plan: [VISION.md](VISION.md)
- Architecture and rules: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Save export format: [docs/EXPORT_FORMAT.md](docs/EXPORT_FORMAT.md)
- The game window, controls and screens (doors and levers, trade, nobles, settings): [crates/rf-app/README.md](crates/rf-app/README.md)

The repository contains no Dwarf Fortress data. Rustfort reads raws and the Premium tileset at runtime from your own DF installation, which you set with the `RUSTFORT_DF_DIR` environment variable. Fort exports are written to `export/`, which is gitignored.

```
cargo run --release -p rf-app -- play            # play (synthetic fort or export/<fort>)
cargo run --release -p rf-app -- bench           # headless ticks/s benchmark
```
