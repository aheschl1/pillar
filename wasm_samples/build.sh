#!/bin/bash

set -e

for dir in $(ls -d ./samples/*/); do
    echo "Building in $dir"
    (cd $dir && cargo build --target wasm32-unknown-unknown --release)
done

echo "All samples built successfully."