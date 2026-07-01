//! The vardiff-survey summary figure — a two-gate DECISION TREE (NOT a radar).
//! Spec + acceptance test: docs/records/VARDIFF_SURVEY.md "SUMMARY FIGURE SPEC".
//!
//! Why a tree, not a radar: the survey's finding is CATEGORICAL (two binary
//! structural gates), and harms are mixed measured/unmeasured. A radar would seat
//! the deadband's measured ~5× and the share-driven family's unmeasured
//! lag-variance on one spoke at equal weight — the value-vs-domain trap as a
//! picture (the killed regret_radar). A taxonomy wants a structure diagram.
//!
//! ACCEPTANCE TEST (must not lie about the unmeasured leaf in EITHER direction):
//!   1. share-driven leaf must NOT read as carrying the deadband's measured ~5×.
//!   2. share-driven leaf must NOT read as LOWER-harm — only as UNMEASURED. The
//!      hatch must say "we don't have the number," not "the number is small."
//!      Beaten here by: explicit label doing the work + a TAIL-SEVERITY cue (the
//!      rare disconnect is the corpus's worst outcome) so the leaf doesn't collapse
//!      to "mild," and by NOT making the hatch a faint/small swatch.
//!
//! Usage: cargo run --release --bin safety-tree
//! Env: VARDIFF_ST_OUT (default docs/figures/safety_tree.svg)

use std::env;

