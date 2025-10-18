#!/bin/bash

count=0
for dir in ./work/* libs/*/test_output/*; do
    if [ -d "$dir" ]; then
        count=$((count + 1))
    fi
done
echo "Found $count directories to remove in ./work/ and libs/*/test_output/."

rm -rf ./work/*
rm -rf libs/*/test_output/*

echo "Cleanup complete."