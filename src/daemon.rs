use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use tokio::sync::RwLock;

use crate::capability::CapabilityAnnouncement;
use crate::config::ComputeConfig;
use crate::error::Result;
use crate::mesh::{
    announce_local_capability, start_capability_subscription_loop, PeerCapabilityRecord,
    TrustedPeerRegistry,
};
use crate::runtime::{
    build_runtime, CreateReservationRequest, LocalModelInventory, ModelReservation,
    OpenAiChatCompletionRequest, OpenAiChatCompletionResponse, OpenAiModelListResponse,
    RuntimeAdapter,
};
use crate::x0x_identity::{build_agent, AgentIdentitySnapshot};

/// Running x0x-compute daemon.
pub struct ComputeDaemon {
    config: ComputeConfig,
    agent: x0x::Agent,
    identity: AgentIdentitySnapshot,
    capability: CapabilityAnnouncement,
    peers: Arc<RwLock<TrustedPeerRegistry>>,
    runtime: Arc<dyn RuntimeAdapter>,
}

#[derive(Clone)]
struct AppState {
    config: ComputeConfig,
    identity: AgentIdentitySnapshot,
    capability: CapabilityAnnouncement,
    peers: Arc<RwLock<TrustedPeerRegistry>>,
    runtime: Arc<dyn RuntimeAdapter>,
}

