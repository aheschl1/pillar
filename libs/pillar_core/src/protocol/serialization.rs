use pillar_crypto::{proofs::MerkleProof, types::StdByteArray};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::{accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::Peer, primitives::{block::{Block, BlockHeader}, messages::Message, transaction::{Transaction, TransactionFilter}}};

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

pub fn package_standard_message<T: PillarSerialize>(message: &T) -> Result<Vec<u8>, std::io::Error> {
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

impl PillarSerialize for BlockHeader {

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

impl PillarSerialize for Block{

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

