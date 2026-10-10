//! [saves11] Exact save/load: the sim's whole runtime state in the save.
//!
//! Round 10 saved the modules' state as small JSON entries (`World::sim_state`, see
//! `persist`), but unit tasks, paths, job lists, workshop jobs, caravans, animal timers, the
//! navigation tables ... restarted after a load, so a reloaded fort continued closely, not
//! identically. Now `Sim` itself is `Serialize`/`Deserialize` (every module struct derives
//! serde; the world is skipped because it is saved on its own) and [`Sim::world_for_save`] puts
//! a MessagePack snapshot of it into `World::sim_exact`. `Sim::new` restores it instead of
//! running the init passes, so **run N, save, load, run M == run N+M** (full state hash).
//!
//! Forward compatibility: the snapshot uses named fields, every module struct that has a
//! `Default` reads missing fields as their default (`#[serde(default)]`), unknown fields are
//! skipped. A snapshot that does not decode or does not belong to the world (see
//! [`WorldCheck`]) is ignored and the round-10 `sim_state` path is used (close continuation).
//!
//! **Rule for new sim state:** a new `Sim` field or module field must be serde-serializable
//! (derive it) or `#[serde(skip)]` if it is a pure cache that is rebuilt after a load (then
//! rebuild it in [`Sim::after_exact_load`]). `&'static str` fields: see [`SStr`].
//!
//! Big per-tile tables of the navigation grid are stored as little-endian byte columns (the
//! save's zstd compresses them well); the landmark table (`Nav::lm`) column-wise and
//! delta-coded, its tile -> index map and queue flags are rebuilt.

use crate::Sim;
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Mutex;

/// Version of the snapshot layout (bump on incompatible changes: older snapshots are then
/// ignored and the round-10 path is used).
const EXACT_VERSION: u32 = 1;

/// The same `&'static str` for equal strings (snapshot strings of `&'static str` fields).
pub(crate) fn intern(s: &str) -> &'static str {
    static POOL: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut p = POOL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&x) = p.get(s) {
        return x;
    }
    let x: &'static str = Box::leak(s.to_string().into_boxed_str());
    p.insert(x);
    x
}

/// `&'static str` under another name: serde's derive treats a field written as `&'static str`
/// as borrowed from the input (so the struct could only be read from `'static` data); a field
/// of this alias type with `#[serde(with = "crate::saves11::interned")]` is read as an owned
/// string and interned. Same type, so no other code changes.
pub(crate) type SStr = &'static str;

/// Types holding `&'static str` (names from the sim's own tables): read back through an
/// owned mirror and interned.
pub(crate) trait Interned: Sized {
    type Owned: serde::de::DeserializeOwned;
    fn from_owned(o: Self::Owned) -> Self;
}
impl Interned for &'static str {
    type Owned = String;
    fn from_owned(o: String) -> Self {
        intern(&o)
    }
}
impl Interned for (u8, &'static str) {
    type Owned = (u8, String);
    fn from_owned(o: (u8, String)) -> Self {
        (o.0, intern(&o.1))
    }
}
impl Interned for (&'static str, u64) {
    type Owned = (String, u64);
    fn from_owned(o: (String, u64)) -> Self {
        (intern(&o.0), o.1)
    }
}
impl Interned for (&'static str, &'static str, &'static str) {
    type Owned = (String, String, String);
    fn from_owned(o: (String, String, String)) -> Self {
        (intern(&o.0), intern(&o.1), intern(&o.2))
    }
}
impl<T: Interned> Interned for Vec<T> {
    type Owned = Vec<T::Owned>;
    fn from_owned(o: Self::Owned) -> Self {
        o.into_iter().map(T::from_owned).collect()
    }
}
impl<V: serde::de::DeserializeOwned> Interned for BTreeMap<&'static str, V> {
    type Owned = BTreeMap<String, V>;
    fn from_owned(o: Self::Owned) -> Self {
        o.into_iter().map(|(k, v)| (intern(&k), v)).collect()
    }
}

/// Field of an [`Interned`] type: `#[serde(with = "crate::saves11::interned")]`.
pub(crate) mod interned {
    use super::*;
    pub fn serialize<S: Serializer, T: Serialize>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        v.serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: Interned>(d: D) -> Result<T, D::Error> {
        T::Owned::deserialize(d).map(T::from_owned)
    }
}

