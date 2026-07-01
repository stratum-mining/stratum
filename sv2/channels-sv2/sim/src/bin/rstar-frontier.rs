//! The r* optimization frame — the capstone figure (info-floor §8.6).
//!
//! NOT a recommended rate. A FRAME: raising r* lowers the variance floor as
//! `1/√(r*·τ)` (benefit, √-flattening — pure benefit, diminishing RETURNS) while
//! bandwidth + pool-side processing rise as `r*·N` (cost, linear). Benefit grows
//! like √r*, cost like r* — a well-posed trade with a knee. But the knee is a
//! DEPLOYMENT decision (the variance↔bandwidth exchange rate is pool-specific:
//! payout scheme, miner size, bandwidth headroom), not a universal number — the
//! same reason the harm was four currencies, not one (§6).
//!
//! What IS universal is the bracket, in four regions on the r* axis:
//!   - below ~2–4 spm: CONTROL FLOOR — decline-safety itself degrades (the
//!     sub-guard cells, §9.2). The genuine "don't go here," control-limited.
//!   - ~4–10 spm: SAFE + tracking, but DETECTION-saturated (blind to small drops,
//!     §5). A knowing trade-off — production lives here (4–6 spm, §9.4).
//!   - ~10–30 spm: full diagnostic visibility; the KNEE lives in here, pool-chosen.
//!   - above ~30 spm: √-returns spent — paying linearly for ~nothing.
//!
//! The figure is PURE CLOSED FORM (no sim run): both curves are analytic, this
//! plots them against each other and marks the band. The y-axis is RELATIVE
//! (shape, not magnitude) on purpose — a frame, not a number. CRITICAL: the curve
//! crossing is NOT the knee (that would mix the two currencies); the knee is
//! where MARGINAL floor-reduction stops being worth MARGINAL bandwidth, which is
//! the pool's exchange rate. The figure marks the region, never a point.
//!
//! Usage: cargo run --release --bin rstar-frontier
//! Env: VARDIFF_RF_OUT (default docs/figures/rstar_frontier.svg), VARDIFF_RF_TAU
//!      (illustrative window in minutes for the floor curve, default 6 = champion).

use std::env;

