use super::{messages::{get_declaration_length, Versions}, peer::{self, Peer}};
use flume::{Receiver, Sender};
use std::{collections::HashMap, net::IpAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

use super::messages::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    blockchain::chain::Chain,
    primitives::{block::BlockHeader, pool::MinerPool, transaction::{FilterMatch, Transaction, TransactionFilter}}, protocol::chain::dicover_chain,
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
    pub chain: Arc<Mutex<Option<Chain>>>,
    /// transactions to be serviced
    pub miner_pool: Option<Arc<MinerPool>>,
    /// transactions to be broadcasted
    pub broadcast_receiver: Receiver<Message>,
    /// transaction input
    pub broadcast_sender: Sender<Message>,
    // transaction filter queue
    pub transaction_filters: Arc<Mutex<Vec<(TransactionFilter, Peer)>>>,
    // registered filters for the local node - producer will be this node, and consumer will be some backgroung thread that polls
    filter_callbacks: Arc<Mutex<HashMap<TransactionFilter, Sender<BlockHeader>>>>,
}

impl Node {
    /// Create a new node
    pub fn new(
        public_key: [u8; 32],
        private_key: [u8; 32],
        ip_address: IpAddr,
        port: u16,
        peers: Vec<Peer>,
        chain: Option<Chain>,
        transaction_pool: Option<MinerPool>,
    ) -> Self {
        let (broadcast_sender, broadcast_receiver) = flume::unbounded();
        let transaction_filters = Arc::new(Mutex::new(Vec::new()));
        Node {
            public_key: public_key.into(),
            private_key: private_key.into(),
            ip_address,
            port: port.into(),
            peers: Mutex::new(peers).into(),
            chain: Arc::new(Mutex::new(chain)),
            miner_pool: transaction_pool.map(Arc::new),
            broadcast_receiver,
            broadcast_sender,
            transaction_filters,
            filter_callbacks: Arc::new(Mutex::new(HashMap::new())), // initially no callbacks
        }
    }

    
    /// when called, launches a new thread that listens for incoming connections
    pub async fn serve(&self) {
        // spawn a new thread to handle the connection
        let self_clone = self.clone();
        if let None = self.chain.lock().await.as_ref(){
            let mut selfcloneclone = self_clone.clone();
            tokio::spawn(async move{
                // try to discover
                let chain = dicover_chain(
                    &mut selfcloneclone
                ).await.unwrap();
                // asign the chain to the node
                selfcloneclone.chain.lock().await.replace(chain);
            });
        }
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
            let mut buffer = [0; get_declaration_length(Versions::V1V4) as usize];
            let n = stream.read_exact(&mut buffer).await.unwrap();
            // deserialize with bincode
            let declaration: Result<Message, Box<bincode::ErrorKind>> =
            bincode::deserialize(&buffer[..n]);
            if declaration.is_err() {
                // halt communication
                return;
            }
            let declaration = declaration.unwrap();
            let mut message_length;
            match declaration {
                Message::Declaration(peer, n) => {
                    message_length = n;
                    // add the peer to the list if and only if it is not already in the list
                    self_clone.maybe_update_peer(peer.clone()).await.unwrap();
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
            let mut buffer = Vec::with_capacity(message_length as usize);
            let n = stream.read_exact(&mut buffer).await.unwrap();
            let message: Result<Message, Box<bincode::ErrorKind>> =bincode::deserialize(&buffer[..n]);
            if message.is_err() {
                // halt
                return;
            }
            let message = message.unwrap();
            let response = self_clone.serve_request(&message).await;
            match response {
                Err(e) => self_clone.send_error_message(&mut stream, e).await,
                Ok(message) => {
                    let nbytes = bincode::serialized_size(&message).unwrap() as u32;
                    // write the size of the message as 4 bytes - 4 bytes because we are using u32
                    stream.write(&nbytes.to_le_bytes()[..4]).await.unwrap();
                    stream
                        .write_all(&bincode::serialize(&message).unwrap())
                        .await
                        .unwrap()
                }
            };
        });
    }
    }
    
    /// Background process that consumes mined blocks, and transactions which must be forwarded
    async fn broadcast_knowledge(self) -> Result<(), std::io::Error> {
        loop {
            // send a message to all peers
            if let Some(pool) = &self.miner_pool { // broadcast out of mining pool
                while pool.block_count() > 0 {
                    self.broadcast(&Message::BlockTransmission(pool.pop_block().unwrap()))
                    .await?;
                }
            }
            let mut i = 0;
            while i < 10 && !self.broadcast_receiver.is_empty() {
                // receive the transaction from the sender
                let message = self.broadcast_receiver.recv().unwrap();
                self.broadcast(&message).await?;
                i += 1; // We want to make sure we check back at the mineing pool
            }
        }
    }

    /// Register a transaction filter callback - adds the callback channel and adds it to the transaction filter queue
    /// Sends a broadcast to request peers to also watch for the block - if a peer catches it, it will be sent back
    pub async fn register_transaction_callback(&mut self, filter: TransactionFilter) -> Receiver<BlockHeader> {
        // create a new channel
        let (sender, receiver) = flume::bounded(1);
        // add the filter to the list
        self.filter_callbacks.lock().await.insert(filter.clone(), sender);
        // add the filter to the transaction filters
        self.transaction_filters.lock().await.push((filter.clone(), self.clone().into()));
        // broadcast the filter to all peers
        self.broadcast_sender.send(Message::TransactionFilterRequest(filter, self.clone().into())).unwrap();
        // return the receiver
        receiver
    }

    /// Derive the response to a request from a peer
    async fn serve_request(&mut self, message: &Message) -> Result<Message, std::io::Error> {
        match message {
            Message::PeerRequest => {
                // send all peers
                Ok(Message::PeerResponse(self.peers.lock().await.clone()))
            }
            Message::ChainRequest => {
                if let Some(chain) = self.chain.lock().await.as_ref(){
                    Ok(Message::ChainResponse(chain.clone()))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
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
                self.broadcast_sender.send(Message::TransactionRequest(transaction.to_owned())).unwrap();
                Ok(Message::TransactionAck)
            }
            Message::BlockTransmission(block) => {
                // spawn a new thread to match transaction requests against the bock
                let filters = self.transaction_filters.clone();
                let block_clone = block.clone();
                let initpeer: Peer = self.clone().into();
                let selfclone = self.clone();
                tokio::spawn(async move {
                    for ( filter, peer) in filters.lock().await.iter_mut() {
                        if filter.matches(&block_clone){
                            peer.communicate(&Message::TransactionFilterResponse(filter.clone(), block_clone.header), &initpeer).await.unwrap();
                            // check if there is a registered callback
                            let mut callbacks = selfclone.filter_callbacks.lock().await;
                            // TODO maybe this is not nececarry - some rework?
                            if let Some(sender) = callbacks.get_mut(filter) {
                                // send the block header to the sender
                                sender.send(block_clone.header.clone()).unwrap();
                                // remove the callback - one time only
                                callbacks.remove(filter);
                            } // we will get the callback here if and only if the current active node is the one that resgistered the callback
                            // otherwise, it will come in the FilterResponse
                        }
                    }
                });
                if block.header.miner_address.unwrap() == *self.public_key {
                    // SKIP OWN BLOCK
                    return Ok(Message::BlockAck);
                }
                // add the block to the chain - first it is verified
                if let Some(chain) = self.chain.lock().await.as_mut(){
                    chain.add_new_block(block.clone())?;
                }
                // broadcast the block
                self.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap();
                Ok(Message::BlockAck)
            },
            Message::BlockRequest(hash) => {
                // send a block to the peer upon request
                if let Some(chain) = self.chain.lock().await.as_ref(){
                    let block = chain.get_block(hash).cloned();
                    Ok(Message::BlockResponse(block))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::ChainShardRequest => {
                // send the block headers to the peer
                if let Some(chain) = self.chain.lock().await.as_ref(){
                    Ok(Message::ChainShardResponse(chain.clone().into()))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionProofRequest(stub) => {
                if let Some(chain) = self.chain.lock().await.as_ref(){
                    let block = chain.get_block(&stub.block_hash);
                    
                    if let Some(block) = block{
                        let proof = block.get_proof_for_transaction(stub.transaction_hash);
                        Ok(Message::TransactionProofResponse(proof.unwrap(), block.header.clone()))
                    }else{
                        Ok(Message::Error("Block does not exist".into()))
                    }                 
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionFilterRequest(filter, peer) => {
                // place into the transaction filter queue - if it is not already there
                let mut transaction_filters = self.transaction_filters.lock().await;
                if transaction_filters.iter().any(|(f, p)| f == filter && p == peer) {
                    self.broadcast_sender.send(Message::TransactionFilterRequest(filter.to_owned(), peer.to_owned())).unwrap();
                    transaction_filters.push((filter.to_owned(), peer.to_owned()));
                }
                // to be broadcasted
                Ok(Message::TransactionFilterAck)
            },
            Message::TransactionFilterResponse(filter, header) => {
                let mut callbacks = self.filter_callbacks.lock().await;
                if let Some(sender) = callbacks.get_mut(filter) {
                    // send the block header to the sender
                    sender.send(*header).unwrap();
                    // remove the callback - one time only
                    callbacks.remove(filter);
                }
                Ok(Message::TransactionFilterAck)
            },
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Expected a request",
            )),
        }
    }

    /// sends an error response when given a string description
    async fn send_error_message(&self, stream: &mut TcpStream, e: impl std::error::Error) {
        // writye message size
        let nbytes = bincode::serialized_size(&Message::Error(e.to_string())).unwrap() as u32;
        // write the size of the message as 4 bytes - 4 bytes because we are using u32
        stream.write(&nbytes.to_le_bytes()[..4]).await.unwrap();
        // write the error message to the stream
        stream
            .write_all(&bincode::serialize(&Message::Error(e.to_string())).unwrap())
            .await
            .unwrap();
    }

    async fn maybe_update_peer(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers = self.peers.lock().await;
        if !peers.iter().any(|p| p.public_key == peer.public_key) {
            // add the peer to the list
            peers.push(peer);
        }
        Ok(())
    }

    pub async fn maybe_update_peers(&self, peers: Vec<Peer>){
        for peer in peers {
            self.maybe_update_peer(peer).await.unwrap();
        }
    }
}

impl Into<Peer> for Node {
    fn into(self) -> Peer {
        Peer::new(*self.public_key, self.ip_address, *self.port)
    }
}

impl Into<Peer> for &Node {
    fn into(self) -> Peer {
        Peer::new(*self.public_key, self.ip_address, *self.port)
    }
}

// impl Into<Peer> for &mut Node {
//     fn into(self) -> Peer {
//         Peer::new(*self.public_key, self.ip_address, *self.port)
//     }
// }

pub trait Broadcaster {
    /// Broadcast a message to all peers
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error>;
}

impl Broadcaster for Node {
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error> {
        // send a message to all peers
        let mut responses = Vec::new();
        for peer in self.peers.lock().await.iter_mut() {
            responses.push(peer.communicate(message, &self.into()).await?);
        }
        Ok(responses)
    }
}