impl ComputeDaemon {
    /// Creates a daemon from configuration.
    pub async fn from_config(config: ComputeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        let agent = build_agent(&config).await?;
        let identity = AgentIdentitySnapshot::from_agent(&agent);
        let capability = CapabilityAnnouncement::local(&config, identity.clone());
        let runtime = build_runtime(&config);

        Ok(Self {
            config,
            agent,
            identity,
            capability,
            peers: Arc::new(RwLock::new(TrustedPeerRegistry::new())),
            runtime,
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
            runtime,
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
            runtime,
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
        .route("/v1/models/local", get(local_models))
        .route(
            "/v1/reservations",
            get(list_reservations).post(create_reservation),
        )
        .route("/v1/reservations/:id", delete(delete_reservation))
        .route("/v1/openai/models", get(openai_models))
        .route("/v1/openai/chat/completions", post(openai_chat_completions))
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

async fn local_models(State(state): State<AppState>) -> Json<Vec<LocalModelInventory>> {
    Json(state.runtime.local_models())
}

async fn list_reservations(State(state): State<AppState>) -> Json<Vec<ModelReservation>> {
    Json(state.runtime.reservations())
}

async fn create_reservation(
    State(state): State<AppState>,
    Json(request): Json<CreateReservationRequest>,
) -> Result<(StatusCode, Json<ModelReservation>)> {
    let reservation = state.runtime.create_reservation(request)?;
    Ok((StatusCode::CREATED, Json(reservation)))
}

async fn delete_reservation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    state.runtime.release_reservation(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn openai_models(State(state): State<AppState>) -> Json<OpenAiModelListResponse> {
    Json(state.runtime.openai_models())
}

async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(request): Json<OpenAiChatCompletionRequest>,
) -> Result<Json<OpenAiChatCompletionResponse>> {
    let response = state.runtime.chat_completion(request)?;
    Ok(Json(response))
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
    use crate::config::{MeshConfig, ModelConfig, RuntimeConfig, X0xConfig};

    fn repeated_hex(ch: char) -> String {
        std::iter::repeat_n(ch, 64).collect()
    }

    fn test_config() -> ComputeConfig {
        ComputeConfig {
            api_bind: "127.0.0.1:12800".parse().expect("parse socket addr"),
            data_dir: std::path::PathBuf::from("./var/test"),
            capability_topic: crate::CAPABILITY_TOPIC.to_string(),
            join_network_on_start: false,
            announce_on_start: false,
            require_user_id: false,
            resource_announce_interval_secs: 60,
            mesh: MeshConfig::default(),
            runtime: RuntimeConfig::default(),
            models: vec![
                ModelConfig {
                    id: "gemma-4:27b".to_string(),
                    family: "gemma-4".to_string(),
                    context_window_tokens: 65_536,
                    max_output_tokens: 4_096,
                    max_parallel_reservations: 1,
                    tags: vec!["fast".to_string()],
                },
                ModelConfig {
                    id: "qwen3.5:32b".to_string(),
                    family: "qwen3.5".to_string(),
                    context_window_tokens: 131_072,
                    max_output_tokens: 8_192,
                    max_parallel_reservations: 2,
                    tags: vec!["coding".to_string(), "reasoning".to_string()],
                },
            ],
            x0x: X0xConfig::default(),
        }
    }

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

    fn test_state() -> AppState {
        let config = test_config();
        AppState {
            identity: AgentIdentitySnapshot {
                machine_id: repeated_hex('a'),
                agent_id: repeated_hex('b'),
                user_id: None,
            },
            capability: CapabilityAnnouncement::local(
                &config,
                AgentIdentitySnapshot {
                    machine_id: repeated_hex('a'),
                    agent_id: repeated_hex('b'),
                    user_id: None,
                },
            ),
            peers: Arc::new(RwLock::new(TrustedPeerRegistry::new())),
            runtime: build_runtime(&config),
            config,
        }
    }

    #[tokio::test]
    async fn peers_endpoint_returns_registry_snapshot() {
        let agent_id = repeated_hex('c');
        let machine_id = repeated_hex('d');
        let capability = sample_capability(&agent_id, &machine_id);
        let mut registry = TrustedPeerRegistry::new();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let mut store = x0x::contacts::ContactStore::new(tempdir.path().join("contacts.json"));
        let parsed_agent = crate::parse_agent_id_hex(&agent_id).expect("parse agent id");
        store.set_trust(&parsed_agent, x0x::contacts::TrustLevel::Trusted);
        registry.apply_message(
            &test_config(),
            &repeated_hex('f'),
            &store,
            &x0x::PubSubMessage {
                topic: crate::CAPABILITY_TOPIC.to_string(),
                payload: bytes::Bytes::from(
                    serde_json::to_vec(&capability).expect("serialize capability"),
                ),
                sender: Some(parsed_agent),
                sender_public_key: None,
                verified: true,
                trust_level: None,
            },
        );

        let mut state = test_state();
        state.peers = Arc::new(RwLock::new(registry));
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
    async fn local_models_endpoint_returns_inventory() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models/local")
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
        let models: serde_json::Value = serde_json::from_slice(&body).expect("deserialize models");
        let model_list = models.as_array().expect("model list");
        assert_eq!(model_list.len(), 2);
        assert_eq!(model_list[0]["id"], "gemma-4:27b");
        assert_eq!(model_list[1]["available_slots"], 2);
    }

    #[tokio::test]
    async fn reservation_endpoints_create_list_and_delete() {
        let app = build_router(test_state());
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/reservations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "model": "qwen3.5:32b",
                            "consumer": "alice",
                            "requested_slots": 1
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(create_response.status(), StatusCode::CREATED);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let reservation: serde_json::Value =
            serde_json::from_slice(&create_body).expect("deserialize reservation");
        let reservation_id = reservation["reservation_id"]
            .as_str()
            .expect("reservation id")
            .to_string();

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/reservations")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let list_body = list_response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let reservations: serde_json::Value =
            serde_json::from_slice(&list_body).expect("deserialize reservations");
        assert_eq!(reservations.as_array().expect("reservation list").len(), 1);

        let delete_response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/reservations/{reservation_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn reservation_endpoint_rejects_capacity_conflict() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/reservations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "model": "gemma-4:27b",
                            "consumer": "alice",
                            "requested_slots": 2
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn openai_gateway_endpoints_return_skeleton_data() {
        let app = build_router(test_state());

        let models_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/openai/models")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(models_response.status(), StatusCode::OK);

        let chat_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/openai/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "model": "qwen3.5:32b",
                            "messages": [
                                {"role": "user", "content": "hello mesh"}
                            ]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(chat_response.status(), StatusCode::OK);
        let chat_body = chat_response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let completion: serde_json::Value =
            serde_json::from_slice(&chat_body).expect("deserialize completion");
        assert_eq!(completion["object"], "chat.completion");
        assert!(completion["choices"][0]["message"]["content"]
            .as_str()
            .expect("message content")
            .contains("hello mesh"));
    }

    #[tokio::test]
    async fn openai_gateway_reports_unsupported_streaming() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/openai/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "model": "qwen3.5:32b",
                            "stream": true,
                            "messages": [
                                {"role": "user", "content": "hello mesh"}
                            ]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn reservation_delete_reports_not_found() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/reservations/resv-999999")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn health_endpoint_reports_ok() {
        let app = build_router(test_state());
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
