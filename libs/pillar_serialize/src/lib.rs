#![feature(specialization)]
use std::collections::HashMap;

use bytemuck::{bytes_of, Pod, Zeroable};

const STANDARD_ARRAY_LENGTH: usize = 32;
pub type StdByteArray = [u8; STANDARD_ARRAY_LENGTH];

pub trait PillarNativeEndian {
    fn to_le(&mut self);
}

pub trait PillarSerialize: Sized {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error>;
    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error>;
}

pub trait PillarFixedSize {}


impl<T> PillarSerialize for T 
where 
    T: PillarNativeEndian + Pod + Zeroable + PillarFixedSize
{
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = bytes_of(self);

        let mut le_tx: T;
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
        let mut tx_le: Self = bytemuck::pod_read_unaligned::<Self>(data);
        if cfg!(target_endian = "big") {
            tx_le.to_le();
        }
        Ok(tx_le)
    }
}

impl PillarFixedSize for u8                          {}
impl PillarFixedSize for u16                         {}
impl PillarFixedSize for u32                         {}
impl PillarFixedSize for u64                         {}
impl PillarFixedSize for i8                          {}
impl PillarFixedSize for i16                         {}
impl PillarFixedSize for i32                         {}
impl PillarFixedSize for i64                         {}
impl PillarFixedSize for [u8; 32]                    {}

impl PillarNativeEndian for StdByteArray {
    fn to_le(&mut self) {}
}

impl PillarNativeEndian for u8 {
    fn to_le(&mut self) {}
}

impl PillarNativeEndian for u16 {
    fn to_le(&mut self) {
        *self = u16::to_le(*self);
    }
}

impl PillarNativeEndian for u32 {
    fn to_le(&mut self) {
        *self = u32::to_le(*self);
    }
}

impl PillarNativeEndian for u64 {
    fn to_le(&mut self) {
        *self = u64::to_le(*self);
    }
}


impl<T> PillarSerialize for Vec<T>
where
    T: PillarSerialize
{
    
    default fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        buffer.extend((self.len() as u32).to_le_bytes());
        for item in self {
            let serialized = item.serialize_pillar()?;
            buffer.extend((serialized.len() as u32).to_le_bytes());
            buffer.extend(serialized);
        }
        Ok(buffer)
    }

    default fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut items = Vec::new();
        let length = u32::from_le_bytes(data[..4].try_into().unwrap());
        let mut offset = 4;
        for _ in 0..length {
            let size = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let item = T::deserialize_pillar(&data[offset..offset + size])?;
            items.push(item);
            offset += size;
        }
        Ok(items)
    }

}

impl<T> PillarSerialize for Vec<T> 
where
    T: PillarSerialize + PillarNativeEndian + Pod + Zeroable + PillarFixedSize
{
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        for item in self {
            buffer.extend(item.serialize_pillar()?);
        }
        Ok(buffer)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let size = size_of::<T>();
        assert!(data.len() % size == 0);
        let length = data.len() / size;
        let mut items = Vec::with_capacity(length as usize);
        let mut offset = 0;
        for _ in 0..length {
            let item = T::deserialize_pillar(&data[offset..offset + size])?;
            items.push(item);
            offset += size;
        }
        Ok(items)
    }
}

impl<T> PillarSerialize for Option<T>
where
    T: PillarSerialize
{
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Some(value) => {
                let mut buffer = vec![1];
                buffer.extend(value.serialize_pillar()?);
                Ok(buffer)
            }
            None => Ok(vec![0]),
        }
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let marker = data.get(0).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Data too short"))?;
        if marker == &0 {
            return Ok(None);
        }
        let value = T::deserialize_pillar(&data[1..])?;
        Ok(Some(value))
    }
}

/// implmentation for hashmap where we don't have a guaranteed size of key or value
impl<K, V> PillarSerialize for HashMap<K, V>
where
    K: PillarSerialize + Eq + std::hash::Hash,
    V: PillarSerialize
{
    default fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        buffer.extend((self.len() as u32).to_le_bytes());
        for (key, value) in self {
            let kser = key.serialize_pillar()?;
            let vser = value.serialize_pillar()?;
            buffer.extend((kser.len() as u32).to_le_bytes());
            buffer.extend(kser);
            buffer.extend((vser.len() as u32).to_le_bytes());
            buffer.extend(vser);
        }
        Ok(buffer)
    }

    default fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut offset = 0;
        let length = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let mut map = HashMap::new();
        for _ in 0..length {
            let size = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let key = K::deserialize_pillar(&data[offset..offset + size])?;
            offset += size;
            let vsize = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let value = V::deserialize_pillar(&data[offset..offset + vsize])?;
            offset += vsize;
            map.insert(key, value);
        }
        Ok(map)
    }
}

/// a common hashmap implmentation for address/hash to fixed size value
impl<V> PillarSerialize for HashMap<StdByteArray, V>
where 
    V: PillarSerialize + PillarNativeEndian + Zeroable + Pod
{
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        for (key, value) in self {
            buffer.extend(key.serialize_pillar()?);
            buffer.extend(value.serialize_pillar()?);
        }
        Ok(buffer)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let mut offset = 0;
        let vsize = size_of::<V>();
        let length = data.len() / (STANDARD_ARRAY_LENGTH + vsize);
        let mut map = HashMap::new();
        for _ in 0..length {
            let key = StdByteArray::deserialize_pillar(data[offset..offset + STANDARD_ARRAY_LENGTH].try_into().unwrap())?;
            offset += STANDARD_ARRAY_LENGTH;
            let value = V::deserialize_pillar(&data[offset..offset + vsize])?;
            offset += vsize;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl PillarSerialize for String {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;
        let mut buffer = Vec::with_capacity(4 + length as usize);
        buffer.extend(length.to_le_bytes());
        buffer.extend(bytes);
        Ok(buffer)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let length = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        let string = String::from_utf8(data[4..4 + length].to_vec())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"))?;
        Ok(string)
    }
}