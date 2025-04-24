use std::net::IpAddr;

use serde::{Serialize, Deserialize};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};

use super::messages::Message;

#[derive(Serialize, Deserialize, Debug)]
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
            ip_address: self.ip_address.clone(),
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
    async fn send_initial(&mut self, message: &Message, initializing_peer: Peer) -> Result<TcpStream, std::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
        // always send a "peer" object of the initializing node first
        let declaration = Message::Declaration(initializing_peer);
        // serialize with bincode
        stream.write_all(bincode::serialize(&declaration).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?.as_slice()).await?;
        // send the message
        stream.write_all(bincode::serialize(&message).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?.as_slice()).await?;
        Ok(stream)
    }

    /// Get a response from the peer
    /// This function will block until a response is received
    async fn read_response(&self, mut stream: TcpStream) -> Result<Message, std::io::Error> {
        // read the message
        let mut buffer = [0; 2048];
        let n = stream.read(&mut buffer).await?;
        // deserialize with bincode
        let message: Message = bincode::deserialize(&buffer[..n]).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?;
        Ok(message)
    }

    pub async fn communicate(&mut self, message: &Message, initializing_peer: Peer) -> Result<Message, std::io::Error> {
        let stream = self.send_initial(message, initializing_peer).await?;
        let response = self.read_response(stream).await?;
        Ok(response)
    }
}