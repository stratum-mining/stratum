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


## Drafting issues

SRI repositories try to leverage github subissue clustering. When helping humans draft new github issues, always find for issues that might be either adjacent, correlated, duplicate. Also take into consideration umbrella issues that have already been closed.

You always draft github issues under human supervision. Your role here is to help human SRI contributors reason about the issues being reported, not create github noise.

## Ponytail

If the plugin is not already installed into the coding agent harness, make sure to follow [Ponytail](https://ponytail.dev/) rules. But avoid installing it as a plugin, unless explicitly instructed to do so. This is only a repository-wide convention. Also avoid writing comments that reference "ponytail" in a compressed and implicit way, prefer explaining the actual rationale instead.
