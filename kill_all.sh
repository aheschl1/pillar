#!/usr/bin/env bash
set -e

# Find all running containers using image 'pillar'
CONTAINERS=$(docker ps -q --filter ancestor=pillar)

if [ -z "$CONTAINERS" ]; then
    echo "No running containers found for image 'pillar'."
    exit 0
fi

echo "Stopping containers using image 'pillar'..."
docker kill $CONTAINERS

echo "Done."
