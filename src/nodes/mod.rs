use std::{collections::HashSet, sync::Arc};
use distribute::peer_distribution;
use tokio::{net::TcpStream, sync::Mutex};

use messages::Message;
use serde::{Serialize, Deserialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::blockchain::chain::Chain;

pub mod miner;
pub mod messages;
pub mod distribute;

#[derive(Serialize, Deserialize, Debug)]
pub struct Peer{
    /// The public key of the peer
    pub public_key: [u8; 32],
    /// The IP address of the peer
    pub ip_address: String,
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
    pub fn new(public_key: [u8; 32], ip_address: String, port: u16) -> Self {
        Peer {
            public_key,
            ip_address,
            port
        }
    }

    /// Send a message to the peer
    /// Initializaes a new connection to the peer
    async fn send(&mut self, message: &Message) -> Result<TcpStream, std::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
        // serialize with bincode
        // send the message
        stream.write_all(bincode::serialize(&message).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?.as_slice()).await?;
        Ok(stream)
    }

    /// Get a response from the peer
    /// This function will block until a response is received
    async fn get_response(&self, mut stream: TcpStream) -> Result<Message, std::io::Error> {
        // read the message
        let mut buffer = [0; 2048];
        let n = stream.read(&mut buffer).await?;
        // deserialize with bincode
        let message: Message = bincode::deserialize(&buffer[..n]).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?;
        Ok(message)
    }

    pub async fn initialize_communication(&mut self, message: &Message) -> Result<Message, std::io::Error> {
        let stream = self.send(message).await?;
        let response = self.get_response(stream).await?;
        Ok(response)
    }
}

#[derive(Clone)]
struct Node{
    pub public_key: Arc<[u8; 32]>,
    /// The private key of the node
    pub private_key: Arc<[u8; 32]>,
    /// The IP address of the node
    pub ip_address: Arc<String>,
    /// The port of the node
    pub port: Arc<u16>,
    // known peers
    pub peers: Arc<Mutex<Vec<Peer>>>,
    // the blockchain
    pub chain: Arc<Mutex<Chain>>
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
            public_key: public_key.into(),
            private_key: private_key.into(),
            ip_address: ip_address.into(),
            port: port.into(),
            peers: Mutex::new(peers).into(),
            chain: Mutex::new(chain).into()
        }
    }

    /// when called, launches a new thread that listens for incoming connections
    async fn listen(&self){
        // spawn a new thread to handle the connection
        let self_clone = self.clone();
        tokio::spawn(peer_distribution(self_clone));
    }

    async fn distribute_peers(self){

    }

    /// Broadcast a message to all peers
    async fn broadcast(&mut self, message: Message){
        for peer in self.peers.lock().await.iter_mut(){
            peer.send(&message).await.unwrap();
        }
    }

    /// Find new peers by queerying the existing peers
    /// and adding them to the list of peers
    async fn discover_peers(&mut self) -> Result<(), std::io::Error> {
        let mut existing_peers = self.peers.lock().await.iter()
            .map(|peer| peer.public_key)
            .collect::<HashSet<_>>();
        let mut new_peers: Vec<Peer> = Vec::new();
        // send a message to the peers
        for peer in self.peers.lock().await.iter_mut(){
            let peers = peer.initialize_communication(&Message::PeerRequest).await?;
            match peers {
                Message::PeerResponse(peers) => {
                    for peer in peers{
                        // check if the peer is already in the list
                        if !existing_peers.contains(&peer.public_key){
                            // add the peer to the list
                            existing_peers.insert(peer.public_key);
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
        self.peers.lock().await.extend(new_peers);
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