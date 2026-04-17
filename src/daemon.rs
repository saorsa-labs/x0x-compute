use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use tokio::sync::RwLock;

use crate::capability::CapabilityAnnouncement;
use crate::config::ComputeConfig;
use crate::error::Result;
use crate::mesh::{
    announce_local_capability, start_capability_subscription_loop, PeerCapabilityRecord,
    TrustedPeerRegistry,
};
use crate::x0x_identity::{build_agent, AgentIdentitySnapshot};

/// Running x0x-compute daemon.
pub struct ComputeDaemon {
    config: ComputeConfig,
    agent: x0x::Agent,
    identity: AgentIdentitySnapshot,
    capability: CapabilityAnnouncement,
    peers: Arc<RwLock<TrustedPeerRegistry>>,
}

#[derive(Clone)]
struct AppState {
    config: ComputeConfig,
    identity: AgentIdentitySnapshot,
    capability: CapabilityAnnouncement,
    peers: Arc<RwLock<TrustedPeerRegistry>>,
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
            peers: Arc::new(RwLock::new(TrustedPeerRegistry::new())),
        })
    }

    /// Runs the daemon until shutdown.
    pub async fn run(self) -> Result<()> {
        let Self {
            config,
            agent,
            identity,
            capability,
            peers,
        } = self;

        let subscription_task = if config.join_network_on_start {
            agent.join_network().await?;
            let handle = start_capability_subscription_loop(
                &agent,
                config.clone(),
                identity.agent_id.clone(),
                peers.clone(),
            )
            .await?;

            if config.announce_on_start {
                let _ = announce_local_capability(&agent, &config, &identity).await?;
            }

            Some(handle)
        } else {
            None
        };

        let state = AppState {
            config: config.clone(),
            identity,
            capability,
            peers,
        };
        let app = build_router(state);

        let listener = tokio::net::TcpListener::bind(config.api_bind).await?;
        tracing::info!(bind = %config.api_bind, "x0x-compute daemon listening");

        let _agent = agent;
        let _subscription_task = subscription_task;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/identity", get(identity_handler))
        .route("/v1/capabilities/local", get(local_capability))
        .route("/v1/capabilities/peers", get(peer_capabilities))
        .route("/v1/config", get(config_view))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn identity_handler(State(state): State<AppState>) -> Json<AgentIdentitySnapshot> {
    Json(state.identity)
}

async fn local_capability(State(state): State<AppState>) -> Json<CapabilityAnnouncement> {
    Json(state.capability)
}

async fn peer_capabilities(State(state): State<AppState>) -> Json<Vec<PeerCapabilityRecord>> {
    let peers = state.peers.read().await;
    Json(peers.snapshot())
}

async fn config_view(State(state): State<AppState>) -> Json<ComputeConfig> {
    Json(state.config)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::capability::{HardwareProfile, TrustedMeshPolicy};

    fn sample_capability(agent_id: &str, machine_id: &str) -> CapabilityAnnouncement {
        CapabilityAnnouncement {
            protocol_version: crate::PROTOCOL_VERSION,
            identity: AgentIdentitySnapshot {
                machine_id: machine_id.to_string(),
                agent_id: agent_id.to_string(),
                user_id: None,
            },
            hardware: HardwareProfile {
                hostname: Some("friendbox".to_string()),
                cpu_brand: "Apple M4".to_string(),
                logical_cores: 10,
                total_memory_bytes: 1024,
                available_memory_bytes: 512,
            },
            policy: TrustedMeshPolicy {
                mesh_name: "friends".to_string(),
                trusted_friends_only: true,
                require_trusted_contacts: true,
                prefer_user_identity: true,
            },
            available_models: vec!["qwen3.5:32b".to_string()],
            observed_at_unix_secs: 1,
        }
    }

    #[tokio::test]
    async fn peers_endpoint_returns_registry_snapshot() {
        let agent_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let machine_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let capability = sample_capability(agent_id, machine_id);
        let mut registry = TrustedPeerRegistry::new();
        registry.apply_message(
            &ComputeConfig::default(),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            &{
                let mut store = x0x::contacts::ContactStore::new(std::path::PathBuf::from(
                    "/tmp/non-persistent-contacts.json",
                ));
                let parsed_agent = crate::parse_agent_id_hex(agent_id).expect("parse agent id");
                store.set_trust(&parsed_agent, x0x::contacts::TrustLevel::Trusted);
                store
            },
            &x0x::PubSubMessage {
                topic: crate::CAPABILITY_TOPIC.to_string(),
                payload: bytes::Bytes::from(
                    serde_json::to_vec(&capability).expect("serialize capability"),
                ),
                sender: Some(crate::parse_agent_id_hex(agent_id).expect("parse sender")),
                sender_public_key: None,
                verified: true,
                trust_level: None,
            },
        );

        let state = AppState {
            config: ComputeConfig::default(),
            identity: AgentIdentitySnapshot {
                machine_id: "local-machine".to_string(),
                agent_id: "local-agent".to_string(),
                user_id: None,
            },
            capability: sample_capability("local-agent", "local-machine"),
            peers: Arc::new(RwLock::new(registry)),
        };
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/capabilities/peers")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let peers: serde_json::Value = serde_json::from_slice(&body).expect("deserialize peers");
        let peer_list = peers.as_array().expect("peer list");
        assert_eq!(peer_list.len(), 1);
        assert_eq!(peer_list[0]["capability"]["identity"]["agent_id"], agent_id);
        assert_eq!(peer_list[0]["trust_decision"], "accept");
    }

    #[tokio::test]
    async fn health_endpoint_reports_ok() {
        let state = AppState {
            config: ComputeConfig::default(),
            identity: AgentIdentitySnapshot {
                machine_id: "local-machine".to_string(),
                agent_id: "local-agent".to_string(),
                user_id: None,
            },
            capability: sample_capability("local-agent", "local-machine"),
            peers: Arc::new(RwLock::new(TrustedPeerRegistry::new())),
        };
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
