
#!/bin/sh

WORKSPACES="benches common protocols roles utils"

for workspace in $WORKSPACES; do
    echo "Executing build on: $workspace"
    cargo +1.75.0 build --manifest-path="$workspace/Cargo.toml" -- 
    if [ $? -ne 0 ]; then
        echo "Build found some errors in: $workspace"
        exit 1
    fi

    echo "Running fmt on: $workspace"
    (cd $workspace && cargo +nightly fmt)
    if [ $? -ne 0 ]; then
        echo "Fmt failed in: $workspace"
        exit 1
    fi
done

echo "build success!"


