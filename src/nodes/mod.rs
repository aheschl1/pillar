use crate::blockchain::chain::Chain;

pub mod miner;


struct Peer{
    /// The public key of the peer
    pub public_key: [u8; 32],
    /// The IP address of the peer
    pub ip_address: String,
    /// The port of the peer
    pub port: u16,
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
}

#[cfg(test)]
mod test{
    use crate::{blockchain::chain::Chain, crypto::hashing::{HashFunction, Sha3_256Hash}, primitives::{block::Block, transaction::Transaction}};
    use crate::nodes::miner::Miner;
    use super::Node;

    #[test]
    fn test_miner(){
        let public_key = [1u8; 32];
        let private_key = [2u8; 32];
        let ip_address = "127.0.0.1".to_string();
        let port = 8080;
        let node = Node::new(public_key, private_key, ip_address, port, vec![], Chain::new_with_genesis());
        let mut hasher = Sha3_256Hash::new();

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