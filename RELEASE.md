## Changelog

The changelog is a list of notable changes for each version of a project. 
It is a way to keep track of the project's progress and to communicate 
the changes to the users and the community.

The changelog is automatically generated on each release page, and extra contextual
information is added as needed, such as:
 - General release information.
 - Breaking changes.
 - Notable changes.

## Release Process

Try to constrain each development cycle to a fixed time period, after which a
release is made. A release is not published straight from `main`: it is first
cut as a **release candidate**, which is tested through the applications built
on it. Only a candidate that gets through the test period without a blocking
problem becomes the release.

Releases are coordinated with [`sv2-apps`](https://github.com/stratum-mining/sv2-apps)
and [`sv2-ui`](https://github.com/stratum-mining/sv2-ui), always in the order
`stratum`, `sv2-apps`, `sv2-ui`, because each one is built on the one before it:
the applications use the `stratum-core` crate, and SV2 UI runs the application
images. The order holds for candidates as well as for the final release.

Each release has a release branch named `release/vX.Y.Z`, created from `main`
when its first candidate is cut. `main` is never frozen: whatever merges there
after the branch was created belongs to the next release.

### Cutting a Release Candidate

A candidate is a tag named `vX.Y.Z-rc.N`, starting at `rc.1`. It publishes
nothing: crates.io never takes back a version, so the crates are only published
by the final release. The `sv2-apps` candidate points its `stratum-core`
dependency at this tag instead.

1. **Create Release Branch:** For the first candidate, create `release/vX.Y.Z` from the `main` branch and push it. CI runs on pushes to `release/v*`: the semver checks, and `./scripts/check-published-crates.sh`, which fails when a crate carries a version that is already on crates.io while its contents differ from what was published. `cargo publish` would refuse that crate, so it needs a version bump before the candidate goes out, made like any other fix (see [Fixing a Blocking Problem](#fixing-a-blocking-problem)).
2. **Tag the Candidate:** Create the `vX.Y.Z-rc.N` tag on the release branch and push it.
3. **Hand Over:** The `sv2-apps` candidate is cut next, on top of this tag.

### Test Period

The test period lasts 7 days, counted from the publication of the newest
candidate among `stratum`, `sv2-apps` and `sv2-ui`, since the three are released
together. The candidate is exercised through the applications, which the SRI
team and the community run on mainnet: the
[`sv2-apps` release process](https://github.com/stratum-mining/sv2-apps/blob/main/RELEASE.md)
describes how problems are reported and which ones block the release.

### Fixing a Blocking Problem

1. **Fix on `main`:** Merge the fix on `main` first, through a regular PR. A candidate counts as a release for the version checks, so CI asks the PR to bump the crates it modifies above the candidate's versions (see [CONTRIBUTING.md](CONTRIBUTING.md)).
2. **Backport:** Open a PR against `release/vX.Y.Z` that cherry-picks the fix, together with its version bump if it has one. CI on a release branch compares each crate with crates.io, and fails if the fix still lacks a bump. If `main` has changed the same code since the branch was created, adapt the fix in this PR. A fix that only applies to the release branch still has to be checked against `main`, and ported there if `main` is affected. The integration tests with `sv2-apps` do not run for these PRs: the `sv2-apps` release branch runs them once it points at the next candidate.
3. **Tag the Next Candidate:** Create the next `vX.Y.Z-rc.N` tag on the release branch. The `sv2-apps` and `sv2-ui` candidates that follow are cut on top of it.
4. **Restart the Test Period:** A new candidate starts a new 7 days.

### Final Release

Once the test period ends, `stratum` is the first of the three to be released:

1. **Create Tag:** Create the `vX.Y.Z` tag on the commit of the last candidate and push it. No commit separates the release from the candidate that was tested. Pushing the tag runs `./scripts/check-published-crates.sh` once more and publishes the crates to crates.io.
2. **Create GitHub Release:** Create a GitHub release for the tag, with changelog and release notes.

### Patch Releases

A patch release follows the same process, with these differences:

- Its release branch is created from the tag of the release it patches, not from `main`.
- It only takes fixes. Each one is merged on `main` first and then backported, as in [Fixing a Blocking Problem](#fixing-a-blocking-problem).
- Its test period lasts at least 48 hours.

A fix for a vulnerability that has to stay private until it ships does not go
through a public candidate, which would disclose it. It is tested privately by
the maintainers and released directly.

## Versioning

Crates follow SemVer 2.0.0. Whenever the repository goes through a global release, all crates are published to crates.io.

The global repository releases follow `X.Y.Z`, which is changed under some subjective criteria:
- If a release includes only bug fixes`, then `Z` is bumped.
- If a release includes breaking and/or non-breaking changes, then `Y` is bumped.
- If a release marks a milestone i.e., crates are reaching a new maturity level, then `X` is bumped.

## Tags and Branches

- Changes to `main` branch should be added through a merge commit.
- The `main` branch is the default branch and it is always active.
- The `main` branch is protected and requires a pull request to merge changes
  with at least 2 approvals.
- Each release is tagged with the version number of the release and a release
  branch is kept for future reference or fixes.
- Each release candidate is tagged `vX.Y.Z-rc.N` on the release branch of its
  release.
- Changes to a release branch are added through a pull request against it.
