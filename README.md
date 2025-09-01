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

## Testing

```bash
cargo test
```

Note: Tests log to `libs/pillar_core/test_output/{timestamp}/output.log`. Some log errors can occur; they don’t indicate failing tests.

## Documentation

Featured modules with examples:

- `pillar_crypto::hashing` – Hashable trait and DefaultHash
  - Example shows implementing `Hashable` for your type and computing a digest.
- `pillar_crypto::merkle` – Build a Merkle tree from your data
  - Example demonstrates creating a tree and fetching its root hash.
- `pillar_crypto::proofs` – Proofs of inclusion
  - Binary Merkle proof example (generate + verify)
  - Merkle trie proof verification (no_run example)
- `pillar_crypto::merkle_trie` – State storage with branching roots
  - Example shows genesis, insert, branch, and get_all.
- `pillar_crypto::signing` – Sign and verify (ed25519 defaults)
  - Example signs a message and verifies it.
- `pillar_core::nodes::node` – Node behavior and broadcasting
  - no_run example for broadcasting a request.
- `pillar_core::accounting` – Accounts, wallets, and state manager
  - Account and Wallet struct docs include simple examples; state manager docs include a no_run branch example.

## Serialization and platform notes

Serialization works only for 64-bit targets but is endian-agnostic. Big-endian machines may serialize less efficiently due to byte swaps for some fixed-size primitives.
