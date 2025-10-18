use std::collections::HashMap;

use pillar_crypto::{merkle_trie::MerkleTrie, types::{StdByteArray, STANDARD_ARRAY_LENGTH}};

use pillar_serialize::{PillarFixedSize, PillarNativeEndian, PillarSerialize};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::{accounting::{account::{Account, TransactionStub}, state::StateManager}, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::{Peer, PillarIPAddr}, primitives::{block::{Block, BlockHeader}, messages::Message, transaction::{Transaction, TransactionFilter}}, reputation::history::{HeaderShard, NodeHistory}};

impl PillarFixedSize for BlockHeader                 {}
impl PillarFixedSize for Transaction                 {}
impl PillarFixedSize for Peer                        {}
impl PillarFixedSize for TransactionStub             {}
impl PillarFixedSize for HeaderShard                 {}
impl PillarFixedSize for PillarIPAddr                {}

/// Pack a serialized `Message` with a 4-byte little-endian length prefix.
///
/// Wire layout: `[len:u32][message-bytes...]`, where `message-bytes` begins with the
/// 1-byte `Message` code followed by the variant payload (see `impl PillarSerialize for Message`).
pub fn package_standard_message(message: &Message) -> Result<Vec<u8>, std::io::Error> {
    let mut buffer = vec![];
    let mbuff = message.serialize_pillar()?;
    buffer.extend((mbuff.len() as u32).to_le_bytes());
    buffer.extend(mbuff);
    Ok(buffer)
}

/// Read one length-prefixed `Message` from a TCP stream.
///
/// Expects the stream to provide a 4-byte little-endian length, then exactly that many
/// bytes comprising a serialized `Message` (code + payload). Returns the decoded `Message`.
pub async fn read_standard_message(stream: &mut TcpStream) -> Result<Message, std::io::Error>{    
    let length = stream.read_u32_le().await?;
    let mut buffer = vec![0u8; length as usize];
    stream.read_exact(&mut buffer).await?;
    let message = PillarSerialize::deserialize_pillar(&buffer)?;
    Ok(message)
}

