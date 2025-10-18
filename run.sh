#!/usr/bin/env bash
set -euo pipefail
trap 'echo "Error on line $LINENO: $BASH_COMMAND" >&2' ERR

NETWORK="pillar"
SUBNET_RANGE="10.9.0.0/24"
WORKDIR="$(pwd)/work"
mkdir -p "$WORKDIR"

# Ensure image exists
if ! docker image inspect pillar >/dev/null 2>&1; then
    echo "Error: Docker image 'pillar' not found." >&2
    exit 1
fi

# Ensure network exists
if ! docker network inspect "$NETWORK" >/dev/null 2>&1; then
    echo "Creating network $NETWORK ($SUBNET_RANGE)..."
    docker network create --subnet="$SUBNET_RANGE" "$NETWORK"
fi

# Detect if --ip-address provided for the container process
USER_IP_ARG=$(echo "$*" | grep -oP '(?<=--ip-address=)[0-9\.]+' || true)

if [ -z "$USER_IP_ARG" ]; then
    # Find largest assigned Docker IP on this network
    BASE=$(echo "$SUBNET_RANGE" | cut -d'.' -f1-3)
    ESCAPED_BASE=$(echo "$BASE" | sed 's/\./\\./g')

    USED=$(docker network inspect "$NETWORK" \
        -f '{{range .Containers}}{{.IPv4Address}}{{"\n"}}{{end}}' \
        | cut -d'/' -f1 \
        | grep -Eo "${ESCAPED_BASE}\.[0-9]+" \
        | awk -F. '{print $4}' \
        | sort -n || true)

    if [ -z "$USED" ]; then
        NEXT=2
    else
        LAST=$(echo "$USED" | tail -n1)
        NEXT=$((LAST + 1))
    fi

    NEXT_IP="$BASE.$NEXT"
    echo "Assigned IP: $NEXT_IP"

    DOCKER_IP_ARG="--ip=$NEXT_IP"
    APP_IP_ARG="--ip-address=$NEXT_IP"
else
    DOCKER_IP_ARG="--ip=$USER_IP_ARG"
    APP_IP_ARG="--ip-address=$USER_IP_ARG"
    echo "Using provided IP: $USER_IP_ARG"
fi

# Unique container name
CONTAINER_NAME="pillar_$(uuidgen | tr 'A-Z' 'a-z' | cut -d'-' -f1)"

echo "Starting container: $CONTAINER_NAME ($DOCKER_IP_ARG, app $APP_IP_ARG)"

if ! OUT=$(docker run -d --rm --init \
    --network "$NETWORK" \
    $DOCKER_IP_ARG \
    -v "$WORKDIR":/usr/local/bin/work \
    --name "$CONTAINER_NAME" \
    pillar "$APP_IP_ARG" "$@" 2>&1); then
    echo "Docker run failed:"
    echo "$OUT"
    exit 1
fi

sleep 0.5

IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$CONTAINER_NAME")
echo "Container started: $CONTAINER_NAME"
echo "Docker IP: $IP"
echo "Mounted host dir: $WORKDIR -> /usr/local/bin/work"
