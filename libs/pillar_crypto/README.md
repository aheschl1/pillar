# Pillar Crypto

This crate provides the core cryptographic primitives for the Pillar protocol.

## Core cryptographic and data structures

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

## Encryption (`pillar_crypto::encryption`)

- Shared secret via X25519 key exchange.
- Symmetric encryption with XChaCha20-Poly1305.
- `SharedSecret` struct encapsulates key exchange and encryption/decryption methods.
- Usage:
  - Generate ephemeral key pair.
  - Exchange public keys.
  - Derive shared secret.
  - Encrypt/decrypt messages with the shared secret.