impl PillarSerialize for crate::primitives::messages::Message {
    /// Serialize a `Message` as a 1-byte code followed by a variant-specific payload.
    ///
    /// Notes on payload layout and alignment:
    /// - All multi-byte integers are little-endian. Types that implement `PillarNativeEndian`
    ///   are converted to LE before raw byte emission.
    /// - Fixed-size, POD-like structs (repr(C, align(8)) + Pod + Zeroable + PillarFixedSize)
    ///   serialize as tight raw bytes after `to_le()`; no internal length prefix is included.
    /// - Vectors are serialized in two ways:
    ///   - For fixed-size items (as above): concatenated items with no length prefix. The count
    ///     is inferred by dividing the slice length by `size_of::<T>()`.
    ///   - Otherwise: a `u32` length followed by each element length-prefixed.
    /// - Special cases:
    ///   - `TransactionFilterRequest(filter, peer)` -> `[filter_len:u32][filter][peer]`.
    ///   - `TransactionFilterResponse(filter, header)` -> `[filter_len:u32][filter][header]`.
    ///   - `PercentileFilteredPeerRequest(lower, upper)` -> `[lower:f32][upper:f32]`.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let payload_buffer = match self {
            Self::ChainResponse(c) => c.serialize_pillar(),
            Self::PeerResponse(c) => c.serialize_pillar(),
            Self::Declaration(c) => c.serialize_pillar(),
            Self::TransactionBroadcast(c) => c.serialize_pillar(),
            Self::BlockTransmission(c) => c.serialize_pillar(),
            Self::BlockRequest(array) => array.serialize_pillar(),
            Self::BlockResponse(c) => c.serialize_pillar(),
            Self::ChainShardResponse(c) => c.serialize_pillar(),
            Self::TransactionProofRequest(c) => c.serialize_pillar(),
            Self::TransactionProofResponse(c) => c.serialize_pillar(),
            Self::TransactionFilterRequest(c, d) => {
                let mut buff = vec![];
                let cbuff = c.serialize_pillar()?;
                buff.extend((cbuff.len() as u32).to_le_bytes());
                buff.extend(cbuff);
                buff.extend(d.serialize_pillar()?);
                Ok(buff)
            },
            Self::TransactionFilterResponse(c, d) => {
                let mut buff = vec![];
                let cbuff = c.serialize_pillar()?;
                buff.extend((cbuff.len() as u32).to_le_bytes());
                buff.extend(cbuff);
                buff.extend(d.serialize_pillar()?);
                Ok(buff)
            },
            Self::ChainSyncRequest(c) => c.serialize_pillar(),
            Self::ChainSyncResponse(c) => c.serialize_pillar(),
            Self::PercentileFilteredPeerRequest(b, t) => {
                let mut buff = vec![];
                buff.extend(b.to_le_bytes());
                buff.extend(t.to_le_bytes());
                Ok(buff)
            },
            Self::PercentileFilteredPeerResponse(c) => c.serialize_pillar(),
            Self::DiscoveryResponse(c) => c.serialize_pillar(),
            Self::Error(c) => c.serialize_pillar(),
            _ => Ok(vec![])
        }?;

        let code = self.code();
        let mut buffer = vec![];
        buffer.extend(code.to_le_bytes());
        buffer.extend(payload_buffer);
        Ok(buffer)
    }

    /// Deserialize a `Message` from bytes beginning with a 1-byte code.
    ///
    /// Important framing details to remember:
    /// - `Block`, `Vec<Transaction>`, `Vec<HeaderShard>`, etc. may use the fixed-size vector encoding
    ///   (no internal length). In these cases, the deserializer consumes the remainder of the provided slice.
    /// - For `TransactionFilterRequest/Response`, the embedded `TransactionFilter` is length-prefixed with a `u32`
    ///   so we can unambiguously parse the subsequent field.
    /// - For `PercentileFilteredPeerRequest`, we expect `[lower:f32][upper:f32]`.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let n = 1;
        let code = match data.first() {
            Some(b) => *b,
            None => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Data too short")),
        };
        // let code = u8::from_le_bytes(code_bytes);
        let result = match code {
            0 => Message::Ping,
            1 => Message::ChainRequest,
            2 => Message::ChainResponse(Chain::deserialize_pillar(&data[n..])?),
            3 => Message::PeerRequest,
            4 => Message::PeerResponse(Vec::deserialize_pillar(&data[n..])?),
            5 => Message::Declaration(Peer::deserialize_pillar(&data[n..])?),
            6 => Message::TransactionBroadcast(Transaction::deserialize_pillar(&data[n..])?),
            7 => Message::TransactionAck,
            8 => Message::BlockTransmission(Block::deserialize_pillar(&data[n..])?),
            9 => Message::BlockAck,
            10 => Message::BlockRequest(StdByteArray::deserialize_pillar(&data[n..])?),
            11 => Message::BlockResponse(Option::<Block>::deserialize_pillar(&data[n..])?),
            12 => Message::ChainShardRequest,
            13 => Message::ChainShardResponse(ChainShard::deserialize_pillar(&data[n..])?),
            14 => Message::TransactionProofRequest(TransactionStub::deserialize_pillar(&data[n..])?),
            15 => Message::TransactionProofResponse(pillar_crypto::proofs::MerkleProof::deserialize_pillar(&data[n..])?),
            16 => {
                let filter_length = u32::from_le_bytes(data[n..n+4].try_into().unwrap());
                let filter = TransactionFilter::deserialize_pillar(&data[n+4..n+4 + filter_length as usize])?;
                let peer = Peer::deserialize_pillar(&data[n+4 + filter_length as usize..])?;
                Message::TransactionFilterRequest(filter, peer)
            },
            17 => Message::TransactionFilterAck,
            18 => {
                let filter_length = u32::from_le_bytes(data[n..n+4].try_into().unwrap());
                let filter = TransactionFilter::deserialize_pillar(&data[n+4..n+4 + filter_length as usize])?;
                let header = BlockHeader::deserialize_pillar(&data[n+4 + filter_length as usize..])?;
                Message::TransactionFilterResponse(filter, header)
            },
            19 => Message::ChainSyncRequest(Vec::<StdByteArray>::deserialize_pillar(&data[n..])?),
            20 => Message::ChainSyncResponse(Vec::<Chain>::deserialize_pillar(&data[n..])?),
            21 => {
                let lower = f32::from_le_bytes(data[n..n+4].try_into().unwrap());
                let upper = f32::from_le_bytes(data[n+4..n+8].try_into().unwrap());
                Message::PercentileFilteredPeerRequest(lower, upper)
            },
            22 => Message::PercentileFilteredPeerResponse(Vec::<Peer>::deserialize_pillar(&data[n..])?),
            23 => Message::Error(String::deserialize_pillar(&data[n..])?),
            24 => Message::DiscoveryRequest,
            25 => Message::DiscoveryResponse(Peer::deserialize_pillar(&data[n..])?),
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown message code")),
        };
        Ok(result)
    }

}

