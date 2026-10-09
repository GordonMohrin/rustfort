//! Small deterministic RNG (splitmix64). Seeded per world, unit and tick so that results do
//! not depend on thread scheduling.

#[inline]
pub fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(splitmix(seed))
    }
    /// RNG for one decision of one unit in one tick.
    pub fn for_unit(seed: u64, slot: usize, tick: u64, salt: u64) -> Rng {
        Rng::new(seed ^ splitmix((slot as u64) << 1 ^ splitmix(tick ^ salt.rotate_left(32))))
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `0..n` (n > 0).
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        (((self.next_u64() >> 32) * n as u64) >> 32) as u32
    }
    /// True with probability `num / den`.
    #[inline]
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den) < num
    }
}
