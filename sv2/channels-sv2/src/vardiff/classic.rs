use crate::target::hash_rate_from_target;
use bitcoin::Target;
use tracing::debug;

/// Default minimum hashrate (H/s) if not specified.
const DEFAULT_MIN_HASHRATE: f32 = 1.0;

/// Most the believed hashrate may fall below the last belief that had evidence behind it.
///
/// *Displacement* is that ratio, `d = H_true / H_believed`. A belief `d` times too low serves a
/// difficulty `d` times too easy, so a miner asked for `r*` shares a minute sends `r*·d` when it
/// returns. Bounding displacement bounds the flood.
///
/// A ratio rather than a count of eases, because a count only equals a ratio for one step size.
/// The two agreed while a silent channel took the fixed `÷3` arm, where two eases and a ratio of
/// `9` were the same bound; the form was deliberately changed to the ratio while they still agreed,
/// so the change was checkable as a no-op against an unchanged bound test. This patch removes that
/// arm, and the forms part company here: silence now eases through the estimator, so the number of
/// evaluations it takes to reach `9×` depends on the step size rather than being fixed at two.
/// `min_allowed_hashrate` is no substitute: from a 200 TH belief its `1.0` H/s default is some
/// thirty eases away.
///
/// Binds only while evidence is absent: [`VardiffState::evidenced_hashrate`] re-anchors whenever
/// shares arrive, so a genuinely declining miner, which still submits, is never held back by it.
const MAX_SILENT_DISPLACEMENT: f32 = 9.0;

/// Share rate below which the decision threshold is sized from a Poisson interval rather than a
/// sequential test, in shares per minute.
///
/// Below this the per-window share count is small enough that its own sampling spread dominates,
/// so the threshold has to be widened by that spread or the controller chases counting noise. At
/// or above it there are enough shares per window for a sequential test to be the tighter choice.
const SPARSE_SPM_SEAM: f32 = 6.0;

/// Two-sided 99% normal quantile, used to widen the sparse-branch threshold by the sampling
/// spread of the observed share count.
const POISSON_Z: f64 = 2.576;

/// Floor on the *base* threshold, as a fraction, before the directional multiplier and the
/// persistence discount scale it.
///
/// Both branches add this, so as a window lengthens and the evidence term vanishes, this is what
/// remains — the thing the fixed 15% ladder set far too high. Shared between them deliberately:
/// "how small a deviation is worth acting on" is one policy, not two.
///
/// It is not the smallest deviation the controller will ever act on, and the difference is the
/// point. [`TIGHTEN_MULTIPLIER`] scales it too, so on a long window the loosening bar approaches
/// this 5% while the tightening bar approaches `5% × 8 = 40%`, or 16% once a same-direction run
/// has earned its full discount. Over-delivery below that is never corrected at any window length.
///
/// That gap is the asymmetric dead zone, and it is deliberate: it is where the controller's
/// settled offset comes from. The offset is recorded at −6.12% in simulation and −6.96% on
/// hardware, comfortably inside the band, and it is load-bearing rather than an error — it parks
/// the excursion band below the level at which a switch-miner leaves. Widening this constant or
/// narrowing the multiplier moves the dead zone and therefore moves the offset.
const MIN_THRESHOLD_FRACTION: f64 = 0.05;

/// Deviation required after one minute of observation, as a fraction, on the dense branch.
///
/// Divided by the window's length in minutes, so the requirement relaxes as observation
/// accumulates: a deviation too small to act on after one minute becomes actionable after several.
///
/// Named for what it is rather than for a method. An earlier form of this called itself a CUSUM,
/// after the cumulative-sum control chart, but nothing here accumulates anything — the value is a
/// fixed requirement divided by elapsed time. Importing the name would have promised a sequential
/// test the code does not perform.
const EVIDENCE_AT_ONE_MINUTE: f64 = 1.5;

/// Share rate at which [`EVIDENCE_AT_ONE_MINUTE`] is calibrated, in shares per minute.
///
/// A channel run at `k` times this rate sees `k` times as many shares per window, and a count's
/// *relative* spread falls as `1/sqrt(count)` — so the same window carries `sqrt(k)` times the
/// resolving power. The requirement is scaled by `sqrt(k)` to match, which keeps the false-fire
/// rate roughly flat across deployments rather than letting a high-rate channel retarget on noise
/// a low-rate one would ignore.
const REFERENCE_SPM: f64 = 30.0;

