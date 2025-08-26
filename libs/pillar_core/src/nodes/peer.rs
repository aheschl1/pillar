use std::{net::IpAddr, time::Duration};

use pillar_crypto::serialization::PillarSerialize;
use serde::{Serialize, Deserialize};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, time::timeout};
use tracing::instrument;

use crate::{primitives::messages::Message};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct Peer{
    /// The public key of the peer
    pub public_key: [u8; 32],
    /// The IP address of the peer
    pub ip_address: IpAddr,
    /// The port of the peer
    pub port: u16,
}

impl Clone for Peer {
    fn clone(&self) -> Self {
        Peer {
            public_key: self.public_key,
            ip_address: self.ip_address,
            port: self.port
        }
    }
}

impl Peer{
    /// Create a new peer
    pub fn new(public_key: [u8; 32], ip_address: IpAddr, port: u16) -> Self {
        Peer {
            public_key,
            ip_address,
            port
        }
    }

    /// Send a message to the peer
    /// Initializaes a new connection to the peer
    #[instrument(skip(self, message, initializing_peer))]
    async fn send_initial(&self, message: &Message, initializing_peer: &Peer) -> Result<TcpStream, std::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
        let serialized_message = message.serialize_pillar();
        // always send a "peer" object of the initializing node first, and length of the message in bytes
        let declaration = Message::Declaration(initializing_peer.clone(), serialized_message.as_ref().unwrap().len() as u32);
        // serialize with bincode
        let bytes = declaration.serialize_pillar().map_err(
            std::io::Error::other
        )?;
        stream.write_all(bytes.as_slice()).await?;
        tracing::debug!("Sent {} bytes to peers", bytes.len());
        // send the message
        let bytes = serialized_message.map_err(
            std::io::Error::other
        )?;
        stream.write_all(bytes.as_slice()).await?;
        tracing::debug!("Sent {} bytes to peers", bytes.len());
        Ok(stream)
    }

    /// Get a response from the peer
    /// This function will block until a response is received
    async fn read_response(&self, mut stream: TcpStream) -> Result<Message, std::io::Error> {
        // read the message
        let mut buffer = [0; 4];
        // read the size (u32)
        stream.read_exact(&mut buffer).await?;
        // get the size of the message - is sent with to_le_bytes
        let size = u32::from_le_bytes(buffer);
        let mut buffer = vec![0; size as usize];
        let n = stream.read_exact(&mut buffer).await?;
        // deserialize with bincode
        let message: Message = PillarSerialize::deserialize_pillar(&buffer[..n]).map_err(
            std::io::Error::other
        )?;
        Ok(message)
    }

    pub async fn communicate(&self, message: &Message, initializing_peer: &Peer) -> Result<Message, std::io::Error> {
        let stream = timeout(Duration::from_secs(1), self.send_initial(message, initializing_peer)).await??;
        let response = self.read_response(stream).await?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests{
    use core::panic;
    use std::net::{IpAddr, Ipv4Addr};

    use pillar_crypto::serialization::PillarSerialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::{nodes::peer::Peer, primitives::messages::{get_declaration_length, Message, Versions}};

    #[test]
    fn test_peer_new(){
        let peer = Peer::new([1u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        assert_eq!(peer.public_key, [1u8; 32]);
        assert_eq!(peer.ip_address, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(peer.port, 8080);
    }

    #[tokio::test]
    async fn test_send_initial(){
        // setup a dummy socket, use it for the initializing_peer. read reponses
        let initializing_peer = Peer::new([1u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9)), 8080);
        let mut peer = Peer::new([2u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8081);
        // bind to peer socket - we will receive the message here
        let initializing_clone = initializing_peer.clone();
        let listener = tokio::net::TcpListener::bind(format!("{}:{}", peer.ip_address, peer.port)).await.unwrap();
        let handle = tokio::spawn(async move{
            // wait for a connection
            let (mut stream, _) = listener.accept().await.unwrap();
            // read the message - expect a declaration and then a ping
            let mut buffer = [0; get_declaration_length(Versions::V1V4) as usize];
            let n = stream.read_exact(&mut buffer).await.unwrap();
            // deserialize with bincode
            let message: Message = PillarSerialize::deserialize_pillar(&buffer[..n]).unwrap();
            let size;
            match message{
                Message::Declaration(peer, n) => {
                    size = n;
                    assert_eq!(peer.public_key, initializing_clone.public_key);
                    assert_eq!(peer.ip_address, initializing_clone.ip_address);
                    assert_eq!(peer.port, initializing_clone.port);
                },
                _ => panic!("Expected a declaration message")
            }
            // read the next message
            let mut buffer = vec![0; size as usize];
            let n = stream.read_exact(&mut buffer).await.unwrap();
            let message: Message = PillarSerialize::deserialize_pillar(&buffer[..n]).unwrap();
            match message{
                Message::Ping => {},
                _ => panic!("Expected a ping message")
            }
        });
        // check for the messages on listener
        // comunicate with the peer
        let message = Message::Ping;
        let _ = peer.send_initial(&message, &initializing_peer).await.unwrap(); // send to peer
        handle.await.unwrap(); // wait for the listener to finish
        
    }

    #[tokio::test]
    async fn test_read_response(){
        // setup a dummy socket, use it for the initializing_peer. read reponses
        let initializing_peer = Peer::new([1u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let mut peer = Peer::new([2u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8081);
        // bind to peer socket - we will receive the message here
        let initializing_clone = initializing_peer.clone();
        let listener = tokio::net::TcpListener::bind(format!("{}:{}", peer.ip_address, peer.port)).await.unwrap();
        let handle = tokio::spawn(async move{
            // wait for a connection
            let (mut stream, _) = listener.accept().await.unwrap();
            // read the message - expect a declaration and then a ping
            let mut buffer = [0; get_declaration_length(Versions::V1V4) as usize];
            let n = stream.read_exact(&mut buffer).await.unwrap();
            // deserialize with bincode
            let message: Message = PillarSerialize::deserialize_pillar(&buffer[..n]).unwrap();
            let size;
            match message{
                Message::Declaration(peer, n) => {
                    size = n;
                    assert_eq!(peer.public_key, initializing_clone.public_key);
                    assert_eq!(peer.ip_address, initializing_clone.ip_address);
                    assert_eq!(peer.port, initializing_clone.port);
                },
                _ => panic!("Expected a declaration message")
            }
            // read the next message
            let mut buffer = vec![0; size as usize];
            let _ = stream.read_exact(&mut buffer).await.unwrap();
            let message: Message = PillarSerialize::deserialize_pillar(&buffer).unwrap();
            match message{
                Message::Ping => {},
                _ => panic!("Expected a ping message")
            }
            // send a response
            let response = Message::Ping;
            // send n bytes of upcoming message
            let serialized_response = response.serialize_pillar().unwrap();
            stream.write_all(&serialized_response.len().to_le_bytes()[..4]).await.unwrap();
            stream.write_all(serialized_response.as_slice()).await.unwrap();
        });
        // check for the messages on listener
        // comunicate with the peer
        let message = Message::Ping;
        let stream = peer.send_initial(&message, &initializing_peer).await.unwrap(); // send to peer
        // read the response
        let response = peer.read_response(stream).await.unwrap();
        match response{
            Message::Ping => {},
            _ => panic!("Expected a ping message")
        }
        handle.await.unwrap(); // wait for the listener to finish
    }

}