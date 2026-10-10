//! [skills9] Tree growth: saplings grow into trees, saplings die, trees seed new saplings
//! (round 9).
//!
//! # DF rules implemented (wiki v50: Tree, Sapling)
//! - Trees spread by themselves: new saplings sprout on soil or grass floors near grown trees,
//!   on the surface and in the caverns. Saplings mature into trees over the years; some die.
//!   A grown tree is a TREE-material trunk that woodcutters can fell like any other tree.
//!
//! # Estimates (observed in Gordon's fort, r4 -> r5 = 2.18 years: of 1 110 live saplings 87
//! matured, 123 died, 104 new saplings appeared, live trees went 3 140 -> 3 205)
//! - Each month a sapling matures with chance 1 in 330 (3.6 % a year) and dies with chance
//!   1 in 240 (5 % a year); each grown tree seeds a sapling with chance 1 in 750 (1.6 % a year)
//!   on a free soil or grass floor within 2 tiles on its level (no building, stockpile,
//!   designation, plant or unit there). DF's own sapling `grow_counter` does not predict
//!   maturation (saplings with counters of 1 500 and 2 500 000 both matured or stayed), so the
//!   chance is flat.
//! - A matured sapling becomes a one-tile trunk (`TreeTrunkPillar`); DF grows a full crown with
//!   branches and twigs over time.
//! - The plants are processed in a 28-day wheel (1/28 of them each day), so the daily cost is
//!   small even with 26 000 plants.

use crate::rng::Rng;
use crate::Sim;
use rf_core::{tile_flags, Plant, Pos, Shape, TtMaterial};

const WHEEL: usize = 28;
const MATURE_IN: u32 = 330;
const DIE_IN: u32 = 240;
const SEED_IN: u32 = 750;

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Growth {
    pub matured: u64,
    pub died: u64,
    pub sprouted: u64,
    pub off: bool,
}

impl Sim {
    /// [skills9] Daily: 1/28 of the plants take their monthly growth step.
    pub(crate) fn growth_day(&mut self, now: u64) {
        if self.growth.off || self.world.plants.is_empty() {
            return;
        }
        let day = (now / rf_core::TICKS_PER_DAY as u64) as usize % WHEEL;
        let (sapling_tt, trunk_tt, dead_tt) = {
            let tt = &self.world.tiletypes;
            (tt.by_name("Sapling"), tt.by_name("TreeTrunkPillar"), tt.by_name("SaplingDead"))
        };
        let mut sprouts: Vec<(usize, String, u16)> = Vec::new();
        let n = self.world.plants.len();
        let mut k = day;
        while k < n {
            let p = &self.world.plants[k];
            if p.dead || !(p.tree || p.sapling) {
                k += WHEEL;
                continue;
            }
            let Some(i) = self.world.idx(p.pos) else {
                k += WHEEL;
                continue;
            };
            let mut r = Rng::for_unit(self.seed, k, now, 0x7EE5);
            if p.sapling {
                if r.chance(1, MATURE_IN) {
                    if let Some(t) = trunk_tt {
                        if self.nav.shape(self.world.tiles[i].tt) == Shape::Sapling && !self.growth_blocked(i) {
                            self.world.tiles[i].tt = t;
                            self.world.tiles[i].flags |= tile_flags::DIRTY;
                            self.after_tile_change(i);
                            let p = &mut self.world.plants[k];
                            p.sapling = false;
                            p.tree = true;
                            self.growth.matured += 1;
                        }
                    }
                } else if r.chance(1, DIE_IN) {
                    if let Some(t) = dead_tt {
                        if self.nav.shape(self.world.tiles[i].tt) == Shape::Sapling {
                            self.world.tiles[i].tt = t;
                            self.world.tiles[i].flags |= tile_flags::DIRTY;
                            self.after_tile_change(i);
                        }
                    }
                    self.world.plants[k].dead = true;
                    self.growth.died += 1;
                }
            } else if r.chance(1, SEED_IN) {
                let (x, y, z) = self.nav.xyz(i);
                let dx = r.below(5) as isize - 2;
                let dy = r.below(5) as isize - 2;
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if (dx != 0 || dy != 0) && nx >= 0 && ny >= 0 && (nx as usize) < self.nav.dx && (ny as usize) < self.nav.dy {
                    let q = self.nav.idx(nx as usize, ny as usize, z);
                    if self.growth_soil(q) {
                        sprouts.push((q, p.token.clone(), self.world.tiles[i].mat));
                    }
                }
            }
            k += WHEEL;
        }
        let Some(st) = sapling_tt else { return };
        for (q, token, mat) in sprouts {
            let pos: Pos = self.world.pos_of(q);
            if self.world.plants.iter().any(|p| p.pos == pos) || self.growth_blocked(q) {
                continue;
            }
            let t = &mut self.world.tiles[q];
            t.tt = st;
            t.mat = mat;
            t.flags |= tile_flags::DIRTY;
            self.after_tile_change(q);
            self.world.plants.push(Plant { pos, token, tree: false, shrub: false, sapling: true, dead: false, grow_counter: 0 });
            self.growth.sprouted += 1;
        }
    }

    /// A free soil or grass floor where a sapling can sprout.
    fn growth_soil(&self, q: usize) -> bool {
        let t = &self.world.tiles[q];
        let info = self.world.tiletypes.get(t.tt);
        info.shape == Shape::Floor
            && (info.material == TtMaterial::Soil || info.material.is_grass())
            && t.desig == 0
            && t.flags & (tile_flags::OCC_BUILDING | tile_flags::PILE | tile_flags::DESIG_SMOOTH | tile_flags::DESIG_ENGRAVE) == 0
    }

    /// A unit or an item on the tile keeps a sapling from growing or sprouting there.
    fn growth_blocked(&self, q: usize) -> bool {
        self.ai.iter().any(|a| a.pos as usize == q && a.kind != crate::Kind::Dead) || {
            let p = self.world.pos_of(q);
            self.world.items.iter().any(|it| it.flags.on_ground && it.pos == p)
        }
    }

    /// [skills9] Saplings matured, saplings died, saplings sprouted.
    pub fn growth_report(&self) -> (u64, u64, u64) {
        (self.growth.matured, self.growth.died, self.growth.sprouted)
    }

    /// [skills9] Switch tree growth on or off (on by default).
    pub fn set_growth_enabled(&mut self, on: bool) {
        self.growth.off = !on;
    }
}
