//! Picks the world to play: a save (`--load`), else a DFHack fort export (`--export DIR`,
//! `RUSTFORT_EXPORT`, or the newest export under `export/`, see `rf_import::find_export`),
//! else the built-in synthetic test fort.

use rf_core::World;
use std::path::Path;

/// Shown (game log, console) when no DF install was found. Short: the log shows one line.
pub const DF_NOT_FOUND_HINT: &str = "Dwarf Fortress not found: set its folder in Settings or RUSTFORT_DF_DIR.";

/// Shown in the game log when no fort export exists yet (first start).
pub const NO_EXPORT_HINT: &str = "No imported fort yet: this is the built-in test fort. Use Import from Dwarf Fortress.";

/// Put [`DF_NOT_FOUND_HINT`] in front of the load warnings when DF is not found (play only;
/// the game log shows the start of the line).
pub fn add_df_hint(l: &mut Loaded) {
    if rf_gfx::df_dir_status().0.is_some() {
        return;
    }
    l.warning = Some(match l.warning.take() {
        Some(w) => format!("{DF_NOT_FOUND_HINT} {w}"),
        None => DF_NOT_FOUND_HINT.to_string(),
    });
}

pub struct Loaded {
    pub world: World,
    /// Human-readable label for the title bar, e.g. `Trussroads (Tileshtulon) - export`.
    pub label: String,
    pub report: Option<rf_import::ImportReport>,
    /// Why we fell back to the synthetic world, if we did.
    pub warning: Option<String>,
    /// [round 6] A fresh start (export or synthetic, not a save): the settings' population cap
    /// and invasion defaults apply.
    pub new_game: bool,
    /// [round 6] Graphics already loaded (and prepared for this world) by the loading thread.
    pub gfx: Option<rf_gfx::Gfx>,
    /// [round 6] Seed from the save header (0 = use `--seed`).
    pub seed: u64,
}

/// World for `play`/`bench`: `--load` save, else the export, else synthetic.
pub fn load(args: &crate::Args) -> Loaded {
    if let Some(p) = &args.load {
        match crate::save::load_world(p) {
            Ok((world, info)) => {
                let label = format!("{} - {} - save {}", world.meta.fort_name, info.header.date, p.display());
                eprintln!("loaded save {} ({}) in {:.2}s", p.display(), crate::save::fmt_bytes(info.bytes), info.secs);
                let warning = info.note.clone(); // [saves10] converted / loaded from the backup
                return Loaded { world, label, report: None, warning, new_game: false, gfx: None, seed: 0 };
            }
            Err(e) => {
                let mut l = load_world(args.export.as_deref(), args.synthetic, args.size);
                l.warning = Some(format!("could not load {}: {e}", p.display()));
                return l;
            }
        }
    }
    load_world(args.export.as_deref(), args.synthetic, args.size)
}

/// [round 6] A save game as a `Loaded` (main menu "Continue" / "Load game").
pub fn load_save_file(p: &Path) -> Result<Loaded, String> {
    let (world, info) = crate::save::load_world(p)?;
    let file = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let label = format!("{} - {} - save {file}", world.meta.fort_name, info.header.date);
    let warning = info.note.clone(); // [saves10] converted / loaded from the backup
    Ok(Loaded { world, label, report: None, warning, new_game: false, gfx: None, seed: info.header.seed })
}

/// [round 6] Load the graphics for `l.world` (CPU work only, so it can run in the loading
/// thread while the window shows progress).
pub fn preload_gfx(l: &mut Loaded) {
    let df = rf_gfx::df_dir();
    let mut gfx = match rf_gfx::Gfx::load_for_world(&df, &l.world) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("graphics: {e}; using fallback tiles");
            rf_gfx::Gfx::default()
        }
    };
    gfx.prepare(&l.world);
    l.gfx = Some(gfx);
}

pub fn load_world(export: Option<&Path>, synthetic: bool, size: (u16, u16, u16)) -> Loaded {
    let synth = |warning: Option<String>| {
        let world = World::synthetic(size.0, size.1, size.2);
        Loaded {
            label: format!("{} (synthetic {}x{}x{})", world.meta.fort_name, size.0, size.1, size.2),
            world,
            report: None,
            warning,
            new_game: true,
            gfx: None,
            seed: 0,
        }
    };
    if synthetic {
        return synth(None);
    }
    let Some(dir) = rf_import::find_export(export) else {
        let w = match export {
            Some(p) => Some(format!("no export found at {} (no manifest.json); started the built-in test fort", p.display())),
            None => Some(NO_EXPORT_HINT.to_string()),
        };
        return synth(w);
    };
    match rf_import::load_export(&dir) {
        Ok((mut world, report)) => {
            if world.meta.source.is_empty() {
                world.meta.source = format!("export:{}", dir.display());
            }
            fill_from_export_files(&mut world, &dir); // [round 3]
            let native = if world.meta.fort_name_native.is_empty() {
                String::new()
            } else {
                format!(" ({})", world.meta.fort_name_native)
            };
            let label = format!(
                "{}{} - {} - {}x{}x{}",
                world.meta.fort_name, native, world.meta.world_name, world.dims.x, world.dims.y, world.dims.z
            );
            Loaded { world, label, report: Some(report), warning: None, new_game: true, gfx: None, seed: 0 }
        }
        Err(e) => synth(Some(format!("import of {} failed: {e}; started the built-in test fort", dir.display()))),
    }
}


/// Read a JSON file of the export into `T`, or None if it is missing or does not parse.
fn read_json<T: serde::de::DeserializeOwned>(dir: &Path, name: &str) -> Option<T> {
    let t = std::fs::read_to_string(dir.join(name)).ok()?;
    serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()
}

