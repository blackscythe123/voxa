//! A small, seedable random number generator (SplitMix64) with the few distributions the engine
//! needs: uniform, normal (Box-Muller), gamma (Marsaglia-Tsang) and beta. No dependencies, and the
//! same seed always gives the same numbers on every platform.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    s: u64,
}

/// One SplitMix64 step: a strong 64-bit mix, also used to hash (seed, day, check) for the
/// simulator's common random numbers.
pub fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A uniform number in [0, 1) from a hash of the parts: the same parts always give the same draw.
pub fn hash_unit(parts: &[u64]) -> f64 {
    let mut h = 0x5EED_u64;
    for p in parts {
        h = mix(h ^ p.wrapping_mul(0xA24B_AED4_963E_E407));
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { s: mix(seed) }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.s = self.s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in 0..n (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f64) as usize).min(n - 1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Standard normal.
    pub fn normal(&mut self) -> f64 {
        let u1 = self.unit().max(1e-300);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Gamma(shape, 1), shape > 0 (Marsaglia and Tsang, with the shape < 1 boost).
    pub fn gamma(&mut self, shape: f64) -> f64 {
        // A bad shape (negative, zero, not a number) would recurse forever below.
        let shape = if shape.is_finite() && shape > 0.0 { shape } else { 1e-3 };
        if shape < 1.0 {
            let u = self.unit().max(1e-300);
            return self.gamma(shape + 1.0) * u.powf(1.0 / shape);
        }
        let d = shape - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let x = self.normal();
            let v = (1.0 + c * x).powi(3);
            if v <= 0.0 {
                continue;
            }
            let u = self.unit().max(1e-300);
            if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
                return d * v;
            }
        }
    }

    /// Beta(a, b), a, b > 0.
    pub fn beta(&mut self, a: f64, b: f64) -> f64 {
        let x = self.gamma(a);
        let y = self.gamma(b);
        if x + y <= 0.0 { 0.5 } else { x / (x + y) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_numbers() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn beta_has_the_right_mean() {
        let mut r = Rng::new(1);
        let n = 20_000;
        let mean: f64 = (0..n).map(|_| r.beta(2.0, 6.0)).sum::<f64>() / n as f64;
        assert!((mean - 0.25).abs() < 0.01, "mean {mean}");
        let mean: f64 = (0..n).map(|_| r.beta(0.5, 0.5)).sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 0.02, "mean {mean}");
    }

    #[test]
    fn hash_unit_is_stable_and_in_range() {
        let a = hash_unit(&[1, 2, 3]);
        assert_eq!(a, hash_unit(&[1, 2, 3]));
        assert_ne!(a, hash_unit(&[1, 2, 4]));
        assert!((0.0..1.0).contains(&a));
    }

    #[test]
    fn bad_shapes_never_recurse_forever() {
        let mut r = Rng::new(9);
        for shape in [-3.0, 0.0, f64::NAN, f64::NEG_INFINITY] {
            assert!(r.gamma(shape).is_finite());
        }
        assert!(r.beta(-1.0, 2.0).is_finite());
    }
}
