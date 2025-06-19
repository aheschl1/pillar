use std::{collections::{HashMap, HashSet}, marker::PhantomData};

use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};

use crate::{crypto::{hashing::{DefaultHash, HashFunction, Hashable}, proofs::{MerkleProofV2, ProofStep, Provable}}, nodes::node::{Node, StdByteArray}};
new_key_type! { struct NodeKey; }

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

struct TrieNode<V: for<'a> Deserialize<'a>> {
    _phantum: PhantomData<V>,
    children: [Option<NodeKey>; 16], // 16 children for each nibble (0-9, a-f)
    value: Option<Vec<u8>>, // Account state
}

impl<T: for<'a> Deserialize<'a>> Clone for TrieNode<T> {
    fn clone(&self) -> Self {
        TrieNode {
            _phantum: PhantomData,
            children: self.children.clone(),
            value: self.value.clone(),
        }
    }
}

impl<T: for<'a> Deserialize<'a>> TrieNode<T>{
    fn new() -> Self {
        TrieNode {
            _phantum: PhantomData,
            children: [None; 16],
            value: None,
        }
    }
}

pub struct MerkleTrie<K: Hashable, V: Serialize + for<'a> Deserialize<'a>> {
    _phantum: PhantomData<K>,
    nodes: SlotMap<NodeKey, TrieNode<V>>, // SlotMap to store Trie nodes
    roots: HashMap<StdByteArray, NodeKey>,
}

/// Hash and convert the key to nibbles
fn to_nibbles(key: &impl Hashable) -> Vec<u8> {
    let key = key.hash(&mut DefaultHash::new()).unwrap();
    key.iter().flat_map(|b| vec![b>>4, b&0x0F]).collect::<Vec<_>>()
}

impl<K: Hashable, V: Serialize + for<'a> Deserialize<'a>> MerkleTrie<K, V>{
    
    /// Creates a new empty Trie with a genesis root.
    /// The trie is not defined when empty as you always need to define the branch.
    /// 
    /// TODO could create a throwaway key value pair; however, it wouldnt make sense as the caller needs
    /// to know the root to call upon. At chain build time, they will create an account for themselves
    /// so this makes sense.
    /// 
    /// # Arguments
    /// * `key` - The key to insert into the genesis node, which must implement the `Hashable` trait.
    /// * `value` - The value to insert into the genesis node, which must implement `Serialize`.
    pub fn new(key: K, value: V) -> (Self, StdByteArray) {
        let genesis_root = TrieNode::<V>::new();
        let mut nodes = SlotMap::with_key();
        let genesis_key = nodes.insert(genesis_root);
        let roots = HashMap::new();

        let mut trie = MerkleTrie {
            _phantum: PhantomData,
            nodes,
            roots
        };

        trie._insert(key, value, genesis_key).expect("Failed to insert genesis node");
        let inital_hash = trie.get_hash_for(genesis_key, &mut DefaultHash::new()).unwrap();
        trie.roots.insert(inital_hash.clone(), genesis_key);
        (trie, inital_hash)
    }


    fn _insert(&mut self, key: K, value: V, root: NodeKey) -> Result<(), std::io::Error>{
        let nibbles = to_nibbles(&key);
        let value = bincode::serialize(&value).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        println!("value: {:?}", value);

        let mut current_node_key = root;
        for nibble in nibbles {
            let index = nibble as usize;
            let mut curr_pointer = self.nodes.get(current_node_key).unwrap().children[index];
            if curr_pointer.is_none() {
                let new_node_key = self.nodes.insert(TrieNode::new());
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
        let mut current_node_key = self.roots.get(&root)?.clone();
        for nibble in nibbles {
            let index = nibble as usize;
            if let Some(child_key) = self.nodes.get(current_node_key).unwrap().children[index] {
                current_node_key = child_key;
            } else {
                return None; // Key not found
            }
        }
        let serialized = self.nodes.get(current_node_key).unwrap().value.as_ref();
        match serialized {
            Some(data) => Some(bincode::deserialize(&mut data.clone()).unwrap()),
            None => None, // No value at this node
        }
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
    pub fn branch(&mut self, origin: StdByteArray, updates: Vec<(K, V)>) -> Result<StdByteArray, std::io::Error> {
        if updates.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "updates cannot be empty"));
        }

        let origin_root_key = *self.roots.get(&origin).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Origin root not found"))?;
        let origin_root = self.nodes.get(origin_root_key).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Root node not found"))?.clone();
        let new_root_key = self.nodes.insert(origin_root);

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
                        let cloned_key = self.nodes.insert(cloned_child);
                        cloned_key
                    }
                } else {
                    self.nodes.insert(TrieNode::new())
                };
                new_keys.insert(new_child_key);

                // let mut updated_node = current_node;
                let current_node = self.nodes.get_mut(current_node_key).unwrap();
                current_node.children[index] = Some(new_child_key);
                current_node_key = new_child_key;
            }
            // update the value in the new branch
            let current_node = self.nodes.get_mut(current_node_key).unwrap();
            current_node.value = Some(bincode::serialize(&value).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?);
        }

        let new_root_hash = self.get_hash_for(new_root_key, &mut DefaultHash::new()).unwrap();
        self.roots.insert(new_root_hash, new_root_key);
        Ok(new_root_hash)
    }

    /// Computes the hash for the given node.
    /// This function recursively computes the hash of the node and its children.
    /// # Arguments
    /// * `node` - The key of the node for which to compute the hash.
    /// 
    /// # Returns
    /// * `Some(StdByteArray)` if the hash is computed successfully.
    /// * `None` if the node does not exist or has no value.
    fn get_hash_for(&self, node: NodeKey, hash_function: &mut impl HashFunction) -> Option<StdByteArray> {
        let node = self.nodes.get(node).expect("Node not found");
        let mut valid = false; 
        for (i, child) in node.children.iter().enumerate() {
            if let Some(child_key) = child {
                hash_function.update(&[i as u8]);
                hash_function.update(&self.get_hash_for(*child_key, &mut DefaultHash::new()).unwrap());
                valid = true;
            }
        }

        if let Some(value) = &node.value {
            hash_function.update(&[node.children.len() as u8]); // 16 is the marker for value because it is the 
            hash_function.update(&value);
            valid = true;
        }
        
        if valid {Some(hash_function.digest().unwrap())} else {None}
    }

}

