# Pushing to the repository
The repository is hosted on GitHub.  To push changes to the repository, you need to have a GitHub account and be added 
as a collaborator to the repository.  You can then push changes to the repository.

To push to the repository a pre-push hook needs to be run locally to ensure that the code is formatted correctly and
tests pass. To install the pre-push hook run the following command from the root of the repo:

This tells git where the githooks are located
`# git config core.hooksPath .githooks`

The githooks rely on the following being install beforehand:

1. [act](https://github.com/nektos/act) - Github actions local runner.
2. [docker](https://docs.docker.com/get-docker/) - Docker is used to run the act runner.

Once these are installed the first time you try to push your source to the repo it'll run 
./githooks/pre-push script which executes the `sv2_header_check`, `fmt`, `clippy-lint` and `ci` jobs found in the 
`.github/workflows` directory.