//! rf-import: loads a DFHack fort export (docs/EXPORT_FORMAT.md) into an rf-core World.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.

use rf_core::World;
use std::path::{Path, PathBuf};

/// Default export folder of Gordon's fort (gitignored, outside of git).
pub const DEFAULT_EXPORT: &str = "export/tileshtulon";

#[derive(Debug)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ImportError {}

/// What was imported and what was approximated or dropped.
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub units: usize,
    pub citizens: usize,
    pub items: usize,
    pub buildings: usize,
    pub plants: usize,
    pub tiles: usize,
    pub warnings: Vec<String>,
    pub seconds: f64,
}

/// Find the export folder: explicit path, env `RUSTFORT_EXPORT`, or `export/tileshtulon`
/// relative to the current dir or the workspace root.
pub fn find_export(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.join("manifest.json").exists().then(|| p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("RUSTFORT_EXPORT").map(PathBuf::from) {
        if p.join("manifest.json").exists() {
            return Some(p);
        }
    }
    let ws = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(DEFAULT_EXPORT);
    [PathBuf::from(DEFAULT_EXPORT), ws].into_iter().find(|p| p.join("manifest.json").exists())
}

pub fn load_export(dir: &Path) -> Result<(World, ImportReport), ImportError> {
    Err(ImportError(format!("import of {} not implemented yet (stub)", dir.display())))
}
