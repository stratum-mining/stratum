# AGENTS.md

This file provides guidance for agents working on this repository. It is committed to the repository, which means agents from all contributors follow the same conventions.

Contributors are free to add their own custom guidance for their agents via `AGENTS_CUSTOM.md`, which is NOT committed to the repository.

Aside from guidelines on these files, also always take into consideration `CONTRIBUTING.md`, `RELEASE.md` and `README.md`.

Pay special attention to `CONTRIBUTING.md` when crafting git commits.

## Cross-repo development

While `stratum` contains low-level libraries, [`sv2-apps`](https://github.com/stratum-mining/sv2-apps) contains the higher-level applications that use them. The two repositories are linked together via the `stratum-core` crate.

As a consequence, any breaking changes into APIs coming from `stratum` need to be reflected in `sv2-apps`. Github CI enforces this cross-repo atomic coherence via the Integration Tests workflow.

In order to get Integration Tests to pass, PRs that introduce breaking changes to `stratum` always need a companion PR in `sv2-apps`. The companion PRs are linked together via the `companion` keyword in their PR descriptions.

PRs on `sv2-apps` always need a temporary commit that replaces `stratum-core` dependency from `main`'s HEAD with the contributor's fork of `stratum`. This temporary commit is only used to get Integration Tests to pass, and its commit message should always make that explicitly clear. After the `stratum` PR is merged, the temporary commit is dropped and `stratum-core` is updated `stratum`'s new `main` HEAD. This coordination is delicate and requires human supervision to avoid accidents.

For local development, `sv2-apps` has a `scripts/cross-repo.sh` script that allows an automated workflow for updating `sv2-apps` with the corresponding changes from `stratum`.

## Pre-mined shares on `channels_sv2` tests

Whenever touching `channels_sv2` crate, if tests need to be adapted and pre-mined shares no longer work, find new pre-mined shares. Do not replace pre-mined shares with dynamic loops that find shares during test execution.

## Bug patching

Whenever patching bugs, always keep me informed about potential side-effect implications on non-trivial aspects of the project functionality (e.g.: scalability, monitorability, new bugs or vulnerabilities).

Whenever writing documentation around bug fixes, always write the comment assuming the reader is simply trying to understand the code as is, not the past history of bugs that existed on that code.

## PR reviews

Whenever helping me review PRs, don't restrict the output to an analysis of the PR. Also help me understand the PR progressively across two axis:

- conceptual (taking issues and other related PRs into consideration)
- commit history

While listing findings, for each finding, give me a draft comment and the file/line where it would be appropriate to drop it. Also mention the finding severity, and whether you believe it's a blocker or not. This is deliberately designed to keep human reviewers on the loop, as opposed to blindly copypasting a huge "clanker review" body of text without ever looking into what each finding means.

Judge a PR against the problem and the expected outcome of the issues it closes. Everything else an issue lists, including suggested approaches and the `fix` / `regression tests` sections of older issues, is context: a PR that reaches the outcome another way is not a finding, as long as its description explains why. Flag a divergence only when part of the expected outcome is left unsolved, and say which part.

## Drafting issues

SRI repositories try to leverage github subissue clustering. When helping humans draft new github issues, always find for issues that might be either adjacent, correlated, duplicate. Also take into consideration umbrella issues that have already been closed.

You always draft github issues under human supervision. Your role here is to help human SRI contributors reason about the issues being reported, not create github noise.

Describe the problem and the outcome a fix must guarantee, observable from outside the code. Implementation and test ideas are suggestions for whoever picks the issue up, so mark them as non-binding: written as requirements, they make reviewers flag every PR that solves the problem another way.

## Triaging audit findings

SRI maintainers triage findings from the private Loupe audit repositories into public stratum issues, so the work is tracked where it happens. Draft them from `.github/ISSUE_TEMPLATE/audit-finding.md`, and:

- Before publishing, check with the maintainer whether the finding is safe to disclose. A severe finding that can be exploited remotely stays in Loupe until its fix has landed.
- Open one issue per defect, listing every Loupe finding that reports it. Loupe files the same defect once per implementation, e.g. the server/client x standard/extended twins in `channels_sv2`.
- Check the sibling implementations for the same defect, and record the ones that are not affected along with the reason.
- File the issue where the defect lives. A defect in a stratum crate that an [`sv2-apps`](https://github.com/stratum-mining/sv2-apps) finding reports is still a stratum issue, and the other way around; link the other repository's finding under "Related issues and PRs" so the companion PR can close it.
- Make the issue a sub-issue of the crate's tracker, and apply the crate's label: #2136 (`binary-sv2`), #2271 (`buffer_sv2`, no label yet), #2278 (`codec-sv2`), #2276 (`framing_sv2`, no label yet), #2246 (`noise-sv2`), #2253 (`channels-sv2`) or #2321 (`handlers-sv2`). A crate missing from this list gets a tracker when its triage starts.
- Once a stratum issue exists, label the Loupe findings it lists `sri:tracked`.
- Leave PR grouping to whoever picks the issue up. If two issues should land together, say why under "Related issues and PRs".
- Record progress in dated comments rather than by editing the issue body.

An issue and every Loupe finding it lists close together, with the PR that completes the fix. A PR that fixes only part of it references them with `ref` instead of `Closes`. A finding that contradicts a recorded design decision is closed as not planned, with a rustdoc note at the site explaining the decision so the scanner stops re-filing it.

## Ponytail

If the plugin is not already installed into the coding agent harness, make sure to follow [Ponytail](https://ponytail.dev/) rules. But avoid installing it as a plugin, unless explicitly instructed to do so. This is only a repository-wide convention. Also avoid writing comments that reference "ponytail" in a compressed and implicit way, prefer explaining the actual rationale instead.
