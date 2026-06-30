#!/usr/bin/env python3
"""Claim-warrant validator — the four cheap hard gates, plus the three structural
rules the thread's catches turned into schema.

Routing IS the win: every claim carries a warrant_type, so it routes to its own
check and nothing ships unrouted. The integrity test the whole design serves: a
claim and its warrant must disagree LOUDLY — the gate fails on its own, with no
human electing to look.

Gates (string/hash, per-commit, near-free):
  1. source / source-private : re-read the pinned lines, CONTENT-match (not mere
     existence — the safe-by-accident catch was a pin that said the opposite).
  2. socket                  : socket tag present AND its 'conditional' status
     propagates to every downstream claim (no-smuggled-default as a graph property).
  3. cross-doc               : resolves + is_about present + load_bearing_type
     honest FOR THE CORPUS BOUNDARY (supports may cross docs in-corpus, never
     cross-corpus; see-also may cross corpus but carries no weight).
  4. execution               : domain present and well-formed (full re-run is the
     medium-cost gate on a code-change trigger; here we hard-gate that the
     load-bearing DOMAIN fields exist, since the EXCESS catch was a dropped domain,
     not a wrong value).

Structural rules:
  - UNDECIDED  : BLOCKS. Cannot be marked green by any path. Routes to its owner.
                 This is the state whose ABSENCE let an earlier draft close an open
                 question by default. Its presence is correct, not a failure to fix.
  - judgment   : REFUSED for validation. Marked, never checked. But depends_on is
                 tracked: a DRIFTED input flags BASIS-CHANGED (re-examine), which is
                 NOT 'judgment invalidated' (category error).
  - cross-corpus canonical-object trigger uses cited_by>1 as a KNOWN-LAGGING proxy
                 for cross-corpus-referenceability (warns, does not gate).

Exit non-zero if any gate fails OR any UNDECIDED claim is present (an UNDECIDED
claim is a deliberate ship-block, not an error in the fixture).

Usage: python3 validate_claims.py <claims.json> [--docroot DIR]
Source pins resolve relative to --docroot (default: the parent of claims/).
"""
import json, sys, os, re

def load(path):
    with open(path) as f:
        return json.load(f)

def content_match(docroot, ref):
    """Re-read the pinned target and check the required content substring is there.
    Returns (ok, detail). Content-match, not existence: the pin must SAY the thing.

    Two pin forms:
      - {doc: ...}            → resolves relative to docroot (sim/), a corpus doc.
      - {file: ..., commit:}  → SOURCE-KILL pin: a repo path (from repo root) at a
        commit SHA. The SHA is the immutable anchor (line numbers drift, the §7
        failure); we content-match the live file and trust the SHA as the
        provenance stamp. (A full check would `git show SHA:file` — the live-file
        match is the cheap gate; the SHA records what was read.)"""
    must = ref.get("content_must_match")
    doc = ref.get("doc")
    fileref = ref.get("file")
    if must is None or (doc in (None, "this-section") and fileref is None):
        return None, f"no external pin to content-match (doc={doc}, file={fileref})"
    if fileref is not None:
        # repo root: docroot is sim/docs (parent of claims/); four levels up is the
        # repo top — docs → sim → channels-sv2 → sv2 → <repo>. The `file` field is
        # a repo-root-relative path (sv2/channels-sv2/...).
        repo_root = os.path.normpath(os.path.join(docroot, "..", "..", "..", ".."))
        path = os.path.join(repo_root, fileref)
        label = f"{fileref}@{ref.get('commit','?')[:8]}"
    else:
        path = os.path.join(docroot, doc)
        label = doc
    if not os.path.exists(path):
        return False, f"pinned target not found: {path}"
    text = open(path, encoding="utf-8", errors="replace").read()
    needle = must.replace("\n>   ", " ").replace("\n", " ")
    hay = re.sub(r"\s+", " ", text)
    found = needle in hay or must in text
    return found, f"content_must_match {'FOUND' if found else 'MISSING'} in {label}: {must[:40]!r}"

