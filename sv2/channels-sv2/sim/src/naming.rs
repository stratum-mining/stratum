//! Drift-proof algorithm naming derived from the three composed parts.
//!
//! A composed algorithm's name is built from the `code()` of each of its
//! three stages ([`Estimator`], [`Boundary`], [`UpdateRule`]). Because
//! each `code()` is computed from the concrete type's live parameters,
//! the name can never drift away from what the algorithm actually does.
//!
//! The display name uses `" / "` separators (e.g.
//! `"Ewma120s / Poisson-z2.58 / Partial-e0.2"`); [`sanitize_filename`]
//! converts it to a filesystem-safe form for `baseline_{name}.toml`.

use crate::composed::{Boundary, Estimator, UpdateRule};

/// Display name: `"Estimator / Boundary / Update"`.
pub fn triple_name(e: &dyn Estimator, b: &dyn Boundary, u: &dyn UpdateRule) -> String {
    format!("{} / {} / {}", e.code(), b.code(), u.code())
}

/// Filesystem-safe variant of a display name: `" / "` → `"__"`, spaces
/// removed, and the monolith marker `'*'` mapped to `"-monolith"` so the
/// `VardiffState` monolith and `ClassicComposed` produce different
/// filenames even though they are fire-equivalent.
pub fn sanitize_filename(display_name: &str) -> String {
    display_name
        .replace(" / ", "__")
        .replace(' ', "")
        .replace('*', "-monolith")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composed::{
        CumulativeCounter, EwmaEstimator, FullRetargetWithClamp, PartialRetarget, PoissonCI,
        StepFunction,
    };

    #[test]
    fn triple_name_full_remedy() {
        let name = triple_name(
            &EwmaEstimator::new(120),
            &PoissonCI::default_parametric(),
            &PartialRetarget::new(0.2),
        );
        assert_eq!(name, "Ewma120s / Poisson-z2.58 / Partial-e0.2");
    }

    #[test]
    fn sanitize_replaces_separators_and_spaces() {
        assert_eq!(
            sanitize_filename("Ewma120s / Poisson-z2.58 / Partial-e0.2"),
            "Ewma120s__Poisson-z2.58__Partial-e0.2"
        );
    }

    #[test]
    fn sanitize_monolith_marker_differs_from_composed() {
        let composed = triple_name(
            &CumulativeCounter::new(),
            &StepFunction::classic_table(),
            &FullRetargetWithClamp::classic(),
        );
        let monolith = format!("{composed}*");
        assert_ne!(
            sanitize_filename(&composed),
            sanitize_filename(&monolith),
            "monolith and composed must produce different filenames"
        );
        assert!(sanitize_filename(&monolith).ends_with("-monolith"));
    }
}
