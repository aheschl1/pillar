use std::hash::Hash;
use slotmap::{SlotMap, new_key_type};

use crate::types::StdByteArray;

use super::hashing::{HashFunction, Hashable};

// Define a key type for our nodes
new_key_type! {
    pub struct NodeKey;
}


#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TreeNode {
    pub left: Option<NodeKey>,
    pub right: Option<NodeKey>,
    pub parent: Option<NodeKey>,
    pub hash: StdByteArray,
}

#[derive(Debug, Clone)]
pub struct MerkleTree {
    pub nodes: SlotMap<NodeKey, TreeNode>,
    pub root: Option<NodeKey>,
    pub leaves: Option<Vec<NodeKey>>,
}

impl Hash for MerkleTree{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (key, node) in self.nodes.iter() {
            node.hash(state);
            key.hash(state);
        }
        self.root.hash(state);
        self.leaves.hash(state);
    }
}

impl PartialEq for MerkleTree {
    fn eq(&self, other: &Self) -> bool {
        // compare hash
        self.get_root_hash() == other.get_root_hash()
    }
}

impl Eq for MerkleTree {
    
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleTree {
    pub fn new() -> Self {
        MerkleTree {
            nodes: SlotMap::with_key(),
            root: None,
            leaves: None,
        }
    }

    pub fn get_root_hash(&self) -> Option<StdByteArray>{
        match self.root{
            None => None,
            Some(root) => {
                let root = self.nodes.get(root);
                root.map(|root| root.hash)
            }
        }
    }
}

/// Generate a Merkle tree from the given data
pub fn generate_tree(data: Vec<&impl Hashable>, hash_function: &mut impl HashFunction) -> Result<MerkleTree, std::io::Error> {
    if data.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Data is empty"));
    }

    let mut tree = MerkleTree::new();

    // Create leaves
    let mut leaves: Vec<NodeKey> = data.into_iter().map(|item| {
        let item_hash = item.hash(hash_function).expect("hashing failed");
        hash_function.update(item_hash);
        let node = TreeNode {
            left: None,
            right: None,
            parent: None,
            hash: hash_function.digest().expect("Hashing failed"),
        };
        tree.nodes.insert(node)
    }).collect();
    
    let leaves_clone = leaves.clone();

    // Build up the tree
    while leaves.len() > 1 {
        if leaves.len() % 2 != 0 {
            leaves.push(*leaves.last().unwrap());
        }

        leaves = leaves.chunks(2).map(|pair| {
            let (left_key, right_key) = (pair[0], pair[1]);
            let left_hash = tree.nodes[left_key].hash;
            let right_hash = tree.nodes[right_key].hash;

            hash_function.update(left_hash);
            hash_function.update(right_hash);

            let new_node = TreeNode {
                left: Some(left_key),
                right: Some(right_key),
                parent: None,
                hash: hash_function.digest().expect("Hashing failed"),
            };

            let parent_key = tree.nodes.insert(new_node);
            
            tree.nodes[left_key].parent = Some(parent_key);
            tree.nodes[right_key].parent = Some(parent_key);

            parent_key
        }).collect();
    }

    tree.root = Some(leaves[0]);
    tree.leaves = Some(leaves_clone);

    Ok(tree)
}


#[cfg(test)]
mod tests {
    

    use super::*;
    use crate::proofs::{generate_proof_of_inclusion, verify_proof_of_inclusion};
    use crate::hashing::DefaultHash;

    #[derive(Debug,  Clone, Copy, PartialEq, Eq)]
    pub struct TransactionHeader{
        // sender is the ed25519 public key of the sender
        pub sender: StdByteArray,
        // receiver is the ed25519 public key of the receiver
        pub receiver: StdByteArray,
        // amount is the amount of tokens being transferred
        pub amount: u64,
        // timestamp is the time the transaction was created
        pub timestamp: u64,
        // the nonce is a random number used to prevent replay attacks
        pub nonce: u64
    }

