//! Compact Merkle trie used for on-chain state.
//!
//! The trie is keyed by the SHA3-256 hash of the provided key type `K`, split
//! into nibbles (0-15). Values are serialized using `PillarSerialize`. The
//! structure supports multiple roots for branching state (e.g., competing
//! chain tips) while sharing unchanged subtrees.
use std::{collections::{HashMap, HashSet, VecDeque}, fmt::Debug, marker::PhantomData};


use pillar_serialize::{PillarFixedSize, PillarNativeEndian, PillarSerialize};
use slotmap::{new_key_type, KeyData, SlotMap};

use crate::{hashing::{DefaultHash, HashFunction, Hashable}, types::StdByteArray};

new_key_type! { pub struct NodeKey; }

/// In order to store account states, a Merkle Patricia Trie will be used
/// At this moment, it will not be a radix tree for the sake of simplicity
/// It will be a Merkle Trie
/// We will operate over nibbles, with an alphabet of 16 (0-9, a-f)
/// 
/// There can be numerous roots to handle chain forking
/// If we introduce a new root, then any path to updated state will be added under the new root; 
/// however, unchanged data will be shared with old root 
/// 
/// Generics K and V are not required for this to work; however it is good to avoid mismatches

/// Internal trie node storing children and an optional serialized value.
pub struct TrieNode<V: PillarSerialize> {
    _phantum: PhantomData<V>,
    references: u16, // track for deletions
    pub(crate) children: [Option<NodeKey>; 16], // 16 children for each nibble (0-9, a-f)
    pub(crate) value: Option<Vec<u8>>, // Account state
}

impl<T: PillarSerialize> Clone for TrieNode<T> {
    fn clone(&self) -> Self {
        TrieNode {
            _phantum: PhantomData,
            references: self.references,
            children: self.children,
            value: self.value.clone(),
        }
    }
}

impl<T: PillarSerialize> TrieNode<T>{
    fn new() -> Self {
        TrieNode {
            _phantum: PhantomData,
            children: [None; 16],
            references: 1, // the one creating the node
            value: None,
        }
    }
}


#[derive(Clone)]
pub struct MerkleTrie<K: Hashable, V: PillarSerialize> {
    _phantum: PhantomData<K>,
    pub(crate) nodes: SlotMap<NodeKey, TrieNode<V>>, // SlotMap to store Trie nodes
    pub(crate) roots: HashMap<StdByteArray, NodeKey>,
}

/// Hash and convert the key to nibbles
pub(crate) fn to_nibbles(key: &impl Hashable) -> Vec<u8> {
    let key = key.hash(&mut DefaultHash::new()).unwrap();
    key.iter().flat_map(|b| vec![b>>4, b&0x0F]).collect::<Vec<_>>()
}

impl<K: Hashable, V: PillarSerialize> Default for MerkleTrie<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hashable, V: PillarSerialize> MerkleTrie<K, V> {

    /// Creates a new empty Trie
    pub fn new() -> Self {
        MerkleTrie {
            _phantum: PhantomData,
            nodes: SlotMap::with_key(),
            roots: HashMap::new(),
        }
    }

    /// Creates a new Trie with an initial key-value pair as the genesis node.
    /// This function initializes the trie with a single root node containing the provided key and value.
    pub fn create_genesis(&mut self, key: K, value: V) -> Result<StdByteArray, std::io::Error> {
        if !self.roots.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Genesis already exists"));
        }
        let genesis_root = TrieNode::<V>::new();
        let genesis_key = self.nodes.insert(genesis_root);

        self._insert(key, value, genesis_key).expect("Failed to insert genesis node");

