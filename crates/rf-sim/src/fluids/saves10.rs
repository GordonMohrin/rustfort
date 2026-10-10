//! [saves10] Save/load of the fluids module: mud left by receding water (DF keeps it as a
//! tile spatter; here a bit per tile). Liquids themselves are tile data and were saved before;
//! flow queues and aquifer lists are rebuilt.

use crate::persist::{get, put};
use crate::Sim;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "fluids";

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct FluidsS {
    /// Tile indices with mud, ascending.
    mud: Vec<u32>,
}

impl Sim {
    pub(crate) fn fluids_save(&self, m: &mut BTreeMap<String, String>) {
        let f = &self.fluids;
        if !f.ready || f.mud_count == 0 {
            return;
        }
        let mut mud = Vec::with_capacity(f.mud_count);
        for (w, &bits) in f.mud.0.iter().enumerate() {
            let mut b = bits;
            while b != 0 {
                let i = b.trailing_zeros() as usize;
                mud.push((w * 64 + i) as u32);
                b &= b - 1;
            }
        }
        put(m, KEY, &FluidsS { mud });
    }

    pub(crate) fn fluids_restore(&mut self, m: &BTreeMap<String, String>) {
        let Some(s) = get::<FluidsS>(m, KEY) else { return };
        if s.mud.is_empty() || !self.fluids.on {
            return;
        }
        if !self.fluids.ready {
            self.fluids_init();
        }
        if !self.fluids.ready {
            return;
        }
        let n = self.world.tiles.len();
        for t in s.mud {
            if (t as usize) < n && self.fluids.mud.set(t as usize) {
                self.fluids.mud_count += 1;
            }
        }
    }
}