/// Extra evidence required before *tightening*, as a multiple of the loosening requirement.
///
/// Tightening is the direction that can compound: raise the difficulty on a miner that is already
/// slowing and it delivers still fewer shares, which reads as slower again. Loosening cannot
/// compound that way, so the two directions do not warrant the same burden of proof.
///
/// Safe here only because a share is worth its own difficulty. A miner producing `H/D` shares at
/// difficulty `D` is credited `(H/D)·D = H`, so `D` cancels and easing the difficulty pays nothing
/// extra — measured at zero gain under difficulty-weighted accounting, against `+29.4%` under
/// share-count accounting. The same asymmetry on a chain, where a solution pays a fixed reward
/// regardless of difficulty, was farmed: Bitcoin Cash's Emergency Difficulty Adjustment shipped in
/// August 2017 and was replaced within months. The mechanism is not safe in general; it is safe
/// under this accounting.
const TIGHTEN_MULTIPLIER: f64 = 8.0;

/// How much the threshold relaxes per extra observation pointing the same way, as a fraction.
///
/// A deviation that keeps appearing in the same direction is different evidence from one that
/// flickers: a shortfall seen five evaluations running is more likely real than five coincidences,
/// while noise changes sign and earns nothing. So the bar comes down as the direction repeats, and
/// resets the moment it reverses.
pub(crate) const DIRECTION_DISCOUNT_PER_OBSERVATION: f64 = 0.06;

/// Ceiling on the accumulated same-direction discount, as a fraction.
///
/// Without a ceiling a long run would drive the threshold to zero and the controller would act on
/// noise indefinitely.
pub(crate) const MAX_DIRECTION_DISCOUNT: f64 = 0.6;

/// Run length at which the discount reaches [`MAX_DIRECTION_DISCOUNT`].
///
/// The stored run is clamped here. Counting past the point where the count changes nothing would
/// keep state whose value is never read.
const DIRECTION_RUN_AT_MAX_DISCOUNT: u32 =
    1 + (MAX_DIRECTION_DISCOUNT / DIRECTION_DISCOUNT_PER_OBSERVATION) as u32;

/// Fraction of the gap to the new estimate that a first retarget closes.
///
/// A full retarget jumps the belief straight to the estimate. That is the largest move consistent
/// with the evidence, and on a tick-quantised loop it is slightly too large — the estimate carries
/// the sampling noise of one window, so moving the whole way transfers that noise into the
/// difficulty. Moving part of the way keeps most of the correction and leaves the rest to the next
/// evaluation, which has fresh evidence.
pub(crate) const STEP_FRACTION_BASE: f32 = 0.2;

/// Extra fraction closed per consecutive retarget in the same direction.
///
/// Repeated moves the same way say the partial steps are not keeping up, so the steps grow. A
/// reversal resets to [`STEP_FRACTION_BASE`], because the direction changing is evidence the previous run had
/// gone far enough.
pub(crate) const STEP_FRACTION_GROWTH: f32 = 0.05;

/// Ceiling on the fraction a single retarget may close.
///
/// Defensive rather than load-bearing: it takes nine consecutive same-direction retargets to reach,
/// and the longest run observed in a measured `3x` step response was six, so in that scenario the
/// ceiling never binds and the effective range is `0.2` to `0.45`.
pub(crate) const STEP_FRACTION_MAX: f32 = 0.6;

/// Consecutive same-direction retargets at which the step fraction reaches [`STEP_FRACTION_MAX`].
///
/// Derived rather than written down, so it cannot drift from the three constants it depends on.
/// The stored run is clamped here, so it never holds a value that changes nothing.
const RUN_AT_MAX_STEP_FRACTION: u32 =
    1 + ((STEP_FRACTION_MAX - STEP_FRACTION_BASE) / STEP_FRACTION_GROWTH) as u32;

/// Most one evaluation may multiply or divide the believed hashrate by.
///
/// One window of evidence should not be able to move the belief arbitrarily far, in either
/// direction, however large the deviation it reports.
///
/// Replaces a one-sided special case that multiplied by 10, 5 or 3 according to which of three
/// elapsed-time buckets the window fell in. Two shapes were wrong there. Bucketing by elapsed time
/// duplicated what the threshold already does with the window — a short window is already held to a
/// higher evidence bar, so widening its permitted step pulls against that. And the factor applied
/// to the *pre-move* estimate rather than to the move made, so once retargets became partial the
/// documented factor was never the factor applied.
///
/// `2.0` is not uniformly tighter than what it replaces, and the comparison worth making is a
/// trajectory rather than a pair of constants. Measured against a source whose share rate does not
/// respond to the target, the buckets applied steps anywhere from `×1.40` to `×6.40` depending on
/// window length and how far a same-direction run had grown the step fraction; this bound applies
/// `×2.00` on every fire. So it is looser than the buckets at the start of a long-window run and
/// tighter everywhere else.
///
/// Sixty evaluations against that source, `k` being the multiple of the target rate delivered:
///
/// | source            | buckets     | this bound  |
/// |-------------------|-------------|-------------|
/// | `k=20`, 61 s      | `3.2e26×`   | `5.8e17×`   |
/// | `k=20`, 29 s      | `3.3e20×`   | `1.1e12×`   |
/// | `k=100`, 61 s     | `5.2e19×`   | `1.2e18×`   |
/// | `k=100`, 29 s     | `6.5e31×`   | `1.2e18×`   |
///
/// The gain is not monotone in intensity, and quoting one figure for it would mislead: it is
/// fourteen orders of magnitude at the worst bucket case and only `46×` at `k=100` with 61-second
/// windows. What the bound does buy unconditionally is that the displacement no longer depends on
/// the source at all — every row above is `2^60` once the bound binds.
///
/// This bounds the *rate* at which such a climb proceeds. It does not stop one — a bound on step
/// size cannot, because each step is individually warranted by the evidence in front of it.
/// Recognising that raising the difficulty is not reducing the share rate is a separate mechanism,
/// and is not attempted here.
const MAX_STEP_RATIO: f32 = 2.0;

