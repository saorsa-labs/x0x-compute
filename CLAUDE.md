# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this repository.

## What is x0x-compute

x0x-compute is a trusted-friends compute mesh built on top of x0x. It treats x0x as the identity, trust, discovery, and coordination substrate for small groups of known people who want to pool local AI capacity.

The project is intentionally narrow:
- trusted friends only
- leverage `machine_id`, `agent_id`, and optional `user_id`
- local-first and peer-to-peer
- no public marketplace, no public attestation layer, no token system

## Build & Test Commands

Use the justfile first:

```bash
just --list
just fmt
just lint
just test
just build
just quick-check
just check
```

Fallback commands:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings -D clippy::panic -D clippy::unwrap_used -D clippy::expect_used
cargo nextest run --all-features
cargo build --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
```

## Architecture

### Current crate layout

- `src/config.rs` — local config loading/writing and defaults
- `src/x0x_identity.rs` — x0x agent construction, canonical hex ids, and identity snapshots
- `src/capability.rs` — local capability announcements derived from x0x identity + machine profile
- `src/mesh.rs` — trusted capability filtering, peer registry, and x0x gossip subscription loop
- `src/runtime.rs` — runtime adapter trait, local inventory, reservations, and OpenAI-compatible skeleton types
- `src/daemon.rs` — local daemon surface for health, identity, inventory, reservations, trusted peer views, and OpenAI-compatible endpoints
- `src/bin/x0x-compute.rs` — operator CLI
- `src/bin/x0x-computed.rs` — daemon entrypoint

### Design rule

Use x0x for:
- trust
- identity
- gossip coordination
- friend membership
- peer discovery

Do not turn x0x-compute into:
- a public inference marketplace
- a billing ledger
- an attestation framework

## Ports

- Local API default: `127.0.0.1:12800`
- Reserve `12800-12899` for x0x-compute development

## Standards

- Zero warnings
- No `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!()` in production code
- Tests may use `expect`/`unwrap` if needed
- Keep the scope tightly aligned with the trusted-friends model
