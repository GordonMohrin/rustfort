//! [audio14] Read-only facts and an off-by-default cue buffer for the app's audio
//! (docs/mechanics/audio14.md section 7).
//!
//! Audio is presentation only: nothing here is saved, hashed or uses the sim's RNG. The cue
//! buffer is disabled unless the interactive app turns it on (`set_audio_cues(true)`), so the
//! bench and the tests run the same code path apart from one `bool` test per dig, performance
//! and blow.

use crate::Sim;
use rf_core::{Pos, TtMaterial};

/// Cheap facts for the app's music and ambience logic (one call per second, no allocation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioFacts {
    pub ticks: u64,
    /// Bit `id` set = cavern layer `id` (1..=31, not the magma sea) has revealed tiles.
    pub caverns_mask: u32,
    pub magma_sea_discovered: bool,
    /// Living citizens (adults and children).
    pub citizens_alive: u32,
    /// Ghosts that rose so far (saved state, so a load does not count as new ones).
    pub ghosts_risen: u32,
    /// Land holder rank: 0 none, 1 barony, 2 county, 3 duchy ([diplo13]).
    pub land_rank: u8,
}

/// Sound class of a dug tile (from the tiletype material).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigSound {
    Stone,
    Ore,
    Gem,
    Soil,
    Ice,
    Wood,
    Other,
}

impl DigSound {
    pub fn of(m: TtMaterial) -> DigSound {
        match m {
            TtMaterial::Stone | TtMaterial::LavaStone | TtMaterial::Feature | TtMaterial::Hfs => DigSound::Stone,
            TtMaterial::Mineral => DigSound::Ore,
            TtMaterial::Soil => DigSound::Soil,
            TtMaterial::FrozenLiquid => DigSound::Ice,
            TtMaterial::Root | TtMaterial::Driftwood | TtMaterial::Plant => DigSound::Wood,
            m if m.is_grass() => DigSound::Soil,
            _ => DigSound::Other,
        }
    }
}

/// What a landed blow sounds like (for the optional extra combat sounds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeSound {
    /// An armed blow (weapon material not tracked here: metal, ESTIMATE).
    Metal,
    /// Unarmed: punches, kicks, bites.
    Flesh,
}

/// One audible event from the sim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCue {
    /// A tile was dug: position, material index of the dug tile and its sound class.
    Dig { pos: Pos, mat: u16, class: DigSound },
    /// A performance in a location: location id, tavern (true) or temple, music (true) or
    /// poetry/story, idle citizens watching.
    Perform { loc: i32, tavern: bool, music: bool, audience: u16 },
    /// A blow landed on a unit at `pos`.
    Strike { pos: Pos, kind: StrikeSound },
}

/// Most cues kept between two drains (overflow drops the newest; fine for audio).
pub const CUE_CAP: usize = 1024;

/// The cue buffer (`#[serde(skip)]` in `Sim`: never saved, never hashed).
#[derive(Clone, Debug, Default)]
pub struct CueBuf {
    pub on: bool,
    buf: Vec<AudioCue>,
}

impl CueBuf {
    #[inline]
    pub fn push(&mut self, c: AudioCue) {
        if !self.on || self.buf.len() >= CUE_CAP {
            return;
        }
        self.buf.push(c);
    }
}

impl Sim {
    /// [audio14] Facts for music contexts and transition events (cheap; the citizen count walks
    /// the units, so call it about once a second).
    pub fn audio_facts(&self) -> AudioFacts {
        let mut mask = 0u32;
        let mut magma = false;
        for l in &self.caverns.layers {
            if !l.discovered() {
                continue;
            }
            if l.magma {
                magma = true;
            } else if (1..=31).contains(&l.id) {
                mask |= 1 << l.id;
            }
        }
        // `pop_counts` returns (all citizens, of which children): the first already counts kids.
        let (cit, _kids) = self.pop_counts();
        let rank = match self.land_title() {
            "Barony" => 1,
            "County" => 2,
            "Duchy" => 3,
            _ => 0,
        };
        AudioFacts {
            ticks: self.ticks(),
            caverns_mask: mask,
            magma_sea_discovered: magma,
            citizens_alive: cit as u32,
            ghosts_risen: self.pop.rep.ghosts_risen.len() as u32,
            land_rank: rank,
        }
    }

