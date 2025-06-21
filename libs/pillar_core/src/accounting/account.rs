use std::cmp::Ordering;

use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionStub{
    // The block hash of the block that created this transaction
    pub block_hash: StdByteArray,
    // The transaction hash of the transaction that created this account
    pub transaction_hash: StdByteArray,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account{
    // The address of the account is the public key
    pub address: StdByteArray,
    // The balance of the account
    pub balance: u64,
    // The nonce of the account, to prevent replay attacks
    pub nonce: u64,
    // the local copy of the next nonce to send
    pub local_nonce: u64,
    // a tracking of blocks/transactions that lead to this balance
    pub history: Vec<TransactionStub>, // (block hash, transaction hash)
}

impl Account{
    // Creates a new account with the given address and balance
    pub fn new(address: StdByteArray, balance: u64) -> Self {
        // for now, this placeholder will work; however, in the long run we need a coinbase account for initial distribution
        // TODO deal with coinbase
        let history = match balance.cmp(&0){
            Ordering::Equal => vec![],
            Ordering::Greater => vec![
                TransactionStub{
                    transaction_hash: [0; 32],
                    block_hash: [0; 32]
                }
            ],
            _ => panic!()
        };
        Account {
            address,
            balance,
            nonce: 0,
            local_nonce: 0,
            history
        }
    }
}