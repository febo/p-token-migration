# `p-token` migration simulator

Simple CLI to simulate `p-token` feature activation while receiving transactions. The program upgrade mechanism is defined in Agave PR [#7125](https://github.com/anza-xyz/agave/pull/7125).

## Overview

The simulation consists of starting a test validator running SPL Token and `10` "clients" submitting transfer transactions. After `10` seconds, the feature is activated and the token program is upgraded to `p-token`. At the upgrade point, some transactions are expected to fail since the program is in `DelayVisibility` mode for one slot but should quickly resume.

## How to use the CLI

First, build all necessary components:
```bash
make build
```

This will build the CLI, p-token and activator programs. After that, to start the simulation use:
```bash
make run
```

## Resouces

* `p-token` [repository](https://github.com/solana-program/token/tree/main/p-token)
* [SIMD-0266](https://github.com/solana-foundation/solana-improvement-documents/pull/266): Efficient Token program
* Agave PR [#7125](https://github.com/anza-xyz/agave/pull/7125): `p-token` feature gate