/// Fill what the import left empty from the export's side files: squads and burrows (round 2
/// and 3 formats) and the round-3 creature/weapon/armor raws. Until the importer reads them,
/// this keeps the military, burrow and health screens showing the fort's real data.
pub fn fill_from_export_files(world: &mut World, dir: &Path) {
    if world.squads.is_empty() {
        // Round 3 has orders/uniforms/alias; round 2 only names and members. Both parse.
        if let Some(sq) = read_json::<Vec<rf_core::Squad>>(dir, "squads.json") {
            let positions: Vec<i32> = read_json::<Vec<serde_json::Value>>(dir, "squads.json")
                .unwrap_or_default()
                .iter()
                .map(|v| v.get("positions").and_then(|p| p.as_i64()).unwrap_or(10) as i32)
                .collect();
            for (k, mut q) in sq.into_iter().enumerate() {
                let n = positions.get(k).copied().unwrap_or(10).max(0) as usize;
                if q.members.len() < n {
                    q.members.resize(n, -1);
                }
                for (i, m) in q.members.iter().enumerate() {
                    if let Some(u) = world.units.iter_mut().find(|u| u.id == *m && *m >= 0) {
                        u.squad_id = q.id;
                        u.squad_position = i as i32;
                    }
                }
                world.squads.push(q);
            }
        }
    }
    if world.burrows.is_empty() {
        // Round 3: tiles as positions; round 2: only a tile count.
        if let Some(b) = read_json::<Vec<rf_core::Burrow>>(dir, "burrows.json") {
            world.burrows = b;
        } else {
            let ex = rf_import::load_extras(dir);
            world.burrows = ex.burrows.iter().map(|b| rf_core::Burrow { id: b.id, name: b.name.clone(), tiles: Vec::new(), units: b.units.clone() }).collect();
        }
    }
    if world.creature_raws.is_empty() {
        world.creature_raws = read_json(dir, "creature_raws.json").unwrap_or_default();
    }
    if world.weapon_raws.is_empty() {
        world.weapon_raws = read_json(dir, "weapon_raws.json").unwrap_or_default();
    }
    if world.armor_raws.is_empty() {
        world.armor_raws = read_json(dir, "armor_raws.json").unwrap_or_default();
    }
    // [round 4] Civilizations, nobles, game settings, historical figure names.
    if world.entities.is_empty() {
        world.entities = read_json(dir, "entities.json").unwrap_or_default();
    }
    if world.nobles.is_empty() {
        world.nobles = read_json(dir, "nobles.json").unwrap_or_default();
    }
    if world.hist_figs.is_empty() {
        world.hist_figs = read_json(dir, "hist_figs.json").unwrap_or_default();
    }
    if world.settings.population_cap == 0 && world.settings.other.is_empty() {
        if let Some(st) = read_json::<rf_core::GameSettings>(dir, "settings.json") {
            world.settings = st;
        }
    }
    // [app8] Round-6 exports: work details, kitchen, locations, justice, hauling routes.
    if world.work_details.is_empty() {
        world.work_details = read_json(dir, "work_details.json").unwrap_or_default();
    }
    if world.kitchen.is_empty() {
        world.kitchen = read_json(dir, "kitchen.json").unwrap_or_default();
    }
    if world.locations.is_empty() {
        world.locations = read_json(dir, "locations.json").unwrap_or_default();
    }
    if world.crimes.is_empty() {
        world.crimes = read_json(dir, "crimes.json").unwrap_or_default();
    }
    if world.haul_routes.is_empty() {
        world.haul_routes = read_json(dir, "haul_routes.json").unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squads_and_burrows_from_export_files() {
        let dir = std::env::temp_dir().join(format!("rf-app-extras-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = World::synthetic(16, 16, 10);
        let uid = w.units[0].id;
        // Round-2 format.
        std::fs::write(dir.join("squads.json"), format!(r#"[{{"id":72,"name":"Wache","members":[-1,{uid}],"member_hfs":[1,2],"positions":10}}]"#)).unwrap();
        std::fs::write(dir.join("burrows.json"), r#"[{"id":1,"name":"Zuflucht","tiles":1716,"blocks":18,"units":[]}]"#).unwrap();
        fill_from_export_files(&mut w, &dir);
        assert_eq!(w.squads.len(), 1);
        assert_eq!(w.squads[0].members.len(), 10);
        assert_eq!(w.squads[0].members[1], uid);
        assert_eq!((w.units[0].squad_id, w.units[0].squad_position), (72, 1));
        assert_eq!(w.burrows[0].name, "Zuflucht");
        assert!(w.burrows[0].tiles.is_empty());
        // A world that has squads keeps them; round-3 burrows come with tiles.
        w.squads[0].name = "Mine".into();
        w.burrows.clear();
        std::fs::write(dir.join("burrows.json"), r#"[{"id":0,"name":"Einzel","tiles":[{"x":1,"y":2,"z":3}],"units":[]}]"#).unwrap();
        fill_from_export_files(&mut w, &dir);
        assert_eq!(w.squads[0].name, "Mine");
        assert_eq!(w.burrows[0].tiles, vec![rf_core::Pos::new(1, 2, 3)]);
        // [round 4] nobles and settings (extra exporter fields are ignored).
        std::fs::write(dir.join("nobles.json"), r#"[{"token":"BROKER","name":"broker","holder":-1,"entity_id":303,"mandates":[],"holder_name":"","flags":["ACTIVE"]}]"#).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"population_cap":90,"invasions":true,"other":["VISITOR_CAP=300"]}"#).unwrap();
        fill_from_export_files(&mut w, &dir);
        assert_eq!(w.nobles.len(), 1);
        assert_eq!((w.nobles[0].token.as_str(), w.nobles[0].holder), ("BROKER", -1));
        assert_eq!(w.settings.population_cap, 90);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