impl PillarSerialize for StateManager {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = vec![];
        let trie_bytes = self.state_trie.serialize_pillar()?;
        bytes.extend((trie_bytes.len() as u32).to_le_bytes());
        bytes.extend(trie_bytes);
        bytes.extend(self.reputations.serialize_pillar()?);
        Ok(bytes)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let trielen = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let trie = MerkleTrie::<StdByteArray, Account>::deserialize_pillar(&data[4..trielen + 4])?;
        let reputations = HashMap::<StdByteArray, NodeHistory>::deserialize_pillar(&data[trielen + 4..])?;
        Ok(StateManager { state_trie: trie, reputations })
    }
}

impl PillarSerialize for Block{
    /// Serialize a `Block` as `[header][transactions...]`.
    ///
    /// - `header` is a fixed-size `BlockHeader` (Pod) serialized as raw bytes after `to_le()`.
    /// - `transactions` uses the fixed-size vector encoding of `Transaction` (no length field);
    ///   the item count must be inferred by the consumer from the total payload length.
    ///
    /// Safety: asserts that the header byte length is a multiple of 8 to preserve alignment
    /// expectations across platforms.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = self.header.serialize_pillar()?;
        assert!(bytes.len().is_multiple_of(8)); // ensure alignment
        bytes.extend(self.transactions.serialize_pillar()?);
        Ok(bytes)
    }

    /// Deserialize a `Block` from `[header][transactions...]`, consuming the entire slice.
    ///
    /// Because transactions use the fixed-size vector encoding, we pass the remainder of the
    /// slice to `Vec::<Transaction>::deserialize_pillar`, which computes the count by dividing
    /// by `size_of::<Transaction>()`. Ensure the slice length is exact.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let nheader_bytes = size_of::<BlockHeader>();
        let header = BlockHeader::deserialize_pillar(&data[..nheader_bytes])?;
        let transactions: Vec<Transaction> = Vec::<Transaction>::deserialize_pillar(&data[nheader_bytes..])?;
        Ok(Block {
            header,
            transactions,
        })
    }
}

impl PillarSerialize for Chain {
    /// Serialize a `Chain` as length-delimited sections followed by fixed fields:
    /// `[blocks_len:u32][blocks_bytes][headers_len:u32][headers_bytes][depth:u64]
    ///  [deepest_hash:32][leaves_len:u32][leaves_bytes]`.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        // key pair hashmap
        let blocks_ser = self.blocks.serialize_pillar()?;
        buffer.extend((blocks_ser.len() as u32).to_le_bytes());
        buffer.extend(blocks_ser);
        let header_ser = self.headers.serialize_pillar()?;
        buffer.extend((header_ser.len() as u32).to_le_bytes());
        buffer.extend(header_ser);
        buffer.extend(self.depth.to_le_bytes());
        buffer.extend(self.deepest_hash.serialize_pillar()?);
        // TODO maybe big clone
        buffer.extend(self.leaves.iter().cloned().collect::<Vec<_>>().serialize_pillar()?);
        // 
        Ok(buffer)
    }

    /// Inverse of `serialize_pillar` for `Chain`.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut offset = 0;

        let blocks_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let blocks = HashMap::<StdByteArray, Block>::deserialize_pillar(&data[offset..offset + blocks_len as usize])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Failed to deserialize blocks: {e}")))?;

        offset += blocks_len as usize;
        let headers_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let headers = HashMap::<StdByteArray, BlockHeader>::deserialize_pillar(&data[offset..offset + headers_len as usize])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Failed to deserialize headers: {e}")))?;

        offset += headers_len as usize;
        let depth = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let deepest_hash = StdByteArray::deserialize_pillar(&data[offset..offset + 32])?;
        offset += 32;

        // TODO make hashet deserialization
        let leaves = Vec::<StdByteArray>::deserialize_pillar(&data[offset..])?;

        Ok(Chain {
            blocks,
            headers,
            depth,
            deepest_hash,
            leaves: leaves.into_iter().collect(),
            state_manager: StateManager::default(),
        })
    }

}

