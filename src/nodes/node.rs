use super::peer::Peer;
use flume::{Receiver, Sender};
use tracing::{debug, instrument, Instrument};
use std::{any::Any, collections::{HashMap, HashSet}, net::IpAddr, sync::Arc};
use tokio::{sync::Mutex, task::JoinHandle};

use super::messages::Message;

use crate::{
    blockchain::chain::Chain,
    crypto::{hashing::{DefaultHash, HashFunction, Hashable}, signing::{DefaultSigner, SigFunction, Signable}},
    persistence::database::{Datastore, EmptyDatastore},
    primitives::{block::{Block, BlockHeader, Stamp},
    pool::MinerPool,
    transaction::{FilterMatch, TransactionFilter}},
    protocol::{chain::{dicover_chain, service_sync, sync_chain},
    communication::{broadcast_knowledge, serve_peers},
    reputation::{nth_percentile_peer, N_TRANSMISSION_SIGNATURES}},
    reputation::history::NodeHistory
};
 
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NodeState{
    ICD,
    ChainOutdated,
    ChainLoading,
    ChainSyncing,
    Serving
}

impl NodeState {
    /// If a state is_track, then the node should track incoming blocks and transactions
    /// These values should be added to chain at the soonest available moment
    /// If is_track, then the node does not add requests immediately 
    fn is_track(&self) -> bool {
        // return true;
        matches!(self, 
            NodeState::ICD | 
            NodeState::ChainSyncing | 
            NodeState::ChainLoading | 
            NodeState::ChainOutdated
        )
    }

    /// If a state is_forward, then the node should forward incoming blocks and transactions
    fn is_forward(&self) -> bool {
        // return true;
        matches!(self,
            NodeState::ICD |
            NodeState::ChainOutdated | 
            NodeState::ChainLoading | 
            NodeState::ChainSyncing | 
            NodeState::Serving
        )
    }

    /// If a state is_consume, then the node should consume incoming blocks and transactions
    /// This means that state should be updated immediately. This includes consuming a tracking queue
    fn is_consume(&self) -> bool {
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

pub const STANDARD_ARRAY_LENGTH: usize = 32;

pub type StdByteArray = [u8; STANDARD_ARRAY_LENGTH];

pub struct NodeInner {
    pub public_key: StdByteArray,
    /// The private key of the node
    pub private_key: StdByteArray,
    // known peers
    pub peers: Mutex<HashMap<StdByteArray, Peer>>,
    // the blockchain
    pub chain: Mutex<Option<Chain>>,
    /// transactions to be broadcasted
    pub broadcast_receiver: Receiver<Message>,
    /// transaction input
    pub broadcast_sender: Sender<Message>,
    // a collection of things already broadcasted
    pub broadcasted_already: Mutex<HashSet<StdByteArray>>,
    // transaction filter queue
    pub transaction_filters: Mutex<Vec<(TransactionFilter, Peer)>>,
    /// mapping of reputations for peers
    pub reputations: Mutex<HashMap<StdByteArray, NodeHistory>>,
    /// registered filters for the local node - producer will be this node, and consumer will be some backgroung thread that polls
    filter_callbacks: Mutex<HashMap<TransactionFilter, Sender<BlockHeader>>>,
    /// the state represents the nodes ability to communicate with other nodes
    state: Mutex<NodeState>,
    /// the datastore
    pub datastore: Option<Arc<dyn Datastore>>,
}

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
        let (broadcast_sender, broadcast_receiver) = flume::unbounded();
        let transaction_filters = Mutex::new(Vec::new());
        let broadcasted_already = Mutex::new(HashSet::new());
        let peer_map = peers
            .iter()
            .map(|peer| (peer.public_key, peer.clone()))
            .collect::<HashMap<_, _>>();
        tracing::info!("Node created with {} initial peers", peer_map.len());
        let (state, maybe_chain) = get_initial_state(&**database.as_ref().unwrap());
        tracing::debug!("Node initial state: {:?}", state);
        Node {
            inner: NodeInner {
            public_key,
            private_key,
            peers: Mutex::new(peer_map),
            chain: Mutex::new(maybe_chain),
            broadcasted_already,
            broadcast_receiver,
            broadcast_sender,
            transaction_filters,
            reputations: Mutex::new(HashMap::new()),
            filter_callbacks: Mutex::new(HashMap::new()), // initially no callbacks
            state: Mutex::new(state), // initially in discovery mode
            datastore: database,
            }.into(),
            ip_address,
            port,
            miner_pool: transaction_pool,
            kill_broadcast: None,
            kill_serve: None,
        }
    }
    
    /// when called, launches a new thread that listens for incoming connections
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
    
