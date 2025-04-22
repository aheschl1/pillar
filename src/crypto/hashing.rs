use sha3::{Digest, Sha3_256};

/// A trait for objects that can be hashed using a hash function.
pub trait Hashable {
    /// Computes the hash of the object using the provided hash function.
    ///
    /// # Arguments
    ///
    /// * `hasher` - A mutable instance of a type implementing the `HashFunction` trait.
    ///
    /// # Returns
    ///
    /// * `Ok([u8; 32])` containing the hash of the object.
    /// * `Err(std::io::Error)` if hashing fails.
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<[u8; 32], std::io::Error>;
}

/// A trait for hash functions that support updating with data and producing a digest.
pub trait HashFunction {
    /// Updates the hash function with the given data.
    ///
    /// # Arguments
    ///
    /// * `data` - The data to be hashed.
    fn update(&mut self, data: impl AsRef<[u8]>);

    /// Finalizes the hash computation and returns the digest.
    ///
    /// # Returns
    ///
    /// * `Ok([u8; 32])` containing the hash digest.
    /// * `Err(std::io::Error)` if no data was added before finalizing.
    fn digest(&mut self) -> Result<[u8; 32], std::io::Error>;

    /// Creates a new instance of the hash function.
    fn new() -> Self;
}

/// A struct implementing the SHA3-256 hash function.
pub struct DefaultHash {
    /// The internal SHA3-256 hasher.
    hasher: Sha3_256,
    /// The number of parameters added to the hasher.
    n_parameters: usize,
}

impl HashFunction for DefaultHash {
    /// Creates a new instance of the SHA3-256 hash function.
    fn new() -> Self {
        DefaultHash {
            hasher: Sha3_256::new(),
            n_parameters: 0,
        }
    }

    /// Updates the SHA3-256 hasher with the given data.
    ///
    /// # Arguments
    ///
    /// * `data` - The data to be hashed.
    fn update(&mut self, data: impl AsRef<[u8]>) {
        self.hasher.update(data);
        self.n_parameters += 1;
    }

    /// Finalizes the hash computation and returns the digest.
    ///
    /// # Returns
    ///
    /// * `Ok([u8; 32])` containing the hash digest.
    /// * `Err(std::io::Error)` if no data was added before finalizing.
    fn digest(&mut self) -> Result<[u8; 32], std::io::Error> {
        if self.n_parameters == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No data has been added to the hasher",
            ));
        }
        let result = Ok(self.hasher.clone().finalize().into());
        self.hasher.reset();
        self.n_parameters = 0;
        result
    }
}

/// Allows cloning of the SHA3-256 hash function.
impl Clone for DefaultHash {
    fn clone(&self) -> Self {
        DefaultHash {
            hasher: self.hasher.clone(),
            n_parameters: self.n_parameters,
        }
    }
}