impl PillarSerialize for ChainShard {
    /// Serialize a `ChainShard` as `[headers_len:u32][headers_bytes][leaves_len:u32][leaves_bytes]`.
    /// `headers_bytes` is a map of `StdByteArray -> BlockHeader` (fixed-size values), and
    /// `leaves_bytes` is a length-prefixed vector of 32-byte hashes.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let header_bytes = self.headers.serialize_pillar()?;
        let leaves_bytes = self.leaves.iter().copied().collect::<Vec<_>>().serialize_pillar()?;
        let mut buffer = vec![];
        buffer.extend((header_bytes.len() as u32).to_le_bytes());
        buffer.extend(header_bytes);
        buffer.extend((leaves_bytes.len() as u32).to_le_bytes());
        buffer.extend(leaves_bytes);
        Ok(buffer)
    }

    /// Inverse of `serialize_pillar` for `ChainShard`.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut offset = 0;
        let header_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let header = HashMap::<StdByteArray, BlockHeader>::deserialize_pillar(&data[offset..offset + header_len])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Failed to deserialize header: {e}")))?;
        offset += header_len;

        let leaves_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let leaves = Vec::<StdByteArray>::deserialize_pillar(&data[offset..offset + leaves_len])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Failed to deserialize leaves: {e}")))?;
        offset += leaves_len;

        Ok(ChainShard { headers: header, leaves: leaves.iter().copied().collect() })
    }
}

impl PillarSerialize for TransactionFilter {
    /// Serialize a transaction filter as the concatenation of its three `Option` fields:
    /// `[sender_opt:33][receiver_opt:33][amount_opt:9]`.
    /// See `Option<T>` encoding in `pillar_serialize` for details.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = self.sender.serialize_pillar()?;
        buffer.extend(self.receiver.serialize_pillar()?);
        buffer.extend(self.amount.serialize_pillar()?);
        Ok(buffer)
    }

    /// Deserialize a transaction filter using fixed offsets (33, 33, 9) for its three options.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut offset = 0;
        let sender = Option::<StdByteArray>::deserialize_pillar(&data[offset..offset + 33])?;
        offset += 33;
        let receiver = Option::<StdByteArray>::deserialize_pillar(&data[offset..offset + 33])?;
        offset += 33;
        let amount = Option::<u64>::deserialize_pillar(&data[offset..offset + 9])?;
        Ok(TransactionFilter { sender, receiver, amount })
    }
}

impl PillarNativeEndian for BlockHeader {
    /// Convert native-endian fields to little-endian in-place for `BlockHeader`.
    fn to_le(&mut self) {
        self.nonce = self.nonce.to_le();
        self.timestamp = self.timestamp.to_le();
        self.depth = self.depth.to_le();
        if let Some(c) = self.completion.as_mut() {
            c.difficulty_target = c.difficulty_target.to_le();
        }
    }
}

impl PillarNativeEndian for Transaction {
    /// Convert native-endian fields to little-endian in-place for `Transaction` header.
    fn to_le(&mut self) {
        self.header.amount = self.header.amount.to_le();
        self.header.timestamp = self.header.timestamp.to_le();
        self.header.nonce = self.header.nonce.to_le();
    }
}

impl PillarNativeEndian for TransactionStub {
    fn to_le(&mut self) {}
}

impl PillarNativeEndian for Peer {
    /// Convert native-endian fields to little-endian in-place for `Peer`.
    fn to_le(&mut self) {
        self.port = self.port.to_le();
    }
}