/// Time constant of the EWMA estimator, in seconds.
///
/// Old evidence decays on the clock at `e^(−Δt/tau)`, which is what makes the window unable to
/// entrench: it forgets whether or not the controller ever decides to act.
const EWMA_TAU_SECS: u64 = 360;

use super::{
    clock::{Clock, SystemClock},
    error::VardiffError,
    Vardiff,
};
use std::sync::Arc;

/// Represents the dynamic state for a variable difficulty (Vardiff) connection.
///
/// Tracks performance and adjusts the mining target to achieve a desired share rate.
///
/// Construct with [`VardiffState::new`] or [`VardiffState::new_with_min`]. The struct is
/// `#[non_exhaustive]` so that controller state can be added without a breaking change:
/// every field of a constructible `pub` struct is part of its public API, whether the field
/// itself is `pub` or private, so without this attribute each added field forces a major
/// version. Nothing is constructing this by struct literal — the only literal is in
/// `new_with_min` below — so the attribute costs callers nothing.
#[derive(Debug)]
#[non_exhaustive]
pub struct VardiffState {
    /// Count of shares received since the last difficulty adjustment.
    pub shares_since_last_update: u32,
    /// Unix timestamp (seconds) of the last difficulty adjustment.
    pub timestamp_of_last_update: u64,
    /// The lowest hashrate (H/s) the system will allow; values below this are clamped.
    pub min_allowed_hashrate: f32,
    /// Source of "current time". Defaults to [`SystemClock`], so production behaviour is
    /// unchanged; tests substitute a [`MockClock`] to drive elapsed time explicitly.
    ///
    /// [`MockClock`]: super::clock::MockClock
    clock: Arc<dyn Clock>,
    /// The belief as of the last evaluation that saw shares, i.e. the last belief backed by
    /// evidence. The ease is not allowed below `evidenced_hashrate / MAX_SILENT_DISPLACEMENT`.
    /// `0.0` means "not yet anchored"; the first evaluation anchors it to the opening belief.
    evidenced_hashrate: f32,
    /// Unix timestamp (seconds) of the last *evaluation*, as distinct from the last retarget.
    ///
    /// The estimator needs its own clock reference. `timestamp_of_last_update` advances only when
    /// the controller retargets, so it measures the window the decision boundary cares about. The
    /// EWMA consumes the pending share count on every evaluation, so its interval is the gap since
    /// the previous evaluation; dividing one by the other would pair a single evaluation's shares
    /// with the time since the last retarget and under-read the rate by exactly the factor by
    /// which the channel had gone unretargeted.
    timestamp_of_last_evaluation: u64,
    /// EWMA-smoothed share rate, in shares per minute. Difficulty-relative: it is rescaled on
    /// every retarget by [`VardiffState::rescale_ewma`].
    rate: f64,
    /// Direction of the last evaluation's move: `+1` tightening, `-1` loosening, `0` unset.
    last_direction: i8,
    /// Direction of the last *retarget*, as distinct from the last evaluation. Kept separately
    /// because the two count different events: the threshold discount responds to how long a
    /// deviation has persisted across evaluations, while the step fraction responds to how many times the
    /// controller has actually moved the belief the same way. Most evaluations do not retarget, so
    /// one counter cannot serve both.
    last_fire_direction: i8,
    /// Consecutive retargets in the same direction, counting the most recent.
    consecutive_fires_same_direction: u32,
    /// How many consecutive evaluations have moved the same way, counting the current one.
    consecutive_same_direction: u32,
    /// Whether `rate` holds an observation yet. An unseeded filter takes its first observation
    /// whole rather than blending it against a zero prior, which would start every channel with a
    /// fictitious idle period.
    rate_seeded: bool,
}

impl VardiffState {
    /// Creates a new `VardiffState` with the default minimum hashrate.
    ///
    /// # Arguments
    /// * `estimated_hashrate` - The initial hashrate estimate.
    pub fn new() -> Result<Self, VardiffError> {
        Self::new_with_min(DEFAULT_MIN_HASHRATE)
    }

