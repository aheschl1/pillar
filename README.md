
<div style="display: flex; align-items: center; gap: 12px;">
  <img src="./figures/logo.svg" alt="Logo" style="height: 50px; margin: 10px">
  <h1 style="margin: 0;">Pillar</h1>
</div>

Zero-trust decentralized ledger with trust layer.

Vision:

- A two tier network where:
  - The main network handles transaction through proof of work consensus
  - A second tier of the network is a rank system for external submition for arbitrary work to be done on a specific level of trust.
- Incentive does not come through transaction fees, but rather through ranking up trust in order to be employed by paying third parties for computations

Currently in heavy development.

## Testing

Running `cargo test` will launch tests, saving logs to `./test_output/{timestamp}/output.log`.

Expect some errors from logging. This does not indicate test failure.

The flow can be rougly seen in the following image.

![Flow](./figures/net_flow.png)
