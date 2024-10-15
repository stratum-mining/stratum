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
release is made. After all release tasks are completed, initiate the release
process by creating a new release branch, named `x.y.z.` referring to the
version number, from the `main` branch.  The release branch is used as a
breaking point and future reference for the release and should include the
changelog entries for the release and any other release-specific tasks. Any bug
fixes or changes for the release should be done on the release branch. When the
release branch is ready, create a new tag and initiate any publishing tasks.

Usually the release process is as follows:

1. Create a new release branch from the `main` branch.
2. Create a new tag for the release branch.
3. Publish the release.

## Versioning

Crates under `protocols` and `utils` workspaces follow SemVer 2.0.0. The version number is stored in
the `Cargo.toml` file of each crate. If a breaking change is introduced to one
of the crates, the version number must be updated accordingly, otherwise a
SemVer CI check would fail. Note that this does not apply to the `roles` and
other crates in the repository.

The global repository releases follow `X.Y.Z`, which is changed under some subjective criteria:
- Changes in `roles` are not taken into account. `roles` crates are still in Proof of Concept phase and not production ready. 
- If a release includes only bug fixes in `protocols` and/or `utils`, then `Z` is bumped.
- If a release includes breaking and/or non-breaking changes to `protocols` and/or `utils`, then `Y` is bumped.
- If a release marks a milestone i.e., `protocols` is reaching a new maturity level, then `X` is bumped.

## Tags and Branches

- Changes to `main` branch should be added through a merge commit.
- The `main` branch is the default branch and it is always active.
- The `main` branch is protected and requires a pull request to merge changes
  with at least 2 approvals.
- Each release is tagged with the version number of the release and a release
  branch is kept for future reference or fixes.