    /// Creates a new `VardiffState` with a specific minimum hashrate.
    ///
    /// # Arguments
    /// * `min_allowed_hashrate` - The minimum hashrate to enforce. A non-positive or non-finite
    ///   value is meaningless as a floor (and would reintroduce the division-by-zero that
    ///   [`Vardiff::try_vardiff`] guards against), so it falls back to the default.
    pub fn new_with_min(min_allowed_hashrate: f32) -> Result<Self, VardiffError> {
        Self::new_with_clock(min_allowed_hashrate, Arc::new(SystemClock))
    }

    /// Creates a new `VardiffState` reading time from `clock`.
    ///
    /// The controller's behaviour is a function of elapsed time — which window has passed, how
    /// long a channel has been silent, how many evaluations have gone by without acting — so
    /// testing it means controlling the clock rather than waiting on one. [`VardiffState::new`]
    /// and [`VardiffState::new_with_min`] supply [`SystemClock`] and behave exactly as before.
    ///
    /// # Arguments
    /// * `min_allowed_hashrate` - As [`VardiffState::new_with_min`].
    /// * `clock` - Time source. Shared, because the algorithm reads it while a test driver
    ///   advances it.
    pub fn new_with_clock(
        min_allowed_hashrate: f32,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, VardiffError> {
        let timestamp_secs = clock.now_secs()?;

        let min_allowed_hashrate = if min_allowed_hashrate.is_finite() && min_allowed_hashrate > 0.0
        {
            min_allowed_hashrate
        } else {
            DEFAULT_MIN_HASHRATE
        };

        Ok(VardiffState {
            shares_since_last_update: 0,
            timestamp_of_last_update: timestamp_secs,
            min_allowed_hashrate,
            clock,
            evidenced_hashrate: 0.0,
            timestamp_of_last_evaluation: timestamp_secs,
            last_direction: 0,
            last_fire_direction: 0,
            consecutive_fires_same_direction: 0,
            consecutive_same_direction: 0,
            rate: 0.0,
            rate_seeded: false,
        })
    }

    /// Sets the count of shares since the last update.
    pub fn set_shares_since_last_update(&mut self, shares_since_last_update: u32) {
        self.shares_since_last_update = shares_since_last_update;
    }

    /// Test-only: the current same-direction run and its recorded direction.
    #[cfg(test)]
    pub(crate) fn direction_run(&self) -> (i8, u32) {
        (self.last_direction, self.consecutive_same_direction)
    }

    /// Test-only: the recorded retarget direction and the length of the current retarget run.
    #[cfg(test)]
    pub(crate) fn fire_run(&self) -> (i8, u32) {
        (
            self.last_fire_direction,
            self.consecutive_fires_same_direction,
        )
    }

    /// Test-only: the EWMA's smoothed rate, in shares per minute.
    #[cfg(test)]
    pub(crate) fn ewma_rate(&self) -> f64 {
        self.rate
    }

    /// Decay factor for an interval of `dt_secs`: `e^(−Δt/tau)`.
    ///
    /// Derived from the *measured* interval rather than a nominal period, so a late, early or
    /// missed evaluation changes how much the filter forgets by the right amount instead of
    /// silently altering its memory. This is the same reason `tau` is parameterised in seconds
    /// rather than in evaluations.
    fn ewma_alpha(dt_secs: u64) -> f64 {
        (-(dt_secs as f64) / (EWMA_TAU_SECS as f64)).exp()
    }

    /// Folds the pending share count into the EWMA and returns
    /// `(realized shares per minute, implied hashrate)`.
    ///
    /// Replaces the cumulative mean `shares / elapsed`, whose window only ever reset when the
    /// controller decided to retarget. That coupling is why the old estimator entrenched: a window
    /// that grows whenever the controller declines to act is least able to notice that it should.
    /// An EWMA holds its memory in the coefficient instead, so evidence ages on the clock and the
    /// estimator cannot be starved of forgetting by a run of no-fire decisions.
    ///
    /// Consumes the pending count, so callers that need it — the silence anchor, the log line —
    /// must read it first.
    fn estimate(
        &mut self,
        dt_secs: u64,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> (f64, f32) {
        // Smooth a *rate*, not a raw count. `try_vardiff` is not called on a fixed cadence, so
        // folding the count directly would make the estimate scale with however long the caller
        // happened to wait — five shares over five minutes would read the same as five over one.
        let observed_spm = if dt_secs > 0 {
            self.shares_since_last_update as f64 / (dt_secs as f64 / 60.0)
        } else {
            0.0
        };

        // An unseeded EWMA takes the first observation whole; blending against a zero prior would
        // start every channel with a fictitious idle period.
        self.rate = if self.rate_seeded {
            let alpha = Self::ewma_alpha(dt_secs);
            alpha * self.rate + (1.0 - alpha) * observed_spm
        } else {
            observed_spm
        };
        self.rate_seeded = true;
        self.shares_since_last_update = 0;

        let realized_share_per_min = self.rate;

        let h_estimate = match hash_rate_from_target(
            target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(e) => {
                debug!(
                    target: "vardiff",
                    "Target->Hashrate conversion failed: {:?}. Falling back using previous hashrate and realized_shares_per_minute", e
                );
                hashrate * realized_share_per_min as f32 / shares_per_minute
            }
        };

        (realized_share_per_min, h_estimate)
    }

    /// Records this evaluation's direction and returns the length of the current same-direction
    /// run, counting this evaluation.
    fn note_direction(&mut self, tightening: bool) -> u32 {
        let direction: i8 = if tightening { 1 } else { -1 };
        if direction == self.last_direction {
            self.consecutive_same_direction =
                (self.consecutive_same_direction + 1).min(DIRECTION_RUN_AT_MAX_DISCOUNT);
        } else {
            self.last_direction = direction;
            self.consecutive_same_direction = 1;
        }
        self.consecutive_same_direction
    }

    /// Smallest deviation worth acting on, as a percentage, given the evidence in this window.
    ///
    /// Replaces a fixed ladder of six rungs whose loosest was 15% after five minutes. That floor
    /// was set once, for no stated share rate, so every deviation under it was unseeable however
    /// long the channel ran or however many shares it delivered: a channel 12% off target simply
    /// never retargeted. Sizing the threshold from the evidence instead means the controller acts
    /// as soon as the deviation is larger than what sampling noise could explain.
    ///
    /// Two branches, because the thing that limits confidence changes with the share rate. Below
    /// [`SPARSE_SPM_SEAM`] the window holds few enough shares that the count's own spread
    /// dominates, so the threshold is widened by that spread. At or above it, enough shares arrive
    /// for a sequential test to be tighter.
    ///
    /// Whichever branch sizes it, tightening then requires [`TIGHTEN_MULTIPLIER`] times as much,
    /// because tightening is the direction that can compound.
    fn threshold(
        &self,
        dt_secs: u64,
        shares_per_minute: f32,
        tightening: bool,
        same_direction_run: u32,
    ) -> f64 {
        let base = if shares_per_minute < SPARSE_SPM_SEAM {
            Self::sparse_threshold(dt_secs, shares_per_minute)
        } else {
            Self::dense_threshold(dt_secs, shares_per_minute)
        };

        // Applied here rather than inside each branch: the direction that needs resisting does not
        // depend on which branch sized the threshold, so it belongs once, after the dispatch.
        //
        // `tightening` is read from the move the controller is about to make, not from whether the
        // observed rate exceeds the target. Those agree only while the caller's `target` and
        // `hashrate` describe the same difficulty, which nothing in this signature enforces — and
        // if they disagree, deciding from the rate would apply the extra burden of proof to a
        // loosening move and withhold it from a tightening one, inverting the property.
        let directional = if tightening {
            base * TIGHTEN_MULTIPLIER
        } else {
            base
        };

        // Relax in proportion to how long the deviation has pointed this way. Applied to both
        // directions, because persistence is evidence either way — but note it therefore erodes
        // the tightening multiplier: at the ceiling an 8x requirement becomes 3.2x.
        let discount = (DIRECTION_DISCOUNT_PER_OBSERVATION
            * same_direction_run.saturating_sub(1) as f64)
            .min(MAX_DIRECTION_DISCOUNT);
        directional * (1.0 - discount)
    }

    /// Sparse branch: widen the threshold by the sampling spread of the expected share count.
    ///
    /// Share arrivals are Poisson, which is the one piece of statistics this needs: the count in a
    /// window has variance equal to its mean, so its standard deviation is the square root of the
    /// mean.
    ///
    /// With `lambda` shares expected in the window, the count's standard deviation is
    /// `sqrt(lambda)`, so a deviation of `z·sqrt(lambda)` shares is within ordinary variation. As a
    /// fraction of `lambda` that is `z/sqrt(lambda)`, which shrinks as the window lengthens — the
    /// threshold tightens on its own as evidence accumulates. The `+0.5` is the continuity
    /// correction for treating a discrete count as continuous.
    fn sparse_threshold(dt_secs: u64, shares_per_minute: f32) -> f64 {
        let lambda = (shares_per_minute as f64 / 60.0) * dt_secs as f64;
        if lambda <= 0.0 {
            // No evidence can arrive in a zero-length window; refuse to act.
            return f64::INFINITY;
        }
        ((POISSON_Z * lambda.sqrt() + 0.5) / lambda + MIN_THRESHOLD_FRACTION) * 100.0
    }

    /// Dense branch: require a fixed deviation after one minute, relaxed in proportion to the
    /// window.
    ///
    /// The requirement is divided by the length of the window in minutes, so a deviation too small
    /// to act on after one minute becomes actionable after several. Deliberately *not* clamped at
    /// one minute: a shorter window then yields a proportionally higher threshold, which is what
    /// makes a separate minimum-interval guard unnecessary. A one-second window demands roughly
    /// 9000%, so noise measured over a moment cannot move the difficulty.
    fn dense_threshold(dt_secs: u64, shares_per_minute: f32) -> f64 {
        let window_minutes = dt_secs as f64 / 60.0;
        if window_minutes <= 0.0 {
            return f64::INFINITY;
        }
        let spm_factor = (shares_per_minute as f64 / REFERENCE_SPM).sqrt();
        (EVIDENCE_AT_ONE_MINUTE * spm_factor / window_minutes + MIN_THRESHOLD_FRACTION) * 100.0
    }

    /// Moves the belief part of the way to the estimate, further on each repeat in one direction.
    ///
    /// Returns the new belief. Records the retarget's direction, so a run of same-direction moves
    /// accelerates and a reversal starts over.
    fn partial_retarget(&self, estimate: f32, current: f32, fire_run: u32) -> f32 {
        let step_fraction = (STEP_FRACTION_BASE
            + STEP_FRACTION_GROWTH * (fire_run.saturating_sub(1)) as f32)
            .min(STEP_FRACTION_MAX);
        current + step_fraction * (estimate - current)
    }

    /// What the retarget run would become if this evaluation's move reaches the wire.
    ///
    /// Read-only, and separate from [`VardiffState::commit_fire`] for a reason the step fraction
    /// depends on. The fraction has to be known *before* the move is computed, but whether the move
    /// counts as a retarget is only known *after* it has survived the floors and clamps below — an
    /// evaluation whose whole move is absorbed sends nothing and has not demonstrated that its steps
    /// are too small. Mutating here would ramp the fraction on moves that never happened: a channel
    /// pinned at `min_allowed_hashrate` or at the silence floor re-enters this path on every
    /// evaluation, and the accumulated run would then be spent on the first move that did land.
    fn prospective_fire_run(&self, tightening: bool) -> u32 {
        let direction: i8 = if tightening { 1 } else { -1 };
        if direction == self.last_fire_direction {
            (self.consecutive_fires_same_direction + 1).min(RUN_AT_MAX_STEP_FRACTION)
        } else {
            1
        }
    }

    /// Records a retarget that reached the wire, advancing or restarting the run.
    fn commit_fire(&mut self, tightening: bool) {
        let direction: i8 = if tightening { 1 } else { -1 };
        self.last_fire_direction = direction;
        self.consecutive_fires_same_direction = self.prospective_fire_run(tightening);
    }

    /// Rescales the EWMA after a retarget so its stored rate refers to the new difficulty.
    ///
    /// `rate` counts shares against whatever target was in force when they were found, so it is a
    /// difficulty-relative quantity. Retargeting changes that reference: after moving the belief by
    /// `ratio`, the same physical hashrate produces `rate / ratio` shares per period. Without this
    /// the next evaluation would compare pre-retarget evidence against a post-retarget target and
    /// read a deviation that is an artefact of the controller's own action.
    fn rescale_ewma(&mut self, new_hashrate: f32, old_hashrate: f32) {
        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.rate = 0.0;
            self.rate_seeded = false;
            return;
        }
        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.rate /= ratio;
        } else {
            self.rate = 0.0;
            self.rate_seeded = false;
        }
    }
}