impl PillarNativeEndian for HeaderShard {
    /// Convert native-endian fields to little-endian in-place for `HeaderShard`.
    fn to_le(&mut self) {
        self.depth = self.depth.to_le();
        self.timestamp = self.timestamp.to_le();
        self.n_stamps = self.n_stamps.to_le();
    }
}

impl PillarNativeEndian for PillarIPAddr {
    fn to_le(&mut self) {}
}

impl PillarSerialize for NodeHistory {
    /// Serialize a node's history as `[public_key:32][blocks_mined_len:u32][blocks_mined_bytes][blocks_stamped...]`.
    ///
    /// `blocks_mined` is explicitly length-delimited. `blocks_stamped` uses the fixed-size vector
    /// encoding (no internal length) and consumes the remainder of the slice.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        buffer.extend(self.public_key.serialize_pillar()?);
        let blockbuf = self.blocks_mined.serialize_pillar()?;
        buffer.extend((blockbuf.len() as u32).to_le_bytes());
        buffer.extend(blockbuf);
        buffer.extend(self.blocks_stamped.serialize_pillar()?);
        Ok(buffer)
    }

    /// Inverse of `serialize_pillar` for `NodeHistory`.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut offset = 0;
        let public_key = StdByteArray::deserialize_pillar(&data[offset..offset + STANDARD_ARRAY_LENGTH])?;
        offset += STANDARD_ARRAY_LENGTH;
        let length = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let blocks_mined = Vec::<HeaderShard>::deserialize_pillar(&data[offset..offset + length])?;
        let blocks_stamped = Vec::<HeaderShard>::deserialize_pillar(&data[offset + length..])?;
        Ok(NodeHistory { public_key, blocks_mined, blocks_stamped })
    }
}

impl PillarSerialize for Account {
    /// Serialize an account as `[address:32][balance:u64][nonce:u64][history:Option<NodeHistory>]`.
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
       let mut buffer = vec![];
       buffer.extend(self.address.serialize_pillar()?);
       buffer.extend(self.balance.serialize_pillar()?);
       buffer.extend(self.nonce.serialize_pillar()?);
       buffer.extend(self.history.serialize_pillar()?);
       Ok(buffer)
    }

    /// Inverse of `serialize_pillar` for `Account`.
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let address = StdByteArray::deserialize_pillar(&data[0..STANDARD_ARRAY_LENGTH])?;
        let balance = u64::deserialize_pillar(&data[STANDARD_ARRAY_LENGTH..STANDARD_ARRAY_LENGTH + 8])?;
        let nonce = u64::deserialize_pillar(&data[STANDARD_ARRAY_LENGTH + 8..STANDARD_ARRAY_LENGTH + 16])?;
        let history = Option::<NodeHistory>::deserialize_pillar(&data[STANDARD_ARRAY_LENGTH + 16..])?;
        Ok(Account { address, balance, nonce, history })
    }
}

mod tests {
    use pillar_crypto::hashing::{DefaultHash, Hashable};
    use pillar_serialize::PillarSerialize;

    use crate::{
        accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::Peer, primitives::{
            block::{Block, BlockHeader, Stamp},
            messages::Message,
            transaction::{Transaction, TransactionFilter},
        }, protocol::{reputation::N_TRANSMISSION_SIGNATURES, versions::Versions}
    };
    use pillar_crypto::proofs::{MerkleProof, HashDirection};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_block_alignment() {
    // this is a fragile test because it relies on the exact memory layout of the Block struct
    // this may differ between architectures
        let previous_hash = [2; 32];
        let sender = [3; 32];
        let receiver = [4; 32];
        let amount = 5;
        let timestamp = 6;
        let nonce = 7;
        let depth = 9;
        let mut hash_function = DefaultHash::new();
        let transaction = Transaction::new(
            sender, 
            receiver, 
            amount, 
            timestamp, 
            nonce, 
            &mut hash_function
        );
        let block = Block::new(
            previous_hash, 
            0,
            timestamp,
            vec![transaction],
            None,
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            depth,
            None,
            None,
            &mut DefaultHash::new(),
        );
        let merkle_root = block.header.merkle_root;
        let total_size = size_of::<Block>();

        let pointer: *const Block = &block;
        unsafe {
            let block_ref: &Block = &*pointer;
            assert_eq!(block_ref.header.previous_hash, previous_hash);

            let slice = std::slice::from_raw_parts(pointer as *const u8, total_size);
            assert_eq!(slice[0..32], previous_hash);
            assert_eq!(slice[32..64], merkle_root);
            assert_eq!(slice[64..72], [0; 8]); // nonce
            assert_eq!(slice[72..80], timestamp.to_ne_bytes());
            assert_eq!(slice[80..88], depth.to_ne_bytes()); // depth
            assert_eq!(slice[88..90], Versions::default().to_ne_bytes()); // version
            assert_eq!(slice[90..96], [0; 6]); // explicit padding
            const STAMP_SIZE: usize = 96;
            for i in 0..N_TRANSMISSION_SIGNATURES {
                let start = 96 + i * STAMP_SIZE;
                let end = start + STAMP_SIZE;
                assert_eq!(slice[start..end], [0; STAMP_SIZE]);
            }
            let start: usize = 96 + N_TRANSMISSION_SIGNATURES * STAMP_SIZE;
            // let end = start;
            assert_eq!(slice[start + 94..start + 102], [0; 8]); // state_root
        }
    }

