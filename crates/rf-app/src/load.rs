//! Picks the world to play: the DFHack export of Gordon's fort if there is one, else a
//! synthetic fort.

use rf_core::World;
use std::path::Path;

pub struct Loaded {
    pub world: World,
    /// Human-readable label for the title bar, e.g. `Trussroads (Tileshtulon) - export`.
    pub label: String,
    pub report: Option<rf_import::ImportReport>,
    /// Why we fell back to the synthetic world, if we did.
    pub warning: Option<String>,
}

/// World for `play`/`bench`: `--load` save, else the export, else synthetic.
pub fn load(args: &crate::Args) -> Loaded {
    if let Some(p) = &args.load {
        match crate::save::load_world(p) {
            Ok((world, info)) => {
                let label = format!("{} - {} - save {}", world.meta.fort_name, info.header.date, p.display());
                eprintln!("loaded save {} ({}) in {:.2}s", p.display(), crate::save::fmt_bytes(info.bytes), info.secs);
                return Loaded { world, label, report: None, warning: None };
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

pub fn load_world(export: Option<&Path>, synthetic: bool, size: (u16, u16, u16)) -> Loaded {
    let synth = |warning: Option<String>| {
        let world = World::synthetic(size.0, size.1, size.2);
        Loaded { label: format!("{} (synthetic {}x{}x{})", world.meta.fort_name, size.0, size.1, size.2), world, report: None, warning }
    };
    if synthetic {
        return synth(None);
    }
    let Some(dir) = rf_import::find_export(export) else {
        let w = match export {
            Some(p) => Some(format!("no export found at {}", p.display())),
            None => Some("no fort export found (export/tileshtulon); using a synthetic fort".to_string()),
        };
        return synth(w);
    };
    match rf_import::load_export(&dir) {
        Ok((mut world, report)) => {
            if world.meta.source.is_empty() {
                world.meta.source = format!("export:{}", dir.display());
            }
            let native = if world.meta.fort_name_native.is_empty() {
                String::new()
            } else {
                format!(" ({})", world.meta.fort_name_native)
            };
            let label = format!(
                "{}{} - {} - {}x{}x{}",
                world.meta.fort_name, native, world.meta.world_name, world.dims.x, world.dims.y, world.dims.z
            );
            Loaded { world, label, report: Some(report), warning: None }
        }
        Err(e) => synth(Some(format!("import of {} failed: {e}; using a synthetic fort", dir.display()))),
    }
}
