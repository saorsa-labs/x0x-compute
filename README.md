# x0x-compute

**Trusted-friends compute mesh built on x0x identities, trust, and gossip.**

x0x-compute is a focused companion project to [x0x](https://github.com/saorsa-labs/x0x). It is for small groups of people who already know each other and already trust each other: friends, families, labs, startups, and local teams who want to pool AI compute without introducing a public marketplace or cloud middleman.

## Why this exists

The key idea is simple:

- **x0x already solves identity and trust** for peer-to-peer agents.
- **x0x-compute reuses that directly** instead of inventing a new trust model.
- A compute mesh should know **which machine** is serving work, **which agent** is coordinating it, and ideally **which human friend** is behind it.

That maps naturally onto x0x's three-layer identity model:

- `machine_id` — the concrete hardware that actually serves compute
- `agent_id` — the portable compute agent identity
- `user_id` — the human friend behind the agent, when present

## Scope

x0x-compute is deliberately narrow.

### In scope

- trusted-friends compute sharing
- x0x-native identity and trust
- capability advertisements over x0x gossip
- small-group coordination around model serving and scheduling
- a local daemon and CLI
- future OpenAI-compatible local gateway for trusted meshes

### Out of scope for v0

- public selling or provisioning
- anonymous providers
- hardware attestation
- token economics, treasury, or billing ledgers
- stranger-to-stranger trust markets

## Architecture

x0x-compute treats **x0x as the control plane**.

- **x0x** provides identity, trust, peer discovery, and gossip coordination
- **x0x-compute** provides compute-specific policy, capability announcements, and local operator tooling
- future inference data-plane work can be added without changing the core trust model

See [`docs/architecture.md`](docs/architecture.md) for the fuller design.

## Current status

This repository starts with the focused scaffold for the trusted-friends model:

- config loading and defaults
- x0x identity integration
- local capability snapshot generation
- lightweight daemon endpoints:
  - `GET /health`
  - `GET /v1/identity`
  - `GET /v1/capabilities/local`
- operator CLI commands for config, identity, capability, and daemon startup

## Install

```bash
cargo install x0x-compute
```

Or for local development:

```bash
git clone git@github.com:saorsa-labs/x0x-compute.git
cd x0x-compute
just build
```

## Quick start

Print the default config:

```bash
x0x-compute print-config
```

Write the default config to disk:

```bash
x0x-compute print-config --write-default
```

Inspect the x0x identity this node will reuse:

```bash
x0x-compute identity
```

Inspect the local capability snapshot that can later be advertised to trusted peers:

```bash
x0x-compute capability
```

Start the local daemon:

```bash
x0x-compute start
```

Or run the daemon entrypoint directly:

```bash
x0x-computed
```

## Configuration

Default config path:

- macOS: `~/Library/Application Support/x0x-compute/config.toml`
- Linux: `~/.config/x0x-compute/config.toml`

The shipped example config is at [`examples/config.toml`](examples/config.toml).

## Development

```bash
just fmt
just lint
just test
just build
just check
```

## Roadmap

### Phase 0
- trusted-friends repo scaffold
- x0x identity reuse
- local daemon surface

### Phase 1
- x0x gossip capability advertisements
- trusted-peer filtering using x0x contacts and trust levels
- named-mesh coordination topics

### Phase 2
- runtime adapters for local model engines
- reservation and scheduling flows for trusted groups
- OpenAI-compatible local gateway

### Phase 3
- direct peer-to-peer tensor/job transport tuned for trusted groups
- mesh failover, model placement, and lightweight federation between friend groups

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))