/// Raw bytes (MessagePack `bin`), also accepted as a sequence of integers.
fn bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bytes")
        }
        fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
            Ok(v.to_vec())
        }
        fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
            Ok(v)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<u8>, A::Error> {
            let mut v = Vec::with_capacity(a.size_hint().unwrap_or(0).min(1 << 24));
            while let Some(b) = a.next_element::<u8>()? {
                v.push(b);
            }
            Ok(v)
        }
    }
    d.deserialize_byte_buf(V)
}

/// `Vec<u8>` as one `bin` value.
pub(crate) mod le_u8 {
    use super::*;
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        bytes(d)
    }
}

macro_rules! le_col {
    ($name:ident, $t:ty, $n:expr) => {
        /// Vector of integers as one little-endian `bin` value.
        pub(crate) mod $name {
            use super::*;
            pub fn serialize<S: Serializer>(v: &[$t], s: S) -> Result<S::Ok, S::Error> {
                let mut b = Vec::with_capacity(v.len() * $n);
                for x in v {
                    b.extend_from_slice(&x.to_le_bytes());
                }
                s.serialize_bytes(&b)
            }
            pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<$t>, D::Error> {
                let b = bytes(d)?;
                if b.len() % $n != 0 {
                    return Err(de::Error::custom("column length"));
                }
                Ok(b.chunks_exact($n).map(|c| <$t>::from_le_bytes(c.try_into().unwrap())).collect())
            }
        }
    };
}
le_col!(le_u32, u32, 4);
le_col!(le_u64, u64, 8);

/// A table that is rebuilt after loading: only its length is stored, it reads back as
/// `len` default values (`Nav::lm.id`, `Nav::lm.inq`; refilled in `after_exact_load`).
pub(crate) mod len_only {
    use super::*;
    pub fn serialize<S: Serializer, T>(v: &[T], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(v.len() as u64)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: Default + Clone>(d: D) -> Result<Vec<T>, D::Error> {
        let n = u64::deserialize(d)? as usize;
        if n > 1 << 31 {
            return Err(de::Error::custom("table length"));
        }
        Ok(vec![T::default(); n])
    }
}

/// `Nav::lm.d` (`[u16; LM_K]` per walkable tile): column by column, each value as the
/// wrapping difference to the previous tile's (neighbouring tiles have close distances, so
/// the save's compressor shrinks this a lot), little-endian.
pub(crate) mod lm_cols {
    use super::*;
    use crate::nav::LM_K;
    pub fn serialize<S: Serializer>(v: &[[u16; LM_K]], s: S) -> Result<S::Ok, S::Error> {
        let mut b = Vec::with_capacity(v.len() * LM_K * 2);
        for j in 0..LM_K {
            let mut prev = 0u16;
            for row in v {
                b.extend_from_slice(&row[j].wrapping_sub(prev).to_le_bytes());
                prev = row[j];
            }
        }
        s.serialize_bytes(&b)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<[u16; LM_K]>, D::Error> {
        let b = bytes(d)?;
        if b.len() % (2 * LM_K) != 0 {
            return Err(de::Error::custom("landmark table length"));
        }
        let n = b.len() / (2 * LM_K);
        let mut v = vec![[0u16; LM_K]; n];
        for j in 0..LM_K {
            let mut prev = 0u16;
            for (k, row) in v.iter_mut().enumerate() {
                let o = 2 * (j * n + k);
                prev = prev.wrapping_add(u16::from_le_bytes([b[o], b[o + 1]]));
                row[j] = prev;
            }
        }
        Ok(v)
    }
}

/// What the snapshot was taken against: a snapshot is only used with its own world.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone)]
#[serde(default)]
pub(crate) struct WorldCheck {
    dims: (u16, u16, u16),
    units: usize,
    items: usize,
    buildings: usize,
    year: i32,
    year_tick: i32,
    /// FNV over unit ids, item ids and item positions.
    ids: u64,
}

impl WorldCheck {
    fn of(w: &rf_core::World) -> WorldCheck {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        for u in &w.units {
            mix(u.id as u64);
        }
        for it in &w.items {
            mix(it.id as u64);
            mix(((it.pos.x as u64) << 32) | ((it.pos.y as u64) << 16) | it.pos.z as u64);
        }
        WorldCheck {
            dims: (w.dims.x, w.dims.y, w.dims.z),
            units: w.units.len(),
            items: w.items.len(),
            buildings: w.buildings.len(),
            year: w.calendar.year,
            year_tick: w.calendar.year_tick,
            ids: h,
        }
    }
}

#[derive(Serialize)]
struct SnapRef<'a> {
    version: u32,
    check: WorldCheck,
    sim: &'a Sim,
}

#[derive(Deserialize)]
struct SnapOwned {
    version: u32,
    check: WorldCheck,
    sim: Sim,
}

impl Sim {
    /// [saves11] Empty sim (no world); the defaults of a snapshot's missing fields.
    pub(crate) fn blank() -> Sim {
        Sim::new(rf_core::World::default(), 0)
    }

