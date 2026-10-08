#!/usr/bin/env bash
set -e

# Find all running containers using image 'blockchain'
CONTAINERS=$(docker ps -q --filter ancestor=blockchain)

if [ -z "$CONTAINERS" ]; then
    echo "No running containers found for image 'blockchain'."
    exit 0
fi

echo "Stopping containers using image 'blockchain'..."
docker kill $CONTAINERS

echo "Done."
