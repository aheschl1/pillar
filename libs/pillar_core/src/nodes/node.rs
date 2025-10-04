use super::peer::Peer;
use flume::{Receiver, Sender};
use pillar_crypto::{hashing::{DefaultHash, Hashable}, signing::{DefaultSigner, SigFunction, Signable}, types::StdByteArray};
use tracing::instrument;
use std::{collections::{HashMap, HashSet}, net::IpAddr, sync::Arc};
use tokio::sync::{Mutex, RwLock};

use crate::{
    blockchain::chain::Chain,
    persistence::database::{Datastore, EmptyDatastore},
    primitives::{block::{Block, BlockHeader, Stamp}, messages::Message, pool::MinerPool, transaction::{FilterMatch, TransactionFilter}},
    protocol::{chain::{block_settle_consumer, dicover_chain, service_sync, sync_chain},
    communication::{broadcast_knowledge, serve_peers},
    reputation::{nth_percentile_peer, N_TRANSMISSION_SIGNATURES}},
};
 
/// Operational state of a node, which controls how it treats incoming data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeState{
    ICD,
    ChainOutdated,
    ChainLoading,
    ChainSyncing,
    Serving
}

impl NodeState {
    /// If a state is_track, then the node tracks incoming blocks/transactions.
    /// They are queued and applied when the chain is ready, not immediately.
    pub fn is_track(&self) -> bool {
        // return true;
        matches!(self, 
            NodeState::ICD | 
            NodeState::ChainSyncing | 
            NodeState::ChainLoading | 
            NodeState::ChainOutdated 
        )
    }

    /// If a state is_forward, then the node forwards incoming blocks/transactions.
    pub fn is_forward(&self) -> bool {
        // return true;
        matches!(self,
            NodeState::ICD |
            NodeState::ChainOutdated | 
            NodeState::ChainLoading | 
            NodeState::ChainSyncing | 
            NodeState::Serving
        )
    }

    /// If a state is_consume, the node consumes data immediately and updates state.
    /// This includes draining any pending tracking queue.
    pub fn is_consume(&self) -> bool {
        // return true;
        matches!(self, 
            NodeState::Serving
        )
    }
}

fn get_initial_state(datastore: &dyn Datastore) -> (NodeState, Option<Chain>) {
    // check if the datastore has a chain
    if datastore.latest_chain().is_some() {
        // if it does, load the chain
        match datastore.load_chain() {
            Ok(chain) => {
                // assign the chain to the node
                (NodeState::ChainOutdated, Some(chain))
            },
            Err(_) => {
                // if it fails, we are in discovery mode
                (NodeState::ICD, None)
            }
        }
    } else {
        // if it does not, we are in discovery mode
        (NodeState::ICD, None)
    }
}

/// Shared inner state of a node (behind `Arc`).
pub struct NodeInner {
    pub public_key: StdByteArray,
    /// The private key of the node
    pub private_key: StdByteArray,
    // known peers
    pub peers: RwLock<HashMap<StdByteArray, Peer>>,
    // the blockchain
    pub chain: Mutex<Option<Chain>>,
    /// transactions to be broadcasted
    pub broadcast_queue: lfqueue::UnboundedQueue<Message>,
    // a collection of things already broadcasted
    pub broadcasted_already: RwLock<HashSet<StdByteArray>>,
    // transaction filter queue
    pub transaction_filters: Mutex<Vec<(TransactionFilter, Peer)>>,
    /// the datastore
    pub datastore: Option<Arc<dyn Datastore>>,
    /// A queue of blocks which are to be settled to the chain
    pub late_settle_queue: lfqueue::UnboundedQueue<Block>,
    /// the state represents the nodes ability to communicate with other nodes
    pub state: RwLock<NodeState>,
    /// registered filters for the local node - producer will be this node, and consumer will be some backgroung thread that polls
    pub filter_callbacks: Mutex<HashMap<TransactionFilter, Sender<BlockHeader>>>,
}

/// A Pillar network node capable of participating in consensus and gossip.
#[derive(Clone)]
pub struct Node {
    pub inner: Arc<NodeInner>,
    /// The IP address of the node
    pub ip_address: IpAddr,
    /// The port of the node
    pub port: u16,
    /// transactions to be serviced
    pub miner_pool: Option<MinerPool>,
    /// kill handles
    kill_broadcast: Option<flume::Sender<()>>,
    kill_serve: Option<flume::Sender<()>>,
    kill_settle: Option<flume::Sender<()>>,
}