def gate(claims_path, docroot):
    data = load(claims_path)
    claims = {c["id"]: c for c in data["claims"]}
    failures, blocks, warns, notes = [], [], [], []

    for cid, c in claims.items():
        wt = c["warrant_type"]
        ref = c.get("warrant_ref", {})

        if wt == "UNDECIDED":
            # The point of the whole exercise: cannot be marked green. Blocks.
            owner = ref.get("owner", "?")
            blocks.append(f"{cid}: UNDECIDED — warrant_type awaits a human decision "
                          f"({ref.get('pending_decision','?')[:80]}...). owner={owner}. "
                          f"Ships only after the call is made; the tool MUST NOT pick a branch.")
            if c.get("status") != "UNDECIDED":
                failures.append(f"{cid}: UNDECIDED warrant but status={c.get('status')!r} (must be 'UNDECIDED').")
            continue

        if wt in ("source", "source-private"):
            # a claim may rest on >1 pin (warrant_ref, warrant_ref_2, ...); every
            # pin must content-match, else a two-source claim hides a bad pin.
            pins = [ref] + [c[k] for k in c if k.startswith("warrant_ref_") and isinstance(c[k], dict)]
            for pin in pins:
                ok, detail = content_match(docroot, pin)
                if ok is False:
                    failures.append(f"{cid}: source content-match FAILED — {detail}")
                elif ok is None:
                    notes.append(f"{cid}: {detail}")
            if wt == "source-private":
                # check is type-honesty, not pin re-read: must be flagged as authorial
                # observation and not phrased as derivation. (Unbound from any claim
                # here until an UNDECIDED resolves into it — so its mere presence on a
                # claim is itself suspect until that resolution exists.)
                notes.append(f"{cid}: source-private — verify phrasing is authorial-observation, not derivation (human).")

        elif wt == "socket":
            if c.get("status") != "unpinned":
                failures.append(f"{cid}: socket but status={c.get('status')!r} (must be 'unpinned').")
            # propagation: every downstream claim must carry the socket in a
            # depends_on/depends_on_sockets list, so 'conditional' can't be dropped.
            for d in c.get("downstream", []):
                dc = claims.get(d)
                if dc is None:
                    failures.append(f"{cid}: downstream claim {d!r} not found.")
                    continue
                deps = dc.get("depends_on_sockets", []) + dc.get("depends_on", [])
                if cid not in deps:
                    failures.append(f"{cid}: socket does NOT propagate — downstream {d} "
                                    f"omits it from depends_on (a default could smuggle in here).")

        elif wt == "cross-doc":
            cb = ref.get("corpus_boundary")
            lbt = ref.get("load_bearing_type")
            if not ref.get("is_about"):
                failures.append(f"{cid}: cross-doc missing is_about (the §7 catch: resolved-but-wrong-thing).")
            if cb not in ("in-corpus", "cross-corpus"):
                failures.append(f"{cid}: cross-doc missing/!invalid corpus_boundary={cb!r}.")
            if lbt not in ("supports", "see-also"):
                failures.append(f"{cid}: cross-doc invalid load_bearing_type={lbt!r}.")
            # the OE catch as a rule: weight must not cross a corpus boundary.
            if cb == "cross-corpus" and lbt == "supports":
                failures.append(f"{cid}: cross-corpus 'supports' — authority borrowed across a "
                                f"corpus boundary (the OE catch). Cross-corpus may only be 'see-also'.")

        elif wt == "execution":
            dom = c.get("domain", {})
            if not dom:
                failures.append(f"{cid}: execution claim with no domain (the EXCESS catch: a value "
                                f"without its domain-of-validity passes a cherry-picked scope).")
            else:
                notes.append(f"{cid}: execution — domain present {list(dom)}; full re-run+value-compare "
                             f"is the medium gate (code-change trigger), not run here.")

        elif wt == "judgment":
            # REFUSED. Never validated. But track factual basis.
            for d in c.get("depends_on", []):
                dc = claims.get(d)
                if dc and dc.get("status") == "DRIFTED":
                    warns.append(f"{cid}: BASIS-CHANGED — judgment input {d} DRIFTED. Re-examine "
                                 f"(NOT 'invalidated' — judgment is uncheckable; its inputs are not).")
            notes.append(f"{cid}: judgment — refused for validation by design (marked, not checked).")

        else:
            failures.append(f"{cid}: unknown/unrouted warrant_type={wt!r} — nothing ships unrouted.")

        # A resolved claim may carry revisit_when: "decided the version right for
        # now, marked the condition under which I'd strengthen it." That is NOT
        # UNDECIDED (the decision was made) — but a revisit condition that never
        # re-surfaces is a deferred decision laundered as resolved, the thread's
        # signature failure. So surface it every run rather than tolerate it
        # silently. It does not block (the claim is validly resolved now); it
        # refuses to disappear.
        if c.get("revisit_when"):
            warns.append(f"{cid}: REVISIT — resolved as {wt} for now, strengthen when: "
                         f"{c['revisit_when']}. (Standing condition; surfaced every run so a "
                         f"matured condition can't pass unnoticed — re-examine, don't auto-resolve.)")

    # cited_by>1 canonical-object proxy (lagging) — warn only.
    cby = {}
    for c in claims.values():
        for d in c.get("depends_on", []) + c.get("depends_on_sockets", []):
            cby[d] = cby.get(d, 0) + 1
        t = c.get("warrant_ref", {}).get("target_doc")
        if t: cby[t] = cby.get(t, 0) + 1
    for cid, n in cby.items():
        if cid in claims and n > 1:
            warns.append(f"{cid}: cited {n}x — register as canonical object (proxy is LAGGING: a "
                         f"cross-corpus claim wrong on first write has count 1; see §7 near-miss).")

    return failures, blocks, warns, notes

def main():
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(2)
    claims_path = sys.argv[1]
    docroot = os.path.abspath(os.path.join(os.path.dirname(claims_path), ".."))
    if "--docroot" in sys.argv:
        docroot = sys.argv[sys.argv.index("--docroot") + 1]

    failures, blocks, warns, notes = gate(claims_path, docroot)

    def section(title, items, sym):
        if items:
            print(f"\n{sym} {title} ({len(items)})")
            for it in items: print(f"   {it}")

    section("GATE FAILURES (claim disagrees with warrant)", failures, "✗")
    section("SHIP-BLOCKS (UNDECIDED — human decision pending, MUST NOT default)", blocks, "■")
    section("WARNINGS (lagging proxies / re-examine)", warns, "▲")
    section("ROUTED-TO-HUMAN / NOTES (not auto-gated, flagged so not mistaken for checked)", notes, "·")

    print("\n" + "=" * 64)
    if failures:
        print(f"RESULT: FAIL — {len(failures)} gate failure(s). Does not ship.")
        sys.exit(1)
    if blocks:
        print(f"RESULT: BLOCKED — {len(blocks)} UNDECIDED claim(s) await a human decision.")
        print("        This is correct behavior: an open question is marked open, not")
        print("        closed by the tool that found it. Resolve the decision, then re-run.")
        sys.exit(3)
    print("RESULT: PASS — all routed claims clear their gates; no UNDECIDED blocks.")
    sys.exit(0)

if __name__ == "__main__":
    main()
