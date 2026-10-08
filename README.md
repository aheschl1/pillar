Blockchain is a zero-trust decentralized ledger that implements a Proof of Reputation (PoR) trust layer. It combines a conventional transaction layer with a reputation-driven incentive layer to reduce wasted computation and provide a trust metric for network participants.

goto andrewheschl.ca and solve the CTF to access the public network, and receive some complimentary andrewcoins

## Repository Layout

- [`blockchain/`](blockchain/) – Entrypoint for managing a blockchain node.
- [`libs/blockchain_core/`](libs/blockchain_core/) – Core protocol logic and data structures.
- [`libs/blockchain_crypto/`](libs/blockchain_crypto/) – Cryptographic primitives, including hashing, signing, encryption, and Merkle structures.
- [`libs/blockchain_serialize/`](libs/blockchain_serialize/) – Lightweight serialization utilities.
- [`blockchain_monitor/`](blockchain_monitor/) – A web-based frontend for monitoring a node.
- [`vm_mesh/`](vm_mesh/) – A framework for distributed testing and simulation using QEMU.

## High-Level Architecture

The network data flow, chain structure, and block settlement process are illustrated in the following diagrams:

- [Network Flow](figures/net_flow.png)
- [Chain Structure](figures/structure.png)
- [Settle Chart](figures/settle_chart.png)

## Getting Started

### Prerequisites

- Docker
- Rust toolchain
- Node.js and npm

### Running a Node

The recommended method for running a blockchain node is via Docker.

1. **Build the Docker image:**

    ```bash
    ./build.sh
    ```

2. **Run the node:**

    ```bash
    # All arguments are optional. A new wallet and random name will be generated if not provided.
    # The node will listen on the next available ip on the docker subnet by default.
    ./run.sh --work-dir=<WORK_DIR> --ip-address=<IP_ADDRESS> --wkps=<WKP_SERVERS> --name=<NODE_NAME> --config=<CONFIG_FILE>
    ```

A convenience script, `./kill_all.sh`, is provided to stop all running blockchain containers.

### Testing

To run the test suite for all crates, execute:

```bash
cargo test
```

Test logs are written to `./test_output/{timestamp}/output.log`.

### Frontend Dashboard

A web-based dashboard is available for monitoring a running node.

1. **Launch a blockchain node** using the instructions above.

2. **Start the frontend application:**

    ```bash
    cd blockchain_monitor
    npm install
    npm run dev
    ```

The dashboard will be accessible in a web browser at the address provided by the development server.
