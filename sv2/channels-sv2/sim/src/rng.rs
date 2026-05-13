//! Deterministic RNG and exponential-distribution sampling for trial simulation.
//!
//! Uses XorShift64 rather than the `rand` crate because:
//! - Reproducibility across crate versions: we hand-roll the bit operations
//!   so two runs of the framework, possibly years apart with different
//!   transitive dependency versions, produce identical share-arrival streams
//!   from the same seed.
//! - Minimal dependency footprint for the sim crate.
//!
//! XorShift64 is statistically adequate for sampling Poisson inter-arrival
//! times in this context. It is not cryptographically secure and not
//! recommended for any use beyond simulation.

/// XorShift64 pseudo-random number generator.
///
/// Algorithm from George Marsaglia, "Xorshift RNGs" (2003). 64-bit state,
/// period 2^64 - 1.
#[derive(Debug, Clone)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    /// Constructs a new generator from the given seed. Zero is replaced with
    /// a non-zero constant since XorShift cannot have a zero state.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF_CAFE_F00D } else { seed },
        }
    }

    /// Generates the next u64 in the stream.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Returns a uniformly-distributed f64 in the open interval (0, 1).
    ///
    /// Never returns exactly 0 (which would make `ln(u)` undefined for
    /// exponential sampling) and never returns exactly 1.
    pub fn next_f64(&mut self) -> f64 {
        // Take 53 bits of randomness (f64 mantissa precision).
        let bits = self.next_u64() >> 11; // 53 bits, range [0, 2^53)
        // Map [0, 2^53) → (0, 1): add 1 to numerator to exclude 0, divide by
        // (2^53 + 1) to keep result strictly < 1.
        ((bits as f64) + 1.0) / ((1u64 << 53) as f64 + 1.0)
    }
}

/// Samples one value from `Exp(rate)` — the exponential distribution with the
/// given rate parameter (events per unit time).
///
/// Returns the inter-arrival time. If `rate` is non-positive or non-finite,
/// returns `f64::INFINITY` (interpreted by callers as "next event never
/// happens within the trial window").
///
/// Uses inverse-CDF sampling: `T = -ln(U) / λ` where `U ~ Uniform(0, 1)`.
pub fn sample_exponential(rng: &mut XorShift64, rate: f64) -> f64 {
    if rate <= 0.0 || !rate.is_finite() {
        return f64::INFINITY;
    }
    -rng.next_f64().ln() / rate
}

/// Samples one value from `Poisson(λ)` — the number of events in a unit time
/// interval given an average rate of `lambda` events per interval.
///
/// Uses Knuth's algorithm for `λ < 30` (exact, but slow for large `λ`) and a
/// normal approximation for `λ >= 30` (accurate to within ~1% of the true
/// distribution's variance, very fast). The threshold is conservative — Knuth
/// is fine well past 30 — and chosen so the normal approximation's tail
/// behavior is reliable for our use cases (share counts during long ticks).
///
/// Returns 0 for non-positive, NaN, or infinite `lambda`. Saturates at
/// `u32::MAX` for absurdly large `lambda` (the trial-loop math can produce
/// extreme values during early cold-start ticks before the algorithm reacts).
pub fn sample_poisson(rng: &mut XorShift64, lambda: f64) -> u32 {
    if !lambda.is_finite() || lambda <= 0.0 {
        return 0;
    }
    if lambda < 30.0 {
        poisson_knuth(rng, lambda)
    } else {
        poisson_normal(rng, lambda)
    }
}

/// Knuth's algorithm for Poisson sampling: multiply uniforms until the running
/// product falls below `e^(-λ)`. The number of multiplications minus one is
/// the sample. O(λ) expected work.
fn poisson_knuth(rng: &mut XorShift64, lambda: f64) -> u32 {
    let l = (-lambda).exp();
    if l == 0.0 {
        // Numerically underflowed (λ very large) — should have gone via normal
        // approximation, but defend against the boundary.
        return poisson_normal(rng, lambda);
    }
    let mut k = 0u32;
    let mut p = 1.0;
    loop {
        // Pathological cap: stops runaway loops if `e^(-λ)` is denormalized.
        if k >= 10_000 {
            return k;
        }
        p *= rng.next_f64();
        if p <= l {
            return k;
        }
        k = k.saturating_add(1);
    }
}

