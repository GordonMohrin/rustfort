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
//! save's zstd compresses them well); the landmark table (`Nav::lm`) column-wise as residuals
//! of a left/above prediction ([`lm_encode`]), its tile -> index map and queue flags are
//! rebuilt.

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

/// Landmark tables: compact indices of the tiles left, above and above-left of compact tile
/// `k` that come before `k` (so a decoder has them already), u32::MAX = none.
fn lm_neighbours(dx: usize, id: &[u32], tile: &[u32]) -> Vec<[u32; 3]> {
    tile.iter()
        .enumerate()
        .map(|(k, &t)| {
            let t = t as usize;
            let mut nb = [u32::MAX; 3];
            for (s, off) in [1, dx, dx + 1].into_iter().enumerate() {
                if (s == 0 || dx > 0) && t >= off {
                    let c = id.get(t - off).copied().unwrap_or(u32::MAX);
                    if c != u32::MAX && (c as usize) < k {
                        nb[s] = c;
                    }
                }
            }
            nb
        })
        .collect()
}

/// Predicted distance of a tile from its left (l), upper (u) and upper-left (ul) neighbours
/// (LOCO-I median edge detector; distance fields are mostly planar), or the previous value.
#[inline]
fn lm_predict(l: Option<u16>, u: Option<u16>, ul: Option<u16>, prev: u16) -> u16 {
    use crate::nav::LM_INF;
    match (l, u, ul) {
        (Some(l), Some(u), Some(ul)) if l != LM_INF && u != LM_INF && ul != LM_INF => {
            let (mn, mx) = (l.min(u), l.max(u));
            if ul >= mx {
                mn
            } else if ul <= mn {
                mx
            } else {
                (l as i32 + u as i32 - ul as i32) as u16
            }
        }
        (Some(l), _, _) => l,
        (None, Some(u), _) => u,
        _ => prev,
    }
}

/// `Nav::lm.d` for a snapshot: column by column (one landmark at a time), each value as the
/// difference to its prediction from the neighbours left and above, zigzag-coded (small
/// negative and positive differences become small numbers) and split into a plane of low bytes
/// followed by a plane of high bytes. Most residuals are 0, which the save's compressor shrinks
/// to almost nothing.
pub(crate) fn lm_encode(dx: usize, id: &[u32], tile: &[u32], d: &[[u16; crate::nav::LM_K]]) -> Vec<u8> {
    let nb = lm_neighbours(dx, id, tile);
    let n = tile.len().min(d.len());
    let total = n * crate::nav::LM_K;
    let mut out = vec![0u8; total * 2];
    for j in 0..crate::nav::LM_K {
        let v = |c: u32| (c != u32::MAX).then(|| d[c as usize][j]);
        for k in 0..n {
            let prev = if k > 0 { d[k - 1][j] } else { 0 };
            let p = lm_predict(v(nb[k][0]), v(nb[k][1]), v(nb[k][2]), prev);
            let r = d[k][j].wrapping_sub(p) as i16;
            let z = ((r << 1) ^ (r >> 15)) as u16;
            let o = j * n + k;
            out[o] = z as u8;
            out[total + o] = (z >> 8) as u8;
        }
    }
    out
}

