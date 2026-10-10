//! [saves10] Save/load of the industry module: melted metal fractions per smelter (tenths of a
//! bar, wiki "Melt item") and the production counters.

use crate::persist::{get, put};
use crate::Sim;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "industry";

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct IndS {
    /// ((smelter id, metal palette index), tenths of a bar).
    melt_tenths: Vec<((i32, u16), i32)>,
    melted: u64,
    made: Vec<(String, i64)>,
}

impl Sim {
    pub(crate) fn ind_save(&self, m: &mut BTreeMap<String, String>) {
        let s = IndS { melt_tenths: self.ind.melt_tenths.iter().map(|(k, v)| (*k, *v)).collect(), melted: self.ind.melted, made: self.ind.made.clone() };
        put(m, KEY, &s);
    }

    pub(crate) fn ind_restore(&mut self, m: &BTreeMap<String, String>) {
        let Some(s) = get::<IndS>(m, KEY) else { return };
        self.ind.melt_tenths = s.melt_tenths.into_iter().collect();
        self.ind.melted = s.melted;
        self.ind.made = s.made;
    }
}
