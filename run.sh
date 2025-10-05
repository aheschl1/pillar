#!/usr/bin/env bash
set -e

# Assert image exists
if ! docker image inspect pillar >/dev/null 2>&1; then
    echo "Error: Docker image 'pillar' not found." >&2
    exit 1
fi

# Ensure ./work exists
WORKDIR="$(pwd)/work"
mkdir -p "$WORKDIR"

# Generate unique container name
CID="pillar_$(uuidgen | tr 'A-Z' 'a-z' | cut -d'-' -f1)"

# Run container detached, mounting ./work -> /work
CID=$(docker run -d --rm --init \
    -v "$WORKDIR":/usr/local/bin/work \
    --name "$CID" \
    pillar "$@")

# Wait for network setup
sleep 0.5

# Print IP and mount info
IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$CID")
echo "Container started: $CID"
echo "IP address: $IP"
echo "Mounted host dir: $WORKDIR -> /usr/local/bin/work"
