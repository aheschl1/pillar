#!/bin/bash
set -euo pipefail

# Usage: ./run_vm.sh -i <base_image.qcow2> -a <arch> -r <ram_mb> -p <ssh_port>
# Example: ./run_vm.sh -i debian-x86.qcow2 -a x86_64 -r 2048 -p 2222

while getopts "i:a:r:p:" opt; do
  case $opt in
    i) BASE_IMAGE="$OPTARG" ;;
    a) ARCH="$OPTARG" ;;
    r) RAM="$OPTARG" ;;
    p) PORT="$OPTARG" ;;
    *) echo "Usage: $0 -i <image> -a <arch> -r <ram_mb> -p <ssh_port>" >&2; exit 1 ;;
  esac
done

if [[ -z "${BASE_IMAGE:-}" || -z "${ARCH:-}" || -z "${RAM:-}" || -z "${PORT:-}" ]]; then
  echo "Missing required args" >&2
  exit 1
fi

case "$ARCH" in
  x86_64)
    QEMU_BIN="qemu-system-x86_64"
    ACCEL="-enable-kvm -cpu host"
    ;;
  aarch64)
    QEMU_BIN="qemu-system-aarch64"
    ACCEL="-cpu cortex-a57 -M virt"
    ;;
  *)
    echo "Unsupported arch: $ARCH" >&2
    exit 1
    ;;
esac

# Create overlay so base image is untouched
OVERLAY="overlay-${ARCH}-$(date +%s).qcow2"
qemu-img create -f qcow2 -F qcow2 -b "$BASE_IMAGE" "$OVERLAY"

# Cleanup on exit
cleanup() {
  rm -f "$OVERLAY"
}
trap cleanup EXIT

# Start VM
$QEMU_BIN \
  -m "$RAM" \
  -smp 4 \
  $ACCEL \
  -drive file="$OVERLAY",if=virtio,format=qcow2 \
  -netdev user,id=net0,hostfwd=tcp::"$PORT"-:22 \
  -device virtio-net-pci,netdev=net0 \
  -nographic