    #[test]
    fn test_block_alignment_complete() {
        // this is a fragile test because it relies on the exact memory layout of the Block struct
        // this may differ between architectures
        let previous_hash = [2; 32];
        let sender = [3; 32];
        let receiver = [4; 32];
        let amount = 5;
        let timestamp = 6;
        let nonce = 7;
        let depth = 9;
        let miner_address = [1; 32];
        let difficulty = 10;
        let state_root = [5; 32];
        let mut hash_function = DefaultHash::new();
        let transaction = Transaction::new(
            sender, 
            receiver, 
            amount, 
            timestamp, 
            nonce, 
            &mut hash_function
        );
        let block = Block::new(
            previous_hash, 
            0,
            timestamp,
            vec![transaction],
            Some(miner_address),
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            depth,
            Some(difficulty),
            Some(state_root),
            &mut DefaultHash::new(),
        );
        let merkle_root = block.header.merkle_root;
        let total_size = size_of::<Block>();

        let pointer: *const Block = &block;
        unsafe {
            let block_ref: &Block = &*pointer;
            assert_eq!(block_ref.header.previous_hash, previous_hash);

            let slice = std::slice::from_raw_parts(pointer as *const u8, total_size);
            assert_eq!(slice[0..32], previous_hash);
            assert_eq!(slice[32..64], merkle_root);
            assert_eq!(slice[64..72], [0; 8]); // nonce
            assert_eq!(slice[72..80], timestamp.to_ne_bytes());
            assert_eq!(slice[80..88], depth.to_ne_bytes()); // depth
            assert_eq!(slice[88..90], Versions::default().to_ne_bytes()); // version
            assert_eq!(slice[90..96], [0; 6]); // explicit padding
            const STAMP_SIZE: usize = 96;
            let mut start: usize = 96;
            for _ in 0..N_TRANSMISSION_SIGNATURES {
                let end = start + STAMP_SIZE;
                assert_eq!(slice[start..end], [0; STAMP_SIZE]);
                start = end;
            }
            // let end = start;
            assert_eq!(slice[start .. start + 32], block.header.hash(&mut DefaultHash::new()).unwrap());
            assert_eq!(slice[start + 32..start + 64], miner_address);
            assert_eq!(slice[start + 64..start + 96], state_root); // state_root
            assert_eq!(slice[start + 96..start + 104], difficulty.to_ne_bytes()); // difficulty
        }
    }

    #[test]
    fn test_header_serialize() {
        let header = BlockHeader::default();
        let serialized = header.serialize_pillar().unwrap();
        let deserialized: BlockHeader = BlockHeader::deserialize_pillar(&serialized).unwrap();
        assert_eq!(header, deserialized);
        // more complex
        let previous_hash = [2; 32];
        let sender = [3; 32];
        let receiver = [4; 32];
        let amount = 5;
        let timestamp = 6;
        let nonce = 7;
        let depth = 9;
        let miner_address = [1; 32];
        let difficulty = 10;
        let state_root = [5; 32];
        let mut hash_function = DefaultHash::new();
        let transaction = Transaction::new(
            sender, 
            receiver, 
            amount, 
            timestamp, 
            nonce, 
            &mut hash_function
        );
        let header = Block::new(
            previous_hash, 
            0,
            timestamp,
            vec![transaction],
            Some(miner_address),
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            depth,
            Some(difficulty),
            Some(state_root),
            &mut DefaultHash::new(),
        ).header;
        let serialized = header.serialize_pillar().unwrap();
        let deserialized: BlockHeader = BlockHeader::deserialize_pillar(&serialized).unwrap();
        assert_eq!(header, deserialized);
    }

