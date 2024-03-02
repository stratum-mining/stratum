# Versioning

SRI releases follow [SemVer 2.0.0](https://semver.org/).

Given a version number `MAJOR.MINOR.PATCH`, we increment the:
- `MAJOR` version when incompatible API changes are introduced
- `MINOR` version when functionality is added in a backward compatible manner
- `PATCH` version when backward compatible bug fixes are introduced

SRI has a global version, which uses git tags and keeps track of how the codebase evolves across time as a whole.

All the other internal SRI crates also follow SemVer 2.0.0, but each crate version is independent of the global release version.

Whenever a `PATCH` is introduced, it is applied to all the latest `MAJOR` releases.

# Git Branching Strategy

We follow a simplified [gitflow](https://nvie.com/posts/a-successful-git-branching-model/) branching strategy.

Although our strategy is very similar to the classic gitflow model, we do not keep release branches.

![](git-branching.png)

## Principal Branches

The SRI repo holds two principal branches with an infinite lifetime:
- `main`
- `dev`

We consider `main` to be the branch where the source code of `HEAD` always reflects a production-ready state.

We consider `dev` to be the branch where the source code of `HEAD` always reflects a state with the latest delivered development changes for the next release.

When the source code in the `dev` branch reaches a stable point and is ready to be released, all the changes are merged back into `main` and then tagged with a release number while bumping `MAJOR` and/or `MINOR`.

## Feature Branches

New features are developed into separate branches that only live in the contributor's forks.

- branch off from: `dev`
- merge back into: `dev`

## Patch Branches

Bugs are patched into separate branches that only live in the contributor's forks.

- branch off from: `main`
- merge back into: `main` + `dev`

# Releasing Roles Binaries

The [release page of SRI repo](https://github.com/stratum-mining/stratum/releases) provides executable binaries for all SRI roles, targeting popular hardware architectures.

The github binary releases of the roles are handled in the `release-bin.yaml` workflow.

This workflow is manually started by navigating to the "Actions" tab in the SRI repo, then navigating to the Release workflow section, and clicking "Run Workflow".

Note: in order to be able to manually trigger the "Run Workflow" button, the user needs to have "Write" permissions on the repository, otherwise the button will not show up on the UI.

# Publishing Library Crates

Although SRI has a global release cycle, which is attached to the binaries, each internal crate also has its own versioning history.

Lib crates are published to crates.io in the `release-lib.yaml` workflow. The workflow tries to update all the library crates. 
If a crate is not to updated, the step will fail for that each step have continue-on-error set to true.

Since each step can fail, the output ot the action must be manually check to macke sure that all the library intended to
be published are published.

Running `cargo release` in the various workspace help to prepare the version number and everything.

Every PR needs to increase the version of whatever crate it is touching. Otherwise, we will mess up the dependency chain of whoever is fetching from crates.io

Every time we bump some crate's version, `release-libs.yaml` is automatically triggered in order to update crates.io