        let inital_hash = self.get_hash_for(genesis_key, &mut DefaultHash::new()).unwrap();
        self.roots.insert(inital_hash, genesis_key);
        Ok(inital_hash)
    }


    fn _insert(&mut self, key: K, value: V, root: NodeKey) -> Result<(), std::io::Error>{
        let nibbles = to_nibbles(&key);
        let value = value.serialize_pillar().map_err(std::io::Error::other)?;

        let mut current_node_key = root;
        for nibble in nibbles {
            let index = nibble as usize;
            let mut curr_pointer = self.nodes.get(current_node_key).unwrap().children[index];
            if curr_pointer.is_none() {
                let new_node = TrieNode::new();
                let new_node_key = self.nodes.insert(new_node);
                self.nodes.get_mut(current_node_key).unwrap().children[index] = Some(new_node_key);
                curr_pointer = Some(new_node_key);
            }
            current_node_key = curr_pointer.unwrap();
        }
        let current_node = self.nodes.get_mut(current_node_key).unwrap();
        if current_node.value.is_some() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Key already exists"));
        }
        current_node.value = Some(value);
        Ok(())
    }
    /// Inserts a key-value pair into the trie with state rooted at `root`.
    ///
    /// # Arguments
    /// * `key` - The key to insert, which must implement the `Hashable` trait.
    /// * `value` - The value to insert, which must implement `Serialize`.
    /// * `root` - An optional root key. If `None`, a new root will be created.
    /// 
    /// # Returns
    /// * `Ok(())` if the insertion was successful.
    /// * `Err(std::io::Error)` if serialization fails or if the root is not found.
    pub fn insert(&mut self, key: K, value: V, root: StdByteArray) -> Result<(), std::io::Error> {
        let root_key = self.roots.get(&root);
        let root_key = if let Some(key) = root_key {
            *key
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Root not found"));
        };

        self._insert(key, value, root_key)
    }

    /// Retrieves the value associated with the given key from the trie.
    /// 
    /// # Arguments
    /// * `key` - The key to look up, which must implement the `Hashable` trait.
    /// * `root` - The root key to start the search from.
    /// 
    /// # Returns
    /// * `Some(T)` if the key exists and the value is found.
    /// * `None` if the key does not exist or if there is no value at the node.
    pub fn get(&self, key: &K, root: StdByteArray) -> Option<V> {
        let nibbles = to_nibbles(key);
        let mut current_node_key = *self.roots.get(&root)?;
        for nibble in nibbles {
            let index = nibble as usize;
            if let Some(child_key) = self.nodes.get(current_node_key).unwrap().children[index] {
                current_node_key = child_key;
            } else {
                return None; // Key not found
            }
        }
        let serialized = self.nodes.get(current_node_key).unwrap().value.as_ref();
        serialized.map(|data| PillarSerialize::deserialize_pillar(data).unwrap())
    }

    /// Retrieves all values stored in the trie starting from the given root.
    /// 
    /// # Arguments
    /// * `root` - The hash of the root node to start traversal from.
    /// 
    /// # Returns
    /// * `Vec<&[u8]>` containing references to all serialized values in the trie.
    pub fn get_all(&self, root: StdByteArray) -> Vec<V> {
        let mut values = Vec::new();
        if self.roots.get(&root).is_none() {
            return values;
        };

        let mut visit_queue = VecDeque::new();
        visit_queue.push_back(self.roots.get(&root).unwrap());
        while let Some(current_key) = visit_queue.pop_front(){
            let node = self.nodes.get(*current_key).unwrap();
            if let Some(value) = &node.value{
                values.push(PillarSerialize::deserialize_pillar(value).unwrap());
            }
            for child_key in node.children.iter().flatten(){
                visit_queue.push_back(child_key);
            }
        }

        values
    }
    

    /// Creates a new branch of the trie.
    /// This branch will yield a new root.
    /// Following the root to anything in "keys" will travel down a cloned branch
    /// Anything else that is not on the path will be the same as is in the origin.
    /// Suppose that the trie is binary, and balanced. Then, if there is 1 element in keys
    /// The extra memory is O(log(n)), where n is the number of elements in the trie.
    /// 
    /// # Arguments
    /// * `origin` - The root of the original trie to branch from.
    /// * `keys` - A vector of keys to branch on, which must implement the `Hashable` trait.
    /// 
    /// # Returns
    /// * `Ok(StdByteArray)` containing the new root hash if the branch is created successfully.
    /// * `Err(std::io::Error)` if the origin root is not found or if the keys are empty.
    pub fn branch(&mut self, origin: Option<StdByteArray>, updates: HashMap<K, V>) -> Result<StdByteArray, std::io::Error> {
 
        if updates.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "updates cannot be empty"));
        }

        let mut updates = updates.into_iter().collect::<Vec<_>>();

        if origin.is_none() {
            let first = updates.pop().unwrap();
            self.create_genesis(first.0, first.1)?;
        }

        let origin = origin.unwrap();

        let origin_root_key = *self.roots.get(&origin).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Origin root not found"))?;
        let origin_root = self.nodes.get(origin_root_key).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Root node not found"))?.clone();
        let new_root_key = self.nodes.insert(origin_root.clone());

        let mut new_keys: HashSet<NodeKey> = HashSet::new();
        
        for (key, value) in updates {
            let nibbles = to_nibbles(&key);
            let mut current_node_key = new_root_key;

            for nibble in nibbles {
                let index: usize = nibble as usize;
                let current_child_opt = self.nodes.get(current_node_key).unwrap().children[index];

                let new_child_key = if let Some(child_key) = current_child_opt {
                    if new_keys.contains(&child_key) {
                        // If the child is already cloned, reuse the cloned key
                        child_key
                    } else {   
                        let cloned_child = self.nodes.get(child_key).unwrap().clone();
                        self.nodes.insert(cloned_child)
                    }
                } else {
                    let k = self.nodes.insert(TrieNode::new());
                    new_keys.insert(k);
                    k
                };

                // THE CURRENT_NODE MUST BE ADDED BEFORE THE REFERENCE COUNTS
                let current_node = self.nodes.get_mut(current_node_key).unwrap();
                current_node.children[index] = Some(new_child_key);
                // let mut updated_node = current_node;
                for child in self.nodes.get_mut(current_node_key).unwrap().children.clone().iter_mut() {
                    if let Some(child_key) = child
                        && !new_keys.contains(child_key){
                            let child_node = self.nodes.get_mut(*child_key).unwrap();
                            child_node.references += 1;
                        }
                }
                // increment references for all the children of the new 
                current_node_key = new_child_key;
            }
            // update the value in the new branch
            let current_node = self.nodes.get_mut(current_node_key).unwrap();
            current_node.value = Some(value.serialize_pillar().map_err(std::io::Error::other)?);
        }

        let new_root_hash = self.get_hash_for(new_root_key, &mut DefaultHash::new()).unwrap();
        self.roots.insert(new_root_hash, new_root_key);
        Ok(new_root_hash)
    }

    /// Decrement references along a branch and remove nodes unique to the branch.
    pub fn trim_branch(&mut self, root: StdByteArray) -> Result<(), std::io::Error> {
        let root_key = self.roots.get(&root).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Root not found"))?;
        let mut visit_queue = VecDeque::new();
        visit_queue.push_back(*root_key);

        while let Some(current_key) = visit_queue.pop_front() {
            if let Some(node) = self.nodes.get_mut(current_key) {
                if node.references == 1 {
                    // If the node is only referenced once, collect its children first
                    let children: Vec<NodeKey> = node.children.iter().filter_map(|&child| child).collect();
                    // Remove the node after processing its children
                    self.nodes.remove(current_key);
                    for child_key in children {
                        visit_queue.push_back(child_key);
                    }
                } else {
                    // Otherwise, decrement the reference count
                    node.references -= 1;
                }
            }
        }
        self.roots.remove(&root);
        Ok(())
        
    }

    /// Computes the hash for the given node.
    /// This function recursively computes the hash of the node and its children.
    /// # Arguments
    /// * `node` - The key of the node for which to compute the hash.
    /// 
    /// # Returns
    /// * `Some(StdByteArray)` if the hash is computed successfully.
    /// * `None` if the node does not exist or has no value.
    pub fn get_hash_for(&self, node: NodeKey, hash_function: &mut impl HashFunction) -> Option<StdByteArray> {
        let node = self.nodes.get(node).expect("Node not found");
        let mut valid = false; 
        for (i, child) in node.children.iter().enumerate() {
            if let Some(child_key) = child {
                hash_function.update([i as u8]);
                hash_function.update(self.get_hash_for(*child_key, &mut DefaultHash::new()).unwrap());
                valid = true;
            }
        }

        if let Some(value) = &node.value {
            hash_function.update([node.children.len() as u8]); // 16 is the marker for value because it is the 
            hash_function.update(value);
            valid = true;
        }
        
        if valid {Some(hash_function.digest().unwrap())} else {None}
    }

}