fn main() {
    let out = env::var("VARDIFF_ST_OUT")
        .unwrap_or_else(|_| "docs/figures/safety_tree.svg".to_string());

    let (w, h) = (1240.0f64, 720.0f64);
    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" font-family=\"system-ui, sans-serif\" font-size=\"13\">\n"
    ));
    // hatch pattern for the UNMEASURED leaf — deliberately NOT faint: full-weight
    // strokes in the danger hue, so it does not read as "less harm" (clause 2).
    s.push_str(
        "<defs>\
         <pattern id=\"unmeas\" width=\"11\" height=\"11\" patternUnits=\"userSpaceOnUse\" \
         patternTransform=\"rotate(45)\"><rect width=\"11\" height=\"11\" fill=\"#fbeae7\"/>\
         <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"11\" stroke=\"#c0392b\" stroke-width=\"1.4\" \
         stroke-opacity=\"0.85\"/></pattern>\
         <marker id=\"arr\" markerWidth=\"8\" markerHeight=\"8\" refX=\"6\" refY=\"4\" orient=\"auto\">\
         <path d=\"M0,0 L7,4 L0,8 Z\" fill=\"#7a7f8a\"/></marker>\
         </defs>\n",
    );

    // title + subtitle
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"30\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" \
         fill=\"#222\">Decline-safety is decided by two structural properties — not by sophistication</text>\n",
        w / 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"50\" text-anchor=\"middle\" font-size=\"11.5\" fill=\"#888\" \
         font-style=\"italic\">safety-tree.rs (generated). A taxonomy, not a metric plot: two yes/no gates sort every controller. \
         Harm shown ONLY where measured.</text>\n",
        w / 2.0
    ));

    // ---- helper closures via inline format; layout coordinates ----
    // Gate 1 diamond (top center-ish)
    let g1x = 300.0;
    let g1y = 110.0;
    let diamond = |cx: f64, cy: f64, hw: f64, hh: f64, fill: &str| {
        format!(
            "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"{fill}\" \
             stroke=\"#566\" stroke-width=\"1.5\"/>\n",
            cx, cy - hh, cx + hw, cy, cx, cy + hh, cx - hw, cy
        )
    };

    // GATE 1
    s.push_str(&diamond(g1x, g1y, 130.0, 52.0, "#eef2f6"));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12.5\" font-weight=\"bold\" fill=\"#2c3e50\">GATE 1</text>\n",
        g1x, g1y - 16.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#2c3e50\">eases without a</text>\n",
        g1x, g1y + 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#2c3e50\">fresh share? (idle path)</text>\n",
        g1x, g1y + 18.0
    ));

    // GATE 1 -> NO (left-down to share-driven leaf).
    // sdy chosen so the share-driven leaf's TOP aligns with the deadband/safe
    // leaves (top at 390): with lh=188, center = 390 + 188/2 = 484. The leaf is
    // taller (more text) so its bottom sits lower, but the three bottom-row tops
    // share a baseline — aligned tops, deliberate height variance.
    let sdx = 240.0;
    let sdy = 484.0;
    s.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#7a7f8a\" stroke-width=\"1.6\" marker-end=\"url(#arr)\"/>\n",
        g1x - 95.0, g1y + 30.0, sdx, sdy - 92.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" font-weight=\"bold\" fill=\"#c0392b\">NO</text>\n",
        g1x - 120.0, g1y + 80.0
    ));

    // GATE 1 -> YES (right-down to gate 2)
    let g2x = 640.0;
    let g2y = 250.0;
    s.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#7a7f8a\" stroke-width=\"1.6\" marker-end=\"url(#arr)\"/>\n",
        g1x + 110.0, g1y + 26.0, g2x - 120.0, g2y - 30.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" font-weight=\"bold\" fill=\"#1e8449\">YES</text>\n",
        g1x + 150.0, g1y + 40.0
    ));

    // GATE 2
    s.push_str(&diamond(g2x, g2y, 135.0, 52.0, "#eef2f6"));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12.5\" font-weight=\"bold\" fill=\"#2c3e50\">GATE 2</text>\n",
        g2x, g2y - 16.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#2c3e50\">ease trigger reachable</text>\n",
        g2x, g2y + 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#2c3e50\">on the modal decline?</text>\n",
        g2x, g2y + 18.0
    ));

    // GATE 2 -> NO (down-left to deadband leaf).
    // Bottom-row leaves share top baseline 390 AND equal height (dbh=188), with
    // even ~55px horizontal gaps: share-driven 95-385, deadband 440-690,
    // safe 745-975. Centers: 240 / 565 / 860.
    let dbx = 565.0;
    let dby = 484.0;
    s.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#7a7f8a\" stroke-width=\"1.6\" marker-end=\"url(#arr)\"/>\n",
        g2x - 70.0, g2y + 40.0, dbx, dby - 92.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" font-weight=\"bold\" fill=\"#c0392b\">NO</text>\n",
        g2x - 110.0, g2y + 90.0
    ));

    // GATE 2 -> YES (down-right to safe leaf). Same baseline + height as the row.
    let safex = 860.0;
    let safey = 484.0;
    s.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#7a7f8a\" stroke-width=\"1.6\" marker-end=\"url(#arr)\"/>\n",
        g2x + 70.0, g2y + 40.0, safex, safey - 92.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" font-weight=\"bold\" fill=\"#1e8449\">YES</text>\n",
        g2x + 130.0, g2y + 90.0
    ));

    // also a "YES" arrow from gate1->escapes leaf path: MiningCore escapes at gate 1
    // (YES) without needing gate 2 — show escapes-via-timer leaf hanging off the
    // gate1-YES branch. Place it under gate 2's right, labeled distinctly.
    let escx = 900.0;
    let escy = 250.0;
    s.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#7a7f8a\" stroke-width=\"1.3\" stroke-dasharray=\"4 3\" marker-end=\"url(#arr)\"/>\n",
        g2x + 130.0, g2y - 8.0, escx - 78.0, escy
    ));

    // ---- LEAF renderer ----
    let leaf = |x: f64, y: f64, ww: f64, hh: f64, fill: &str, stroke: &str| {
        format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"8\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"1.6\"/>\n",
            x - ww / 2.0, y - hh / 2.0, ww, hh
        )
    };

    // LEAF: share-driven (gate-1 NO) — UNMEASURED, the hard leaf.
    let lw = 290.0;
    let lh = 188.0;
    s.push_str(&leaf(sdx, sdy, lw, lh, "url(#unmeas)", "#c0392b"));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12.5\" font-weight=\"bold\" fill=\"#922b21\">share-driven, no idle path</text>\n",
        sdx, sdy - lh / 2.0 + 20.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#922b21\">NOMP + forks · ckpool · classic SMA</text>\n",
        sdx, sdy - lh / 2.0 + 36.0
    ));
    // explicit magnitude-status label (clause 2: the LABEL does the work, not the shade)
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"66\" rx=\"5\" fill=\"#ffffff\" fill-opacity=\"0.94\" stroke=\"#c0392b\" stroke-width=\"1\"/>\n",
        sdx - lw / 2.0 + 12.0, sdy - 24.0, lw - 24.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11.5\" font-weight=\"bold\" fill=\"#922b21\">harm magnitude: UNMEASURED</text>\n",
        sdx, sdy - 8.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#5a3530\">common case: gradual lag (size unknown)</text>\n",
        sdx, sdy + 7.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" font-weight=\"bold\" fill=\"#922b21\">tail: rare — but a DROPPED CONNECTION (severe)</text>\n",
        sdx, sdy + 22.0
    ));
    // fix tag (gate-1 = easy)
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" font-weight=\"bold\" fill=\"#1e8449\">FIX: easy — port MiningCore's idle ease-path</text>\n",
        sdx, sdy + lh / 2.0 - 10.0
    ));

    // LEAF: deadband (gate-2 NO) — MEASURED ~5×, the easy leaf (solid).
    // Equal height to the share-driven leaf (lh=188) so the bottom-row boxes align
    // top AND bottom — a clean row, not staggered.
    let dbh = 188.0;
    s.push_str(&leaf(dbx, dby, 250.0, dbh, "#f4c9c2", "#c0392b"));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12.5\" font-weight=\"bold\" fill=\"#922b21\">deadband, unreachable trigger</text>\n",
        dbx, dby - dbh / 2.0 + 22.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#922b21\">DMND · current MARA production</text>\n",
        dbx, dby - dbh / 2.0 + 38.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"22\" font-weight=\"bold\" fill=\"#c0392b\">~5× variance</text>\n",
        dbx, dby + 4.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#5a3530\">MEASURED (1/(1−d)); fails on depth alone</text>\n",
        dbx, dby + 22.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" font-weight=\"bold\" fill=\"#b9690e\">FIX: hard — narrow deadband / rework pow2</text>\n",
        dbx, dby + dbh / 2.0 - 12.0
    ));

    // LEAF: escapes-via-timer (gate-1 YES, MiningCore)
    s.push_str(&leaf(escx, escy, 210.0, 88.0, "#eafaf1", "#1e8449"));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" font-weight=\"bold\" fill=\"#1e7a45\">escapes via idle timer</text>\n",
        escx, escy - 22.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#1e7a45\">MiningCore (IdleUpdate)</text>\n",
        escx, escy - 6.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#46604f\">has the escaping mechanism —</text>\n",
        escx, escy + 12.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#46604f\">NOT verified-optimal (unmeasured)</text>\n",
        escx, escy + 26.0
    ));

    // LEAF: safe-by-construction (gate-2 YES, champion)
    s.push_str(&leaf(safex, safey, 230.0, dbh, "#d8f0e2", "#1e8449"));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12.5\" font-weight=\"bold\" fill=\"#1e7a45\">safe by construction</text>\n",
        safex, safey - dbh / 2.0 + 22.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#1e7a45\">the champion (EWMA + asym. seq. test)</text>\n",
        safex, safey - dbh / 2.0 + 40.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" fill=\"#1e8449\">near-floor</text>\n",
        safex, safey + 4.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#46604f\">decline-safe as a hard minimax constraint</text>\n",
        safex, safey + 22.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" font-weight=\"bold\" fill=\"#1e8449\">no fix needed</text>\n",
        safex, safey + dbh / 2.0 - 12.0
    ));

    // bottom caption — the mechanism-not-deploy caveat (governs the whole figure)
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" fill=\"#555\">\
         \"FIX easy/hard\" = effort to make decline-safe + whether a working example exists — NOT a deploy recommendation. \
         Easing still costs responsiveness/churn each pool weighs itself.</text>\n",
        w / 2.0, h - 38.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#777\" font-style=\"italic\">\
         The hatched leaf's harm is UNMEASURED in size (not small): its rare tail is the corpus's worst outcome. \
         The solid leaf's ~5× is the one magnitude measured.</text>\n",
        w / 2.0, h - 20.0
    ));

    s.push_str("</svg>\n");
    match std::fs::write(&out, s) {
        Ok(()) => eprintln!("wrote figure: {out}"),
        Err(e) => eprintln!("WARN: could not write {out}: {e}"),
    }
}
