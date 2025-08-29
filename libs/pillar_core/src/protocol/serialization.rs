use std::alloc::Layout;

use bytemuck::bytes_of;
use pillar_crypto::{proofs::MerkleProof, types::StdByteArray};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::{accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::Peer, primitives::{block::{Block, BlockHeader}, messages::Message, transaction::{Transaction, TransactionFilter}}};

/// This trait is for converting to protocol endian format
/// is a noop on a LE machine, which is a machine that can 
/// interpret it by default
pub trait PillarNativeEndian {
    fn to_le(&mut self);
}

pub trait PillarSerialize : Serialize + for<'a> Deserialize<'a> + Sized {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let encoded = bincode::serialize(&self)
            .map_err(std::io::Error::other)?;
        Ok(encoded)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let decoded = bincode::deserialize::<Self>(data)
            .map_err(std::io::Error::other)?;
        Ok(decoded)
    }
}

pub fn package_standard_message(message: &Message) -> Result<Vec<u8>, std::io::Error> {
    let mut buffer = vec![];
    let mbuff = message.serialize_pillar()?;
    buffer.extend((mbuff.len() as u32).to_le_bytes());
    buffer.extend(mbuff);
    Ok(buffer)
}

pub async fn read_standard_message(stream: &mut TcpStream) -> Result<Message, std::io::Error>{    
    let length = stream.read_u32_le().await?;
    let layout = Layout::from_size_align(length as usize, 8).unwrap();
    unsafe{
        let buffer = std::alloc::alloc(layout);
        let message_buffer = core::slice::from_raw_parts_mut(buffer, length as usize);
        stream.read_exact(message_buffer).await?;
        let message = PillarSerialize::deserialize_pillar(&message_buffer)?;
        Ok(message)
    }
}

impl PillarSerialize for crate::primitives::messages::Message {
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
                let bbuff = b.to_le_bytes();
                buff.extend((bbuff.len() as u32).to_le_bytes());
                buff.extend(bbuff);
                buff.extend(t.to_le_bytes());
                Ok(buff)
            },
            Self::PercentileFilteredPeerResponse(c) => c.serialize_pillar(),
            Self::Error(c) => c.serialize_pillar(),
            _ => Ok(vec![])
        }?;

        let code = self.code();
        let mut buffer = vec![];
        buffer.extend(code.to_le_bytes());
        buffer.extend(payload_buffer);
        Ok(buffer)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let n = 1;
        let code = match data.get(0) {
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
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown message code")),
        };
        Ok(result)
    }
}


impl PillarSerialize for Transaction {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = bytes_of(self);

        let mut le_tx: Transaction;
        if cfg!(target_endian = "big") {
            le_tx = *self;
            le_tx.to_le();
            bytes = bytes_of(&le_tx);
        }
        Ok(bytes.to_vec())
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < size_of::<Self>() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Insufficient data"));
        }

        let layout = Layout::from_size_align(size_of::<Self>(), 8);
        // copy buffer into the new memory
        let buffer = unsafe{
            let ptr = std::alloc::alloc(layout.unwrap());
            std::slice::from_raw_parts_mut(ptr, size_of::<Self>())
        };
        buffer.copy_from_slice(data);
        let mut tx_le: Self = *bytemuck::from_bytes(&buffer);
        if cfg!(target_endian = "big") {
            tx_le.to_le();
        }
        Ok(tx_le)
    }
}

impl PillarSerialize for BlockHeader {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = bytes_of(self);

        let mut le_block: BlockHeader;
        if cfg!(target_endian = "big") {
            le_block = *self;
            le_block.to_le();
            bytes = bytes_of(&le_block);
        }
        Ok(bytes.to_vec())
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < size_of::<Self>() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Insufficient data"));
        }
        let layout = Layout::from_size_align(size_of::<Self>(), 8);
        let buffer = unsafe {
            let ptr = std::alloc::alloc(layout.unwrap());
            std::slice::from_raw_parts_mut(ptr, size_of::<Self>())
        };
        buffer.copy_from_slice(data);
        let mut tx_le: Self = *bytemuck::from_bytes(&buffer);
        if cfg!(target_endian = "big") {
            tx_le.to_le();
        }
        Ok(tx_le)
    }
}

impl PillarSerialize for Block{

    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = self.header.serialize_pillar()?;
        // extend with transactions one at a time.
        assert!(bytes.len() % 8 == 0); // ensure alignment
        bytes.extend((self.transactions.len() as u64).to_le_bytes());
        for transaction in &self.transactions {
            bytes.extend(transaction.serialize_pillar()?);
        }
        Ok(bytes)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let nheader_bytes = size_of::<BlockHeader>();
        let size_transaction = size_of::<Transaction>();

        let header = BlockHeader::deserialize_pillar(&data[..nheader_bytes])?;
        let ntransactions = u64::from_le_bytes(data[nheader_bytes..nheader_bytes + 8].try_into().unwrap());
        let mut transactions = Vec::with_capacity(ntransactions as usize);
        let mut offset = nheader_bytes + 8;
        for _ in 0..ntransactions {
            let tx = Transaction::deserialize_pillar(&data[offset..offset+size_transaction])?;
            transactions.push(tx);
            offset += size_transaction;
        }
        Ok(Block {
            header,
            transactions,
        })
    }
    
}

impl PillarSerialize for TransactionFilter {

}

impl PillarSerialize for MerkleProof {

}

impl PillarSerialize for String{

}

impl<T: PillarSerialize> PillarSerialize for Vec<T>{

}

impl<T: PillarSerialize> PillarSerialize for Option<T> {

}

impl PillarSerialize for Peer {

}

impl PillarSerialize for Chain {

}

impl PillarSerialize for ChainShard {

}

impl PillarSerialize for TransactionStub{

}

impl PillarSerialize for StdByteArray {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        Ok(self.to_vec())
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let buff: &StdByteArray = data
            .try_into()
            .map_err(|_| std::io::ErrorKind::InvalidData)?;
        Ok(*buff)
    }
}

impl PillarNativeEndian for BlockHeader {
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
    fn to_le(&mut self) {
        self.header.amount = self.header.amount.to_le();
        self.header.timestamp = self.header.timestamp.to_le();
        self.header.nonce = self.header.nonce.to_le();
    }
}

mod tests {
    use std::num::{NonZero, NonZeroU64};

    use pillar_crypto::hashing::{DefaultHash, Hashable};
    use serde::de::IntoDeserializer;

    use crate::{primitives::{block::{Block, BlockHeader, Stamp}, transaction::Transaction}, protocol::{reputation::N_TRANSMISSION_SIGNATURES, serialization::PillarSerialize, versions::Versions}};

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
}