/// Inverse of [`lm_encode`] (`id` must already map tiles to compact indices).
pub(crate) fn lm_decode(dx: usize, id: &[u32], tile: &[u32], b: &[u8]) -> Option<Vec<[u16; crate::nav::LM_K]>> {
    let n = tile.len();
    let total = n * crate::nav::LM_K;
    if b.len() != total * 2 {
        return None;
    }
    let nb = lm_neighbours(dx, id, tile);
    let mut d = vec![[0u16; crate::nav::LM_K]; n];
    for j in 0..crate::nav::LM_K {
        for k in 0..n {
            let v = |c: u32| (c != u32::MAX).then(|| d[c as usize][j]);
            let prev = if k > 0 { d[k - 1][j] } else { 0 };
            let p = lm_predict(v(nb[k][0]), v(nb[k][1]), v(nb[k][2]), prev);
            let o = j * n + k;
            let z = b[o] as u16 | (b[total + o] as u16) << 8;
            let r = ((z >> 1) as i16) ^ -((z & 1) as i16);
            d[k][j] = p.wrapping_add(r as u16);
        }
    }
    Some(d)
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

/// Layout of a MessagePack value: the set of (path, map key) pairs it contains (array elements
/// share their array's path; keys that are not strings count as one placeholder). Two
/// encodings of the same data have the same layout whatever the order of their (hash) maps;
/// a missing or an extra field changes it. None: not well-formed MessagePack.
fn layout(b: &[u8]) -> Option<BTreeSet<u64>> {
    fn h(path: u64, key: &[u8]) -> u64 {
        let mut x = path ^ 0x9e3779b97f4a7c15;
        for &c in key {
            x ^= c as u64;
            x = x.wrapping_mul(0x100000001b3);
        }
        x.wrapping_mul(0x100000001b3)
    }
    fn take<'a>(b: &'a [u8], p: &mut usize, n: usize) -> Option<&'a [u8]> {
        let s = b.get(*p..p.checked_add(n)?)?;
        *p += n;
        Some(s)
    }
    fn be(b: &[u8], p: &mut usize, n: usize) -> Option<usize> {
        Some(take(b, p, n)?.iter().fold(0usize, |a, &c| (a << 8) | c as usize))
    }
    /// Skips one value; returns the bytes of a string value (to use it as a key).
    fn walk<'a>(b: &'a [u8], p: &mut usize, path: u64, out: &mut BTreeSet<u64>, depth: u32) -> Option<Option<&'a [u8]>> {
        if depth > 64 {
            return None;
        }
        let m = *b.get(*p)?;
        *p += 1;
        let (maps, arrays) = match m {
            0x00..=0x7f | 0xe0..=0xff | 0xc0 | 0xc2 | 0xc3 => return Some(None),
            0x80..=0x8f => ((m & 0x0f) as usize, 0),
            0x90..=0x9f => (0, (m & 0x0f) as usize),
            0xa0..=0xbf => return take(b, p, (m & 0x1f) as usize).map(Some),
            0xd9 => return { let n = be(b, p, 1)?; take(b, p, n).map(Some) },
            0xda => return { let n = be(b, p, 2)?; take(b, p, n).map(Some) },
            0xdb => return { let n = be(b, p, 4)?; take(b, p, n).map(Some) },
            0xc4 | 0xc5 | 0xc6 => {
                let n = be(b, p, 1 << (m - 0xc4))?;
                return take(b, p, n).map(|_| None);
            }
            0xc7 | 0xc8 | 0xc9 => {
                let n = be(b, p, 1 << (m - 0xc7))? + 1;
                return take(b, p, n).map(|_| None);
            }
            0xca | 0xce | 0xd2 => return take(b, p, 4).map(|_| None),
            0xcb | 0xcf | 0xd3 => return take(b, p, 8).map(|_| None),
            0xcc | 0xd0 => return take(b, p, 1).map(|_| None),
            0xcd | 0xd1 => return take(b, p, 2).map(|_| None),
            0xd4..=0xd8 => return take(b, p, 1 + (1 << (m - 0xd4))).map(|_| None),
            0xdc => (0, be(b, p, 2)?),
            0xdd => (0, be(b, p, 4)?),
            0xde => (be(b, p, 2)?, 0),
            0xdf => (be(b, p, 4)?, 0),
            _ => return None, // 0xc1 is never used
        };
        let elem = h(path, b"[]");
        for _ in 0..arrays {
            walk(b, p, elem, out, depth + 1)?;
        }
        for _ in 0..maps {
            let kp = h(path, b"{k}");
            let k = match walk(b, p, kp, out, depth + 1)? {
                Some(s) => h(path, s),
                None => h(path, b"{v}"),
            };
            out.insert(k);
            walk(b, p, k, out, depth + 1)?;
        }
        Some(None)
    }
    let mut out = BTreeSet::new();
    let mut p = 0;
    walk(b, &mut p, 0, &mut out, 0)?;
    (p == b.len()).then_some(out)
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
        // Same layout: every field the snapshot has is known here and every field this build
        // has was in the snapshot (none filled with a default, none ignored). A snapshot of
        // another build that added or dropped state would skip the init passes with tables
        // missing, so it takes the round-10 path instead.
        let again = rmp_serde::to_vec_named(&SnapRef { version: snap.version, check: snap.check, sim: &sim }).map_err(|e| format!("re-encode: {e}"))?;
        match (layout(bytes), layout(&again)) {
            (Some(a), Some(b)) if a == b => Ok(sim),
            (None, _) | (_, None) => Err("snapshot is not well-formed".into()),
            _ => Err("snapshot from a Rustfort build with other sim state".into()),
        }
    }

    /// [saves11] How this sim came from a save: "" (not from a save), "exact" (snapshot:
    /// continues bit-identically) or "close (reason)" (module state only, see `persist`).
    pub fn restore_mode(&self) -> &str {
        &self.restore
    }

    /// [saves11] Rebuild what the snapshot does not store (`#[serde(skip)]` caches). Nothing
    /// today: the landmark tables rebuild their own index maps while decoding.
    pub(crate) fn after_exact_load(&mut self) {}

    /// [saves11] Encoded size of each part of the snapshot (bytes, before compression).
    pub fn exact_sizes(&self) -> Vec<(&'static str, usize)> {
        self.exact_parts().into_iter().map(|(k, v)| (k, v.len())).collect()
    }

    /// [saves11] The snapshot split by part (each part's encoding; for size checks).
    pub fn exact_parts(&self) -> Vec<(&'static str, Vec<u8>)> {
        fn enc<T: Serialize>(v: &T) -> Vec<u8> {
            rmp_serde::to_vec_named(v).unwrap_or_default()
        }
        let mut comp = Vec::with_capacity(self.nav.comp.len() * 4);
        for c in &self.nav.comp {
            comp.extend_from_slice(&c.to_le_bytes());
        }
        let (nav, lm) = (enc(&self.nav), enc(&self.nav.lm));
        let rest = nav.len().saturating_sub(lm.len() + comp.len() + self.nav.flags.len());
        let cat = |v: Vec<Vec<u8>>| v.concat();
        vec![
            ("nav.flags", self.nav.flags.clone()),
            ("nav.comp", comp),
            ("nav.lm", lm),
            ("nav.rest", nav[..rest.min(nav.len())].to_vec()),
            ("ai", enc(&self.ai)),
            ("jobs", cat(vec![enc(&self.jobs), enc(&self.job_at), enc(&self.chunks), enc(&self.chunk_of), enc(&self.chunk_order)])),
            ("piles", cat(vec![enc(&self.piles), enc(&self.pile_at)])),
            ("items.meta", enc(&self.imeta)),
            ("items.idx", enc(&self.item_idx)),
            ("items.contents", enc(&self.contents)),
            ("haul", cat(vec![enc(&self.haul), enc(&self.haul_ix), enc(&self.haul_cat), enc(&self.haul_pos)])),
            ("path_cache", enc(&self.path_cache)),
            ("water", cat(vec![enc(&self.water), enc(&self.water_set)])),
            ("prod", enc(&self.prod)),
            ("social", enc(&self.social)),
            ("store", enc(&self.store)),
            ("ind", enc(&self.ind)),
            ("combat", enc(&self.combat)),
            ("fluids", enc(&self.fluids)),
            ("power", enc(&self.power)),
            ("pop", enc(&self.pop)),
            ("trade", enc(&self.trade)),
            ("def", enc(&self.def)),
            ("ranged", enc(&self.ranged)),
            ("cx", enc(&self.cx)),
            ("moods", enc(&self.moods)),
            ("health", enc(&self.health)),
            ("caverns", enc(&self.caverns)),
            ("animals", enc(&self.animals)),
            ("locs", enc(&self.locs)),
            ("justice", enc(&self.justice)),
            ("life", enc(&self.life)),
            ("skills", cat(vec![enc(&self.skills), enc(&self.growth)])),
            ("total", self.exact_snapshot()),
        ]
    }
}
