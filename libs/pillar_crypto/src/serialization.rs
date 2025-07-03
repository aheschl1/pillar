use serde::{Deserialize, Serialize};

pub trait PillarSerialize : Serialize + for<'a> Deserialize<'a> + Sized {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let encoded = bincode::serialize(&self)
            .map_err(std::io::Error::other)?;
        let compressed = lz4_flex::compress_prepend_size(&encoded);
        Ok(compressed)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let decompressed = lz4_flex::decompress_size_prepended(data)
            .map_err(std::io::Error::other)?;
        let decoded = bincode::deserialize::<Self>(&decompressed)
            .map_err(std::io::Error::other)?;
        Ok(decoded)
    }
}