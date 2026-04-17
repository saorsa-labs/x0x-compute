use serde::{Deserialize, Serialize};

use crate::config::ComputeConfig;
use crate::error::{ComputeError, Result};

/// Snapshot of the x0x identity that x0x-compute reuses.
///
/// All identifiers are encoded as full lowercase hexadecimal so they can be
/// compared, serialized, and round-tripped without losing information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentIdentitySnapshot {
    pub machine_id: String,
    pub agent_id: String,
    pub user_id: Option<String>,
}

impl AgentIdentitySnapshot {
    /// Builds a serializable identity snapshot from an x0x agent.
    #[must_use]
    pub fn from_agent(agent: &x0x::Agent) -> Self {
        Self {
            machine_id: encode_machine_id(&agent.machine_id()),
            agent_id: encode_agent_id(&agent.agent_id()),
            user_id: agent.user_id().map(|user_id| encode_user_id(&user_id)),
        }
    }
}

/// Encodes an x0x `MachineId` as full lowercase hexadecimal.
#[must_use]
pub fn encode_machine_id(machine_id: &x0x::identity::MachineId) -> String {
    hex::encode(machine_id.0)
}

/// Encodes an x0x `AgentId` as full lowercase hexadecimal.
#[must_use]
pub fn encode_agent_id(agent_id: &x0x::identity::AgentId) -> String {
    hex::encode(agent_id.0)
}

/// Encodes an x0x `UserId` as full lowercase hexadecimal.
#[must_use]
pub fn encode_user_id(user_id: &x0x::identity::UserId) -> String {
    hex::encode(user_id.0)
}

/// Parses a full hexadecimal agent id string into an x0x `AgentId`.
pub fn parse_agent_id_hex(value: &str) -> Result<x0x::identity::AgentId> {
    parse_hex_32(value, "agent_id").map(x0x::identity::AgentId)
}

/// Parses a full hexadecimal machine id string into an x0x `MachineId`.
pub fn parse_machine_id_hex(value: &str) -> Result<x0x::identity::MachineId> {
    parse_hex_32(value, "machine_id").map(x0x::identity::MachineId)
}

fn parse_hex_32(value: &str, field: &'static str) -> Result<[u8; 32]> {
    let decoded = hex::decode(value).map_err(|error| ComputeError::InvalidIdentityEncoding {
        field,
        details: error.to_string(),
    })?;

    decoded
        .try_into()
        .map_err(|bytes: Vec<u8>| ComputeError::InvalidIdentityEncoding {
            field,
            details: format!("expected 32 bytes, got {}", bytes.len()),
        })
}

/// Builds an x0x agent using x0x-compute configuration.
pub async fn build_agent(config: &ComputeConfig) -> Result<x0x::Agent> {
    let mut builder = x0x::Agent::builder();

    if let Some(path) = config.x0x.machine_key_path.as_ref() {
        builder = builder.with_machine_key(path);
    }
    if let Some(path) = config.x0x.agent_key_path.as_ref() {
        builder = builder.with_agent_key_path(path);
    }
    if let Some(path) = config.x0x.user_key_path.as_ref() {
        builder = builder.with_user_key_path(path);
    }
    if let Some(path) = config.x0x.contact_store_path.as_ref() {
        builder = builder.with_contact_store_path(path);
    }

    let agent = builder.build().await?;
    if config.require_user_id && agent.user_id().is_none() {
        return Err(ComputeError::UserIdentityRequired);
    }

    Ok(agent)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn identity_snapshot_uses_full_hex_identifiers() {
        let machine_keypair = x0x::identity::MachineKeypair::generate().expect("machine keypair");
        let agent_keypair = x0x::identity::AgentKeypair::generate().expect("agent keypair");

        let snapshot = AgentIdentitySnapshot {
            machine_id: encode_machine_id(&machine_keypair.machine_id()),
            agent_id: encode_agent_id(&agent_keypair.agent_id()),
            user_id: None,
        };

        assert_eq!(snapshot.machine_id.len(), 64);
        assert_eq!(snapshot.agent_id.len(), 64);
    }

    #[test]
    fn parse_agent_id_round_trips_full_hex() {
        let keypair = x0x::identity::AgentKeypair::generate().expect("agent keypair");
        let original = keypair.agent_id();
        let encoded = encode_agent_id(&original);
        let parsed = parse_agent_id_hex(&encoded).expect("parse agent id");
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_machine_id_rejects_wrong_length() {
        let error = parse_machine_id_hex("abcd").expect_err("expected wrong length to fail");
        assert!(matches!(
            error,
            ComputeError::InvalidIdentityEncoding {
                field: "machine_id",
                ..
            }
        ));
    }
}