impl PillarSerialize for NodeKey {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        Ok(self.0.as_ffi().serialize_pillar()?)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        Ok(NodeKey(KeyData::from_ffi(
            u64::from_le_bytes(data[0..8].try_into().unwrap())
        )))
    }
}

impl<K: PillarSerialize + Hashable, V: PillarSerialize> PillarSerialize for MerkleTrie<K, V> {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = Vec::new();
        buffer.extend((self.nodes.len() as u32).to_le_bytes());
        for (key, node) in &self.nodes {
            let mut internal_buffer = vec![];
            // key
            internal_buffer.extend(key.serialize_pillar()?); // always 8
            // references
            internal_buffer.extend(node.references.to_le_bytes()); // always 2
            // value
            let vbuff = node.value.serialize_pillar()?;
            internal_buffer.extend((vbuff.len() as u32).to_le_bytes());
            internal_buffer.extend(vbuff);
            // children
            let d = node.children.serialize_pillar()?;
            internal_buffer.extend((d.len() as u32).to_le_bytes());
            internal_buffer.extend(d);

            // give length and add
            buffer.extend((internal_buffer.len() as u32).to_le_bytes());
            buffer.extend(internal_buffer);
        }
        buffer.extend(self.roots.serialize_pillar()?);
        Ok(buffer)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let n = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let mut offset = 4;
        let nodes = SlotMap::new();
        for _ in 0..n {
            let key = NodeKey::deserialize_pillar(&data[offset..offset + 8])?;
            offset += 8;
            let references = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap());
            offset += 2;
            let value_size = u32::from_le_bytes(data[offset..offset+4].try_into().unwrap()) as usize;
            offset += 4;
            let values: Option<Vec<u8>> = Option::<Vec::<u8>>::deserialize_pillar(&data[offset.. offset + value_size])?;
            offset += value_size;
            let children_size = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let children: [Option<NodeKey>; 16] = <[Option<NodeKey>; 16]>::deserialize_pillar(&data[offset.. offset + children_size])?;
            offset += children_size;
            let node = TrieNode{
                references: references,
                children: children,
                value: values,
                _phantum: PhantomData
            };
            nodes.insert(value)
        }

        todo!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hashing::DefaultHash, proofs::generate_proof_of_state};

    #[derive( Debug, PartialEq, Clone)]
    struct AccountState {
        balance: u64,
        nonce: u64,
    }

    impl PillarSerialize for AccountState {
        fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();
            buf.extend(&self.balance.to_le_bytes());
            buf.extend(&self.nonce.to_le_bytes());
            Ok(buf)
        }

        fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
            if data.len() != 16 {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid data length"));
            }
            let balance = u64::from_le_bytes(data[0..8].try_into().unwrap());
            let nonce = u64::from_le_bytes(data[8..16].try_into().unwrap());
            Ok(AccountState { balance, nonce })
        }
    }

    #[test]
    fn test_trie_insert_and_get() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };

        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");
        let account = AccountState { balance: 100, nonce: 1 };
        
        trie.insert("account1", account.clone(), initial_root).unwrap();
        assert_eq!(trie.get(&"account1", initial_root), Some(account.clone()));
        
        let account2 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account2", account2.clone(), initial_root).unwrap();
        assert_eq!(trie.get(&"account2",initial_root), Some(account2.clone()));

        let r = trie.insert("account2", account2.clone(), initial_root);
        assert!(r.is_err(), "Inserting an existing key should return an error");
        
        assert_eq!(trie.get(&"non_existent", initial_root), None);

        // Test branching
        let new_account = AccountState { balance: 300, nonce: 3 };
        // let branch_keys = vec![("account1", new_account.clone())]; // this means we want to be able to update account1 in a new state but account 2 will not change 
        let mut branch_keys = HashMap::new();
        branch_keys.insert("account1", new_account.clone());
        let new_root = trie.branch(Some(initial_root), branch_keys).unwrap();
        assert_eq!(trie.get(&"account1", new_root), Some(new_account.clone()));
        assert_eq!(trie.get(&"account2", new_root), Some(account2.clone()));
        // no change at old root
        assert_eq!(trie.get(&"account1", initial_root), Some(account.clone()));
        assert_eq!(trie.get(&"account2", initial_root), Some(account2.clone()));
        // insert to new root - assert not in old root
        let account3 = AccountState { balance: 400, nonce: 4 };
        trie.insert("account3", account3.clone(), new_root).unwrap();
        assert_eq!(trie.get(&"account3", new_root), Some(account3.clone()));
        // assert that account3 is not in the old root
        assert_eq!(trie.get(&"account3", initial_root), None);

        // now secondary branch off of new root
        // let branch_keys2 = vec![("account2", AccountState { balance: 500, nonce: 5 }), ("account3", AccountState { balance: 600, nonce: 6 })];
        let mut branch_keys2 = HashMap::new();
        branch_keys2.insert("account2", AccountState { balance: 500, nonce: 5 });
        branch_keys2.insert("account3", AccountState { balance: 600, nonce: 6 });
        let new_root2 = trie.branch(Some(new_root), branch_keys2).unwrap();
        assert_eq!(trie.get(&"account1", new_root2), Some(new_account.clone()));
        assert_eq!(trie.get(&"account2", new_root2), Some(AccountState { balance: 500, nonce: 5 }));
        assert_eq!(trie.get(&"account3", new_root2), Some(AccountState { balance: 600, nonce: 6 }));
        // assert that account2 and account3 are not in the old root
        assert_eq!(trie.get(&"account2", initial_root), Some(account2.clone()));
        assert_eq!(trie.get(&"account3", initial_root), None);
        // make sure that in new_root they are unchanged
        assert_eq!(trie.get(&"account2", new_root), Some(account2));
        assert_eq!(trie.get(&"account3", new_root), Some(account3));
        // now, check that we can update the original root with no impact to the new ones
        let new_account2 = AccountState { balance: 700, nonce: 7 };
        trie.insert("account4", new_account2.clone(), initial_root).unwrap();
        assert_eq!(trie.get(&"account4", initial_root), Some(new_account2.clone()));
        // make sure it is not in the new roots
        assert_eq!(trie.get(&"account4", new_root), None);
        assert_eq!(trie.get(&"account4", new_root2), None);

    }

    #[test]
    fn test_proofs() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = 
        // MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();
        
        let (proof, _) = generate_proof_of_state(&trie, "account0", Some(initial_root), &mut DefaultHash::new()).expect("Proof generation failed");
        let root_key = trie.roots.get(&initial_root).expect("Root not found");
        let valid = proof.verify(
            initial_account_info.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new()
        );
        assert!(valid, "Proof verification failed");

        let bin = account1.serialize_pillar().unwrap();
        println!("Account1 bin: {bin:?}");
        let valid2 = proof.verify(
            bin, 
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new()
        );
        assert!(!valid2, "Proof verification should fail for a different account");
    }

    #[test]
    fn test_proof_for_single_key() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let (proof, _) = generate_proof_of_state(&trie, "account0", Some(initial_root), &mut DefaultHash::new()).expect("Proof generation failed");
        let root_key = trie.roots.get(&initial_root).expect("Root not found");
        let valid = proof.verify(
            initial_account_info.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new(),
        );
        assert!(valid, "Proof verification failed for single key");
    }

    #[test]
    fn test_proof_for_multiple_keys() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        let account2 = AccountState { balance: 300, nonce: 3 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();
        trie.insert("account2", account2.clone(), initial_root).unwrap();

        let (proof1, _) = generate_proof_of_state(&trie, "account1", Some(initial_root), &mut DefaultHash::new()).expect("Proof generation failed for account1");
        let (proof2, _) = generate_proof_of_state(&trie, "account2", Some(initial_root), &mut DefaultHash::new()).expect("Proof generation failed for account2");

        let root_key = trie.roots.get(&initial_root).expect("Root not found");

        let valid1 = proof1.verify(
            account1.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new(),
        );
        assert!(valid1, "Proof verification failed for account1");

        let valid2 = proof2.verify(
            account2.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new(),
        );
        assert!(valid2, "Proof verification failed for account2");
    }

    #[test]
    fn test_proof_verification_failure() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        let (proof, _) = generate_proof_of_state(&trie, "account1", Some(initial_root), &mut DefaultHash::new()).expect("Proof generation failed");

        let root_key = trie.roots.get(&initial_root).expect("Root not found");

        let invalid_account = AccountState { balance: 500, nonce: 5 };
        let valid = proof.verify(
            invalid_account.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new(),
        );
        assert!(!valid, "Proof verification should fail for invalid account");
    }

    #[test]
    fn test_proof_for_branch() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        // let branch_keys = vec![("account1", AccountState { balance: 300, nonce: 3 })];
        let mut branch_keys = HashMap::new();
        branch_keys.insert("account1", AccountState { balance: 300, nonce: 3 });
        let new_root = trie.branch(Some(initial_root), branch_keys).unwrap();

        let (proof, _) = generate_proof_of_state(&trie, "account1", Some(new_root), &mut DefaultHash::new()).expect("Proof generation failed for branch");

        let root_key = trie.roots.get(&new_root).expect("Root not found");

        let valid = proof.verify(
            AccountState { balance: 300, nonce: 3 }.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new(),
        );
        assert!(valid, "Proof verification failed for branch");

        let valid = proof.verify(
            AccountState { balance: 300, nonce: 3 }.serialize_pillar().unwrap(),
            trie.get_hash_for(*root_key, &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new(),
        );
        assert!(valid, "Proof verification failed for branch");
    }

    #[test]
    fn test_branch_with_new_account(){
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        // let branch_keys = vec![("account2", AccountState { balance: 300, nonce: 3 })];
        let mut branch_keys = HashMap::new();
        branch_keys.insert("account2", AccountState { balance: 300, nonce: 3 });
        let new_root = trie.branch(Some(initial_root), branch_keys).unwrap();

        assert_eq!(trie.get(&"account2", new_root), Some(AccountState { balance: 300, nonce: 3 }));
        assert_eq!(trie.get(&"account1", new_root), Some(account1));
    }

    #[test]
    fn test_branch_twice_same(){
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        // let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        // let branch_keys = vec![("account1", AccountState { balance: 300, nonce: 3 })];
        let mut branch_keys = HashMap::new();
        branch_keys.insert("account2", AccountState { balance: 300, nonce: 3 });
        let new_root = trie.branch(Some(initial_root), branch_keys.clone()).unwrap();

        // now branch again with the same keys
        let new_root2 = trie.branch(Some(initial_root), branch_keys).unwrap();

        assert_eq!(new_root, new_root2);

    }

    #[test]
    fn test_branch_multiple_times_with_complexity() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        let account2 = AccountState { balance: 300, nonce: 3 };
        trie.insert("account2", account2.clone(), initial_root).unwrap();

        // First branch with updates
        let mut branch_keys = HashMap::new();
        branch_keys.insert("account1", AccountState { balance: 400, nonce: 4 });
        branch_keys.insert("account3", AccountState { balance: 500, nonce: 5 });
        let new_root = trie.branch(Some(initial_root), branch_keys.clone()).unwrap();

        // Verify the first branch
        assert_eq!(trie.get(&"account1", new_root), Some(AccountState { balance: 400, nonce: 4 }));
        assert_eq!(trie.get(&"account3", new_root), Some(AccountState { balance: 500, nonce: 5 }));
        assert_eq!(trie.get(&"account2", new_root), Some(account2.clone()));

        // Create the same branch again
        let new_root2 = trie.branch(Some(initial_root), branch_keys.clone()).unwrap();

        // Verify the second branch is identical to the first
        assert_eq!(new_root, new_root2);
        assert_eq!(trie.get(&"account1", new_root2), Some(AccountState { balance: 400, nonce: 4 }));
        assert_eq!(trie.get(&"account3", new_root2), Some(AccountState { balance: 500, nonce: 5 }));
        assert_eq!(trie.get(&"account2", new_root2), Some(account2.clone()));

        // Add more complexity: branch from the first branch
        let mut branch_keys2 = HashMap::new();
        branch_keys2.insert("account3", AccountState { balance: 600, nonce: 6 });
        branch_keys2.insert("account4", AccountState { balance: 700, nonce: 7 });
        let new_root3 = trie.branch(Some(new_root), branch_keys2).unwrap();

        // Verify the new branch
        assert_eq!(trie.get(&"account1", new_root3), Some(AccountState { balance: 400, nonce: 4 }));
        assert_eq!(trie.get(&"account3", new_root3), Some(AccountState { balance: 600, nonce: 6 }));
        assert_eq!(trie.get(&"account4", new_root3), Some(AccountState { balance: 700, nonce: 7 }));
        assert_eq!(trie.get(&"account2", new_root3), Some(account2.clone()));

        // Ensure the original and first branch remain unchanged
        assert_eq!(trie.get(&"account3", new_root), Some(AccountState { balance: 500, nonce: 5 }));
        assert_eq!(trie.get(&"account4", new_root), None);
        assert_eq!(trie.get(&"account3", initial_root), None);
        assert_eq!(trie.get(&"account4", initial_root), None);
    }

    #[test]
    fn test_get_all() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        let account2 = AccountState { balance: 300, nonce: 3 };
        trie.insert("account2", account2.clone(), initial_root).unwrap();

        let account3 = AccountState { balance: 400, nonce: 4 };
        trie.insert("account3", account3.clone(), initial_root).unwrap();

        // branch 

        let account4 = AccountState { balance: 4000, nonce: 0};
        let mut updates = HashMap::new();
        updates.insert("account4", account4.clone());
        let _ = trie.branch(Some(initial_root), updates).unwrap();

        let all_values = trie.get_all(initial_root);

        assert_eq!(all_values.len(), 4); // account0, account1, account2, account3
        assert!(all_values.contains(&initial_account_info));
        assert!(all_values.contains(&account1));
        assert!(all_values.contains(&account2));
        assert!(all_values.contains(&account3));
        assert!(!all_values.contains(&account4));
    }

    #[test]
    fn test_trim(){
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        let account2 = AccountState { balance: 300, nonce: 3 };
        trie.insert("account2", account2.clone(), initial_root).unwrap();

        // branch 
        let mut updates = HashMap::new();
        updates.insert("account3", AccountState { balance: 0, nonce: 1 });
        let new_root = trie.branch(Some(initial_root), updates).unwrap();

        let all_values = trie.get_all(initial_root);
        assert_eq!(all_values.len(), 3); // account0, account1, account2

        let mut pass = false;
        for (_, node) in trie.nodes.iter(){
            if node.value.is_some() && node.value.as_ref().unwrap() == &(AccountState { balance: 0, nonce: 1 }).serialize_pillar().unwrap() {
                pass = true;
                break;
            }
        }
        assert!(pass);

        // trim the branch
        trie.trim_branch(new_root).expect("Failed to trim branch");

        // check that the new root is still there
        assert!(trie.get(&"account3", new_root).is_none());
        
        // check that the old root is still there
        assert_eq!(trie.get(&"account1", initial_root), Some(account1));
        assert_eq!(trie.get(&"account2", initial_root), Some(account2));
        assert_eq!(trie.get(&"account0", initial_root), Some(initial_account_info));

        // getall, make sure account 3 does not exist
        let all_values = trie.get_all(initial_root);
        assert_eq!(all_values.len(), 3); // account0, account1, account2

        // check all node values for not having acc3
        for (_, node) in trie.nodes.iter(){
            if node.value.is_some() && node.value.as_ref().unwrap() == &(AccountState { balance: 0, nonce: 1 }).serialize_pillar().unwrap() {
                panic!("Node with account3 value still exists after trim");
            }
        }
    }

    #[test]
    fn test_trim_complex(){
        let initial_account_info = AccountState { balance: 100, nonce: 1 };
        let mut trie = MerkleTrie::<&str, AccountState>::new();
        let initial_root = trie.create_genesis("account0", initial_account_info.clone()).expect("Failed to create genesis");

        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();

        let account2 = AccountState { balance: 300, nonce: 3 };
        trie.insert("account2", account2.clone(), initial_root).unwrap();

        // branch 
        let mut updates = HashMap::new();
        updates.insert("account3", AccountState { balance: 2000, nonce: 44 });
        let new_root = trie.branch(Some(initial_root), updates).unwrap();

        // branch again
        let mut updates2 = HashMap::new();
        updates2.insert("accont4", AccountState { balance: 0, nonce: 1 });
        let new_root2 = trie.branch(Some(new_root), updates2).unwrap();

        // check that the new root is still there
        assert!(trie.get(&"account3", new_root).is_some());
        assert!(trie.get(&"accont4", new_root2).is_some());
        
        // check that the old root is still there
        assert_eq!(trie.get(&"account1", initial_root), Some(account1.clone()));
        assert_eq!(trie.get(&"account2", initial_root), Some(account2.clone()));
        assert_eq!(trie.get(&"account0", initial_root), Some(initial_account_info.clone()));

        // getall, make sure account 3 does not exist
        let all_values = trie.get_all(initial_root);
        assert_eq!(all_values.len(), 3); // account0, account1, account2

        // trim the branch
        trie.trim_branch(new_root).expect("Failed to trim branch");

        // check that the new root is still there
        assert!(trie.get(&"account3", new_root).is_none());
        assert!(trie.get(&"account3" , new_root2).is_some());
        
        // check that the old root is still there
        assert_eq!(trie.get(&"account1", initial_root), Some(account1));
        assert_eq!(trie.get(&"account2", initial_root), Some(account2));
        assert_eq!(trie.get(&"account0", initial_root), Some(initial_account_info));


    }
}