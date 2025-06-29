use serde::{Deserialize, Serialize};

pub fn serialize(data: impl Serialize) -> Result<Vec<u8>, std::io::Error> 
{
    let encoded = bincode::serialize(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let compressed = lz4_flex::compress_prepend_size(&encoded);
    Ok(compressed)
}

pub fn deserialize<T: for<'a> Deserialize<'a>>(data: &[u8]) -> Result<T, std::io::Error> 
{
    let decompressed = lz4_flex::decompress_size_prepended(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let decoded = bincode::deserialize::<T>(&decompressed)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(decoded)
}

pub fn serialize_no_compress(data: impl Serialize) -> Result<Vec<u8>, std::io::Error> 
{
    let encoded = bincode::serialize(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(encoded)
}

pub fn deserialize_no_compress<T: for<'a> Deserialize<'a>>(data: &[u8]) -> Result<T, std::io::Error> 
{
    let decompressed = bincode::deserialize::<T>(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(decompressed)
}