impl Node {
    /// Create a new node
    #[instrument(
        name = "Node::new",
        skip_all,
        fields(
            public_key = ?public_key,
            ip_address = ?ip_address,
            port = port
        )
    )]
    pub fn new(
        public_key: StdByteArray,
        private_key: StdByteArray,
        ip_address: IpAddr,
        port: u16,
        peers: Vec<Peer>,
        mut database: Option<Arc<dyn Datastore>>,
        transaction_pool: Option<MinerPool>,
    ) -> Self {
        if database.is_none() {
            tracing::warn!("Node created without a database. This will not persist the chain or transactions.");
            database = Some(Arc::new(EmptyDatastore::new()));
        }
        let broadcast_queue = lfqueue::UnboundedQueue::new();
        let late_settle_queue = lfqueue::UnboundedQueue::new();
        let transaction_filters = Mutex::new(Vec::new());
        let broadcasted_already = RwLock::new(HashSet::new());
        let peer_map = peers
            .iter()
            .map(|peer| (peer.public_key, *peer))
            .collect::<HashMap<_, _>>();
        tracing::info!("Node created with {} initial peers", peer_map.len());
        let (state, maybe_chain) = get_initial_state(&**database.as_ref().unwrap());
        tracing::debug!("Node initial state: {:?}", state);
        Node {
            inner: NodeInner {
            public_key,
            private_key,
            peers: RwLock::new(peer_map),
            chain: Mutex::new(maybe_chain),
            broadcasted_already,
            transaction_filters,
            broadcast_queue,
            filter_callbacks: Mutex::new(HashMap::new()), // initially no callbacks
            state: RwLock::new(state), // initially in discovery mode
            late_settle_queue,
            datastore: database,
            }.into(),
            ip_address,
            port,
            miner_pool: transaction_pool,
            kill_broadcast: None,
            kill_serve: None,
            kill_settle: None,
        }
    }
    
    /// Launch peer serving, broadcast, and block-settle tasks; start ICD/sync if needed.
    #[instrument(skip_all, name = "Node::serve", fields(
        public_key = ?self.inner.public_key,
        ip_address = ?self.ip_address,
        port = self.port
    ))]
    pub async fn serve(&mut self) {
        // spawn a new thread to handle the connection
        tracing::info!("Node is starting up on {}:{}", self.ip_address, self.port);
        
        let broadcast_killer = flume::bounded(1);
        let serve_killer = flume::bounded(1);
        let settle_killer = flume::bounded(1);
    
        let state = self.inner.state.read().await.clone();
        let handle = match state {
            NodeState::ICD => {
                tracing::info!("Starting ICD.");
                *self.inner.state.write().await = NodeState::ChainLoading; // update state to chain loading
                let handle = tokio::spawn(dicover_chain(self.clone()));
                Some(handle)
            },
            NodeState::ChainOutdated => {
                tracing::info!("Node has an outdated chain. Starting sync.");
                *self.inner.state.write().await = NodeState::ChainSyncing; // update state to chain syncing
                let handle = tokio::spawn(sync_chain(self.clone()));
                Some(handle)
            },
            _ => {
                panic!("Unexpected node state for startup: {:?}", self.inner.state);
            }
        };
        if let Some(handle) = handle {
            let self_clone = self.clone();
            tokio::spawn(async move {
                let result = handle.await;
                match result {
                    Ok(Ok(_)) => {
                        tracing::info!(target: "node_serve", "Node setup successfully.");
                        let mut state = self_clone.inner.state.write().await;
                        // update to serving
                        *state = NodeState::Serving;
                    },
                    Ok(Err(e)) => tracing::error!(target: "node_serve", "Node failed to start: {:?}", e),
                    Err(e) => tracing::error!(target: "node_serve", "Failed to start node: {:?}", e),
                }
            });
        }
        tracing::trace!("Node is now in state: {:?}", self.inner.state.read().await);
        let _ = tokio::spawn(serve_peers(self.clone(), Some(serve_killer.1.clone())));
        let _ = tokio::spawn(broadcast_knowledge(self.clone(), Some(broadcast_killer.1.clone())));
        let _ = tokio::spawn(block_settle_consumer(self.clone(), Some(settle_killer.1.clone())));
        self.kill_broadcast = Some(broadcast_killer.0);
        self.kill_serve = Some(serve_killer.0);
        self.kill_settle = Some(settle_killer.0);
        tracing::info!("Node processes finished launching. Broadcasting and serving threads are now running.");
    }

    #[instrument(name = "Node::stop", skip(self), fields(
        public_key = ?self.inner.public_key,
        ip_address = ?self.ip_address,
        port = self.port
    ))]
    pub async fn stop(&mut self) {
        // stop the broadcast and serve threads
        let _ = self.kill_broadcast.as_ref().unwrap().send(());
        let _ = self.kill_serve.as_ref().unwrap().send(());
        let _ = self.kill_settle.as_ref().unwrap().send(());
        tracing::debug!("Kill signals sent.");
        *self.inner.state.write().await = NodeState::ChainOutdated;
        tracing::info!("Node stopping.");
    }

    /// Register a transaction filter callback, enqueue filter, and broadcast the request.
    /// If a peer observes a matching block, the header will be sent on the returned channel.
    #[instrument(name = "Node::register_transaction_callback", skip(self, filter), fields(
        public_key = ?self.inner.public_key,
        filter = ?filter
    ))]
    pub async fn register_transaction_callback(&mut self, filter: TransactionFilter) -> Receiver<BlockHeader> {
        // create a new channel
        let (sender, receiver) = flume::bounded(1);
        // add the filter to the list
        self.inner.filter_callbacks.lock().await.insert(filter.clone(), sender);
        // add the filter to the transaction filters
        self.inner.transaction_filters.lock().await.push((filter.clone(), self.clone().into()));
        tracing::info!("Transaction filter registered, starting brodcast");
        // broadcast the filter to all peers
        self.inner.broadcast_queue.enqueue(Message::TransactionFilterRequest(filter, self.clone().into()));
        // return the receiver
        receiver
    }

    /// Derive the response to a request from a peer.
    #[instrument(name = "Node::serve_request", skip(self, message, _declared_peer), fields(
        public_key = ?self.inner.public_key,
        peer = ?_declared_peer.public_key,
        message = ?message.name()
    ))]
    pub async fn serve_request(&mut self, message: &Message, _declared_peer: Peer) -> Result<Message, std::io::Error> {
        let state = self.inner.state.read().await.clone();
        match message {
            Message::PeerRequest => {
                // send all peers
                let response = Message::PeerResponse(self.inner.peers.read().await.values().cloned().collect());
                tracing::debug!("Sending {:?}", response);
                Ok(response)
            }
            Message::ChainRequest => {
                let response = if state.is_consume(){ // in consume state, chain is actively being consumed
                    Ok(Message::ChainResponse(self.inner.chain.lock().await.clone().unwrap()))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                };
                tracing::debug!("Sending {:?}", response);
                response
            },
            Message::TransactionBroadcast(transaction) => {
                // add the transaction to the pool
                if let Some(ref pool) = self.miner_pool
                    && state.is_consume() {
                        tracing::info!("Adding transaction to mining pool.");
                        pool.add_transaction(*transaction);
                    }
                // to be broadcasted
                if state.is_forward(){
                    tracing::info!("Broadcasting transaction");
                    self.inner.broadcast_queue.enqueue(Message::TransactionBroadcast(transaction.to_owned()));
                }
                Ok(Message::TransactionAck)
            }
            Message::BlockTransmission(block) => {
                // add the block to the chain if we have downloaded it already - first it is verified
                let mut block = block.clone();
                if state.is_consume() && block.header.completion.is_none(){
                    self.settle_unmined_block(&mut block).await?;
                }
                
                // send block to be settled, 
                // and handle callback if mined
                if (state.is_track() || state.is_consume()) && block.header.completion.is_some() {
                    tracing::info!("Handling callbacks and settle for mined block.");
                    self.inner.late_settle_queue.enqueue(block.clone());
                    self.handle_callbacks(&block).await;
                }
                
                if state.is_forward(){
                    // TODO handle is_track instead
                    // if we do not have the chain, just forward the block if there is room in the stamps
                    if block.header.tail.n_stamps() < N_TRANSMISSION_SIGNATURES && !state.is_consume() && block.header.completion.is_none() {
                        tracing::info!("Stamping and broadcasting only because not ");
                        let _ = self.stamp_block(&mut block);
                    }
                    self.inner.broadcast_queue.enqueue(Message::BlockTransmission(block)); // forward
                }
                Ok(Message::BlockAck)
            },
            Message::BlockRequest(hash) => {
                // send a block to the peer upon request
                if state.is_consume(){
                    let lock = self.inner.chain.lock().await;
                    let chain = lock.as_ref().unwrap();
                    let block = chain.get_block(hash).cloned();
                    Ok(Message::BlockResponse(block))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::ChainShardRequest => {
                // send the block headers to the peer
                if state.is_consume(){
                    let lock = self.inner.chain.lock().await;
                    let chain = lock.as_ref().unwrap().clone();
                    Ok(Message::ChainShardResponse(chain.into()))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionProofRequest(stub) => {
                if state.is_consume(){
                    let lock = self.inner.chain.lock().await;
                    let chain = lock.as_ref().unwrap().clone();

                    let block = chain.get_block(&stub.block_hash);
                    
                    if let Some(block) = block{
                        let proof = block.get_proof_for_transaction(stub.transaction_hash);
                        Ok(Message::TransactionProofResponse(proof.unwrap()))
                    }else{
                        Ok(Message::Error("Block does not exist".into()))
                    }                 
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionFilterRequest(filter, peer) => {
                // place into the transaction filter queue - if it is not already there
                let mut transaction_filters = self.inner.transaction_filters.lock().await;
                if transaction_filters.iter().any(|(f, p)| f == filter && p == peer) {
                    self.inner.broadcast_queue.enqueue(Message::TransactionFilterRequest(filter.to_owned(), peer.to_owned()));
                    transaction_filters.push((filter.to_owned(), peer.to_owned()));
                }
                // to be broadcasted
                Ok(Message::TransactionFilterAck)
            },
            Message::TransactionFilterResponse(filter, header) => {
                if state.is_track(){
                    let mut callbacks = self.inner.filter_callbacks.lock().await;
                    if let Some(sender) = callbacks.get_mut(filter) {
                        // send the block header to the sender
                        sender.send(*header).unwrap();
                        // remove the callback - one time only
                        callbacks.remove(filter);
                    }
                }
                Ok(Message::TransactionFilterAck)
            },
            Message::PercentileFilteredPeerRequest(lower_n, upper_n) => {
                if state.is_consume(){
                    let _chain = self.inner.chain.lock().await;
                    let chain = _chain.as_ref().unwrap();
                    let peers = nth_percentile_peer(*lower_n, *upper_n, chain);
                    let peers_map = self.inner.peers.read().await.clone();
                    // find the peer objects in the address list 
                    let filtered_peers = peers.iter().filter_map(|peer| {
                        peers_map.get(peer).cloned()
                    }).collect::<Vec<_>>();
                    Ok(Message::PercentileFilteredPeerResponse(filtered_peers))
                }else{
                    Ok(Message::PercentileFilteredPeerResponse(vec![])) // just say nothing - info not up to date
                }
            },
            Message::ChainSyncRequest(leaves) => {
                if state.is_consume() {
                    let chains = service_sync(self.clone(), leaves).await?;
                    Ok(Message::ChainSyncResponse(chains))
                }else{
                    Ok(Message::ChainSyncResponse(vec![]))
                }
            },
            Message::DiscoveryRequest => {
                Ok(Message::DiscoveryResponse(self.clone().into()))
            },
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Expected a request",
            )),
        }
    }

    /// After receiving an unmined block, stamp/broadcast/maybe enqueue for mining.
    /// Includes reputation tracking, broadcasting, and callback responses.
    #[instrument(name = "Node::settle_unmined_block", skip(self, block))]
    async fn settle_unmined_block(&self, block: &mut Block) -> Result<(), std::io::Error> {
        tracing::info!("Block is not mined, stamping and transmitting.");
        let mut n_stamps = block.header.tail.n_stamps();
        let has_our_stamp = block.header.tail.stamps.iter().any(|stamp| stamp.address == self.inner.public_key);
        let already_broadcasted = self.inner.broadcasted_already.read().await.contains(
            &Message::BlockTransmission(block
                .clone())
                .hash(&mut DefaultHash::new())
                .unwrap()
            );
        tracing::debug!("Block has {} stamps, our stamp: {}, already broadcasted: {}", n_stamps, has_our_stamp, already_broadcasted);
        
        if n_stamps < N_TRANSMISSION_SIGNATURES && !has_our_stamp {
            assert!(!already_broadcasted, "Block already broadcasted, but not stamped by us. This should not happen.");
            tracing::info!("Stamping block with our signature.");
            let _ = self.stamp_block(block);
            n_stamps += 1; // we have stamped the block
        }
        
        if ((already_broadcasted || n_stamps == N_TRANSMISSION_SIGNATURES)) && self.miner_pool.is_some(){
            // add the block to the pool
            tracing::info!("Adding block to miner pool.");
            self.miner_pool.as_ref().unwrap().add_mine_ready_block(block.clone());
        }
        Ok(())
    }

    /// Spawn a task to match registered transaction filters against this block.
    #[instrument(name = "Node::handle_callbacks", skip(self, block))]
    async fn handle_callbacks(&self, block: &Block){
        let block_clone = block.clone();
        let initpeer: Peer = self.clone().into();
        let selfclone = self.clone();
        tracing::debug!("Spawning callback handler for block: {:?}", block_clone.header.hash(&mut DefaultHash::new()).unwrap());
        tokio::spawn(async move {
            // check filters for callback
            let mut filters = selfclone.inner.transaction_filters.lock().await;
            for ( filter, peer) in filters.iter_mut() {
                if filter.matches(&block_clone){
                    tracing::info!("Found callback for filter: {:?}", filter);
                    peer.communicate(&Message::TransactionFilterResponse(filter.clone(), block_clone.header), &initpeer).await.unwrap();
                    // check if there is a registered callback
                    let mut callbacks: tokio::sync::MutexGuard<'_, HashMap<TransactionFilter, Sender<BlockHeader>>> = selfclone.inner.filter_callbacks.lock().await;
                    // TODO maybe this is not nececarry - some rework?
                    if let Some(sender) = callbacks.get_mut(filter) {
                        // send the block header to the sender
                        sender.send(block_clone.header).unwrap();
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
            address: self.inner.public_key,
            signature,
        };
        block.header.tail.stamp(stamp)
    }

    /// ed25519 signature for the node over the canonical bytes of `sign`.
    fn signature_for<T: Signable<64>>(&self, sign: &T) -> Result<[u8; 64], std::io::Error> {
        // sign the data with the private key
        let mut signer = DefaultSigner::new(self.inner.private_key);
        let signature = signer.sign(sign);
        // return the signature
        Ok(signature)
    }

    /// Insert a peer into the set if it's not ourselves and not already present.
    pub async fn maybe_update_peer(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers = self.inner.peers.write().await;
        if (peer.public_key != self.inner.public_key) && !peers.iter().any(|(public_key, _)| public_key == &peer.public_key) {
            // add the peer to the list
            peers.insert(peer.public_key, peer);
        }
        Ok(())
    }

    /// Convenience to insert multiple peers via `maybe_update_peer`.
    pub async fn maybe_update_peers(&self, peers: Vec<Peer>){
        for peer in peers {
            self.maybe_update_peer(peer).await.unwrap();
        }
    }

}

impl From<&Node> for Peer {
    fn from(node: &Node) -> Self {
        Peer::new(node.inner.public_key, node.ip_address, node.port)
    }
}

impl From<Node> for Peer {
    fn from(node: Node) -> Self {
        Peer::new(node.inner.public_key, node.ip_address, node.port)
    }
}

pub trait Broadcaster {
    /// Broadcast a message to all peers.
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error>;
}

impl Broadcaster for Node {
    #[instrument(name = "Node::broadcast", skip(self, message), fields(
        public_key = ?self.inner.public_key,
        message = ?message.name()
    ))]
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error> {
        // send a message to all peers
        let mut responses = Vec::new();
        let peers = self.inner.peers.read().await.clone();
        for (_, peer) in peers.iter(){
            let response = peer.communicate(message, &self.into()).await;
            if let Err(e) = response {
                tracing::error!("Failed to communicate with peer {:?}: {:?}", peer.public_key, e);
                continue; // skip this peer
            }
            responses.push(response.unwrap());
        }
        Ok(responses)
    }
}