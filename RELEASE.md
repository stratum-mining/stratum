# Releasing Roles Binaries

The github binary releases of the roles and the publishing of the SRI crates are both handled in the `release-bin.yaml` workflow. This workflow must be manually
started by navigating to the "Actions" tab in the SRI repo, then navigating to the Release workflow section, and clicking "Run Workflow". Note: in order to be
able to manually trigger the "Run Workflow" button, the user needs to have "Write" permissions on the repository, otherwise the button will not show up on the UI.

# Publishing Library Crates

Lib crates are published in the `release-lib.yaml` workflow. The workflow tries to update all the library crates. 
If a crate is not to updated, the step will fail for that each step have continue-on-error set to true.

Since each step can fail, the output ot the action must be manually check to macke sure that all the library intended to
be published are published.

Running `cargo release` in the various workspace help to prepare the version number and everything.

Every PR needs to increase the version of whatever crate it is touching. Otherwise, we will mess up the dependency chain of whoever is fetching from crates.io

Every time we bump some crate's version, `release-libs.yaml` is aumatically triggered in order to update crates.io

# Versioning

SRI follows [SemVer 2.0.0](https://semver.org/).

Given a version number `MAJOR.MINOR.PATCH`, we increment the:
- `MAJOR` version when incompatible API changes are introduced (e.g.: `protocols`, `roles`)
- `MINOR` version when functionality is added in a backward compatible manner (e.g.: `roles`)
- `PATCH` version when backward compatible bug fixes are introduced
