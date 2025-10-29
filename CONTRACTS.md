# Contracts Proposal

Smart contracts allow for arbitrary code execution, with state managed by the blockchain. These contracts have their state validated by constructing a Merkle tree over the state, which impacts the global state root indirectly.

Accounts will be one of two types:

1. **Externally Owned Accounts (EOAs)**: Controlled by private keys, these accounts can send transactions and hold funds.
2. **Contract Accounts**: These accounts contain code that is executed when they receive transactions.
    2.1: Gas fees are paid to the executor of the contract code - the first one to win the block reward.
    2.2: Money can be paid into a contract account, the quantity of which is passed to the contract code when executed.
    2.3: Contracts can transfer native tokens to other accounts, by outputting transactions.
    2.4: Contracts are identified by their address, which is the bytecode hash.

## Contract Creation

Contracts are created by sending a transaction to a special address with the contract bytecode in the data field. Nodes will then recognize the hash of the bytecode as a contract address.

## Managing Contract State

Contract state is managed through a Merkle tree structure, where each contract has its own subtree. The root of this subtree is included in the global state root, ensuring that any changes to the contract state are reflected in the overall blockchain state.

When a contract is executed, it can read from and write to its own state subtree. Any changes made during execution are recorded and the new state root is computed and updated in the global state root.

The contract should be able to interact over a key value interface, in which each key maps to a value. The contract can read, write, and create new key-value pairs in its state. It cannot delete keys, but it can overwrite existing values. Values are stored as arbitrary byte arrays.