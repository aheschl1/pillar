use std::num::NonZeroU64;

use pillar_crypto::{hashing::{DefaultHash, HashFunction, Hashable}, signing::{DefaultVerifier, SigFunction, SigVerFunction, Signable}, types::StdByteArray};

use crate::{primitives::{block::{BlockHeader, BlockTail, HeaderCompletion}, errors::BlockValidationError}, protocol::{difficulty::MIN_DIFFICULTY, pow::is_valid_hash, reputation::N_TRANSMISSION_SIGNATURES, versions::Versions}};

impl BlockHeader {
    pub fn new(
        hash: Option<StdByteArray>,
        previous_hash: StdByteArray, 
        merkle_root: StdByteArray, 
        state_root: Option<StdByteArray>,
        nonce: u64, timestamp: u64,
        miner_address: Option<StdByteArray>,
        tail: BlockTail,
        depth: u64,
        difficulty_target: Option<NonZeroU64>,
    ) -> Self {
        Self::new_with_version(
            hash,
            previous_hash, 
            merkle_root, 
            state_root, 
            nonce, 
            timestamp, 
            miner_address, tail, depth, difficulty_target, 
            Versions::default()
        )
    }

    fn new_with_version(
        hash: Option<StdByteArray>,
        previous_hash: StdByteArray, 
        merkle_root: StdByteArray, 
        state_root: Option<StdByteArray>,
        nonce: u64, timestamp: u64,
        miner_address: Option<StdByteArray>,
        tail: BlockTail,
        depth: u64,
        difficulty_target: Option<NonZeroU64>,
        version: Versions
    ) -> Self {
        match (&state_root, &miner_address, &difficulty_target) {
            (Some(_), Some(_), Some(_)) | (None, None, None) => {},
            _ => panic!("state_root, miner_address, and difficulty_target must all be Some or all be None"),
        }
        let completion = if state_root.is_some() {Some(HeaderCompletion {
            hash: hash.unwrap_or([0; 32]),
            state_root: state_root.unwrap_or([0; 32]),
            miner_address: miner_address.unwrap_or([0; 32]),
            difficulty_target: difficulty_target.unwrap_or(MIN_DIFFICULTY),
        })} else{
            None
        };
        let mut header = BlockHeader {
            previous_hash,
            merkle_root,
            nonce,
            timestamp,
            depth,
            tail,
            _pad: [0; 6],
            completion,
            version: version.to_le_bytes()
        };
        if completion.is_some() && hash.is_none() {
            // derive the hash
            let mut hasher = DefaultHash::new();
            let hash = header.hash(&mut hasher).unwrap();
            header.completion.as_mut().unwrap().hash = hash;
        }
        if completion.is_some() && hash.is_some() {
            let mut hasher = DefaultHash::new();
            let hash = header.hash(&mut hasher).unwrap();
            assert_eq!(hash, header.completion.unwrap().hash);
        }
        header
    }

    /// Validate header of the block
    /// Checks:
    /// * The miner is declared
    /// * The difficulty is correct
    /// * The hash is valid
    /// * The time is not too far in the future
    /// 
    /// # Arguments
    /// 
    /// * `expected_hash` - The expected hash of the block
    /// * `hasher` - A mutable instance of a type implementing the HashFunction trait
    pub fn validate(
        &self, 
        expected_hash: StdByteArray,
        hasher: &mut impl HashFunction
    ) -> Result<(), BlockValidationError> {
        // check the miner is declared
        if self.completion.is_none() {
            return Err(BlockValidationError::NoMinerAddress(*self));
        }
        if expected_hash != self.hash(hasher).unwrap() {
            return Err(BlockValidationError::HashMismatch(expected_hash, self.hash(hasher).unwrap()));
        }
        if !is_valid_hash(self.completion.unwrap().difficulty_target.get(), &self.hash(hasher).unwrap()) {
            return Err(BlockValidationError::DifficultyMismatch(self.completion.unwrap().difficulty_target.get(), *self));
        }
        // check that all the signatures work in the tail
        let tail = &mut self.tail.clone();
        tail.collapse();
        for i in 0..tail.n_stamps() {
            let stamp = tail.stamps[i];
            let sigver = DefaultVerifier::from_bytes(&stamp.address);
            if !sigver.verify(&stamp.signature, self) {
                return Err(BlockValidationError::InvalidStampSignature(stamp.address));
            }
        }


        // check the time is not too far in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if self.timestamp > current_time + 60 * 60 {
            // one hour margin
            return Err(BlockValidationError::FutureTimestamp(self.timestamp));
        }
        Ok(())
    }

    /// A hashing function that doesnt rely on any moving pieces like the miner address
    /// This is used for stamping - so that you can stamp before the miner address is set, and it doesnt change based on future stamps.
    fn hash_clean(
        &self,
        hasher: &mut impl HashFunction
    ) -> Result<StdByteArray, std::io::Error>{
        hasher.update(self.previous_hash);
        hasher.update(self.merkle_root);
        hasher.update(self.timestamp.to_le_bytes());
        hasher.update(self.depth.to_le_bytes());
        hasher.update(self.version);
        Ok(hasher.digest().unwrap())
    }
}

impl Signable<64> for BlockHeader {
    fn get_signing_bytes(&self) -> impl AsRef<[u8]> {
        self.hash_clean(&mut DefaultHash::new()).unwrap()
    }
    
    fn sign<const K: usize, const P: usize>(&mut self, signing_function: &mut impl SigFunction<K, P, 64>) -> [u8; 64] {
        signing_function.sign(self)
    }
}

impl Hashable for BlockHeader {
    /// Hash the block header using SHA3-256
    /// 
    /// # Returns
    /// 
    /// * The SHA3-256 hash of the block header
    fn hash(&self, hash_function: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error>{
        if self.completion.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Miner address is not set"
            ));
        }
        hash_function.update(self.version);
        hash_function.update(self.nonce.to_le_bytes());
        hash_function.update(self.depth.to_le_bytes());
        hash_function.update(self.timestamp.to_le_bytes());
        hash_function.update(self.completion.unwrap().miner_address);
        hash_function.update(self.completion.unwrap().state_root);
        hash_function.update(self.completion.unwrap().difficulty_target.get().to_le_bytes());
        hash_function.update(self.merkle_root);
        hash_function.update(self.previous_hash);
        for i in 0..N_TRANSMISSION_SIGNATURES {
            hash_function.update(self.tail.stamps[i].signature);
            hash_function.update(self.tail.stamps[i].address);
        }
        Ok(hash_function.digest().unwrap())
    }
}