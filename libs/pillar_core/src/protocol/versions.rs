use serde::{Serialize, Deserialize};

use crate::protocol::PROTOCOL_VERSION;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
    pub fn to_le_bytes(&self) -> [u8; 2] {
        match self {
            Versions::V1V4 => 1u16.to_le_bytes(),
            Versions::V1V6 => 2u16.to_le_bytes(),
        }
    }
}

/// Returns the expected bincode-encoded size in bytes for a `Message::Declaration(Peer, u64)`
/// under version-specific assumptions (IPv4 or IPv6). Assumes default bincode config.
pub const fn get_declaration_length(version: Versions) -> u64 {
    match version {
        Versions::V1V4 => 50, // enum tag + public key + IP tag + IPv4 + port + u32
        Versions::V1V6 => 62, // enum tag + public key + IP tag + IPv6 + port + u32
    }
}

mod tests{

    use crate::{nodes::peer::Peer, primitives::messages::Message, protocol::{serialization::PillarSerialize, versions::{get_declaration_length, Versions}}};


    #[test]
    fn test_declaration_length() {
        let declaration = Message::Declaration(Peer::new(
            [0; 32], 
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 
            8000), 
            1
        );

        let declarationv1v6 = Message::Declaration(Peer::new(
            [0; 32], 
            std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 2, 0, 0, 0, 0, 0, 1)), 
            8000), 
            1
        );
        assert_eq!(get_declaration_length(Versions::V1V4), declaration.serialize_pillar().unwrap().len() as u64);
        assert_eq!(get_declaration_length(Versions::V1V6), declarationv1v6.serialize_pillar().unwrap().len() as u64);
    }
}