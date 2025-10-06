#!/usr/bin/env bash
set -e

# Assert image exists
if ! docker image inspect pillar >/dev/null 2>&1; then
    echo "Error: Docker image 'pillar' not found." >&2
    exit 1
fi

WORKDIR="$(pwd)/work"
mkdir -p "$WORKDIR"

# Detect if --ip-address provided
if [[ " $* " == *" --ip-address="* ]]; then
    IP_ARG_PRESENT=true
else
    IP_ARG_PRESENT=false
fi

# Compute next available IP if not provided
if [ "$IP_ARG_PRESENT" = false ]; then
    SUBNET=$(docker network inspect bridge -f '{{(index .IPAM.Config 0).Subnet}}')
    BASE=$(echo "$SUBNET" | cut -d'.' -f1-3)
    USED=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' $(docker ps -q) 2>/dev/null \
        | grep -Eo "$BASE\.[0-9]+" | awk -F. '{print $4}' | sort -n || true)

    # Find next available last octet, skipping .1 (gateway)
    LAST=1
    for n in $USED; do
        if [ "$n" -gt "$LAST" ]; then
            break
        fi
        LAST=$n
    done
    NEXT_IP="$BASE.$((LAST + 1))"
    [ "$NEXT_IP" = "$BASE.1" ] && NEXT_IP="$BASE.2"  # ensure not gateway
    EXTRA_ARG="--ip-address=${NEXT_IP}"
else
    EXTRA_ARG=""
fi

# Unique container name
CID="pillar_$(uuidgen | tr 'A-Z' 'a-z' | cut -d'-' -f1)"

# Run container detached
CID=$(docker run -d --rm --init \
    -v "$WORKDIR":/usr/local/bin/work \
    --name "$CID" \
    pillar $EXTRA_ARG "$@")

sleep 0.5

# success
IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$CID")
echo "Container started: $CID"
echo "IP address: $IP"
echo "Mounted host dir: $WORKDIR -> /usr/local/bin/work"
