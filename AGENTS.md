# AGENTS.md

x0x-compute is a focused companion project to x0x.

## Core intent

- Build a **trusted-friends compute mesh** on top of x0x.
- Reuse x0x identities and trust directly:
  - `machine_id` identifies the hardware actually serving work
  - `agent_id` identifies the portable compute agent
  - `user_id` identifies the human friend when present
- Prefer simple, explicit trust over public marketplaces or attestation systems.

## Non-goals for v0

- No public compute selling
- No anonymous provisioning marketplace
- No hardware attestation or stranger-to-stranger trust model
- No token economics or treasury logic

See `CLAUDE.md` for build commands and repo-specific guidance.
