use serde::{Deserialize, Serialize};

use crate::primitives::block::Block;

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

impl PillarSerialize for crate::primitives::messages::Message {}

impl PillarSerialize for Block {
    
}