    #[test]
    fn test_transaction_serialize() {
        let tx = Transaction::new(
            [1; 32],
            [2; 32],
            3,
            4,
            5,
            &mut DefaultHash::new()
        );
        let serialized = tx.serialize_pillar().unwrap();
        let deserialized: Transaction = Transaction::deserialize_pillar(&serialized).unwrap();
        assert_eq!(tx, deserialized);

        // now test more convoluted transaction
        let tx2 = Transaction::new(
            [8; 32],
            [1; 32],
            30000,
            434882983,
            5289432,
            &mut DefaultHash::new()
        );
        let serialized = tx2.serialize_pillar().unwrap();
        let deserialized: Transaction = Transaction::deserialize_pillar(&serialized).unwrap();
        assert_eq!(tx2, deserialized);

    }

    #[test]
    fn test_block_serialization(){
        let previous_hash = [2; 32];
        let sender = [3; 32];
        let receiver = [4; 32];
        let amount = 5;
        let timestamp = 6;
        let nonce = 7;
        let depth = 9;
        let miner_address = [1; 32];
        let difficulty = 10;
        let state_root = [5; 32];
        let mut hash_function = DefaultHash::new();
        let transaction = Transaction::new(
            sender,
            receiver,
            amount,
            timestamp,
            nonce,
            &mut hash_function
        );
        let block = Block::new(
            previous_hash,
            0,
            timestamp,
            vec![transaction],
            Some(miner_address),
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            depth,
            Some(difficulty),
            Some(state_root),
            &mut DefaultHash::new(),
        );
        let serialized = block.serialize_pillar().unwrap();
        let deserialized: Block = Block::deserialize_pillar(&serialized).unwrap();
        assert_eq!(block, deserialized);

        // now block multiple transaction

        let tx2 = Transaction::new(
            [1; 32],
            [2; 32],
            3,
            4,
            5,
            &mut DefaultHash::new()
        );
        let tx3 = Transaction::new(
            [1; 32],
            [2; 32],
            3,
            4,
            5,
            &mut DefaultHash::new()
        );
        let block = Block::new(
            previous_hash,
            0,
            timestamp,
            vec![transaction, tx2, tx3],
            Some(miner_address),
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            depth,
            Some(difficulty),
            Some(state_root),
            &mut DefaultHash::new(),
        );

        let serialized = block.serialize_pillar().unwrap();
        let deserialized: Block = Block::deserialize_pillar(&serialized).unwrap();
        assert_eq!(block, deserialized);

        // test incomplete block - one with None root
        let block = Block::new(
            previous_hash,
            0,
            timestamp,
            vec![transaction, tx2, tx3],
            None,
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            depth,
            None,
            None,
            &mut DefaultHash::new(),
        );

        let serialized = block.serialize_pillar().unwrap();
        let deserialized: Block = Block::deserialize_pillar(&serialized).unwrap();
        assert_eq!(block, deserialized);

    }

    // =============================
    // Message serialization tests
    // =============================

    fn assert_roundtrip(msg: &Message) {
        let bytes = msg.serialize_pillar().expect("serialize");
        let rt = Message::deserialize_pillar(&bytes).expect("deserialize");
        let bytes2 = rt.serialize_pillar().expect("serialize 2");
        assert_eq!(bytes, bytes2, "Round-trip bytes must match for {}", msg.name());
    }

