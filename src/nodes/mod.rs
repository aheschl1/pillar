use std::collections::HashSet;

use messages::Message;
use serde::{Serialize, Deserialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::blockchain::chain::Chain;

pub mod miner;
pub mod messages;

#[derive(Serialize, Deserialize, Debug)]
pub struct Peer{
    /// The public key of the peer
    pub public_key: [u8; 32],
    /// The IP address of the peer
    pub ip_address: String,
    /// The port of the peer
    pub port: u16,
    /// connection
    #[serde(skip)]
    stream: Option<tokio::net::TcpStream>
}

impl Peer{
    /// Create a new peer
    pub fn new(public_key: [u8; 32], ip_address: String, port: u16) -> Self {
        Peer {
            public_key,
            ip_address,
            port,
            stream: None
        }
    }

    /// Send a message to the peer
    pub async fn send(&mut self, message: &Message) -> Result<(), std::io::Error> {
        if self.stream.is_none(){
            let stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
            self.stream = Some(stream);
        }
        let stream = self.stream.as_mut().unwrap();
        // serialize with bincode
        // send the message
        stream.write_all(bincode::serialize(&message).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?.as_slice()).await?;
        Ok(())
    }

    /// Receive a message from the peer
    pub async fn receive(&mut self) -> Result<Message, std::io::Error> {
        if self.stream.is_none(){
            let stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
            self.stream = Some(stream);
        }
        let stream = self.stream.as_mut().unwrap();
        // read the message
        let mut buffer = [0; 2048];
        let n = stream.read(&mut buffer).await?;
        // deserialize with bincode
        let message: Message = bincode::deserialize(&buffer[..n]).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?;
        Ok(message)
    }
}

struct Node{
    pub public_key: [u8; 32],
    /// The private key of the node
    pub private_key: [u8; 32],
    /// The IP address of the node
    pub ip_address: String,
    /// The port of the node
    pub port: u16,
    // known peers
    pub peers: Vec<Peer>,
    // the blockchain
    pub chain: Chain
}

impl Node {
    /// Create a new node
    pub fn new(
        public_key: [u8; 32], 
        private_key: [u8; 32], 
        ip_address: String, 
        port: u16,
        peers: Vec<Peer>,
        chain: Chain
    ) -> Self {
        Node {
            public_key,
            private_key,
            ip_address,
            port,
            peers: peers,
            chain
        }
    }

    /// Broadcast a message to all peers
    async fn broadcast(&mut self, message: Message){
        for peer in self.peers.iter_mut(){
            peer.send(&message).await.unwrap();
        }
    }

    /// Find new peers by queerying the existing peers
    /// and adding them to the list of peers
    async fn discover_peers(&mut self) -> Result<(), std::io::Error> {
        let existing_peers = self.peers.iter()
            .map(|peer| peer.public_key)
            .collect::<HashSet<_>>();
        let mut new_peers: Vec<Peer> = Vec::new();
        // send a message to the peers
        for peer in self.peers.iter_mut(){
            let message = Message::PeerRequest;
            peer.send(&message).await.unwrap();
            let peers = peer.receive().await.unwrap();
            match peers {
                Message::PeerResponse(peers) => {
                    for peer in peers{
                        // check if the peer is already in the list
                        if !existing_peers.contains(&peer.public_key){
                            // add the peer to the list
                            new_peers.push(peer);
                        }
                    }
                },
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other, 
                        "Invalid message received"
                    ));
                }
            }

        }
        // extend the peers list with the new peers
        self.peers.extend(new_peers);
        Ok(())
    }

}

#[cfg(test)]
mod test{
    use crate::{blockchain::chain::Chain, crypto::hashing::{HashFunction, DefaultHash}, primitives::{block::Block, transaction::Transaction}};
    use crate::nodes::miner::Miner;
    use super::Node;

    #[test]
    fn test_miner(){
        let public_key = [1u8; 32];
        let private_key = [2u8; 32];
        let ip_address = "127.0.0.1".to_string();
        let port = 8080;
        let node = Node::new(public_key, private_key, ip_address, port, vec![], Chain::new_with_genesis());
        let mut hasher = DefaultHash::new();

        // block
        let previous_hash = [3u8; 32];
        let nonce = 12345;
        let timestamp = 1622547800;
        let transactions = vec![
            Transaction::new(
                [0u8; 32], 
                [0u8; 32], 
                1, 
                timestamp, 
                0,
                &mut hasher.clone()
            )
        ];
        let difficulty = 1;
        let miner_address = None;

        let mut block = Block::new(previous_hash, nonce, timestamp, transactions, difficulty, miner_address, &mut hasher);

        // mine the block
        node.mine(&mut block, &mut hasher);

        assert!(block.header.nonce > 0);
        assert!(block.header.miner_address.is_some());
        assert_eq!(block.header.previous_hash, previous_hash);
        assert_eq!(block.header.timestamp, timestamp);
        assert_eq!(block.header.difficulty, difficulty);
        assert!(block.hash.is_some());
    }
}