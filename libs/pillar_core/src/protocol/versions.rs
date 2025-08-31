use crate::protocol::PROTOCOL_VERSION;

#[derive( Clone, Copy, Debug, PartialEq, Eq)]
pub enum Versions{
    V1V4 = 1,
    #[allow(dead_code)]
    V1V6 = 2,
}

impl Default for Versions{
    fn default() -> Self {
        PROTOCOL_VERSION
    }
}

impl Versions{
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(Versions::V1V4),
            2 => Some(Versions::V1V6),
            _ => None,
        }
    }
    pub fn to_u16(&self) -> u16 {
        match self {
            Versions::V1V4 => 1,
            Versions::V1V6 => 2,
        }
    }
    pub fn to_le_bytes(&self) -> [u8; 2] {
        match self {
            Versions::V1V4 => 1u16.to_le_bytes(),
            Versions::V1V6 => 2u16.to_le_bytes(),
        }
    }
    pub fn to_ne_bytes(&self) -> [u8; 2] {
        match self {
            Versions::V1V4 => 1u16.to_ne_bytes(),
            Versions::V1V6 => 2u16.to_ne_bytes(),
        }
    }
}