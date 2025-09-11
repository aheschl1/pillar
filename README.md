# <img src="./figures/logo.svg" alt="Pillar Logo" width="50" style="vertical-align: middle; margin-right: 8px;" /> Pillar <!-- markdownlint-disable-line MD033 -->

Zero-trust decentralized ledger with a Proof of Reputation (PoR) trust layer. It combines a conventional transaction layer with a reputation-driven incentive layer to reduce wasted computation, and provide a trust metric.

## Core cryptographic and data structures

This project centers on a small set of transparent primitives.

- Hashing (`pillar_crypto::hashing`)
  - Default: SHA3-256 via `DefaultHash`.
  - `Hashable` trait defines how a type contributes bytes to the hasher.
  - Provided impls for `&str`, `String`, `StdByteArray` (`[u8; 32]`), `Vec<u8>`.
  - Error: calling `digest()` with no prior `update()` yields `std::io::ErrorKind::InvalidInput`.

- Signing (`pillar_crypto::signing`)
  - ed25519 (ed25519-dalek): `DefaultSigner`/`DefaultVerifier`.
  - Traits `SigFunction<K,P,S>` / `SigVerFunction<K,S>` use fixed sizes; defaults are K=32, P=32, S=64.
  - `verify_strict` is used; randomness only in key generation.
  - Messages implement `Signable<S>` and expose canonical bytes via `get_signing_bytes()`.

- Binary Merkle tree (`pillar_crypto::merkle`)
  - Leaf: `leaf = H(H(item_bytes))` (item hashed via `Hashable`, then hashed again as leaf).
  - Internal: `node = H(left || right)` (32-byte concatenations, order preserved).
  - Odd levels duplicate the last leaf to pair.
  - Tree equality is by root hash equality.

- Merkle proof (`pillar_crypto::proofs::MerkleProof`)
  - Contains sibling hashes and `Left`/`Right` directions.
  - Verifier recomputes from `H(prehash)` upward. Input is the pre-hash of the item.
  - Serialized with `u32` lengths (LE) followed by raw bytes.

- Merkle trie (`pillar_crypto::merkle_trie`)
  - Keys: `H(key)` split into nibbles (hi 4 bits, lo 4 bits) to traverse a 16-ary trie.
  - Node hash: append `[i] || hash(child_i)` for present children in ascending `i`; if value present, append `[16] || value_bytes`; hash the concatenation.
  - Values serialized with `PillarSerialize` and stored as raw bytes.
  - Branching (`branch`): clones only necessary path nodes; reference counts track sharing.
  - Trimming (`trim_branch`): decrements references, removes unique nodes.

## High-level architecture

The network and data flow are illustrated below, alongside how blocks are settled and how components interact.

- Network flow: `figures/net_flow.png`
- Chain structure: `figures/structure.png`
- Settle chart: `figures/settle_chart.png`

![Network Flow](./figures/net_flow.png)

## Repository layout

- `libs/pillar_core/` – Core protocol and logic
- `libs/pillar_crypto/` – Cryptographic primitives
- `libs/pillar_serialize/` – Lightweight serialization utilities used across crates.
- `vm_mesh/` – VM Mesh framework for distributed testing and simulation

## Testing

```bash
cargo test
```

Note: Tests log to `libs/pillar_core/test_output/{timestamp}/output.log`. Some log errors can occur; they don’t indicate failing tests.

## Serialization and platform notes

Serialization works only for 64-bit targets but is endian-agnostic. Big-endian machines may serialize less efficiently due to byte swaps for some fixed-size primitives.

## VM Mesh: Distributed Testing and Simulation

The VM Mesh framework enables distributed testing and simulation of the Pillar protocol across multiple virtual machines, supporting both x86_64 and aarch64 architectures. It automates the provisioning, networking, and orchestration of VMs using QEMU, with unified repository injection and cloud-init configuration.

### Features
- **Automated VM Provisioning:** Launch any number of VMs with isolated overlays and custom network bridges.
- **Unified Codebase Injection:** Clones the repository and injects it into all VMs via a shared ISO, ensuring consistent test environments.
- **Cloud-Init Integration:** Uses cloud-init to configure SSH keys, networking, and repository setup for each VM.
- **Network Simulation:** Creates a virtual bridge and tap devices, allowing VMs to communicate as if on a real network.
- **Lifecycle Management:** Start, monitor, and terminate all VMs from a single interface. Supports context-managed operation for clean setup and teardown.
- **Architecture Flexibility:** Supports both x86_64 and aarch64 VMs for cross-platform protocol validation.

### Usage
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

### Integration
The VM Mesh is ideal for:
- End-to-end protocol validation across multiple nodes
- Simulating network conditions and consensus scenarios
- Automated CI/CD pipelines for distributed ledger testing
- Cross-architecture compatibility checks
