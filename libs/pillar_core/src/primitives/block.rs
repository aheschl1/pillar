
use bytemuck::{Pod, Zeroable};
use pillar_crypto::{merkle_trie::MerkleTrie, types::StdByteArray};


use crate::{accounting::account::Account, protocol::reputation::N_TRANSMISSION_SIGNATURES};
use super::transaction::Transaction;


pub struct BlockMetaData {
    pub height: u64,
    pub global_state: MerkleTrie<StdByteArray, Account>
}


#[derive(Debug,  Clone, PartialEq, Eq)]
#[repr(C, align(8))]
pub struct Block{
    // header is the header of the block
    pub header: BlockHeader,
    // transactions is a vector of transactions in this block
    pub transactions: Vec<Transaction>
}

#[derive(Pod, Zeroable, Debug, PartialEq, Clone, Copy, Eq, Hash)]
#[repr(C, align(8))]
pub struct Stamp{
    // the signature of the person who broadcasted the block
    pub signature: [u8; 64],
    // the address of the person who stamped the block
    pub address: StdByteArray
}

impl Default for Stamp {
    fn default() -> Self {
        Stamp {
            signature: [0; 64],
            address: [0; 32]
        }
    }
}


/// A block tail tracks the signatures of people who have broadcasted the block
/// This is used for immutibility of participation reputation
#[derive(Pod, Zeroable, Debug,  PartialEq, Clone, Copy, Eq, Default, Hash)]
#[repr(C, align(8))]
pub struct BlockTail{
    // the signatures of the people who have broadcasted the block
    pub stamps: [Stamp; N_TRANSMISSION_SIGNATURES]
}

#[derive(Default, Pod, Zeroable, Debug,  PartialEq, Clone, Copy, Eq)]
#[repr(C, align(8))]
/// Header completion is Some in the BlockHeader if the block is mined
pub struct HeaderCompletion {
    inner: HeaderCompletionInner
}

#[derive(Pod, Zeroable, Debug,  PartialEq, Clone, Copy, Eq, Default)]
#[repr(C, align(8))]
pub struct HeaderCompletionInner{
    // hash of the block
    pub hash: StdByteArray,
    // the address of the miner is the sha3_256 hash of the miner address
    pub miner_address: StdByteArray,
    // the root hash of the global state after this block
    pub state_root: StdByteArray,
    // difficulty target of the block
    pub difficulty_target: u64, // this will be set to 0 in memory for the option
}

impl HeaderCompletion {
    pub fn new(
        hash: StdByteArray,
        miner_address: StdByteArray,
        state_root: StdByteArray,
        difficulty_target: u64,
    ) -> Self{
        if difficulty_target == 0 {
            panic!("difficulty_target must be non-zero, or use HeaderCompletion::new_none()");
        }
        HeaderCompletion { inner: HeaderCompletionInner{
            hash,
            miner_address,
            state_root,
            difficulty_target
        } }
    }

    pub fn new_none() -> Self {
        Self::default()
    }

    pub fn is_none(&self) -> bool {
        self.inner.difficulty_target == 0
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn as_mut(&mut self) -> Option<&mut HeaderCompletionInner> {
        if self.is_none() {
            return None;
        }
        Some(&mut self.inner)
    }

    pub fn as_ref(&self) -> Option<&HeaderCompletionInner> {
        if self.is_none() {
            return None;
        }
        Some(&self.inner)
    }
}

#[derive(Pod, Zeroable, Debug,  PartialEq, Clone, Copy, Eq, Default)]
#[repr(C, align(8))]
pub struct BlockHeader{
    // previous_hash is the sha3_356 hash of the previous block in the chain
    pub previous_hash: StdByteArray,
    // merkle_root is the root hash of the transactions in this block
    pub merkle_root: StdByteArray,
    // nonce is a random number used to find a valid hash
    pub nonce: u64,
    // timestamp is the time the block was created
    pub timestamp: u64,
    // the depth is a depth of the block in the chain
    pub depth: u64,
    // version of the protocol used in this block
    pub version: [u8; 2], // le rep of u16
    // phantum pad. with repr C this should in theory add no extra size to the struct
    pub _pad: [u8; 6],
    // tail is the tail of the block which can contain stamps
    pub tail: BlockTail,
    // state_root is the root hash of the global state after this block
    pub completion: HeaderCompletion, 
}



#[cfg(test)]
mod tests {


    use pillar_crypto::signing::{DefaultSigner, SigFunction, SigVerFunction};

    use super::*;

    

    #[test]
    fn test_tail() {
        let mut tail = BlockTail::default();
        assert_eq!(tail.n_stamps(), 0);
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        assert_eq!(tail.n_stamps(), 1);
        tail.collapse();
        assert_eq!(tail.n_stamps(), 1);
    }

    #[test]
    fn test_complex_collapse() {
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[2] = Stamp {
            signature: [3; 64],
            address: [3; 32]
        };
        tail.stamps[3] = Stamp {
            signature: [0; 64],
            address: [0; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 3);
    }

    #[test]
    fn test_collapse_with_move() {
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[2] = Stamp {
            signature: [3; 64],
            address: [3; 32]
        };
        tail.stamps[4] = Stamp {
            signature: [4; 64],
            address: [4; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 4);
        // make sure the empty space is at the end
        assert_eq!(tail.stamps[3].signature, [1; 64]);
    }

    #[test]
    fn test_very_complex_collapse() {
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[3] = Stamp {
            signature: [3; 64],
            address: [3; 32]
        };
        tail.stamps[5] = Stamp {
            signature: [4; 64],
            address: [4; 32]
        };
        tail.stamps[8] = Stamp {
            signature: [5; 64],
            address: [5; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 5);
        assert_eq!(tail.stamps[0].signature, [5; 64]);
        assert_eq!(tail.stamps[1].signature, [4; 64]);
        assert_eq!(tail.stamps[2].signature, [3; 64]);
        assert_eq!(tail.stamps[3].signature, [2; 64]);
        assert_eq!(tail.stamps[4].signature, [1; 64]);
    }

    #[test]
    fn test_collapse_with_duplicates() {
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[2] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 2);
        assert_eq!(tail.stamps[1].address, [1; 32]);
        assert_eq!(tail.stamps[0].address, [2; 32]);
    }

    #[test]
    fn test_clean_invalid_signatures() {
        let mut tail = BlockTail::default();
        let target = BlockHeader::default(); // Assuming BlockHeader implements Default
        let private = DefaultSigner::generate_random();
        let public = private.get_verifying_function().to_bytes();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: public
        };
        let mut private = DefaultSigner::generate_random();
        let public = private.get_verifying_function().to_bytes();
        tail.stamps[1] = Stamp {
            signature: private.sign(&target),
            address: public
        };
        tail.clean(&target);
        assert_eq!(tail.n_stamps(), 1);
    }

    #[test]
    fn test_stamp_addition() {
        let mut tail = BlockTail::default();
        let stamp = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        assert!(tail.stamp(stamp).is_ok());
        assert_eq!(tail.n_stamps(), 1);
        assert_eq!(tail.stamps[0].address, [1; 32]);
    }

    #[test]
    fn test_stamp_overflow() {
        let mut tail = BlockTail::default();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            tail.stamps[i] = Stamp {
                signature: [(i+1) as u8; 64],
                address: [(i+1) as u8; 32]
            };
        }
        let new_stamp = Stamp {
            signature: [255; 64],
            address: [255; 32]
        };
        assert!(tail.stamp(new_stamp).is_err());
    }
}