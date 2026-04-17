use serde::{Deserialize, Serialize};
use sysinfo::System;

use crate::config::ComputeConfig;
use crate::x0x_identity::AgentIdentitySnapshot;

/// Compute capability advertisement for trusted meshes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAnnouncement {
    pub protocol_version: u32,
    pub identity: AgentIdentitySnapshot,
    pub hardware: HardwareProfile,
    pub policy: TrustedMeshPolicy,
    pub available_models: Vec<String>,
    pub observed_at_unix_secs: u64,
}

/// Local hardware profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub hostname: Option<String>,
    pub cpu_brand: String,
    pub logical_cores: usize,
    pub total_memory_bytes: u64,
    pub available_memory_bytes: u64,
}

/// Mesh policy derived from x0x-compute configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedMeshPolicy {
    pub mesh_name: String,
    pub trusted_friends_only: bool,
    pub require_trusted_contacts: bool,
    pub prefer_user_identity: bool,
}

impl CapabilityAnnouncement {
    /// Builds a local capability snapshot from config and a live x0x identity.
    #[must_use]
    pub fn local(config: &ComputeConfig, identity: AgentIdentitySnapshot) -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        let cpu_brand = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .filter(|brand| !brand.is_empty())
            .unwrap_or_else(|| "unknown".to_string());

        Self {
            protocol_version: crate::PROTOCOL_VERSION,
            identity,
            hardware: HardwareProfile {
                hostname: System::host_name(),
                cpu_brand,
                logical_cores: system.cpus().len(),
                total_memory_bytes: system.total_memory(),
                available_memory_bytes: system.available_memory(),
            },
            policy: TrustedMeshPolicy {
                mesh_name: config.mesh.name.clone(),
                trusted_friends_only: config.mesh.trusted_friends_only,
                require_trusted_contacts: config.mesh.require_trusted_contacts,
                prefer_user_identity: config.mesh.prefer_user_identity,
            },
            available_models: Vec::new(),
            observed_at_unix_secs: unix_timestamp_secs(),
        }
    }
}

#[must_use]
fn unix_timestamp_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_snapshot_carries_identity_and_policy() {
        let config = ComputeConfig::default();
        let identity = AgentIdentitySnapshot {
            machine_id: "machine-123".to_string(),
            agent_id: "agent-456".to_string(),
            user_id: Some("user-789".to_string()),
        };

        let capability = CapabilityAnnouncement::local(&config, identity.clone());

        assert_eq!(capability.identity.machine_id, identity.machine_id);
        assert_eq!(capability.identity.agent_id, identity.agent_id);
        assert!(capability.policy.trusted_friends_only);
        assert!(capability.observed_at_unix_secs > 0);
    }
}
