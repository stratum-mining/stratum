# Special branch used in bitcoin core

This branch contains a subset of of the main branch. It only contain the file needed by core to
implement the TP. Is important to keep this branch as small as possible, so we need to follow a
special procedure when we need to update this branch with new code in the master branch.

# Update
When changes from master needs to be merged into sv2-tp-crates
1. do ``git checkout main `find ./protocols -name \*.*  | grep -v "target" | grep -v "toml" | grep -v "lock"` ``
2. do `./test.sh`
3. manually fix errors if any