fn main() {
    let out = env::var("VARDIFF_RF_OUT")
        .unwrap_or_else(|_| "docs/figures/rstar_frontier.svg".to_string());
    let tau: f64 = env::var("VARDIFF_RF_TAU").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);

    let (w, h) = (920.0f64, 560.0f64);
    let (ml, mt) = (74.0f64, 76.0f64);
    let (pw, ph) = (770.0f64, 392.0f64);
    let (rmin, rmax) = (1.0f64, 100.0f64);

    // log-x for r*; y in [0,1] relative.
    let lx = |r: f64| ml + pw * (r.ln() - rmin.ln()) / (rmax.ln() - rmin.ln());
    let ly = |v: f64| mt + ph * (1.0 - v.clamp(0.0, 1.0));

    // benefit = variance-floor WIDTH 1/√(r*·τ), normalized to its value at r*=1
    // (so floor_norm = 1/√r*, independent of τ for the SHAPE — τ only sets the
    // absolute scale we deliberately suppress). Falls fast, then flattens.
    let floor_norm = |r: f64| (1.0 / (r * tau)).sqrt() / (1.0 / (rmin * tau)).sqrt();
    // cost = bandwidth ∝ r*, normalized to r*=rmax. Linear.
    let cost_norm = |r: f64| r / rmax;

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         font-family=\"system-ui, sans-serif\" font-size=\"13\">\n"
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"24\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" \
         fill=\"#222\">Where to set r*: a frame, not a number — √-benefit vs linear cost, and the band that brackets the knee</text>\n",
        w / 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"43\" text-anchor=\"middle\" font-size=\"11\" fill=\"#888\" \
         font-style=\"italic\">rstar-frontier.rs (generated, closed form). Benefit = floor width 1/√(r*·τ); cost = bandwidth ∝ r*. \
         y-axis relative (shape, not magnitude). The knee is the pool's call, NOT the crossing.</text>\n",
        w / 2.0
    ));

    // four regions (vertical bands).
    let regions: [(f64, f64, &str, &str); 4] = [
        (rmin, 3.0, "#f9d6d5", "control floor"),
        (3.0, 10.0, "#fdebd0", "safe · blind"),
        (10.0, 30.0, "#eafaf1", "knee region"),
        (30.0, rmax, "#eceff1", "returns spent"),
    ];
    for (r0, r1, fill, _lab) in regions {
        s.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{mt:.1}\" width=\"{:.1}\" height=\"{ph:.1}\" fill=\"{fill}\"/>\n",
            lx(r0), lx(r1) - lx(r0)
        ));
    }

    // region labels (top, inside band).
    let rlabels: [(f64, &str, &str, &str); 4] = [
        (1.7, "control floor", "~2–4 spm", "#a93226"),
        (5.5, "safe · tracking · blind", "~4–10 spm", "#a85c00"),
        (17.0, "full visibility", "~10–30 spm", "#1e8449"),
        (50.0, "√-returns spent", ">~30 spm", "#5d6d7e"),
    ];
    for (rc, top, sub, col) in rlabels {
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" \
             font-weight=\"bold\" fill=\"{col}\">{top}</text>\n",
            lx(rc), mt + 16.0
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"{col}\">{sub}</text>\n",
            lx(rc), mt + 30.0
        ));
    }

    // detection-saturation line at ~10 spm (annotation INSIDE, the soft threshold).
    s.push_str(&format!(
        "<line x1=\"{0:.1}\" y1=\"{mt:.1}\" x2=\"{0:.1}\" y2=\"{1:.1}\" stroke=\"#e67e22\" \
         stroke-width=\"1.4\" stroke-dasharray=\"5 3\"/>\n",
        lx(10.0), mt + ph
    ));
    // production band 4–6 spm marker (where production actually runs, §9.4).
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#2c3e50\" \
         fill-opacity=\"0.55\"/>\n",
        lx(4.0), mt + ph - 16.0, lx(6.0) - lx(4.0)
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"9\" fill=\"#2c3e50\" \
         font-weight=\"bold\">production runs here (4–6 spm, §9.4) — below ~10: safe but diagnostically blind, a knowing trade</text>\n",
        lx(4.0), mt + ph - 24.0
    ));

    // benefit curve (floor width — falling, flattening). Blue.
    let mut bpts = String::new();
    let steps = 120;
    for k in 0..=steps {
        let r = (rmin.ln() + (rmax.ln() - rmin.ln()) * k as f64 / steps as f64).exp();
        bpts.push_str(&format!("{:.1},{:.1} ", lx(r), ly(floor_norm(r))));
    }
    s.push_str(&format!(
        "<polyline points=\"{bpts}\" fill=\"none\" stroke=\"#2980b9\" stroke-width=\"2.6\"/>\n"
    ));
    // cost curve (bandwidth — linear). Red, lighter weight (secondary).
    let mut cpts = String::new();
    for k in 0..=steps {
        let r = (rmin.ln() + (rmax.ln() - rmin.ln()) * k as f64 / steps as f64).exp();
        cpts.push_str(&format!("{:.1},{:.1} ", lx(r), ly(cost_norm(r))));
    }
    s.push_str(&format!(
        "<polyline points=\"{cpts}\" fill=\"none\" stroke=\"#c0392b\" stroke-width=\"2.0\" \
         stroke-dasharray=\"2 2\"/>\n"
    ));

    // curve labels.
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"11\" font-weight=\"bold\" \
         fill=\"#1f618d\">benefit: variance floor 1/√(r*·τ) — falls fast, then flattens (√)</text>\n",
        lx(11.0), ly(floor_norm(11.0)) - 8.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"11\" font-weight=\"bold\" \
         fill=\"#c0392b\">cost: bandwidth ∝ r* — linear, never flattens</text>\n",
        lx(60.0), ly(cost_norm(60.0)) - 6.0
    ));

    // frame + x ticks.
    s.push_str(&format!(
        "<rect x=\"{ml:.1}\" y=\"{mt:.1}\" width=\"{pw:.1}\" height=\"{ph:.1}\" fill=\"none\" \
         stroke=\"#999\"/>\n"
    ));
    for r in [1.0, 2.0, 4.0, 6.0, 10.0, 20.0, 30.0, 50.0, 100.0] {
        let x = lx(r);
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9\" fill=\"#666\">{}</text>\n",
            mt + ph + 14.0, r as u32
        ));
    }
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#444\">\
         target share rate r* (shares/min, log)</text>\n",
        ml + pw / 2.0, mt + ph + 32.0
    ));
    s.push_str(&format!(
        "<text x=\"22\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#444\" \
         transform=\"rotate(-90 22 {:.1})\">relative magnitude (shape, not absolute)</text>\n",
        mt + ph / 2.0, mt + ph / 2.0
    ));

    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" fill=\"#555\">\
         The band is universal: below ~2–4 control breaks, above ~30 returns are spent. The KNEE inside ~10–30 is \
         the pool's call — its variance↔bandwidth exchange rate, NOT the curve crossing.</text>\n",
        w / 2.0, h - 16.0
    ));

    s.push_str("</svg>\n");
    match std::fs::write(&out, s) {
        Ok(()) => eprintln!("wrote figure: {out}"),
        Err(e) => eprintln!("WARN: could not write {out}: {e}"),
    }
}
