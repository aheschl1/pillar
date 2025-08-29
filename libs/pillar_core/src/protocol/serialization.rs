use std::num::NonZeroU64;

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
    let mut buffer = [0; 4];
    stream.read_exact(&mut buffer).await?;
    let length = u32::from_le_bytes(buffer);
    let mut message_buffer = vec![0; length as usize];
    stream.read_exact(&mut message_buffer).await?;
    let message = PillarSerialize::deserialize_pillar(&message_buffer)?;
    Ok(message)
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
        let code_bytes = match data.get(0){
            Some(&b) => [b],
            None => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing message code")),
        };
        let code = u8::from_le_bytes(code_bytes);
        let result = match code {
            0 => Message::Ping,
            1 => Message::ChainRequest,
            2 => Message::ChainResponse(Chain::deserialize_pillar(&data[1..])?),
            3 => Message::PeerRequest,
            4 => Message::PeerResponse(Vec::deserialize_pillar(&data[1..])?),
            5 => Message::Declaration(Peer::deserialize_pillar(&data[1..])?),
            6 => Message::TransactionBroadcast(Transaction::deserialize_pillar(&data[1..])?),
            7 => Message::TransactionAck,
            8 => Message::BlockTransmission(Block::deserialize_pillar(&data[1..])?),
            9 => Message::BlockAck,
            10 => Message::BlockRequest(StdByteArray::deserialize_pillar(&data[1..])?),
            11 => Message::BlockResponse(Option::<Block>::deserialize_pillar(&data[1..])?),
            12 => Message::ChainShardRequest,
            13 => Message::ChainShardResponse(ChainShard::deserialize_pillar(&data[1..])?),
            14 => Message::TransactionProofRequest(TransactionStub::deserialize_pillar(&data[1..])?),
            15 => Message::TransactionProofResponse(pillar_crypto::proofs::MerkleProof::deserialize_pillar(&data[1..])?),
            16 => {
                let filter_length = u32::from_le_bytes(data[1..5].try_into().unwrap());
                let filter = TransactionFilter::deserialize_pillar(&data[5..5 + filter_length as usize])?;
                let peer = Peer::deserialize_pillar(&data[5 + filter_length as usize..])?;
                Message::TransactionFilterRequest(filter, peer)
            },
            17 => Message::TransactionFilterAck,
            18 => {
                let filter_length = u32::from_le_bytes(data[1..5].try_into().unwrap());
                let filter = TransactionFilter::deserialize_pillar(&data[5..5 + filter_length as usize])?;
                let header = BlockHeader::deserialize_pillar(&data[5 + filter_length as usize..])?;
                Message::TransactionFilterResponse(filter, header)
            },
            19 => Message::ChainSyncRequest(Vec::<StdByteArray>::deserialize_pillar(&data[1..])?),
            20 => Message::ChainSyncResponse(Vec::<Chain>::deserialize_pillar(&data[1..])?),
            21 => {
                let lower = f32::from_le_bytes(data[1..5].try_into().unwrap());
                let upper = f32::from_le_bytes(data[5..9].try_into().unwrap());
                Message::PercentileFilteredPeerRequest(lower, upper)
            },
            22 => Message::PercentileFilteredPeerResponse(Vec::<Peer>::deserialize_pillar(&data[1..])?),
            23 => Message::Error(String::deserialize_pillar(&data[1..])?),
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown message code")),
        };
        Ok(result)
    }
}

impl PillarSerialize for BlockHeader {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut ptr: *const u8 = self as *const _ as *const u8;

        if cfg!(target_endian = "big") {
            let mut le_block = *self;
            le_block.to_le();
            ptr = &le_block as *const _ as *const u8;
        }
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, size_of::<Self>());
            Ok(Vec::from(bytes))
        }
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let ptr = data.as_ptr();
        let header_ptr: *const BlockHeader = ptr as *const BlockHeader;
        let mut header = unsafe { *header_ptr };
        if cfg!(target_endian = "big") {
            header.to_le();
        }
        Ok(header)
    }
}

impl PillarSerialize for Block{
    
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

impl PillarSerialize for Transaction {

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
        self.version = self.version.to_le();
        if let Some(c) = self.completion.as_mut() {
            c.difficulty_target = NonZeroU64::new(c.difficulty_target.get().to_le()).expect("bad assumption");
        }
    }
}

mod tests {
    use std::num::{NonZero, NonZeroU64};

    use pillar_crypto::hashing::{DefaultHash, Hashable};
    use serde::de::IntoDeserializer;

    use crate::{primitives::{block::{Block, BlockHeader, Stamp}, transaction::Transaction}, protocol::{difficulty, reputation::N_TRANSMISSION_SIGNATURES, serialization::PillarSerialize, versions::Versions}};

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
            Some(NonZeroU64::new(difficulty).unwrap()),
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
            Some(NonZeroU64::new(difficulty).unwrap()),
            Some(state_root),
            &mut DefaultHash::new(),
        ).header;
        let serialized = header.serialize_pillar().unwrap();
        let deserialized: BlockHeader = BlockHeader::deserialize_pillar(&serialized).unwrap();
        assert_eq!(header, deserialized);
    }
}