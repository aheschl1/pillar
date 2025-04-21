use sha3::{Digest, Sha3_256};


pub trait Hashable {
    fn hash(&self, hasher: &mut impl HashFunction) -> [u8; 32];
}

pub trait HashFunction {
    fn update(&mut self, data: impl AsRef<[u8]>);
    fn digest(&mut self) -> Result<[u8; 32], std::io::Error>;
    fn new() -> Self;
}

pub struct Sha3_256Hash{
    hasher: Sha3_256,
    n_parameters: usize,
}

impl HashFunction for Sha3_256Hash {

    fn new() -> Self {
        Sha3_256Hash {
            hasher: Sha3_256::new(),
            n_parameters: 0,
        }
    }

    fn update(&mut self, data: impl AsRef<[u8]>){
        self.hasher.update(data);
        self.n_parameters += 1;
    }

    fn digest(&mut self)-> Result<[u8; 32], std::io::Error> {
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

// clonable
impl Clone for Sha3_256Hash {
    fn clone(&self) -> Self {
        Sha3_256Hash {
            hasher: self.hasher.clone(),
            n_parameters: self.n_parameters,
        }
    }
}
