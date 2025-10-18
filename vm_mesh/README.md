# VM Mesh: Distributed Testing and Simulation

The VM Mesh framework enables distributed testing and simulation of the Pillar protocol across multiple virtual machines, supporting both x86_64 and aarch64 architectures. It automates the provisioning, networking, and orchestration of VMs using QEMU, with unified repository injection and cloud-init configuration. This is used for testing across different architectures, in order to assert serialization compatibility.

## Features

- **Automated VM Provisioning:** Launch any number of VMs with isolated overlays and custom network bridges.
- **Unified Codebase Injection:** Clones the repository and injects it into all VMs via a shared ISO, ensuring consistent test environments.
- **Cloud-Init Integration:** Uses cloud-init to configure SSH keys, networking, and repository setup for each VM.
- **Network Simulation:** Creates a virtual bridge and tap devices, allowing VMs to communicate as if on a real network.
- **Lifecycle Management:** Start, monitor, and terminate all VMs from a single interface. Supports context-managed operation for clean setup and teardown.
- **Architecture Flexibility:** Supports both x86_64 and aarch64 VMs for cross-platform protocol validation.

## Usage

To launch a mesh of VMs for distributed testing:

```bash
python vm_mesh/runner.py --n-x86 2 --n-aarch 1 --name test-mesh
```

This will:

- Create a dedicated directory for the mesh and overlays
- Provision 2 x86_64 and 1 aarch64 VM, each with the Pillar repository injected
- Set up a virtual network bridge for inter-VM communication
- Wait for all VMs to become responsive, then terminate them after tests

You can also launch a single VM interactively for debugging:

```bash
python vm_mesh/runner.py --n-x86 1 --name debug-vm
```

## Integration

The VM Mesh is ideal for:

- End-to-end protocol validation across multiple nodes
- Simulating network conditions and consensus scenarios
- Automated CI/CD pipelines for distributed ledger testing
- Cross-architecture compatibility checks