    /// [saves11] MessagePack snapshot of the whole runtime state (without the world).
    pub fn exact_snapshot(&self) -> Vec<u8> {
        let snap = SnapRef { version: EXACT_VERSION, check: WorldCheck::of(&self.world), sim: self };
        rmp_serde::to_vec_named(&snap).unwrap_or_default()
    }

    /// [saves11] Rebuild a sim from a snapshot of it and its world. Err: the snapshot is from
    /// another version or another world (the caller falls back to the round-10 restore).
    pub(crate) fn from_exact(bytes: &[u8], world: &rf_core::World) -> Result<Sim, String> {
        let snap: SnapOwned = rmp_serde::from_slice(bytes).map_err(|e| format!("snapshot: {e}"))?;
        if snap.version != EXACT_VERSION {
            return Err(format!("snapshot version {} (this Rustfort reads {EXACT_VERSION})", snap.version));
        }
        let check = WorldCheck::of(world);
        if snap.check != check {
            return Err("snapshot belongs to another world state".into());
        }
        let sim = snap.sim;
        let n = world.tiles.len();
        if sim.nav.flags.len() != n || sim.ai.len() != world.units.len() || sim.imeta.len() != world.items.len() {
            return Err("snapshot tables do not match the world".into());
        }
        Ok(sim)
    }

    /// [saves11] How this sim came from a save: "" (not from a save), "exact" (snapshot:
    /// continues bit-identically) or "close (reason)" (module state only, see `persist`).
    pub fn restore_mode(&self) -> &str {
        &self.restore
    }

    /// [saves11] Rebuild what the snapshot does not store (see the `len_only` fields).
    pub(crate) fn after_exact_load(&mut self) {
        self.nav.lm.after_load();
    }

    /// [saves11] Encoded size of each part of the snapshot (bytes, before compression).
    pub fn exact_sizes(&self) -> Vec<(&'static str, usize)> {
        fn sz<T: Serialize>(v: &T) -> usize {
            rmp_serde::to_vec_named(v).map(|b| b.len()).unwrap_or(0)
        }
        vec![
            ("nav", sz(&self.nav)),
            ("ai", sz(&self.ai)),
            ("jobs", sz(&self.jobs) + sz(&self.job_at) + sz(&self.chunks) + sz(&self.chunk_of) + sz(&self.chunk_order)),
            ("piles", sz(&self.piles) + sz(&self.pile_at)),
            ("items", sz(&self.imeta) + sz(&self.item_idx) + sz(&self.contents)),
            ("haul", sz(&self.haul) + sz(&self.haul_ix) + sz(&self.haul_cat) + sz(&self.haul_pos)),
            ("path_cache", sz(&self.path_cache)),
            ("water", sz(&self.water) + sz(&self.water_set)),
            ("prod", sz(&self.prod)),
            ("social", sz(&self.social)),
            ("store", sz(&self.store)),
            ("ind", sz(&self.ind)),
            ("combat", sz(&self.combat)),
            ("fluids", sz(&self.fluids)),
            ("power", sz(&self.power)),
            ("pop", sz(&self.pop)),
            ("trade", sz(&self.trade)),
            ("def", sz(&self.def)),
            ("ranged", sz(&self.ranged)),
            ("cx", sz(&self.cx)),
            ("moods", sz(&self.moods)),
            ("health", sz(&self.health)),
            ("caverns", sz(&self.caverns)),
            ("animals", sz(&self.animals)),
            ("locs", sz(&self.locs)),
            ("justice", sz(&self.justice)),
            ("life", sz(&self.life)),
            ("skills", sz(&self.skills) + sz(&self.growth)),
            ("total", self.exact_snapshot().len()),
        ]
    }
}