    fn make_peer(pk: u8, port: u16) -> Peer {
        Peer::new([pk; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    fn make_tx() -> Transaction {
        Transaction::new([1; 32], [2; 32], 3, 4, 5, &mut DefaultHash::new())
    }

    fn make_block() -> Block {
        let transaction = make_tx();
        Block::new(
            [9; 32],
            0,
            42,
            vec![transaction],
            None,
            [Stamp::default(); N_TRANSMISSION_SIGNATURES],
            1,
            None,
            None,
            &mut DefaultHash::new(),
        )
    }

    fn make_chain() -> Chain {
        Chain::new_with_genesis()
    }

    fn make_chain_shard() -> ChainShard {
        let c = make_chain();
        c.into()
    }

    fn make_tx_stub() -> TransactionStub { TransactionStub { block_hash: [7; 32], transaction_hash: [8; 32] } }

    fn make_merkle_proof() -> MerkleProof {
        MerkleProof { hashes: vec![[1; 32], [2; 32]], directions: vec![HashDirection::Left, HashDirection::Right], root: [3; 32] }
    }

    fn make_filter() -> TransactionFilter { TransactionFilter::new(Some([1; 32]), Some([2; 32]), Some(3)) }

    #[test]
    fn test_message_ping() { assert_roundtrip(&Message::Ping); }

    #[test]
    fn test_message_chain_request() { assert_roundtrip(&Message::ChainRequest); }

    #[test]
    fn test_message_chain_response() { assert_roundtrip(&Message::ChainResponse(make_chain())); }

    #[test]
    fn test_message_peer_request() { assert_roundtrip(&Message::PeerRequest); }

    #[test]
    fn test_message_peer_response() { assert_roundtrip(&Message::PeerResponse(vec![make_peer(5, 18080), make_peer(6, 18081)])); }

    #[test]
    fn test_message_declaration() { assert_roundtrip(&Message::Declaration(make_peer(7, 18082))); }

    #[test]
    fn test_message_transaction_broadcast() { assert_roundtrip(&Message::TransactionBroadcast(make_tx())); }

    #[test]
    fn test_message_transaction_ack() { assert_roundtrip(&Message::TransactionAck); }

    #[test]
    fn test_message_block_transmission() { assert_roundtrip(&Message::BlockTransmission(make_block())); }

    #[test]
    fn test_message_block_ack() { assert_roundtrip(&Message::BlockAck); }

    #[test]
    fn test_message_block_request() { assert_roundtrip(&Message::BlockRequest([9; 32])); }

    #[test]
    fn test_message_block_response_some() { assert_roundtrip(&Message::BlockResponse(Some(make_block()))); }

    #[test]
    fn test_message_block_response_none() { assert_roundtrip(&Message::BlockResponse(None)); }

    #[test]
    fn test_message_chain_shard_request() { assert_roundtrip(&Message::ChainShardRequest); }

    #[test]
    fn test_message_chain_shard_response() { assert_roundtrip(&Message::ChainShardResponse(make_chain_shard())); }

    #[test]
    fn test_message_tx_proof_request() { assert_roundtrip(&Message::TransactionProofRequest(make_tx_stub())); }

    #[test]
    fn test_message_tx_proof_response() { assert_roundtrip(&Message::TransactionProofResponse(make_merkle_proof())); }

    #[test]
    fn test_message_tx_filter_request() { assert_roundtrip(&Message::TransactionFilterRequest(make_filter(), make_peer(11, 18083))); }

    #[test]
    fn test_message_tx_filter_ack() { assert_roundtrip(&Message::TransactionFilterAck); }

    #[test]
    fn test_message_tx_filter_response() { assert_roundtrip(&Message::TransactionFilterResponse(make_filter(), BlockHeader::default())); }

    #[test]
    fn test_message_chain_sync_request() { assert_roundtrip(&Message::ChainSyncRequest(vec![[1; 32], [2; 32]])); }

    #[test]
    fn test_message_chain_sync_response() { assert_roundtrip(&Message::ChainSyncResponse(vec![make_chain(), make_chain()])); }

    #[test]
    fn test_message_percentile_filtered_peer_request() { assert_roundtrip(&Message::PercentileFilteredPeerRequest(0.1, 0.9)); }

    #[test]
    fn test_message_percentile_filtered_peer_response() { assert_roundtrip(&Message::PercentileFilteredPeerResponse(vec![make_peer(21, 19000)])); }

    #[test]
    fn test_message_error() { assert_roundtrip(&Message::Error("oops".to_string())); }

    
}