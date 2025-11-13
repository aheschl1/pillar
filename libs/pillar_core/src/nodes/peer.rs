use std::{fmt::{Debug}, net::IpAddr, time::Duration};

use bytemuck::{Pod, Zeroable};
use pillar_crypto::{encryption::PillarSharedSecret, types::StdByteArray};
use pillar_serialize::PillarSerialize;
use tokio::{io::AsyncWriteExt, net::TcpStream, time::timeout};
use tracing::instrument;
use crate::protocol::serialization::{package_standard_message, read_standard_message};

use crate::{primitives::messages::Message};

#[derive(Pod, Zeroable, Copy, Clone, Debug, PartialEq, Eq,  Hash)]
#[repr(C)]
pub struct PillarIPAddr([u8; 16]);

impl From<PillarIPAddr> for IpAddr {
    fn from(pillar_ip: PillarIPAddr) -> Self {
        if pillar_ip.0[..12] == [0u8; 12] {
            // IPv4
            let octets: [u8; 4] = pillar_ip.0[12..].try_into().unwrap();
            IpAddr::V4(octets.into())
        } else {
            // IPv6
            let octets: [u8; 16] = pillar_ip.0;
            IpAddr::V6(octets.into())
        }
    }
}

impl From<IpAddr> for PillarIPAddr {
    fn from(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                let mut bytes = [0u8; 16];
                bytes[12..].copy_from_slice(&octets);
                PillarIPAddr(bytes)
            },
            IpAddr::V6(v6) => {
                let octets = v6.octets();
                PillarIPAddr(octets)
            }
        }
    }
}

impl std::fmt::Display for PillarIPAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ip: IpAddr = (*self).into();
        write!(f, "{ip}")
    }
}

#[derive(Pod, Zeroable,  Debug, PartialEq, Eq, Hash, Copy, Clone)]
#[repr(C)]
pub struct Peer{
    /// The public key of the peer
    pub public_key: StdByteArray,
    /// The IP address of the peer
    pub ip_address: PillarIPAddr, // 16 bytes
    /// The port of the peer
    pub port: u16, // 2 bytes
    // pads at end
}

impl Peer{

    /// Create a new peer
    pub fn new(public_key: [u8; 32], ip_address: IpAddr, port: u16) -> Self {
        Peer {
            public_key,
            ip_address: ip_address.into(),
            port
        }
    }

    /// Send a message to the peer
    /// Initializaes a new connection to the peer
    #[instrument(skip(self, message, initializing_peer))]
    async fn send_initial(&self, message: &Message, initializing_peer: &Peer) -> Result<TcpStream, std::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
        // always send a "peer" object of the initializing node first, and length of the message in bytes
        let declaration = Message::Declaration(*initializing_peer);
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

    /// start a persistent encrypted communication channel with the peer
    /// each message put into the `messages` receiver will be sent to the peer
    /// each message sent will expect a response, which will be sent to the `responses` sender
    /// 
    /// This is meant to be spawned as a separate task.
    /// 
    /// Returns an error if the communication fails
    pub async fn encrypted_communicate(
        &self, 
        messages: flume::Receiver<Message>, 
        responses: flume::Sender<Message>,
        initializing_peer: &Peer
    ) -> Result<(), std::io::Error> {
        let mut shared_secret = PillarSharedSecret::initiate();
        let m = Message::PrivacyRequest(shared_secret.public);
        let response = self.communicate(&m, initializing_peer).await?;
        let remote_public = match response {
            Message::PrivacyResponse(pubkey) => pubkey,
            _ => return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid response to privacy request")),
        };
        shared_secret.hydrate(remote_public)?;
        while let Ok(message) = messages.recv_async().await {
            let encrypted_bytes = shared_secret.encrypt(message.serialize_pillar()?)?;
            let encrypted_message = Message::EncryptedMessage(encrypted_bytes);
            let response = self.communicate(&encrypted_message, initializing_peer).await?;
            let decrypted_response = match response {
                Message::EncryptedMessage(payload) => {
                    let decrypted_bytes = shared_secret.decrypt(payload)?;
                    Message::deserialize_pillar(&decrypted_bytes)?
                },
                _ => return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid encrypted message response")),
            };
            responses.send_async(decrypted_response).await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Failed to send response"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests{
    use core::panic;
    use std::net::{IpAddr, Ipv4Addr};

    use tokio::io::AsyncWriteExt;

    use crate::{nodes::peer::Peer, primitives::messages::Message, protocol::serialization::{package_standard_message, read_standard_message}};

    #[test]
    fn test_peer_new(){
        let peer = Peer::new([1u8; 32], IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        assert_eq!(peer.public_key, [1u8; 32]);
        assert_eq!(peer.ip_address, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)).into());
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
