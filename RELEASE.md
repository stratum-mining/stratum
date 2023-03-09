# Releasing Roles Binaries and Publishing

The github binary releases of the roles and the publishing of the SRI crates are both handled in the `release.yaml` workflow. This workflow must be manually
started by navigating to the "Actions" tab in the SRI repo, then navigating to the Release workflow section, and clicking "Run Workflow".

## Crates Publishing

For publishing crates we use the `cargo-release` tools because it easily handles the order in which crates can be published. Crates are published 
in the `crates_publish` job. Currently, crates.io rate limits the publishing of more than 5 NEW crates. This means that if we have more than 5 crates
to publish, that are not currently published to crates.io, this workflow will not publish anything. To bypass this we will need to add the `--exclude [crate_name]`
flag for as many crates as needed to the `cargo-release` command in the "Publish" step. For example, if there are 7 new crates to publish, we will need to add 
`--exclude [crate_1] --exclude [crate_2]` to the command. This is not an issue for publishing version updates since the crates are not NEW publishes.

### Current Successful Publishes

Due to either github dependencies or a crate failing the build stage during publish not all crates are being published.

- [x] buffer_sv2
- [x] binary_codec_sv2
- [x] binary_sv2
- [x] common_messages_sv2
- [x] const_sv2
- [x] derive_codec_sv2
- [x] framing_sv2
- [x] serde_sv2
- [x] sv1_api
- [] noise_sv2 - github depependency (ed25519-dalek)
- [] codec_sv2 - noise dependency
- [] template_distribution_sv2 - build failure
- [] mining_sv2 - publishes after template_distribution_sv2
- [] job-negotiation_sv2 - publishes after template_distribution_sv2
- [] roles_logic_sv2 - publishes after template_distribution_sv2
- [] network_helpers - publishes after template_distribution_sv2
- [] error_handling - had to exclude to bypass rate limit but will probably publish now
- [] sv2_ffi - had to exclude to bypass rate limit but will probably publish now
- [] all roles - noise dependency

## Github Roles Releases

The roles binaries are released in github in all jobs other than the `crates_publish` job. To be able to publish,
the job must be able to find a new tag (one that hasnt already been released), matching the crate name of the role to be
released. For example, imagine there is currently a release for pool_sv2-v1.0.0. For the jobs releasing a new pool version to succeed,
it must be able to find a tag containing the string "pool_sv2" with a version greater than 1.0.0. This should not be an issue since the tags
are automatically created by the `autoversion.yaml` workflow, and moved into main when the generated PR is merged.

### Github Release Issue

Currently we do not support windows releases because we are unable to run the step to return the last tag for a given crate name. See the section above
for an explaination of the process.



## Github Binary Releases

# Versioning

Versioning is handled by the `autorelease.yaml` workflow. The workflow will  will auto detect changes in crates and auto bump the patch versions 
in the target crate, as well as any crate that uses the target crate as a dependency, if changes are detected. It will then push a new commit with the 
versioning changes to the `bot/versioning` branch, and Lastly, it will auto create a PR (bot/versioning -> main). Since cargo-smart-release does not yet 
support autodetection of MAJOR and MINOR changes with regards to Semver, any MAJOR/MINOR versioning will need to be manually changed in the package Cargo.toml.

## Versioning Notes

This workflow should not run if the push is resulting from this workflow, but this not necessarily guaranteed since the 
a developer could make a minor change to the bot/versioning PR, and the latest author would no longer be "Github Actions Bot".
If the workflow does run again, there will just be no changes, and the PR can be closed.