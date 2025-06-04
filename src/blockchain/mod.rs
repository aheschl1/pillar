use std::collections::{HashMap, HashSet};

use crate::{crypto::hashing::{DefaultHash, HashFunction, Hashable}, nodes::node::StdByteArray, primitives::block::BlockHeader};

pub mod chain;
pub mod chain_shard;

pub trait TrimmableChain {
    fn get_headers(&self) -> &HashMap<StdByteArray, BlockHeader>;
    fn get_leaves_mut(&mut self) -> &mut HashSet<StdByteArray>;
    fn remove_header(&mut self, hash: &StdByteArray);

    fn trim(&mut self) {
        let headers = self.get_headers().clone();
        let mut seen = HashMap::<StdByteArray, StdByteArray>::new(); // node: leaf leading there
        let mut forks_to_kill = HashSet::<StdByteArray>::new();
        let mut sorted_leaves: Vec<_> = self.get_leaves_mut().iter().cloned().collect();
        // visit deepest leaves first so that we can look back at deeper forks later
        sorted_leaves.sort_by_key(|x| -(headers[x].depth as i64)); // deepest first

        for leaf in &sorted_leaves {
            // iterate backwards, marking each node
            // if the node is already seen, then check the leaf that saw it - can they coexist?
            let current_fork_depth = headers[leaf].depth;
            let mut current_node = headers.get(leaf);
            while let Some(node) = current_node {
                let hash = node.hash(&mut DefaultHash::new()).unwrap();
                if seen.contains_key(&hash) {
                    let fork = seen[&hash];
                    let fork_depth = headers[&fork].depth;
                    if fork_depth >= current_fork_depth + 10 {
                         // kill this current fork from the leaf
                        forks_to_kill.insert(*leaf); // leave this fork early - everything downstream has been marked, and we kill eitherway
                        break;
                    }
                } else {
                    // indicate that this node is seem
                    seen.insert(hash, *leaf);
                }
                current_node = headers.get(&node.previous_hash);
            }
        }
        // actully trim
        let mut nodes_to_remove = Vec::new();
        for fork in &forks_to_kill {
            let leaves = self.get_leaves_mut();
            // update leaves
            leaves.remove(fork);
            let mut current_node = headers.get(fork);
            // collect nodes to remove until the seen is no longer the fork
            while let Some(node) = current_node {
                let hash = node.hash(&mut DefaultHash::new()).unwrap();
                if seen[&hash] == *fork {
                    nodes_to_remove.push(hash);
                } else {
                    // we have reached the end of this fork
                    break;
                }
                current_node = headers.get(&node.previous_hash);
            }
        }
        // perform removal after collecting nodes
        for hash in nodes_to_remove {
            self.remove_header(&hash);
        }
    }
}