use serde::{Deserialize, Serialize};

use crate::config::ComputeConfig;
use crate::error::{ComputeError, Result};

/// Snapshot of the x0x identity that x0x-compute reuses.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            machine_id: agent.machine_id().to_string(),
            agent_id: agent.agent_id().to_string(),
            user_id: agent.user_id().map(|user_id| user_id.to_string()),
        }
    }
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

    let agent = builder.build().await?;
    if config.require_user_id && agent.user_id().is_none() {
        return Err(ComputeError::UserIdentityRequired);
    }

    Ok(agent)
}