/// Normal approximation for Poisson: `λ + sqrt(λ) * Z` where `Z ~ N(0, 1)`,
/// rounded and clamped to non-negative. Accurate for `λ >= ~10`; we use 30 as
/// the threshold to give the approximation some margin. O(1).
fn poisson_normal(rng: &mut XorShift64, lambda: f64) -> u32 {
    // Box-Muller transform for one N(0, 1) sample.
    let u1 = rng.next_f64();
    let u2 = rng.next_f64();
    let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
    let raw = lambda + lambda.sqrt() * z;
    if !raw.is_finite() || raw <= 0.0 {
        return 0;
    }
    if raw >= u32::MAX as f64 {
        return u32::MAX;
    }
    raw.round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xorshift64_is_deterministic_from_seed() {
        let mut a = XorShift64::new(42);
        let mut b = XorShift64::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn xorshift64_zero_seed_is_remapped() {
        let mut zero = XorShift64::new(0);
        // First output should not be zero (state was remapped, not left as 0).
        assert_ne!(zero.next_u64(), 0);
    }

    #[test]
    fn next_f64_is_in_open_unit_interval() {
        let mut rng = XorShift64::new(1);
        for _ in 0..10_000 {
            let u = rng.next_f64();
            assert!(u > 0.0 && u < 1.0, "next_f64 returned {u}, expected (0, 1)");
        }
    }

    #[test]
    fn exponential_mean_approximates_one_over_rate() {
        let mut rng = XorShift64::new(7);
        let rate = 0.5; // Exp(0.5) has mean 2.0
        let n = 100_000;
        let mean: f64 = (0..n).map(|_| sample_exponential(&mut rng, rate)).sum::<f64>() / n as f64;
        // Standard error of mean for Exp(0.5) over n samples is 2 / sqrt(n) ≈ 0.0063.
        // 6-sigma envelope is ±0.04 — very forgiving.
        assert!(
            (mean - 2.0).abs() < 0.04,
            "Exp(0.5) sample mean {mean} should be near 2.0"
        );
    }

    #[test]
    fn exponential_nonpositive_rate_returns_infinity() {
        let mut rng = XorShift64::new(1);
        assert!(sample_exponential(&mut rng, 0.0).is_infinite());
        assert!(sample_exponential(&mut rng, -1.0).is_infinite());
        assert!(sample_exponential(&mut rng, f64::NAN).is_infinite());
    }

    #[test]
    fn poisson_zero_or_negative_lambda_returns_zero() {
        let mut rng = XorShift64::new(1);
        assert_eq!(sample_poisson(&mut rng, 0.0), 0);
        assert_eq!(sample_poisson(&mut rng, -1.0), 0);
        assert_eq!(sample_poisson(&mut rng, f64::NAN), 0);
        assert_eq!(sample_poisson(&mut rng, f64::INFINITY), 0);
    }

    #[test]
    fn poisson_small_lambda_mean_approximates_lambda() {
        // Knuth path: λ = 5.0
        let mut rng = XorShift64::new(42);
        let n = 50_000;
        let sum: u64 = (0..n).map(|_| sample_poisson(&mut rng, 5.0) as u64).sum();
        let mean = sum as f64 / n as f64;
        // SE of mean for Poisson(5) over n=50k samples is sqrt(5/50000) ≈ 0.010.
        // 5-sigma envelope is ±0.05.
        assert!(
            (mean - 5.0).abs() < 0.05,
            "Poisson(5) sample mean {mean} should be near 5.0"
        );
    }

    #[test]
    fn poisson_large_lambda_mean_approximates_lambda() {
        // Normal-approximation path: λ = 1000.0
        let mut rng = XorShift64::new(42);
        let n = 10_000;
        let sum: u64 = (0..n).map(|_| sample_poisson(&mut rng, 1000.0) as u64).sum();
        let mean = sum as f64 / n as f64;
        // SE of mean for Poisson(1000) over n=10k samples is sqrt(1000/10000) ≈ 0.32.
        // 5-sigma envelope is ±1.6.
        assert!(
            (mean - 1000.0).abs() < 1.6,
            "Poisson(1000) sample mean {mean} should be near 1000.0"
        );
    }

    #[test]
    fn poisson_is_deterministic_from_seed() {
        let mut a = XorShift64::new(123);
        let mut b = XorShift64::new(123);
        for _ in 0..1000 {
            // Mix of small and large λ to exercise both paths
            assert_eq!(sample_poisson(&mut a, 3.0), sample_poisson(&mut b, 3.0));
            assert_eq!(sample_poisson(&mut a, 500.0), sample_poisson(&mut b, 500.0));
        }
    }
}
