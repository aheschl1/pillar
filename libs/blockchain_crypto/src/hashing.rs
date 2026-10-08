//! Hashing traits and a default SHA3-256 implementation.
//!
//! The `Hashable` trait abstracts how a type contributes bytes to a hash
//! function. The `HashFunction` trait exposes a minimal update/finalize API
//! to keep implementations simple and easily swappable in tests.

use sha3::{Digest, Sha3_256};

use crate::types::StdByteArray;


/// A trait for objects that can be hashed using a hash function.
///
/// Implementors should call `hasher.update(...)` for each field to include
/// and then return `hasher.digest()`.
pub trait Hashable {
    /// Computes the hash of the object using the provided hash function.
    ///
    /// # Arguments
    ///
    /// * `hasher` - A mutable instance of a type implementing the `HashFunction` trait.
    ///
    /// # Returns
    ///
    /// * `Ok(StdByteArray)` containing the hash of the object.
    /// * `Err(std::io::Error)` if hashing fails.
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error>;
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
    /// * `Ok(StdByteArray)` containing the hash digest.
    /// * `Err(std::io::Error)` if no data was added before finalizing.
    fn digest(&mut self) -> Result<StdByteArray, std::io::Error>;
}

/// A struct implementing the SHA3-256 hash function.
///
/// This wrapper tracks whether any data was provided before finalizing. Calling
/// `digest` without prior `update` returns `std::io::ErrorKind::InvalidInput`.
pub struct DefaultHash {
    /// The internal SHA3-256 hasher.
    hasher: Sha3_256,
    /// The number of parameters added to the hasher.
    n_parameters: usize,
}

impl Default for DefaultHash {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultHash{
    /// Creates a new instance of the SHA3-256 hash function.
    pub fn new() -> Self {
        DefaultHash {
            hasher: Sha3_256::new(),
            n_parameters: 0,
        }
    }
}

impl HashFunction for DefaultHash {

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
    /// * `Ok(StdByteArray)` containing the hash digest.
    /// * `Err(std::io::Error)` if no data was added before finalizing.
    fn digest(&mut self) -> Result<StdByteArray, std::io::Error> {
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


mod implementations{
    use crate::{hashing::Hashable, types::StdByteArray};

    impl Hashable for &str{
        fn hash(&self, hasher: &mut impl super::HashFunction) -> Result<StdByteArray, std::io::Error> {
            hasher.update(self.as_bytes());
            hasher.digest()
        }
    }

    impl Hashable for String {
        fn hash(&self, hasher: &mut impl super::HashFunction) -> Result<StdByteArray, std::io::Error> {
            hasher.update(self.as_bytes());
            hasher.digest()
        }
        
    }

    impl Hashable for StdByteArray {
        fn hash(&self, hasher: &mut impl super::HashFunction) -> Result<StdByteArray, std::io::Error> {
            hasher.update(self.as_ref());
            hasher.digest()
        }
    }

    impl Hashable for Vec<u8> {
        fn hash(&self, hasher: &mut impl super::HashFunction) -> Result<StdByteArray, std::io::Error> {
            hasher.update(self.as_slice());
            hasher.digest()
        }
    }
}