    impl Hashable for TransactionHeader {
        fn hash(&self, hash_function: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error> {
            hash_function.update(self.sender);
            hash_function.update(self.receiver);
            hash_function.update(self.amount.to_le_bytes());
            hash_function.update(self.timestamp.to_le_bytes());
            hash_function.update(self.nonce.to_le_bytes());
            hash_function.digest()
        }
    }

    impl TransactionHeader {
        pub fn new(
            sender: StdByteArray,
            receiver: StdByteArray,
            amount: u64,
            timestamp: u64,
            nonce: u64
        ) -> Self {
            TransactionHeader {
                sender,
                receiver,
                amount,
                timestamp,
                nonce
            }
        }
    }

    impl From<TransactionHeader> for [u8; 32] {
        fn from(val: TransactionHeader) -> Self {
            let mut bytes = [0u8; 32];
            bytes[..].copy_from_slice(&val.hash(&mut DefaultHash::new()).unwrap());
            bytes
        }
    }

    #[test]
    fn test_generate_tree() {
        let mut hash_function = DefaultHash::new();
        let transaction1 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0
        );
        let transaction2 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1
        );
        let transaction3 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2
        );
        let transaction4 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            3
        );

        let data = vec![&transaction1, &transaction2, &transaction3, &transaction4];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        assert!(merkle_tree.root.is_some());
        assert!(merkle_tree.leaves.is_some());

        let mut height = 0;
        let mut current_node = merkle_tree.root;
        while let Some(node) = current_node {
            let node = merkle_tree.nodes.get(node).unwrap();
            if node.left.is_some() {
                height += 1;
                current_node = node.left;
            } else {
                break;
            }
        }
        assert_eq!(height, 2);
        // verify each parent has two children or 0 children
        let mut current_node = merkle_tree.root;
        while let Some(node) = current_node {
            let node = merkle_tree.nodes.get(node).unwrap();
            if node.left.is_some() {
                assert!(node.right.is_some());
                current_node = node.left;
            } else {
                break;
            }
        }

    }

    #[test]
    fn test_generate_proof_of_inclusion() {
        let mut hash_function = DefaultHash::new();
        let transaction1 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0
        );
        let transaction2 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1
        );
        let transaction3 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2
        );
        let transaction4 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            3
        );

        let data = vec![&transaction1, &transaction2, &transaction3, &transaction4];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&merkle_tree, transaction1.hash(&mut DefaultHash::new()).unwrap(), &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
    }

    #[test]
    fn test_verification_of_proof(){
        let mut hash_function = DefaultHash::new();
        let transaction1 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0
        );
        let transaction2 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1
        );
        let transaction3 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2
        );
        let transaction4 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            3
        );

        let data = vec![&transaction1, &transaction2, &transaction3, &transaction4];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&merkle_tree, transaction3.hash(&mut DefaultHash::new()).unwrap(), &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
        // pass
        assert!(verify_proof_of_inclusion(transaction3, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
        // fail
        assert!(!verify_proof_of_inclusion(transaction1, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
    }
    #[test]
    fn test_empty_tree() {
        let mut hash_function = DefaultHash::new();
        let data: Vec<&TransactionHeader> = vec![];
        let merkle_tree = generate_tree(data, &mut hash_function);
        assert!(merkle_tree.is_err());
    }

    #[test]
    fn test_odd_tree() {
        let mut hash_function = DefaultHash::new();
        let transaction1 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0
        );
        let transaction2 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1
        );
        let transaction3 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2
        );

        let data = vec![&transaction1, &transaction2, &transaction3];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        assert!(merkle_tree.root.is_some());
        assert!(merkle_tree.leaves.is_some());
    }

    #[test]
    fn test_odd_tree_proof(){
        let mut hash_function = DefaultHash::new();
        let transaction1 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0
        );
        let transaction2 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1
        );
        let transaction3 = TransactionHeader::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2
        );

        let data = vec![&transaction1, &transaction2, &transaction3];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&merkle_tree, transaction1.hash(&mut DefaultHash::new()).unwrap(), &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
        // test verification
        assert!(verify_proof_of_inclusion(transaction1, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
        // test failure
        assert!(!verify_proof_of_inclusion(transaction2, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
    }
}
