use std::{net::IpAddr, time::Duration};

use serde::{Serialize, Deserialize};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, time::timeout};
use tracing::instrument;
use crate::protocol::serialization::{package_standard_message, read_standard_message};

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
        // always send a "peer" object of the initializing node first, and length of the message in bytes
        let declaration = Message::Declaration(initializing_peer.clone());
        // serialize with bincode
        let bytes = package_standard_message(&declaration)?;

        stream.write_all(bytes.as_slice()).await?;
        tracing::debug!("Sent {} bytes to peers", bytes.len());
        // send the message
        let bytes = package_standard_message(message)?;
        stream.write_all(bytes.as_slice()).await?;
        tracing::debug!("Sent {} bytes to peers", bytes.len());
        Ok(stream)
    }

    /// Get a response from the peer
    /// This function will block until a response is received
    async fn read_response(&self, mut stream: TcpStream) -> Result<Message, std::io::Error> {
        let message: Message = read_standard_message(&mut stream).await?;
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

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::{nodes::peer::Peer, primitives::messages::Message, protocol::{serialization::{package_standard_message, read_standard_message}, PROTOCOL_VERSION}};

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
        let peer = Peer::new([2u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8081);
        // bind to peer socket - we will receive the message here
        let initializing_clone = initializing_peer.clone();
        let listener = tokio::net::TcpListener::bind(format!("{}:{}", peer.ip_address, peer.port)).await.unwrap();
        let handle = tokio::spawn(async move{
            // wait for a connection
            let (mut stream, _) = listener.accept().await.unwrap();
            // read the message - expect a declaration and then a ping
            let message: Message = read_standard_message(&mut stream).await.unwrap();
            match message{
                Message::Declaration(peer) => {
                    assert_eq!(peer.public_key, initializing_clone.public_key);
                    assert_eq!(peer.ip_address, initializing_clone.ip_address);
                    assert_eq!(peer.port, initializing_clone.port);
                },
                _ => panic!("Expected a declaration message")
            }
            // read the next message
            let message: Message = read_standard_message(&mut stream).await.unwrap();
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
        let peer = Peer::new([2u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8081);
        // bind to peer socket - we will receive the message here
        let initializing_clone = initializing_peer.clone();
        let listener = tokio::net::TcpListener::bind(format!("{}:{}", peer.ip_address, peer.port)).await.unwrap();
        let handle = tokio::spawn(async move{
            // wait for a connection
            let (mut stream, _) = listener.accept().await.unwrap();
            // read the message - expect a declaration and then a ping
            let message: Message = read_standard_message(&mut stream).await.unwrap();
            match message{
                Message::Declaration(peer) => {
                    assert_eq!(peer.public_key, initializing_clone.public_key);
                    assert_eq!(peer.ip_address, initializing_clone.ip_address);
                    assert_eq!(peer.port, initializing_clone.port);
                },
                _ => panic!("Expected a declaration message")
            }
            // read the next message
            let message: Message = read_standard_message(&mut stream).await.unwrap();
            match message{
                Message::Ping => {},
                _ => panic!("Expected a ping message")
            }
            // send a response
            let response = Message::Ping;
            // send n bytes of upcoming message
            let serialized_response = package_standard_message(&response).unwrap();
            stream.write_all(&serialized_response).await.unwrap();
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