impl<K, V> Provable<K> for MerkleTrie<K, V>
    where K: Hashable, V: Serialize + for<'a> Deserialize<'a>
{   
    /// Generate a Merkle proof for a specific value in the Merkle Trie.
    /// # Arguments
    /// * `target_key` - The key for which the proof is generated.
    fn generate_proof_for_key(&self, target_key: K, root: Option<StdByteArray>, hash_function: &mut impl HashFunction) -> Option<MerkleProofV2> {
        let value = self.get(&target_key, root.expect("Root must be provided"));
        if value.is_none() {
            return None; // If the key does not exist, no proof can be generated.
        }
        let path = to_nibbles(&target_key);
        println!("{:?}", path);
        let mut steps: Vec<ProofStep> = Vec::new();
        
        let mut current_node_key = Some(self.roots.get(&root.expect("Root must be provided")).unwrap().clone());
        // work the way down, computing the proof steps along the way.
        let mut nibble = Some(path[0]);
        let mut i = 0;
        while let Some(key) = current_node_key {
            let current_node = self.nodes.get(key).expect("Node not found");
            let mut siblings: Vec<(u8, StdByteArray)> = Vec::new();
            for (j, child) in current_node.children.iter().enumerate() {
                if (nibble.is_none() || j != nibble.unwrap() as usize) && child.is_some() {
                    let sibling_key = child.unwrap();
                    let sibling_hash = self.get_hash_for(sibling_key, hash_function).expect("Sibling hash not found");
                    siblings.push((j as u8, sibling_hash));
                }
            }
            steps.push(ProofStep {
                native: nibble.unwrap_or(0), // where is the nibble in the path
                siblings,
                value: if let Some(value) = current_node.value.clone() {
                    Some((current_node.children.len() as u8, value))
                } else {None}
            });
            current_node_key = match nibble {
                Some(n) => {
                    i += 1;
                    nibble = if i < path.len() { Some(path[i]) } else { None };
                    current_node.children[n as usize]
                },
                None => None
            };
        }

        return Some(MerkleProofV2::new(steps));
    }

    /// Generate a Merkle proof for a specific value in the Merkle Trie.
    fn generate_proof_for_value(&self, _: K, _: Option<StdByteArray>, _: &mut impl HashFunction) -> Option<MerkleProofV2> {
        unimplemented!("generate_proof_for_value is not implemented yet");
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct AccountState {
        balance: u64,
        nonce: u64,
    }

    #[test]
    fn test_trie_insert_and_get() {
        let initial_account_info = AccountState { balance: 100, nonce: 1 };

        let (mut trie, initial_root) = MerkleTrie::<&str, AccountState>::new("account0", initial_account_info);
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
        let branch_keys = vec![("account1", new_account.clone())]; // this means we want to be able to update account1 in a new state but account 2 will not change 
        let new_root = trie.branch(initial_root, branch_keys).unwrap();
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
        let branch_keys2 = vec![("account2", AccountState { balance: 500, nonce: 5 }), ("account3", AccountState { balance: 600, nonce: 6 })];
        let new_root2 = trie.branch(new_root, branch_keys2).unwrap();
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
        let (mut trie, initial_root) = 
        MerkleTrie::<&str, AccountState>::new("account0", initial_account_info.clone());
        
        let account1 = AccountState { balance: 200, nonce: 2 };
        trie.insert("account1", account1.clone(), initial_root).unwrap();
        
        let proof = trie.generate_proof_for_key("account0", Some(initial_root), &mut DefaultHash::new()).expect("Proof generation failed");
        let root_key = trie.roots.get(&initial_root).expect("Root not found");
        let valid = proof.verify(
            bincode::serialize(&initial_account_info).unwrap(), 
            trie.get_hash_for(root_key.clone(), &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new()
        );
        assert!(valid, "Proof verification failed");

        let bin = bincode::serialize(&account1).unwrap();
        println!("Account1 bin: {:?}", bin);
        let valid2 = proof.verify(
            bin, 
            trie.get_hash_for(root_key.clone(), &mut DefaultHash::new()).unwrap(),
            &mut DefaultHash::new()
        );
        assert!(!valid2, "Proof verification should fail for a different account");
    }
}