        let state = self.inner.state.lock().await.clone();
        let handle = match state {
            NodeState::ICD => {
                tracing::info!("Starting ICD.");
                *self.inner.state.lock().await = NodeState::ChainLoading; // update state to chain loading
                let handle = tokio::spawn(dicover_chain(self.clone()));
                Some(handle)
            },
            NodeState::ChainOutdated => {
                tracing::info!("Node has an outdated chain. Starting sync.");
                *self.inner.state.lock().await = NodeState::ChainSyncing; // update state to chain syncing
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
                        let mut state = self_clone.inner.state.lock().await;
                        // update to serving
                        *state = NodeState::Serving;
                    },
                    Ok(Err(e)) => tracing::error!(target: "node_serve", "Node failed to start: {:?}", e),
                    Err(e) => tracing::error!(target: "node_serve", "Failed to start node: {:?}", e),
                }

            });
        }
        tracing::trace!("Node is now in state: {:?}", self.inner.state.lock().await);
        let spawn_handle = tokio::spawn(serve_peers(self.clone(), Some(serve_killer.1.clone())));
        let b_handle = tokio::spawn(broadcast_knowledge(self.clone(), Some(broadcast_killer.1.clone())));
        self.kill_broadcast = Some(broadcast_killer.0);
        self.kill_serve = Some(serve_killer.0);
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
        tracing::debug!("Kill signals sent.");
        *self.inner.state.lock().await = NodeState::ChainOutdated;
        tracing::info!("Node stopping.");
    }

    /// Register a transaction filter callback - adds the callback channel and adds it to the transaction filter queue
    /// Sends a broadcast to request peers to also watch for the block - if a peer catches it, it will be sent back
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
        self.inner.broadcast_sender.send(Message::TransactionFilterRequest(filter, self.clone().into())).unwrap();
        // return the receiver
        receiver
    }

    /// Derive the response to a request from a peer
    #[instrument(name = "Node::serve_request", skip(self, message, _declared_peer), fields(
        public_key = ?self.inner.public_key,
        peer = ?_declared_peer.public_key,
        message = ?message.type_id()
    ))]
    pub async fn serve_request(&mut self, message: &Message, _declared_peer: Peer) -> Result<Message, std::io::Error> {
        let state = self.inner.state.lock().await.clone();
        match message {
            Message::PeerRequest => {
                // send all peers
                let response = Message::PeerResponse(self.inner.peers.lock().await.values().cloned().collect());
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
                if let Some(ref pool) = self.miner_pool{
                    if state.is_consume() {
                        tracing::info!("Adding transaction to mining pool.");
                        pool.add_transaction(*transaction).await;
                    }
                }
                // to be broadcasted
                if state.is_forward(){
                    tracing::info!("Broadcasting transaction");
                    self.inner.broadcast_sender.send(Message::TransactionBroadcast(transaction.to_owned())).unwrap();
                }
                Ok(Message::TransactionAck)
            }
            Message::BlockTransmission(block) => {
                // add the block to the chain if we have downloaded it already - first it is verified TODO add to a queue to be added later
                let mut block = block.clone();
                if state.is_consume(){
                    let mut chain_lock = self.inner.chain.lock().await;
                    tracing::info!("Going to settle this block.");
                    let chain = chain_lock.as_mut().unwrap();
                    self.settle_block(&mut block, chain).await?;
                }else{
                    // TODO handle is_track instead
                    // if we do not have the chain, just forward the block if there is room in the stamps
                    if block.header.tail.n_stamps() < N_TRANSMISSION_SIGNATURES {
                        tracing::info!("Stamping and broadcasting");
                        let _ = self.stamp_block(&mut block.clone());
                        self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap();
                    }
                }
                // handle callback if mined
                if state.is_track() && block.header.miner_address.is_some() {
                    tracing::info!("Handling callbacks for block.");
                    self.handle_callbacks(&block).await;
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
                        Ok(Message::TransactionProofResponse(proof.unwrap(), block.header))
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
                    self.inner.broadcast_sender.send(Message::TransactionFilterRequest(filter.to_owned(), peer.to_owned())).unwrap();
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
                    let reputations = self.inner.reputations.lock().await;
                    let peers = nth_percentile_peer(*lower_n, *upper_n, &reputations);
                    let peers_map = self.inner.peers.lock().await.clone();
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
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Expected a request",
            )),
        }
    }

    /// After receiving a block - settle it to the chain
    /// Includes the tracking of reputation, braodcasting, and responding to callbacks
    #[instrument(name = "Node::settle_block", skip(self, block, chain), fields(
        block_hash = ?block.hash,
        miner_address = ?block.header.miner_address
    ))]
    async fn settle_block(&self, block: &mut Block, chain: &mut Chain) -> Result<(), std::io::Error> {
        if block.header.miner_address.is_some(){ // meaning it is reqrd time
            tracing::info!("Settling mined block with miner address: {:?}", block.header.miner_address);
            chain.add_new_block(block.clone())?; // if we pass this line, valid block
            tracing::info!("Valid block added to chain.");
            if let Some(ref pool) = self.miner_pool{
                // signal to stop trying to mine the current block
                let _ = pool.mine_abort_sender.send(block.header.depth);
            }
            // initialize broadcast
            self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap(); // forward
            // the stamping process is done. do, if there is a miner address, then stamping is done on this block.
            // update the reputation tracker with the block to rwared the miner
            self.maybe_create_history(block.header.miner_address.unwrap()).await;
            let mut reputations = self.inner.reputations.lock().await;
            let miner_history = reputations.get_mut(&block.header.miner_address.unwrap()).unwrap();
            miner_history.settle_head(block.header); // SETTLE TO REWARD MINING
            // REWARD STAMPERS
            // 3. reward reputation for the new stamps
            drop(reputations);
            for stamp in block.header.tail.iter_stamps() {
                self.maybe_create_history(stamp.address).await;
            }
            let mut reputations = self.inner.reputations.lock().await;
            for stamp in block.header.tail.iter_stamps() {
                let history = reputations.get_mut(&stamp.address).unwrap();
                history.settle_tail(&block.header.tail, block.header); // SETTLE TO REWARD
            }
            tracing::info!("Reputations recorded for new block.");
            return Ok(());
        }
        // ================================================
        // stamp and transmit
        tracing::info!("Block is not mined, stamping and transmitting.");
        let mut n_stamps = block.header.tail.n_stamps();
        let has_our_stamp = block.header.tail.stamps.iter().any(|stamp| stamp.address == self.inner.public_key);
        let already_broadcasted = self.inner.broadcasted_already.lock().await.contains(
            &Message::BlockTransmission(block
                .clone())
                .hash(&mut DefaultHash::new())
                .unwrap()
        );
        tracing::debug!("Block has {} stamps, our stamp: {}, already broadcasted: {}", n_stamps, has_our_stamp, already_broadcasted);

        if n_stamps < N_TRANSMISSION_SIGNATURES && !has_our_stamp {
            assert!(!already_broadcasted, "Block already broadcasted, but not stamped by us. This should not happen.");
            tracing::info!("Stamping block with our signature.");
            let signature = self.signature_for(&block.header)?;
            let stamp = Stamp {
                address: self.inner.public_key,
                signature,
            };
            let _ = block.header.tail.stamp(stamp); // if failure, then full - doesnt matter anyways
            n_stamps += 1; // we have stamped the block
        }

        if (already_broadcasted || n_stamps == N_TRANSMISSION_SIGNATURES) && self.miner_pool.is_some(){
            // add the block to the pool
            tracing::info!("Adding block to miner pool.");
            self.miner_pool.as_ref().unwrap().add_ready_block(block.clone()).await;
        }

        tracing::debug!("starting broadcast");
        self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap(); // forward

        Ok(())
    }

    /// spawn a new thread to match transaction callback requests against the bock
    #[instrument(name = "Node::handle_callbacks", skip(self, block), fields(
        block_hash = ?block.hash
    ))]
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

    /// ed25519 signature for the node over the hash of some data
    fn signature_for<T: Signable<64>>(&self, sign: &T) -> Result<[u8; 64], std::io::Error> {
        // sign the data with the private key
        let mut signer = DefaultSigner::new(self.inner.private_key);
        let signature = signer.sign(sign);
        // return the signature
        Ok(signature)
    }

    pub async fn maybe_update_peer(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers: tokio::sync::MutexGuard<'_, HashMap<StdByteArray, Peer>> = self.inner.peers.lock().await;
        if (peer.public_key != self.inner.public_key) && !peers.iter().any(|(public_key, _)| public_key == &peer.public_key) {
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
    async fn maybe_create_history(&self, public_key: StdByteArray) {
        // get the history of the node
        let mut reputations = self.inner.reputations.lock().await;
        if reputations.get(&public_key).is_none() {
            // create a new history
            reputations.insert(public_key, NodeHistory::new(public_key, vec![], vec![], 0)); // create new
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
    /// Broadcast a message to all peers
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error>;
}

impl Broadcaster for Node {
    #[instrument(name = "Node::broadcast", skip(self, message), fields(
        public_key = ?self.inner.public_key,
        message = ?message.type_id()
    ))]
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error> {
        // send a message to all peers
        let mut responses = Vec::new();
        let mut peers = self.inner.peers.lock().await.clone(); // do not hold lock
        for (_, peer) in peers.iter_mut(){
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

#[cfg(test)]
mod tests{

    use chrono::Local;
    use tracing::{level_filters::LevelFilter, Level};
    use tracing_appender::rolling;
    use tracing_subscriber::{fmt::{self, writer::BoxMakeWriter}, layer::SubscriberExt, util::SubscriberInitExt, Layer, Registry};
    use tracing_tree::HierarchicalLayer;

    use crate::{crypto::{hashing::{DefaultHash, HashFunction, Hashable}, signing::{DefaultSigner, SigFunction, SigVerFunction, Signable}}, nodes::{messages::Message, miner::{Miner, MAX_TRANSACTION_WAIT_TIME}, node::StdByteArray, peer::Peer}, persistence::database::{Datastore, EmptyDatastore, GenesisDatastore}, primitives::{pool::MinerPool, transaction::Transaction}, protocol::{peers::discover_peers, transactions::submit_transaction}};

    use super::Node;
    
    use std::{fs::File, net::{IpAddr, Ipv4Addr}, sync::Arc};
    
    // always setup tracing first
    #[ctor::ctor]
    fn setup() {
        // === Setup folder structure under ./test_output/{timestamp} ===
        let timestamp = Local::now().format("%d_%H-%M-%S").to_string();
        let log_dir = format!("./test_output/{}", timestamp);
        std::fs::create_dir_all(&log_dir).expect("failed to create log directory");
    
        let filename = format!("{log_dir}/output.log");
        let file = File::create(filename).expect("failed to create log file");
    
        // === Console output for WARN and above ===
        let console_layer = fmt::layer()
            .with_ansi(true)
            .with_level(true)
            .with_filter(LevelFilter::ERROR);

        let file_layer = fmt::layer()
            .with_writer(BoxMakeWriter::new(file))
            .with_ansi(false)
            .with_level(true)
            .with_filter(LevelFilter::DEBUG);

        // === Combined subscriber ===
        Registry::default()
            .with(file_layer)
            .with(console_layer)
            .try_init();
    }

    #[tokio::test]
    async fn test_create_empty_node() {
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8080;
        let peers = vec![];
        let chain = None;
        let transaction_pool = None;

        let node = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            chain,
            transaction_pool,
        );

        assert_eq!(node.inner.public_key, public_key);
        assert_eq!(node.inner.private_key, private_key);
        assert_eq!(node.ip_address, ip_address);
        assert_eq!(node.port, port);
        assert!(node.inner.peers.lock().await.is_empty());
        assert!(node.inner.chain.lock().await.is_none());
        assert!(node.miner_pool.is_none());
        assert!(node.inner.transaction_filters.lock().await.is_empty());
        assert!(node.inner.reputations.lock().await.is_empty());
        assert!(node.inner.filter_callbacks.lock().await.is_empty());
        assert!(node.inner.state.lock().await.clone() == super::NodeState::ICD);
    }

    #[tokio::test]
    async fn test_create_empty_node_genisis() {
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8080;
        let peers = vec![];
        let datastore = Some(Arc::new(GenesisDatastore::new()));
        let transaction_pool = None;

        let node = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            datastore.map(|ds| ds as Arc<dyn Datastore>),
            transaction_pool,
        );

        assert_eq!(node.inner.public_key, public_key);
        assert_eq!(node.inner.private_key, private_key);
        assert_eq!(node.ip_address, ip_address);
        assert_eq!(node.port, port);
        assert!(node.inner.peers.lock().await.is_empty());
        assert!(node.inner.chain.lock().await.is_some());
        assert!(node.miner_pool.is_none());
        assert!(node.inner.transaction_filters.lock().await.is_empty());
        assert!(node.inner.reputations.lock().await.is_empty());
        assert!(node.inner.filter_callbacks.lock().await.is_empty());
        assert!(node.inner.state.lock().await.clone() == super::NodeState::ChainOutdated);
    }



    #[tokio::test]
    async fn test_serve(){
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8070;
        let peers = vec![];
        let datastore = GenesisDatastore::new();
        let transaction_pool = None;

        let mut node = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            Some(Arc::new(datastore)),
            transaction_pool,
        );

        node.serve().await;

        // check state update
        let state = node.inner.state.lock().await;
        assert!(*state == super::NodeState::ChainSyncing);
        drop(state);

        tokio::time::sleep(std::time::Duration::from_secs(1)).await; // wait for the node to start
        // check if the node is in serving state
        let state = node.inner.state.lock().await;
        assert!(*state == super::NodeState::Serving);

    }

    #[tokio::test]
    async fn test_two_node_chat(){
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8069;
        let peers = vec![
            Peer::new(
                [8u8; 32],
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                8081,
            ),
        ];
        let datastore = GenesisDatastore::new();
        let transaction_pool = None;

        let mut node1 = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            Some(Arc::new(datastore.clone())),
            transaction_pool,
        );

        node1.serve().await;

        // check state update
        let state = node1.inner.state.lock().await;
        assert!(*state == super::NodeState::ChainSyncing);
        drop(state);

        // create a second node
        let public_key2 = [2u8; 32];
        let private_key2 = [3u8; 32];
        let ip_address2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port2 = 8070;
        
        let mut node2 = Node::new(
            public_key2,
            private_key2,
            ip_address2,
            port2,
            vec![node1.clone().into()],
            Some(Arc::new(datastore)),
            None,
        );

        node2.serve().await;

        // check state update
        let state = node2.inner.state.lock().await;
        assert!(*state == super::NodeState::ChainSyncing);
        drop(state);

        // attemp to make node 2 work alongside node 1
        tokio::time::sleep(std::time::Duration::from_secs(3)).await; // wait for the node to start
        // check if the node is in serving state
        let state = node1.inner.state.lock().await.clone();
        assert!(state == super::NodeState::Serving);
        let state = node2.inner.state.lock().await.clone();
        assert!(state == super::NodeState::Serving);

        // check if the nodes can communicate
        let result = discover_peers(&mut node2).await;
        result.unwrap(); // should be successful
        // make sure that node2 is in the list of peers for node 1
        let peers = node1.inner.peers.lock().await;
        assert!(peers.contains_key(&public_key2));
        drop(peers);

        // make sure the [9u8; 32] is in the list of peers for node 2
        let peers = node2.inner.peers.lock().await;
        assert!(peers.contains_key(&public_key));
        assert!(peers.contains_key(&[8u8; 32])); // the peer we added
        drop(peers);
        assert!(node2.inner.chain.lock().await.as_ref().unwrap().blocks.len() == 1);
    }

    #[tokio::test]
    async fn test_complex_peer_discovery(){
        let public_key_a = [0u8; 32];
        let private_key_a = [1u8; 32];
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port_a = 8060;

        let public_key_b = [2u8; 32];
        let private_key_b = [3u8; 32];
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port_b = 8061;

        let public_key_c = [4u8; 32];
        let private_key_c = [5u8; 32];
        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_c = 8062;

        let public_key_d = [6u8; 32];
        let private_key_d = [7u8; 32];
        let ip_address_d = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_d = 8063;

        let datastore = GenesisDatastore::new();

        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let mut node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![Peer::new(public_key_c, ip_address_c, port_c)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let mut node_c = Node::new(
            public_key_c,
            private_key_c,
            ip_address_c,
            port_c,
            vec![Peer::new(public_key_d, ip_address_d, port_d)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let mut node_d = Node::new(
            public_key_d,
            private_key_d,
            ip_address_d,
            port_d,
            vec![Peer::new(public_key_a, ip_address_a, port_a)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        node_d.serve().await; // no discovery
        node_c.serve().await; // D learns of C
        node_b.serve().await; // C learns of B
        node_a.serve().await; // B learns of A

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;        
        discover_peers(&mut node_a).await.unwrap(); // 
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(peers_a.contains_key(&public_key_c));
        assert!(!peers_a.contains_key(&public_key_d)); // node A does not know about D yet
        drop(peers_a);

        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));
        assert!(peers_b.contains_key(&public_key_c));
        assert!(!peers_b.contains_key(&public_key_d));
        drop(peers_b);

        let peers_c = node_c.inner.peers.lock().await;
        assert!(!peers_c.contains_key(&public_key_a));
        assert!(peers_c.contains_key(&public_key_b));
        assert!(peers_c.contains_key(&public_key_d));
        drop(peers_c);

        let peers_d = node_d.inner.peers.lock().await;
        assert!(peers_d.contains_key(&public_key_a));
        assert!(!peers_d.contains_key(&public_key_b));
        assert!(peers_d.contains_key(&public_key_c));
        drop(peers_d);

        // D does not know of B.
        // A does not know of D.
        // C does not know of A.
        // B does not know of D.

        discover_peers(&mut node_d).await.unwrap(); // 
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(peers_a.contains_key(&public_key_c));
        assert!(!peers_a.contains_key(&public_key_a));
        assert!(peers_a.contains_key(&public_key_d)); // node A does not know about D yet
        drop(peers_a);

        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));
        assert!(peers_b.contains_key(&public_key_c));
        assert!(!peers_b.contains_key(&public_key_d));
        drop(peers_b);

        let peers_c = node_c.inner.peers.lock().await;
        assert!(!peers_c.contains_key(&public_key_a));
        assert!(peers_c.contains_key(&public_key_b));
        assert!(peers_c.contains_key(&public_key_d));
        drop(peers_c);

        let peers_d = node_d.inner.peers.lock().await;
        assert!(peers_d.contains_key(&public_key_a));
        assert!(peers_d.contains_key(&public_key_b));
        assert!(peers_d.contains_key(&public_key_c));
        drop(peers_d);
        
    }

    #[tokio::test]
    async fn test_start_account(){
        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();

        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port_a = 8020;

        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port_b = 8021;

        let datastore = GenesisDatastore::new();

        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let mut node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            None,
        );

        node_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        
        // Node A sends a peer request to Node B
        discover_peers(&mut node_a).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify that Node B knows Node A
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));

        drop(peers_b);

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(!peers_a.contains_key(&public_key_a)); // Node A does not know itself
        drop(peers_a);
        // now, A makes a transaction of 0 dollars to B

        let mut chain = node_a.inner.chain.lock().await;
        let sender_account = chain.as_mut().unwrap().account_manager.get_or_create_account(&public_key_a);
        drop(chain);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let result = submit_transaction(
            &mut node_a, 
            &mut sender_account.lock().unwrap(), 
            &mut signing_a, 
            public_key_b, 
            0, 
            false,
            Some(timestamp)
        ).await.unwrap();

        assert!(result.is_none());

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        // node a already broadcasted
        
        let mut transaction = Transaction::new(
            sender_account.lock().unwrap().address, 
            public_key_b, 
            0, 
            timestamp,
            0, 
            &mut DefaultHash::new()
        );
        transaction.sign(&mut signing_a);

        let message_to_hash = Message::TransactionBroadcast(transaction);

        // also, node b should be forward by now
        assert!(node_b.inner.state.lock().await.is_forward());

        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));
        drop(already_b);
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));

        // at this point, everything is broadcasted - and recorded. this means we had no loop, we are happy.

        // make sure the accounts do NOT exist for the receiver - because it has never been settled.
        let chain_a = node_a.inner.chain.lock().await;
        let account_a = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_a).unwrap();
        let account_b = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_b);
        assert!(account_b.is_none());
        assert_eq!(account_a.lock().unwrap().balance, 0); // because we made it
        drop(chain_a);
        let chain_b = node_b.inner.chain.lock().await;
        let account_b = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_b);
        let account_a = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_a);
        assert!(account_b.is_none());
        assert!(account_a.is_none());  // on node b, it has no record of either yet as the transactions are not in blocks
        drop(chain_b);

    }

    async fn inner_test_transaction_and_block_proposal(
        node_a: &mut Node,
        node_b: &Node,
        signing_a: &mut DefaultSigner,
        public_key_b: StdByteArray,
        public_key_a: StdByteArray
    ){
        let mut miner_b = Miner::new(node_b.clone()).unwrap();

        miner_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Node A sends a peer request to Node B
        println!("Node A starting discovery");

        discover_peers(node_a).await.unwrap();
        println!("Node A discovered Node B");

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify that Node B knows Node A
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));

        drop(peers_b);

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(!peers_a.contains_key(&public_key_a)); // Node A does not know itself
        drop(peers_a);
        // now, A makes a transaction of 0 dollars to B


        let mut chain = node_a.inner.chain.lock().await;
        let sender_account = chain.as_mut().unwrap().account_manager.get_or_create_account(&public_key_a);
        drop(chain);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let result = submit_transaction(
            node_a, 
            &mut sender_account.lock().unwrap(), 
            signing_a, 
            public_key_b, 
            0, 
            false,
            Some(timestamp)
        ).await.unwrap();

        assert!(result.is_none());

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        // node a already broadcasted

        let mut transaction = Transaction::new(
            sender_account.lock().unwrap().address, 
            public_key_b, 
            0, 
            timestamp,
            0, 
            &mut DefaultHash::new()
        );
        transaction.sign(signing_a);

        let message_to_hash = Message::TransactionBroadcast(transaction);

        // also, node b should be forward by now

        assert!(node_b.inner.state.lock().await.is_forward());

        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));
        assert_eq!(already_b.len(), 1);
        drop(already_b);
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));
        assert_eq!(already_b.len(), 1);
        drop(already_b);

        // at this point, everything is broadcasted - and recorded. this means we had no loop, we are happy.

        // make sure the accounts do NOT exist for the receiver - because it has never been settled.

        let chain_a = node_a.inner.chain.lock().await;
        let account_a = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_a).unwrap();
        let account_b = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_b);
        assert!(account_b.is_none());
        assert_eq!(account_a.lock().unwrap().balance, 0); // because we made it
        drop(chain_a);
        let chain_b = node_b.inner.chain.lock().await;
        let account_b = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_b);
        let account_a = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_a);
        assert!(account_b.is_none());
        assert!(account_a.is_none());  // on node b, it has no record of either yet as the transactions are not in blocks
        drop(chain_b);

        // miners wait 10 seconds for more transactions to come in
        println!("waiting for miner to process transactions");
        tokio::time::sleep(std::time::Duration::from_secs(MAX_TRANSACTION_WAIT_TIME)).await;
        // now, by this time it should have been consumed into a block proposition.
        // this block proposition will be sent out from node_b to node_a.
        // we will check the already broadcasted messages first.
        // we cannot know the exact hash, but check that 2 are in there
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert_eq!(already_b.len(), 4); // at least the transaction and the block. also the mined block
        drop(already_b);
        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert_eq!(already_b.len(), 4); // at least the transaction and the block
        drop(already_b);

        // assert the block is in the chain of node_b
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 1); // 2 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 2); // 2 blocks
        drop(chain_b);
        // assert the block is in the chain of node_a
        let chain_a = node_a.inner.chain.lock().await;
        assert!(chain_a.as_ref().unwrap().depth == 1); // 2 blocks
        assert!(chain_a.as_ref().unwrap().blocks.len() == 2); // 2 blocks
        drop(chain_a);
        // check reputations.

        let reputations_b = node_b.inner.reputations.lock().await;
        assert!(reputations_b.contains_key(&public_key_a));
        assert!(reputations_b.contains_key(&public_key_b));
        let history_a = reputations_b.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 1); // 0 blocks stamped
        let b_a = history_a.compute_reputation();

        let history_b = reputations_b.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 1); // 1 block mined
        assert_eq!(history_b.blocks_stamped.len(), 1); // 0 blocks stamped
        let b_b = history_b.compute_reputation();

        drop(reputations_b);

        let reputations_a = node_a.inner.reputations.lock().await;
        assert!(reputations_a.contains_key(&public_key_a));
        assert!(reputations_a.contains_key(&public_key_b));
        let history_a = reputations_a.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 1); // 0 blocks stamped
        let a_a = history_a.compute_reputation();
        let history_b = reputations_a.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 1); // 1 block mined
        assert_eq!(history_b.blocks_stamped.len(), 1); // 0 blocks stamped
        let a_b = history_b.compute_reputation();
        drop(reputations_a);


        assert!(a_a == b_a);
        assert!(a_b == b_b);
        assert!(a_b > a_a);

        // check miner got paid
        let chain_b = node_b.inner.chain.lock().await;
        let account_b = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_b).unwrap();
        assert!(account_b.lock().unwrap().balance > 0); // miner got paid
        let balance_b = account_b.lock().unwrap().balance;
        drop(chain_b);
        let chain_a = node_a.inner.chain.lock().await;
        let account_b = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_b).unwrap();
        assert!(account_b.lock().unwrap().balance > 0); // miner got paid
        assert_eq!(account_b.lock().unwrap().balance, balance_b); // balance is the same on both nodes
        drop(chain_a);

    }

    #[tokio::test]
    async fn test_transaction_and_block_proposal(){

        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();

        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_a = 8020;

        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_b = 8021;

        let datastore = GenesisDatastore::new();

        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );

        inner_test_transaction_and_block_proposal(
            &mut node_a,
            &node_b,
            &mut signing_a,
            public_key_b,
            public_key_a
        ).await;

    }


    #[tokio::test]
    async fn test_double_transaction(){

        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_a = 8020;
        let mut signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_b = 8021;
        let datastore = GenesisDatastore::new();
        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );
        let mut node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );

        inner_test_transaction_and_block_proposal(
            &mut node_a,
            &node_b,
            &mut signing_a,
            public_key_b,
            public_key_a
        ).await;

        // one transaction has passed, now we will try to make another one
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let account = node_b.inner.chain.lock().await.as_mut().unwrap().account_manager.get_account(&public_key_b).unwrap();
        let result = submit_transaction(
            &mut node_b, 
            &mut account.lock().unwrap(), 
            &mut signing_b,
            public_key_a,
            0,
            false, 
            Some(timestamp)
        ).await.unwrap();
        assert!(result.is_none());
        // at this point, there should be 8 total broadcasts each - after a pause
        tokio::time::sleep(std::time::Duration::from_secs(MAX_TRANSACTION_WAIT_TIME+5)).await;
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert!(already_b.len() == 8);
        drop(already_b);
        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert!(already_b.len() == 8);
        drop(already_b);
        
        // now, make sure the chains are length 2
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_b);
        // assert the block is in the chain of node_a
        let chain_a = node_a.inner.chain.lock().await;
        assert!(chain_a.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_a.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_a);
        // check reputations.
        let reputations_b = node_b.inner.reputations.lock().await;
        assert!(reputations_b.contains_key(&public_key_a));
        assert!(reputations_b.contains_key(&public_key_b));
        let history_a = reputations_b.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 2); // 2 blocks stamped
        let b_a = history_a.compute_reputation();
        let history_b = reputations_b.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 2); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 2); // 2 blocks stamped
        let b_b = history_b.compute_reputation();
        drop(reputations_b);
        let reputations_a = node_a.inner.reputations.lock().await;
        assert!(reputations_a.contains_key(&public_key_a));
        assert!(reputations_a.contains_key(&public_key_b));
        let history_a = reputations_a.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 2); // 2 blocks stamped
        let a_a = history_a.compute_reputation();
        let history_b = reputations_a.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 2); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 2); // 2 blocks stamped
        let a_b = history_b.compute_reputation();
        drop(reputations_a);
        assert!(a_a == b_a);
        assert!(a_b == b_b);
        assert!(a_b > a_a); // B has more reputation than A
    }

    /// Launches three nodes - matching the third one in delay
    /// There are two blocks - genesis + 1.
    /// They are synced.
    async fn inner_test_new_node_online(
        node_a: &mut Node,
        node_b: &mut Node,
        node_c: &mut Node,
        signing_a: &mut DefaultSigner,
        public_key_b: StdByteArray,
        public_key_a: StdByteArray,
        public_key_c: StdByteArray
    ) {

        inner_test_transaction_and_block_proposal(
            node_a,
            &node_b,
            signing_a,
            public_key_b,
            public_key_a
        ).await;

        // node c serves
        assert!(node_c.inner.state.lock().await.clone() == super::NodeState::ICD);
        node_c.serve().await;
        // now, the node should launch chain discovery
        assert!(node_c.inner.state.lock().await.clone() == super::NodeState::ChainLoading);
        // wait for a bit
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // check if node c knows about node a and b
        let peers_c = node_c.inner.peers.lock().await;
        assert!(peers_c.contains_key(&public_key_a));
        assert!(peers_c.contains_key(&public_key_b));
        assert!(!peers_c.contains_key(&public_key_c)); // node c does not know about itself
        drop(peers_c);
        // make sure chain is length 3
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 1); // 2 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 2); // 3 blocks
        let chain_a = node_a.inner.chain.lock().await;
        // check hash equalities
        let mut ablocks = chain_a.as_ref().unwrap().blocks.values().collect::<Vec<_>>();
        ablocks.sort_by_key(|b| b.header.depth);
        let mut cblocks = chain_c.as_ref().unwrap().blocks.values().collect::<Vec<_>>();
        cblocks.sort_by_key(|b| b.header.depth);

        assert_eq!(
            ablocks[0].hash.unwrap(), 
            cblocks[0].hash.unwrap(), 
        );
        assert_eq!(
            ablocks[1].hash.unwrap(), 
            cblocks[1].hash.unwrap(), 
        );        
        drop(chain_c);
        drop(chain_a);
        assert!(node_c.inner.state.lock().await.clone() == super::NodeState::Serving);
        // make sure reputations are correct
        let reputations_c = node_c.inner.reputations.lock().await;
        assert!(reputations_c.contains_key(&public_key_a));
        assert!(reputations_c.contains_key(&public_key_b));
        let history_a = reputations_c.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 1); // 2 blocks stamped
        let c_a = history_a.compute_reputation();
        let history_b = reputations_c.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 1); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 1); // 2 blocks stamped
        let c_b = history_b.compute_reputation();
        drop(reputations_c);
        let reputations_a = node_a.inner.reputations.lock().await;
        assert!(reputations_a.contains_key(&public_key_a));
        assert!(reputations_a.contains_key(&public_key_b));
        let history_a = reputations_a.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 1); // 2 blocks stamped
        let a_a = history_a.compute_reputation();
        let history_b = reputations_a.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 1); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 1); // 2 blocks stamped
        let a_b = history_b.compute_reputation();
        drop(reputations_a);
        let reputations_b = node_b.inner.reputations.lock().await;
        assert!(reputations_b.contains_key(&public_key_a));
        assert!(reputations_b.contains_key(&public_key_b));
        let history_a = reputations_b.get(&public_key_a).unwrap();
        assert_eq!(history_a.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_a.blocks_stamped.len(), 1); // 2 blocks stamped
        let b_a = history_a.compute_reputation();
        let history_b = reputations_b.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 1); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 1); // 2 blocks stamped
        let b_b = history_b.compute_reputation();
        drop(reputations_b);
        assert!(a_a == c_a);
        assert!(a_b == c_b);
        assert!(b_a == c_a);
        assert!(b_b == c_b);
        assert!(c_b > c_a); // C has more reputation than A

    }

    #[tokio::test]
    async fn test_new_node_online(){

        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_a = 8015;
        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_b = 8016;
        let datastore = GenesisDatastore::new();
        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );
        let mut node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );
        // new node - node c
        let signing_c = DefaultSigner::generate_random();
        let public_key_c = signing_c.get_verifying_function().to_bytes();
        let private_key_c = signing_c.to_bytes();
        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5));
        let port_c = 8022;
        let mut node_c = Node::new(
            public_key_c,
            private_key_c,
            ip_address_c,
            port_c,
            vec![Peer::new(public_key_a, ip_address_a, port_a)],
            Some(Arc::new(EmptyDatastore::new())), // empty datastore, needs ICD
            None,
        );

        inner_test_new_node_online(
            &mut node_a,
            &mut node_b,
            &mut node_c,
            &mut signing_a,
            public_key_b,
            public_key_a,
            public_key_c
        ).await;
        
    }

    #[tokio::test]
    async fn test_sync_later(){
        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_a = 8008;
        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_b = 8009;
        let datastore = GenesisDatastore::new();
        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );
        let mut node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );
        // node c
        let mut signing_c = DefaultSigner::generate_random();
        let public_key_c = signing_c.get_verifying_function().to_bytes();
        let private_key_c = signing_c.to_bytes();
        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5));
        let port_c = 8010;
        let mut node_c = Node::new(
            public_key_c,
            private_key_c,
            ip_address_c,
            port_c,
            vec![Peer::new(public_key_a, ip_address_a, port_a)],
            Some(Arc::new(EmptyDatastore::new())), // empty datastore, needs ICD
            None,
        );

        inner_test_new_node_online(
            &mut node_a,
            &mut node_b,
            &mut node_c,
            &mut signing_a,
            public_key_b,
            public_key_a,
            public_key_c
        ).await;
        // now, let A go offline. C will submit a transaction.
        println!("Stopping node A - expect ConnetionRefused errors");
        node_a.stop().await;
        // pause a sec - TODO remove this after fixing the join on stoping
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        // now, C will submit a transaction to B
        let acc = node_c.inner.chain.lock().await.as_mut().unwrap().account_manager.get_or_create_account(&public_key_c);
        let _ = submit_transaction(
            &mut node_c, 
            &mut acc.lock().unwrap(), 
            &mut signing_c,
            public_key_b,
            0,
            false, 
            None
        ).await.unwrap();
        // pause a sec
        tokio::time::sleep(std::time::Duration::from_secs(15)).await; // time to settle, and give up waiting for full block
        // now both shold have 3 blocks
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_b);
        // now c 
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_c);
        // put A back online
        println!("Restarting node A");
        node_a.serve().await;
        assert!(node_a.inner.state.lock().await.clone() == super::NodeState::ChainSyncing);
        // wait for a bit
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // check if node a knows about node b and c
        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(peers_a.contains_key(&public_key_c));
        drop(peers_a);
        // now check that it collected the block
        let chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 2); // 3 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 3); // 3 blocks
        // double check reputations
        let reputations_a = node_a.inner.reputations.lock().await;
        assert!(reputations_a.contains_key(&public_key_a));
        assert!(reputations_a.contains_key(&public_key_b));
        assert!(reputations_a.contains_key(&public_key_c));
        let history_c = reputations_a.get(&public_key_c).unwrap();
        assert_eq!(history_c.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_c.blocks_stamped.len(), 1); // 2 blocks stamped
        let a_c = history_c.compute_reputation();
        let history_b = reputations_a.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 2); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 2); // 2 blocks stamped
        let a_b = history_b.compute_reputation();
        // now check against C
        let reputations_c = node_c.inner.reputations.lock().await;
        assert!(reputations_c.contains_key(&public_key_a));
        assert!(reputations_c.contains_key(&public_key_b));
        assert!(reputations_c.contains_key(&public_key_c));
        let history_c = reputations_c.get(&public_key_c).unwrap();
        assert_eq!(history_c.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_c.blocks_stamped.len(), 1); // 2 blocks stamped
        let c_c = history_c.compute_reputation();
        let history_b = reputations_c.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 2); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 2); // 2 blocks stamped
        let c_b = history_b.compute_reputation();
        drop(reputations_c);
        assert!(a_c == c_c);
        assert!(a_b == c_b);
        assert!(c_b > c_c); // C has more reputation than A
        // now, node A should be in serving state
        assert!(node_a.inner.state.lock().await.clone() == super::NodeState::Serving);
    }
    
    #[tokio::test]
    async fn test_sync_two_blocks(){
        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_a = 7999;
        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_b = 7998;
        let datastore = GenesisDatastore::new();
        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );
        let mut node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );
        // node c
        let mut signing_c = DefaultSigner::generate_random();
        let public_key_c = signing_c.get_verifying_function().to_bytes();
        let private_key_c = signing_c.to_bytes();
        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5));
        let port_c = 7997;
        let mut node_c = Node::new(
            public_key_c,
            private_key_c,
            ip_address_c,
            port_c,
            vec![Peer::new(public_key_a, ip_address_a, port_a)],
            Some(Arc::new(EmptyDatastore::new())), // empty datastore, needs ICD
            None,
        );

        inner_test_new_node_online(
            &mut node_a,
            &mut node_b,
            &mut node_c,
            &mut signing_a,
            public_key_b,
            public_key_a,
            public_key_c
        ).await;
        // now, let A go offline. C will submit a transaction.
        println!("Stopping node A - expect ConnetionRefused errors");
        node_a.stop().await;
        // pause a sec - TODO remove this after fixing the join on stoping
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        // now, C will submit a transaction to B
        let acc = node_c.inner.chain.lock().await.as_mut().unwrap().account_manager.get_or_create_account(&public_key_c);
        let _ = submit_transaction(
            &mut node_c, 
            &mut acc.lock().unwrap(), 
            &mut signing_c,
            public_key_b,
            0,
            false, 
            None
        ).await.unwrap();
        // pause a sec
        tokio::time::sleep(std::time::Duration::from_secs(15)).await; // time to settle, and give up waiting for full block
        // now both shold have 3 blocks
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_b);
        // now c 
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_c);
        // DO A SECOND TRANSACTION TO GET 2 BLOCKS OUTDATED
        let _ = submit_transaction(
            &mut node_c, 
            &mut acc.lock().unwrap(), 
            &mut signing_c,
            public_key_b,
            0,
            false, 
            None
        ).await.unwrap();
        // pause a sec
        tokio::time::sleep(std::time::Duration::from_secs(15)).await; // time to settle, and give up waiting for full block
        // now both shold have 4 blocks
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 3); // 4 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 4); // 4 blocks
        drop(chain_b);
        // now c
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 3); // 4 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 4); // 4 blocks
        drop(chain_c);
        // put A back online
        println!("Restarting node A");
        node_a.serve().await;
        assert!(node_a.inner.state.lock().await.clone() == super::NodeState::ChainSyncing);
        // wait for a bit
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // check if node a knows about node b and c
        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(peers_a.contains_key(&public_key_c));
        drop(peers_a);
        // now check that it collected the block
        let chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 3); // 3 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 4); // 3 blocks
        // double check reputations
        let reputations_a = node_a.inner.reputations.lock().await;
        assert!(reputations_a.contains_key(&public_key_a));
        assert!(reputations_a.contains_key(&public_key_b));
        assert!(reputations_a.contains_key(&public_key_c));
        let history_c = reputations_a.get(&public_key_c).unwrap();
        assert_eq!(history_c.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_c.blocks_stamped.len(), 2); // 2 blocks stamped
        let a_c = history_c.compute_reputation();
        let history_b = reputations_a.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 3); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 3); // 2 blocks stamped
        let a_b = history_b.compute_reputation();
        // now check against C
        let reputations_c = node_c.inner.reputations.lock().await;
        assert!(reputations_c.contains_key(&public_key_a));
        assert!(reputations_c.contains_key(&public_key_b));
        assert!(reputations_c.contains_key(&public_key_c));
        let history_c = reputations_c.get(&public_key_c).unwrap();
        assert_eq!(history_c.blocks_mined.len(), 0); // 0 blocks mined
        assert_eq!(history_c.blocks_stamped.len(), 2); // 2 blocks stamped
        let c_c = history_c.compute_reputation();
        let history_b = reputations_c.get(&public_key_b).unwrap();
        assert_eq!(history_b.blocks_mined.len(), 3); // 2 blocks mined
        assert_eq!(history_b.blocks_stamped.len(), 3); // 2 blocks stamped
        let c_b = history_b.compute_reputation();
        drop(reputations_c);
        assert!(a_c == c_c);
        assert!(a_b == c_b);
        assert!(c_b > c_c); // C has more reputation than A
        // now, node A should be in serving state
        assert!(node_a.inner.state.lock().await.clone() == super::NodeState::Serving);
    }

    #[tokio::test]
    async fn test_multiple_transaction_one_block(){
        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_a = 7997;
        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_b = 7996;
        let datastore = GenesisDatastore::new();
        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );
        let node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );

        let mut miner_b = Miner::new(node_b.clone()).unwrap();

        miner_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await; // wait for the nodes to connect
        // check states
        assert!(node_a.inner.state.lock().await.clone() == super::NodeState::Serving);
        assert!(node_b.inner.state.lock().await.clone() == super::NodeState::Serving);
        let acc = node_a.inner.chain.lock().await.as_mut().unwrap().account_manager.get_or_create_account(&public_key_a);
        println!("Submitting multiple transactions from A to B");
        submit_transaction(
            &mut node_a, 
            &mut acc.lock().unwrap(),
            &mut signing_a,
            public_key_b,
            0,
            false,
            None
        ).await.unwrap(); 
        println!("Submitting multiple transactions from A to B");
        submit_transaction(
            &mut node_a, 
            &mut acc.lock().unwrap(),
            &mut signing_a,
            public_key_b,
            0,
            false,
            None
        ).await.unwrap();
        drop(acc);
        println!("waiting on transactions to be processed");
        tokio::time::sleep(std::time::Duration::from_secs(2*MAX_TRANSACTION_WAIT_TIME)).await; // wait for the transactions to be processed
        println!("transactions processed, checking the chain");
        // now, lets check that a is in bs peer list
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));
        drop(peers_b);
        // now, grab the chains - check depth
        let chain_b = miner_b.node.inner.chain.lock().await;
        assert_eq!(chain_b.as_ref().unwrap().depth, 1); // 2 blocks
        assert_eq!(chain_b.as_ref().unwrap().blocks.len(), 2); // 3 blocks
        // check that the leaf has 2 transactions
        let chain_b = chain_b.as_ref().unwrap();
        let leaf = chain_b.blocks.get(&chain_b.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 2); // 2 transactions
        // now, check the chain of a
        let chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 1); // 3 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 2); // 3 blocks
        // check that the leaf has 2 transactions
        let chain_a = chain_a.as_ref().unwrap();
        let leaf = chain_a.blocks.get(&chain_a.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 2); // 2 transactions

    }
}