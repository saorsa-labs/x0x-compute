# x0x-compute architecture

## Design goal

x0x-compute exists to let a **trusted group of friends** pool AI compute while staying grounded in x0x's identity and trust model.

This project is explicitly **not** trying to solve the open public marketplace problem.

## Core stance

Use x0x as the substrate for:

- peer identity
- trust relationships
- peer discovery
- group membership
- coordination gossip

Do **not** add, for v0:

- public selling or provisioning
- attestation systems
- treasury logic
- token economics
- stranger-to-stranger trust

## Identity model

x0x-compute inherits x0x's three-layer identity model directly.

### `machine_id`

Represents the actual hardware node serving compute.

Use it for:
- hardware inventory
- placement history
- machine-specific allowlists
- tracking which physical host ran a job

### `agent_id`

Represents the portable compute agent.

Use it for:
- logical worker identity
- compute-side policy
- portable mesh membership
- friend-to-friend coordination

### `user_id`

Represents the human friend behind the agent when present.

Use it for:
- grouping multiple machines under one person
- higher-trust social policies
- friend identity in a family/team/lab mesh

`user_id` remains optional because x0x never auto-generates it. x0x-compute should treat it as a strong signal when present, not a hard dependency in every deployment.

## Trust model

The default trust policy for x0x-compute is:

- **trusted friends only**
- no anonymous workers
- no open admission
- no public market routing

Practical implications:

1. Mesh membership should come from x0x contact trust and/or explicit invites.
2. `machine_id` allows a friend's trusted agent to still be tied to a known host.
3. `user_id`, when present, gives the richest social trust anchor.
4. Unknown peers should never be scheduled for compute by default.

## Control plane vs data plane

### Control plane: x0x

x0x is the right layer for:

- announcing capabilities
- advertising availability
- mesh membership changes
- friend-to-friend coordination
- scheduling intents
- reservation negotiation
- model metadata and policies

### Data plane: future x0x-compute runtime

The actual inference/job transport will likely need stricter semantics than gossip alone.

That means x0x-compute should keep room for a dedicated data-plane transport for:

- long-lived streaming
- backpressure-sensitive payloads
- structured request/response flows
- model shard or job payload movement

But the trust and coordination story should still remain x0x-native.

## Phase 1: trusted capability gossip

Phase 1 keeps the feature set intentionally narrow:

- subscribe to `x0x.compute.capabilities.v1`
- only accept announcements whose signed x0x sender matches the advertised `agent_id`
- evaluate trust with x0x's `TrustEvaluator` using both `agent_id` and `machine_id`
- respect machine pinning from the x0x contact store
- keep an in-memory registry of accepted peers

This means x0x-compute does not merely trust a claimed agent string. It trusts the signed x0x sender identity and then cross-checks the advertised machine against x0x's trust model.

## Initial local daemon

The first daemon surface is intentionally small but Phase 1 adds the trusted peer view:

- `GET /health`
- `GET /v1/identity`
- `GET /v1/capabilities/local`
- `GET /v1/capabilities/peers`
- `GET /v1/config`

This gives operators a stable local surface while the x0x-backed coordination layer evolves.

## Capability topic

Initial reserved topic:

- `x0x.compute.capabilities.v1`

This topic is for signed capability advertisements that describe:

- machine identity
- agent identity
- optional user identity
- local hardware profile
- available model/runtime slots
- trust policy

## Non-goals

The following are intentionally excluded from the first design:

- public compute market design
- public API key minting and budget systems
- hardware attestation chains
- remote billing and settlement
- cloud-provider routing as a first-class requirement

Those may be explored in separate systems later, but they should not distort x0x-compute's initial focus.
