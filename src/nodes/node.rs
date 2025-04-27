use super::peer::Peer;
use flume::{Receiver, Sender};
use std::{collections::HashSet, net::IpAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

use super::messages::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    blockchain::chain::Chain,
    primitives::{pool::MinerPool, transaction::Transaction},
};

#[derive(Clone)]
pub struct Node {
    pub public_key: Arc<[u8; 32]>,
    /// The private key of the node
    pub private_key: Arc<[u8; 32]>,
    /// The IP address of the node
    pub ip_address: IpAddr,
    /// The port of the node
    pub port: Arc<u16>,
    // known peers
    pub peers: Arc<Mutex<Vec<Peer>>>,
    // the blockchain
    pub chain: Arc<Mutex<Chain>>,
    /// transactions to be serviced
    pub miner_pool: Option<Arc<MinerPool>>,
    /// transactions to be broadcasted
    pub transaction_receiver: Receiver<Transaction>,
    /// transaction input
    pub transaction_sender: Sender<Transaction>,
}

impl Node {
    /// Create a new node
    pub fn new(
        public_key: [u8; 32],
        private_key: [u8; 32],
        ip_address: IpAddr,
        port: u16,
        peers: Vec<Peer>,
        chain: Chain,
        transaction_pool: Option<MinerPool>,
    ) -> Self {
        let (transaction_sender, transaction_receiver) = flume::unbounded();
        Node {
            public_key: public_key.into(),
            private_key: private_key.into(),
            ip_address,
            port: port.into(),
            peers: Mutex::new(peers).into(),
            chain: Mutex::new(chain).into(),
            miner_pool: transaction_pool.map(Arc::new),
            transaction_receiver,
            transaction_sender,
        }
    }

    /// when called, launches a new thread that listens for incoming connections
    pub fn serve(&self) {
        // spawn a new thread to handle the connection
        let self_clone = self.clone();
        tokio::spawn(self_clone.clone().serve_peers());
        tokio::spawn(self_clone.broadcast_knowledge());
    }

    /// This function serves as a loop that accepts incoming requests, and handles the main protocol
    /// For each connection, listens for a peer declaration. Then, it adds this peer to the peer list if not alteady there
    /// After handling the peer, it reads for the actual message. Then, it calls off to the serve_message function.
    /// The response from serve_message is finally returned back to the peer
    async fn serve_peers(self) {
        let listener = TcpListener::bind(format!("{}:{}", self.ip_address, self.port))
            .await
            .unwrap();
        loop {
            // handle connection
            let (mut stream, _) = listener.accept().await.unwrap();
            // spawn a new thread to handle the connection
            let mut self_clone = self.clone();
            tokio::spawn(async move {
                // first read the peer declaration
                let mut buffer = [0; 2048];
                let n = stream.read(&mut buffer).await.unwrap();
                // deserialize with bincode
                let declaration: Result<Message, Box<bincode::ErrorKind>> =
                    bincode::deserialize(&buffer[..n]);
                if declaration.is_err() {
                    // halt communication
                    return;
                }
                let declaration = declaration.unwrap();
                match declaration {
                    Message::Declaration(peer) => {
                        // add the peer to the list if and only if it is not already in the list
                        self_clone.maybe_update_peers(peer.clone()).await.unwrap();
                        // send a response
                        peer
                    }
                    _ => {
                        self_clone
                            .send_error_message(
                                &mut stream,
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidInput,
                                    "Expected peer delaration",
                                ),
                            )
                            .await;
                        return;
                    }
                };
                // read actual the message
                let n = stream.read(&mut buffer).await.unwrap();
                let message: Result<Message, Box<bincode::ErrorKind>> =
                    bincode::deserialize(&buffer[..n]);
                if message.is_err() {
                    // halt
                    return;
                }
                let message = message.unwrap();
                let response = self_clone.serve_request(&message).await;
                match response {
                    Err(e) => self_clone.send_error_message(&mut stream, e).await,
                    Ok(message) => stream
                        .write_all(&bincode::serialize(&message).unwrap())
                        .await
                        .unwrap(),
                };
            });
        }
    }

    /// Background process that consumes mined blocks, and transactions which must be forwarded
    async fn broadcast_knowledge(self) -> Result<(), std::io::Error> {
        loop {
            // send a message to all peers
            if let Some(pool) = &self.miner_pool {
                while pool.block_count() > 0 {
                    self.broadcast(&Message::BlockTransmission(pool.pop_block().unwrap()))
                        .await?;
                }
            }
            while !self.transaction_receiver.is_empty() {
                // receive the transaction from the sender
                let transaction = self.transaction_receiver.recv().unwrap();
                self.broadcast(&Message::TransactionRequest(transaction))
                    .await?;
            }
        }
    }

    /// Derive the response to a request from a peer
    async fn serve_request(&mut self, message: &Message) -> Result<Message, std::io::Error> {
        match message {
            Message::PeerRequest => {
                // send all peers
                Ok(Message::PeerResponse(self.peers.lock().await.clone()))
            }
            Message::ChainRequest => Ok(Message::ChainResponse(self.chain.lock().await.clone())),
            Message::TransactionRequest(transaction) => {
                // IF IT HAS OUR PUBLIC KEY< DROP
                if transaction.header.sender == *self.public_key {
                    return Ok(Message::TransactionAck);
                }
                // add the transaction to the pool
                if let Some(ref pool) = self.miner_pool {
                    pool.add_transaction(transaction.clone());
                }
                // to be broadcasted
                self.transaction_sender.send(transaction.clone()).unwrap();
                Ok(Message::TransactionAck)
            }
            Message::BlockTransmission(block) => {
                if block.header.miner_address.unwrap() == *self.public_key {
                    // SKIP OWN BLOCK
                    return Ok(Message::BlockAck);
                }
                // add the block to the chain - first it is verified
                self.chain.lock().await.add_new_block(block.clone()).await?;
                // broadcast the block
                self.broadcast(&Message::BlockTransmission(block.clone()))
                    .await?;
                Ok(Message::BlockAck)
            },
            Message::BlockRequest(hash) => {
                // send a block to the peer upon request
                let block = self.chain.lock().await.get_block(hash).cloned();
                Ok(Message::BlockResponse(block))
            },
            Message::ChainDiscoveryRequest => {
                // send the block headers to the peer
                let headers = self.chain.lock().await.get_block_headers();
                Ok(Message::ChainDiscoveryResponse(headers))
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Expected a request",
            )),
        }
    }

    /// sends an error response when given a string description
    async fn send_error_message(&self, stream: &mut TcpStream, e: impl std::error::Error) {
        stream
            .write_all(&bincode::serialize(&Message::Error(e.to_string())).unwrap())
            .await
            .unwrap();
    }

    async fn maybe_update_peers(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers = self.peers.lock().await;
        if !peers.iter().any(|p| p.public_key == peer.public_key) {
            // add the peer to the list
            peers.push(peer);
        }
        Ok(())
    }
}

impl Into<Peer> for Node {
    fn into(self) -> Peer {
        Peer::new(*self.public_key, self.ip_address, *self.port)
    }
}

pub trait Broadcaster {
    /// Broadcast a message to all peers
    async fn broadcast(&self, message: &Message) -> Result<(), std::io::Error>;
}

impl Broadcaster for Node {
    async fn broadcast(&self, message: &Message) -> Result<(), std::io::Error> {
        // send a message to all peers
        for peer in self.peers.lock().await.iter_mut() {
            peer.communicate(message, self.clone().into()).await?;
        }
        Ok(())
    }
}