impl Vardiff for VardiffState {
    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.shares_since_last_update
    }

    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    /// Sets the timestamp of the last update.
    fn set_timestamp_of_last_update(&mut self, timestamp_of_last_update: u64) {
        self.timestamp_of_last_update = timestamp_of_last_update;
        // Re-anchoring the decision window re-anchors the estimator's clock with it. Leaving the
        // two references to disagree would let the estimator measure its pending shares against a
        // different interval than the one the caller just declared, which reads as a rate error
        // proportional to the gap between them.
        self.timestamp_of_last_evaluation = timestamp_of_last_update;
    }

    /// Increments the share counter by one.
    fn increment_shares_since_last_update(&mut self) {
        self.shares_since_last_update += 1;
    }

    /// Resets the share counter and updates the timestamp to now.
    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        let timestamp_secs = self.clock.now_secs()?;
        self.set_timestamp_of_last_update(timestamp_secs);
        self.set_shares_since_last_update(0);
        // The EWMA is discarded rather than rescaled here: `reset_counter` is the "start over"
        // path (a backwards clock step), where there is no difficulty ratio to rescale by.
        // Rescaling belongs to `rescale_ewma`, on the retarget path.
        self.rate = 0.0;
        self.rate_seeded = false;
        self.timestamp_of_last_evaluation = timestamp_secs;
        // Also clear both same-direction runs. A stale count would carry its accumulated discount
        // into the next cycle and lower the bar on evidence that no longer exists — and because the
        // discount applies to tightening too, it would erode the very reluctance the asymmetry
        // provides. The retarget run is cleared for the same reason: a stale one would let the next
        // move be larger than a first move should be.
        self.last_direction = 0;
        self.consecutive_same_direction = 0;
        self.last_fire_direction = 0;
        self.consecutive_fires_same_direction = 0;
        Ok(())
    }

    /// Checks channel performance and potentially updates the hashrate and target.
    ///
    /// It calculates the realized share rate since the last update. If the
    /// deviation from the target rate is significant enough (based on internal,
    /// time-sensitive thresholds), it estimates a new hashrate and applies it.
    ///
    /// It returns `Ok(Some(new_hashrate))` when an update occurs,
    /// `Ok(None)` when conditions don't warrant an update, and
    /// `Err` for actual processing errors.
    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        let now = self.clock.now_secs()?;

        let delta_time = match now.checked_sub(self.timestamp_of_last_update) {
            Some(delta_time) => delta_time,
            None => {
                // The clock stepped backwards (e.g. NTP correction), leaving the recorded
                // timestamp in the future, so elapsed time is unmeasurable. Re-anchor the
                // window to `now` and skip just this round: the `delta_time <= 15` guard
                // below returns before `reset_counter()` runs, so without re-anchoring here
                // every later round would measure against the same stale future timestamp
                // and vardiff would stall for the whole length of the backwards step.
                debug!(
                    target: "vardiff",
                    "Clock stepped backwards (recorded {}, now {}); re-anchoring vardiff window",
                    self.timestamp_of_last_update,
                    now
                );
                self.reset_counter()?;
                return Ok(None);
            }
        };

        if delta_time <= 15 {
            return Ok(None);
        }

        // `min_allowed_hashrate` is validated in `new_with_min`, but the field is `pub`,
        // so direct assignment can still plant a non-finite or non-positive floor;
        // sanitize it here as well before relying on it.
        let min_hashrate =
            if self.min_allowed_hashrate.is_finite() && self.min_allowed_hashrate > 0.0 {
                self.min_allowed_hashrate
            } else {
                DEFAULT_MIN_HASHRATE
            };

        // `hashrate` is the channel's nominal hashrate, which originates from the
        // miner-supplied `nominal_hash_rate` and is only checked for negativity upstream.
        // It is the divisor for the delta percentage below and the base every special-case
        // cap scales (`hashrate * 10.0`, `hashrate / 1.5`), so a value at or below the floor
        // just pins the result back to that floor instead of converging, and `0.0`/`NaN`
        // produce `inf`/`NaN` percentages that disable every `should_update` arm outright.
        // The output is already clamped to the floor below, so clamp the baseline to the
        // same floor. `is_finite` is still needed: `inf` survives a plain `max()` and makes
        // the percentage `inf / inf == NaN`.
        let hashrate = if hashrate.is_finite() {
            hashrate.max(min_hashrate)
        } else {
            debug!(
                target: "vardiff",
                "Prior hashrate {hashrate} unusable; using minimum {min_hashrate}",
            );
            min_hashrate
        };

        // Interval the estimator measures over: since the previous evaluation, not since the
        // previous retarget. Saturates rather than wrapping if the clock stepped backwards between
        // the two references; `delta_time` above already handles that case for the window.
        let eval_dt = now.saturating_sub(self.timestamp_of_last_evaluation).max(1);
        self.timestamp_of_last_evaluation = now;

        // Anchor the silence bound before the estimator consumes the pending count. Evidence
        // re-anchors it to the belief that evidence was measured against; an unanchored channel
        // anchors to its opening belief, so a channel that is silent from the start is bounded
        // relative to where it started rather than being allowed to walk to the floor.
        //
        // The anchor is the belief *entering* this evaluation, not the one leaving it, so after a
        // retarget the floor still refers to the previous belief and lags by one fire. Deliberate:
        // it keeps the anchor to a value some evidence was actually measured against, where the
        // post-retarget belief is a value the controller has only just proposed.
        let pending_shares = self.shares_since_last_update;
        if pending_shares > 0 || self.evidenced_hashrate <= 0.0 {
            self.evidenced_hashrate = hashrate;
        }

        let (realized_share_per_min, estimated_hashrate) =
            self.estimate(eval_dt, hashrate, target, shares_per_minute);

        debug!(
            target: "vardiff",
            "Hashrate update check triggered:
            - Elapsed time: {}s
            - Shares since last update: {}
            - Realized shares per minute: {:.4}
            - Current miner target: {:?}",
            delta_time,
            pending_shares,
            realized_share_per_min,
            target
        );

        let mut new_hashrate = estimated_hashrate;

        let hashrate_delta = new_hashrate - hashrate;
        let hashrate_delta_percentage = (hashrate_delta.abs() / hashrate) * 100.0;

        debug!(
            target: "vardiff",
            "Calculated new hashrate: {:.2} H/s (Δ {:.2}%, previous {:.2} H/s)",
            new_hashrate,
            hashrate_delta_percentage,
            hashrate,
        );

        let tightening = estimated_hashrate > hashrate;
        let same_direction_run = self.note_direction(tightening);
        let threshold = self.threshold(
            delta_time,
            shares_per_minute,
            tightening,
            same_direction_run,
        );

        debug!(
            target: "vardiff",
            "Deviation {:.2}% against a threshold of {:.2}% over {}s",
            hashrate_delta_percentage,
            threshold,
            delta_time,
        );

        if (hashrate_delta_percentage as f64) < threshold {
            return Ok(None);
        }

        // The `÷1.5, ÷2, ÷3` zero-share arm is gone with the ladder that made it necessary. It
        // existed because the old `>= 100.0` rung fired on a zero estimate, which would otherwise
        // have driven the belief straight at `min_allowed_hashrate`.
        //
        // The threshold above mostly covers that now, but not everywhere, and the exception is
        // worth stating precisely. A zero estimate is a deviation of exactly 100%, so it fires
        // wherever the threshold sits below that. At one evaluation period the dense threshold is
        // 155% *at 30 shares a minute*, the rate `EVIDENCE_AT_ONE_MINUTE` is calibrated for, and it
        // scales as `sqrt(spm / REFERENCE_SPM)` — so it crosses 100% at about 12.4 shares a minute,
        // and below `SPARSE_SPM_SEAM` the sparse branch raises the bar out of reach again. Measured
        // at a 61-second window, a zero-share evaluation fires for `6 <= spm < 12.4` and nowhere
        // else, a band that contains this crate's own test rate of 10.
        //
        // When it does fire, `MAX_STEP_RATIO` bounds the single step and the displacement floor
        // below still pins where the descent settles — the same `9x` the arm was bounded to.
        // Move part of the way rather than all of it. The run this reads is *prospective*: it is
        // what the run becomes if this move reaches the wire, and it is committed below only once
        // that is known. See `prospective_fire_run` for why the two cannot be one step.
        let fire_run = self.prospective_fire_run(tightening);
        new_hashrate = self.partial_retarget(new_hashrate, hashrate, fire_run);

        // Bound the move itself, in both directions. One window of evidence does not warrant an
        // unbounded change of belief, however large the deviation it reports.
        let bounded = new_hashrate.clamp(hashrate / MAX_STEP_RATIO, hashrate * MAX_STEP_RATIO);
        if bounded != new_hashrate {
            debug!(
                target: "vardiff",
                "Move bounded to {:.0}x per evaluation: {:.2} -> {:.2} H/s",
                MAX_STEP_RATIO,
                new_hashrate,
                bounded,
            );
            new_hashrate = bounded;
        }

        // Bound the ease on absent evidence. Applied as a floor on the value rather than as a gate
        // on the decision, which is what makes the bound exact: a gate lets the step that crosses
        // it through, so the realized displacement depends on the step size, while a floor pins it
        // at `MAX_SILENT_DISPLACEMENT` for any estimator. Only downward movement is constrained,
        // and only while `evidenced_hashrate` is stale — any share re-anchors it above.
        if pending_shares == 0 && self.evidenced_hashrate > 0.0 {
            let silent_floor = self.evidenced_hashrate / MAX_SILENT_DISPLACEMENT;
            if new_hashrate < silent_floor {
                debug!(
                    target: "vardiff",
                    "Ease bounded at {:.0}x below the last evidenced belief: {:.2} -> {:.2} H/s",
                    MAX_SILENT_DISPLACEMENT,
                    new_hashrate,
                    silent_floor,
                );
                new_hashrate = silent_floor;
            }
        }

        if new_hashrate < min_hashrate {
            debug!(
                target: "vardiff",
                "New hashrate {:.2} H/s below minimum threshold {:.2} H/s — clamping",
                new_hashrate,
                min_hashrate
            );
            new_hashrate = min_hashrate;
        }

        // Re-anchor the evaluation window directly rather than through `reset_counter`, which also
        // discards the EWMA. The estimator's memory must survive a retarget — rescaled, not
        // dropped — or the controller would forget its evidence every time it acted, which is the
        // coupling this patch exists to remove. `estimate` has already consumed the share count.
        //
        // Nothing left to say when the bound absorbed the whole move: stay off the wire rather
        // than resending the difficulty the channel already has.
        if new_hashrate == hashrate {
            self.set_timestamp_of_last_update(now);
            return Ok(None);
        }

        // Past every floor and clamp, so this move is going out. Only now does it count toward the
        // run that sizes the next step.
        self.commit_fire(tightening);
        self.rescale_ewma(new_hashrate, hashrate);
        self.set_timestamp_of_last_update(now);

        Ok(Some(new_hashrate))
    }
}
