use super::peer::Peer;
use ed25519::signature::SignerMut;
use ed25519_dalek::SigningKey;
use flume::{Receiver, Sender};
use std::{collections::{HashMap, HashSet}, net::IpAddr, sync::Arc};
use tokio::sync::Mutex;

use super::messages::Message;

use crate::{
    blockchain::chain::Chain, crypto::hashing::{DefaultHash, HashFunction, Hashable}, primitives::{block::{Block, BlockHeader, Stamp}, pool::MinerPool, transaction::{FilterMatch, TransactionFilter}}, protocol::{chain::dicover_chain, communication::{broadcast_knowledge, serve_peers}, reputation::N_TRANSMISSION_SIGNATURES}, reputation::history::NodeHistory
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
    pub peers: Arc<Mutex<HashMap<[u8; 32], Peer>>>,
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
    /// mapping of reputations for peers
    pub reputations: Arc<Mutex<HashMap<[u8; 32], NodeHistory>>>,
    /// registered filters for the local node - producer will be this node, and consumer will be some backgroung thread that polls
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
        let peer_map = peers
            .iter()
            .map(|peer| (peer.public_key, peer.clone()))
            .collect::<HashMap<_, _>>();
        Node {
            public_key: public_key.into(),
            private_key: private_key.into(),
            ip_address,
            port: port.into(),
            peers: Arc::new(Mutex::new(peer_map)),
            chain: Arc::new(Mutex::new(chain)),
            miner_pool: transaction_pool.map(Arc::new),
            broadcast_receiver,
            broadcast_sender,
            transaction_filters,
            reputations: Arc::new(Mutex::new(HashMap::new())),
            filter_callbacks: Arc::new(Mutex::new(HashMap::new())), // initially no callbacks
        }
    }

    
    /// when called, launches a new thread that listens for incoming connections
    pub async fn serve(&self) {
        // spawn a new thread to handle the connection
        let self_clone = self.clone();
        if let None = self.chain.lock().await.as_ref(){
            let mut selfcloneclone = self_clone.clone();
            // TODO this should be for brand new nodes only
            // After persistence use sync_chain
            tokio::spawn(async move{
                // try to discover
                let chain = dicover_chain(
                    &mut selfcloneclone
                ).await.unwrap();
                // asign the chain to the node
                selfcloneclone.chain.lock().await.replace(chain);
            });
        }
        tokio::spawn(serve_peers(self_clone.clone()));
        tokio::spawn(broadcast_knowledge(self_clone));
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
    pub async fn serve_request(&mut self, message: &Message, _declared_peer: Peer) -> Result<Message, std::io::Error> {
        match message {
            Message::PeerRequest => {
                // send all peers
                Ok(Message::PeerResponse(self.peers.lock().await.values().cloned().collect()))
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
                // if block.header.miner_address.unwrap() == *self.public_key {
                //     // SKIP OWN BLOCK 
                //     return Ok(Message::BlockAck);
                // }
                // add the block to the chain if we have downloaded it already - first it is verified TODO add to a queue to be added later
                if let Some(chain) = self.chain.lock().await.as_mut(){
                    self.settle_block(block, chain).await?;
                }else{
                    // TODO is it ok to ignore rep here?
                    // if we do not have the chain, just forward the block if there is room in the stamps
                    if block.tail.n_stamps() < N_TRANSMISSION_SIGNATURES {
                        let _ = self.stamp_block(&mut block.clone());
                        self.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap();
                    }
                }
                // handle callback
                self.handle_callbacks(block).await;
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

    /// After receiving a block - settle it to the chain
    /// Includes the tracking of reputation, braodcasting, and responding to callbacks
    async fn settle_block(&self, block: &Block, chain: &mut Chain) -> Result<(), std::io::Error> {
        let existing_block = chain.get_block(&block.hash.unwrap()).cloned(); // we will compare for changes to stamps
        chain.add_new_block(block.clone())?; // if we pass this line, valid block
        // update the reputation tracker with the block to rwared the miner
        self.maybe_create_history(block.header.miner_address.unwrap()).await;
        let mut reputations = self.reputations.lock().await;
        let miner_history = reputations.get_mut(&block.header.miner_address.unwrap()).unwrap();
        miner_history.settle_head(block.header.clone()); // SETTLE TO REWARD MINING
        // reward the broadcaster
        // self.maybe_create_history(declared_peer.public_key).await;
        // let broadcaster_history = reputations.get_mut(&declared_peer.public_key).unwrap();
        // broadcaster_history.reward_block_transmission();
        // ================================================
        // 1. check for stamp differences
        let existing_stamps = match existing_block { // what was there before
            Some(block) => HashSet::from(block.tail.stamps),
            None => HashSet::new(),
        };
        let has_our_stamp = existing_stamps.iter().any(|stamp| stamp.address == *self.public_key);
        // 2. start mutation by adding our own if relevant
        let block = chain.get_block_mut(&block.hash.unwrap()).unwrap();
        // stamp if we have not already
        if !has_our_stamp {
            let signature = self.signature_for(&block.header)?;
            let stamp = Stamp {
                address: *self.public_key,
                signature,
            };
            let _ = block.tail.stamp(stamp); // if failure, then full
        }
        let new_stamps = HashSet::from(block.tail.stamps); // what is there now
        let added_stamps = new_stamps.difference(&existing_stamps).cloned().collect::<HashSet<_>>(); // the new ones
        // 3. reward reputation for the new stamps
        for stamp in added_stamps.iter() {
            self.maybe_create_history(stamp.address).await;
            let history = reputations.get_mut(&stamp.address).unwrap();
            history.settle_tail(&block.tail, block.header); // SETTLE TO REWARD
        }
        // 4. if changes have been made - broadcast the block
        if !added_stamps.is_empty() { // if it is, we simply broadcast the block
            // broadcast the block to all peers
            self.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap();
            // TODO maybe we should not do this here - but rather in the broadcast thread
            // self.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap();
        }
        Ok(())
    }

    /// spawn a new thread to match transaction callback requests against the bock
    async fn handle_callbacks(&self, block: &Block){
        let filters = self.transaction_filters.clone();
        let block_clone = block.clone();
        let initpeer: Peer = self.clone().into();
        let selfclone = self.clone();
        tokio::spawn(async move {
            // check filters for callback
            for ( filter, peer) in filters.lock().await.iter_mut() {
                if filter.matches(&block_clone){
                    peer.communicate(&Message::TransactionFilterResponse(filter.clone(), block_clone.header), &initpeer).await.unwrap();
                    // check if there is a registered callback
                    let mut callbacks: tokio::sync::MutexGuard<'_, HashMap<TransactionFilter, Sender<BlockHeader>>> = selfclone.filter_callbacks.lock().await;
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
    }

    fn stamp_block(&self, block: &mut Block) -> Result<(), std::io::Error> {
        // sign the block with the private key
        let signature = self.signature_for(&block.header)?;
        // add the stamp to the block
        let stamp = Stamp {
            address: *self.public_key,
            signature: signature,
        };
        block.tail.stamp(stamp)
    }

    /// ed25519 signature for the node over the hash of some data
    fn signature_for<T: Hashable>(&self, sign: &T) -> Result<[u8; 64], std::io::Error> {
        // sign the data with the private key
        let mut signer = SigningKey::from_bytes(&self.private_key);
        let signature = signer.sign(&sign.hash(&mut DefaultHash::new())?);
        // return the signature
        Ok(signature.to_bytes())
    }

    pub async fn maybe_update_peer(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers = self.peers.lock().await;
        if !peers.iter().any(|(public_key, _)| public_key == &peer.public_key) {
            // add the peer to the list
            peers.insert(peer.public_key, peer);
        }
        Ok(())
    }

    pub async fn maybe_update_peers(&self, peers: Vec<Peer>){
        for peer in peers {
            self.maybe_update_peer(peer).await.unwrap();
        }
    }

    /// create a new history for the node if it does not exist
    async fn maybe_create_history(&self, public_key: [u8; 32]) {
        // get the history of the node
        let mut reputations = self.reputations.lock().await;
        if let None = reputations.get_mut(&public_key) {
            // create a new history
            reputations.insert(public_key, NodeHistory::default()); // create new
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

pub trait Broadcaster {
    /// Broadcast a message to all peers
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error>;
}

impl Broadcaster for Node {
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error> {
        // send a message to all peers
        let mut responses = Vec::new();
        for (_, peer) in self.peers.lock().await.iter_mut() {
            responses.push(peer.communicate(message, &self.into()).await?);
        }
        Ok(responses)
    }
}
