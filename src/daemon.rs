use axum::{extract::State, routing::get, Json, Router};

use crate::capability::CapabilityAnnouncement;
use crate::config::ComputeConfig;
use crate::error::Result;
use crate::x0x_identity::{build_agent, AgentIdentitySnapshot};

/// Running x0x-compute daemon.
pub struct ComputeDaemon {
    config: ComputeConfig,
    agent: x0x::Agent,
    identity: AgentIdentitySnapshot,
    capability: CapabilityAnnouncement,
}

#[derive(Clone)]
struct AppState {
    config: ComputeConfig,
    identity: AgentIdentitySnapshot,
    capability: CapabilityAnnouncement,
}

impl ComputeDaemon {
    /// Creates a daemon from configuration.
    pub async fn from_config(config: ComputeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        let agent = build_agent(&config).await?;
        let identity = AgentIdentitySnapshot::from_agent(&agent);
        let capability = CapabilityAnnouncement::local(&config, identity.clone());

        Ok(Self {
            config,
            agent,
            identity,
            capability,
        })
    }

    /// Runs the daemon until shutdown.
    pub async fn run(self) -> Result<()> {
        let Self {
            config,
            agent,
            identity: identity_snapshot,
            capability,
        } = self;

        if config.join_network_on_start {
            agent.join_network().await?;
            if config.announce_on_start {
                let payload = serde_json::to_vec(&capability)?;
                agent.publish(&config.capability_topic, payload).await?;
            }
        }

        let state = AppState {
            config: config.clone(),
            identity: identity_snapshot,
            capability,
        };

        let app = Router::new()
            .route("/health", get(health))
            .route("/v1/identity", get(identity))
            .route("/v1/capabilities/local", get(local_capability))
            .route("/v1/config", get(config_view))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(config.api_bind).await?;
        tracing::info!(bind = %config.api_bind, "x0x-compute daemon listening");

        let _agent = agent;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn identity(State(state): State<AppState>) -> Json<AgentIdentitySnapshot> {
    Json(state.identity)
}

async fn local_capability(State(state): State<AppState>) -> Json<CapabilityAnnouncement> {
    Json(state.capability)
}

async fn config_view(State(state): State<AppState>) -> Json<ComputeConfig> {
    Json(state.config)
}