    /// [audio14] Turn cue recording on or off (default off; the buffer is not saved). Turning
    /// it on reserves the whole buffer once, so pushes never allocate.
    pub fn set_audio_cues(&mut self, on: bool) {
        self.audio_cues.on = on;
        if on {
            self.audio_cues.buf.reserve(CUE_CAP);
        } else {
            self.audio_cues.buf.clear();
        }
    }

    /// [audio14] Is cue recording on?
    pub fn audio_cues_on(&self) -> bool {
        self.audio_cues.on
    }

    /// [audio14] Move the pending cues into `out` (keeps the buffer's capacity).
    pub fn drain_audio_cues(&mut self, out: &mut Vec<AudioCue>) {
        out.extend(self.audio_cues.buf.drain(..));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_buffer_is_off_by_default_and_bounded() {
        let mut b = CueBuf::default();
        let c = AudioCue::Strike { pos: Pos::new(1, 2, 3), kind: StrikeSound::Flesh };
        b.push(c);
        assert!(b.buf.is_empty());
        b.on = true;
        for _ in 0..CUE_CAP + 10 {
            b.push(c);
        }
        assert_eq!(b.buf.len(), CUE_CAP);
    }

    #[test]
    fn dig_sound_classes() {
        assert_eq!(DigSound::of(TtMaterial::Stone), DigSound::Stone);
        assert_eq!(DigSound::of(TtMaterial::Mineral), DigSound::Ore);
        assert_eq!(DigSound::of(TtMaterial::GrassLight), DigSound::Soil);
        assert_eq!(DigSound::of(TtMaterial::FrozenLiquid), DigSound::Ice);
        assert_eq!(DigSound::of(TtMaterial::Air), DigSound::Other);
    }

    /// Cues on vs off: same state hash (audio never feeds back into the sim), and a dig
    /// arrives as a cue only when recording is on.
    #[test]
    fn cues_do_not_change_the_sim() {
        let mk = || Sim::new(rf_core::World::synthetic(64, 64, 24), 7);
        let mut a = mk();
        let mut b = mk();
        b.set_audio_cues(true);
        a.run_ticks(2_000);
        b.run_ticks(2_000);
        assert_eq!(a.state_hash(), b.state_hash());
        let mut out = Vec::new();
        a.drain_audio_cues(&mut out);
        assert!(out.is_empty());
        b.drain_audio_cues(&mut out);
        // Whatever was dug in b produced Dig cues; facts are readable either way.
        assert!(out.len() <= CUE_CAP);
        let f = a.audio_facts();
        assert_eq!(f.ticks, a.ticks());
        assert!(f.citizens_alive > 0);
        // Children are citizens too, but must not be counted twice.
        assert_eq!(f.citizens_alive as usize, a.world.citizens().count());
    }

    /// Children are citizens: `pop_counts` already includes them, so they must not be added twice.
    #[test]
    fn children_are_counted_once() {
        let mut w = rf_core::World::synthetic(48, 48, 20);
        let k = w.units.iter().position(|u| u.is_worker()).unwrap();
        w.units[k].profession = "CHILD".into();
        let sim = Sim::new(w, 3);
        let n = sim.world.citizens().count();
        assert_eq!(sim.audio_facts().citizens_alive as usize, n);
    }

    #[test]
    fn dig_arrives_as_cue_only_when_on() {
        let mut sim = Sim::new(rf_core::World::synthetic(48, 48, 20), 3);
        let solid = |sim: &Sim| {
            (0..sim.world.tiles.len())
                .rev()
                .find(|&i| {
                    let info = sim.world.tiletypes.get(sim.world.tiles[i].tt);
                    info.shape.is_solid() && info.material.is_diggable()
                })
                .unwrap()
        };
        let mut out = Vec::new();
        let i = solid(&sim);
        sim.dig_tile_for_test(i, rf_core::Dig::Dig);
        sim.drain_audio_cues(&mut out);
        assert!(out.is_empty());
        sim.set_audio_cues(true);
        let i = solid(&sim);
        sim.dig_tile_for_test(i, rf_core::Dig::Dig);
        sim.drain_audio_cues(&mut out);
        assert!(matches!(out.as_slice(), [AudioCue::Dig { .. }]), "{out:?}");
    }
}
