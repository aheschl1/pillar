use std::collections::{HashSet, VecDeque};

use pillar_crypto::{signing::{DefaultVerifier, SigVerFunction, Signable}, types::StdByteArray};

use crate::{primitives::block::{BlockTail, Stamp}, protocol::reputation::N_TRANSMISSION_SIGNATURES};

impl BlockTail {
    pub fn new(stamps: [Stamp; N_TRANSMISSION_SIGNATURES]) -> Self {
        BlockTail {
            stamps
        }
    }

    pub fn n_stamps(&self) -> usize {
        self.stamps.iter().filter(|s| s.signature != [0; 64]).count()
    }

    /// remove space between the signatures to ensure all empty space is at the end
    /// remove duplicate signatures
    pub fn collapse(&mut self){
        let mut seen: HashSet<StdByteArray> = HashSet::new();
        let mut empty = VecDeque::new();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            if self.stamps[i].address == [0; 32] {
                empty.push_back(i);
            }else{
                if seen.contains(&self.stamps[i].address) {
                    // if the address is already seen, remove it
                    self.stamps[i] = Stamp::default();
                    empty.push_back(i);
                } else if !empty.is_empty() {
                    self.stamps.swap(i, empty.pop_front().unwrap());
                    empty.push_back(i);
                }
                seen.insert(self.stamps[i].address); // record the address
            }
        }
    }


    /// removes any stamps with invalid signatures
    pub fn clean(&mut self, target: &impl Signable<64>) {
        for i in 0..self.n_stamps() {
            let sigver = DefaultVerifier::from_bytes(&self.stamps[i].address);
            if !sigver.verify(&self.stamps[i].signature, target) {
                self.stamps[i] = Stamp::default();
            }
        }
        self.collapse();
    }

    /// Stamp the block with a signature
    /// Collapses the tail to remove empty space
    /// 
    /// # Arguments
    /// * `stamp` - The stamp to add to the block
    /// 
    /// # Returns
    /// * `Ok(())` if the stamp was added successfully
    /// * `Err(std::io::Error)` if the stamp was not added successfully
    pub fn stamp(&mut self, stamp: Stamp) -> Result<(), std::io::Error>{
        self.collapse();
        if self.n_stamps() >= N_TRANSMISSION_SIGNATURES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Too many stamps"
            ));
        }
        self.stamps[self.n_stamps()] = stamp;
        Ok(())
    }

    pub fn get_stampers(&self) -> HashSet<StdByteArray> {
        let mut stampers = HashSet::new();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            if self.stamps[i].signature != [0; 64] {
                stampers.insert(self.stamps[i].address);
            }
        }
        stampers
    }

    // iter stamps
    pub fn iter_stamps(&self) -> impl Iterator<Item = &Stamp> {
        self.stamps.iter().filter(|s| s.signature != [0; 64])
    }
}