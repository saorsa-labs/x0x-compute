use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::config::{ComputeConfig, ModelConfig, RuntimeBackend};
use crate::error::{ComputeError, Result};

/// Inventory record for a local model runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalModelInventory {
    pub id: String,
    pub family: String,
    pub runtime_backend: RuntimeBackend,
    pub context_window_tokens: u32,
    pub max_output_tokens: u32,
    pub total_slots: u16,
    pub reserved_slots: u16,
    pub available_slots: u16,
    pub tags: Vec<String>,
}

/// Reservation creation request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateReservationRequest {
    pub model: String,
    pub consumer: String,
    #[serde(default = "default_requested_slots")]
    pub requested_slots: u16,
}

/// Active reservation record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelReservation {
    pub reservation_id: String,
    pub model: String,
    pub consumer: String,
    pub requested_slots: u16,
    pub granted_at_unix_secs: u64,
}

/// Minimal OpenAI-compatible `/models` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiModelListResponse {
    pub object: String,
    pub data: Vec<OpenAiModelCard>,
}

/// Minimal OpenAI-compatible model card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiModelCard {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

/// Minimal OpenAI-compatible chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiChatCompletionRequest {
    pub model: String,
    pub messages: Vec<OpenAiChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

/// Minimal OpenAI-compatible chat message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiChatMessage {
    pub role: String,
    pub content: String,
}

/// Minimal OpenAI-compatible chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChatChoice>,
    pub usage: OpenAiUsage,
}

/// Single OpenAI-compatible choice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiChatChoice {
    pub index: u32,
    pub message: OpenAiChatMessage,
    pub finish_reason: String,
}

/// Minimal OpenAI-compatible usage block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Runtime adapter trait for local model inventory and gateway integration.
pub trait RuntimeAdapter: Send + Sync {
    fn local_models(&self) -> Vec<LocalModelInventory>;
    fn create_reservation(&self, request: CreateReservationRequest) -> Result<ModelReservation>;
    fn reservations(&self) -> Vec<ModelReservation>;
    fn release_reservation(&self, reservation_id: &str) -> Result<()>;
    fn openai_models(&self) -> OpenAiModelListResponse;
    fn chat_completion(
        &self,
        request: OpenAiChatCompletionRequest,
    ) -> Result<OpenAiChatCompletionResponse>;
}

/// Builds the configured runtime adapter.
#[must_use]
pub fn build_runtime(config: &ComputeConfig) -> Arc<dyn RuntimeAdapter> {
    Arc::new(SkeletonRuntimeAdapter::new(config))
}

/// Deterministic local runtime used for Phase 2a wiring and tests.
#[derive(Debug)]
pub struct SkeletonRuntimeAdapter {
    backend: RuntimeBackend,
    response_prefix: String,
    models: BTreeMap<String, ModelConfig>,
    state: Mutex<SkeletonRuntimeState>,
}

#[derive(Debug, Default)]
struct SkeletonRuntimeState {
    next_reservation_id: u64,
    next_completion_id: u64,
    reservations: BTreeMap<String, ModelReservation>,
}

impl SkeletonRuntimeAdapter {
    /// Creates a new skeleton runtime from configuration.
    #[must_use]
    pub fn new(config: &ComputeConfig) -> Self {
        let models = config
            .models
            .iter()
            .cloned()
            .map(|model| (model.id.clone(), model))
            .collect();

        Self {
            backend: config.runtime.backend,
            response_prefix: config.runtime.skeleton_response_prefix.clone(),
            models,
            state: Mutex::new(SkeletonRuntimeState::default()),
        }
    }

    fn inventory_for_model(&self, model: &ModelConfig, reserved_slots: u16) -> LocalModelInventory {
        let available_slots = model
            .max_parallel_reservations
            .saturating_sub(reserved_slots);

        LocalModelInventory {
            id: model.id.clone(),
            family: model.family.clone(),
            runtime_backend: self.backend,
            context_window_tokens: model.context_window_tokens,
            max_output_tokens: model.max_output_tokens,
            total_slots: model.max_parallel_reservations,
            reserved_slots,
            available_slots,
            tags: model.tags.clone(),
        }
    }

    fn reserved_slots_for_model(state: &SkeletonRuntimeState, model: &str) -> u16 {
        state
            .reservations
            .values()
            .filter(|reservation| reservation.model == model)
            .fold(0u16, |acc, reservation| {
                acc.saturating_add(reservation.requested_slots)
            })
    }

    fn ensure_model_exists(&self, model: &str) -> Result<&ModelConfig> {
        self.models
            .get(model)
            .ok_or_else(|| ComputeError::ModelNotFound(model.to_string()))
    }

    fn lock_state(&self) -> MutexGuard<'_, SkeletonRuntimeState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl RuntimeAdapter for SkeletonRuntimeAdapter {
    fn local_models(&self) -> Vec<LocalModelInventory> {
        let state = self.lock_state();
        self.models
            .values()
            .map(|model| {
                let reserved_slots = Self::reserved_slots_for_model(&state, &model.id);
                self.inventory_for_model(model, reserved_slots)
            })
            .collect()
    }

    fn create_reservation(&self, request: CreateReservationRequest) -> Result<ModelReservation> {
        if request.consumer.trim().is_empty() {
            return Err(ComputeError::InvalidReservationRequest(
                "consumer must not be empty".to_string(),
            ));
        }
        if request.requested_slots == 0 {
            return Err(ComputeError::InvalidReservationRequest(
                "requested_slots must be at least 1".to_string(),
            ));
        }

        let model = self.ensure_model_exists(&request.model)?;
        let mut state = self.lock_state();
        let reserved_slots = Self::reserved_slots_for_model(&state, &model.id);
        let available_slots = model
            .max_parallel_reservations
            .saturating_sub(reserved_slots);
        if request.requested_slots > available_slots {
            return Err(ComputeError::ModelCapacityExceeded {
                model: model.id.clone(),
                requested_slots: request.requested_slots,
                available_slots,
            });
        }

        state.next_reservation_id = state.next_reservation_id.saturating_add(1);
        let reservation = ModelReservation {
            reservation_id: format!("resv-{:06}", state.next_reservation_id),
            model: model.id.clone(),
            consumer: request.consumer,
            requested_slots: request.requested_slots,
            granted_at_unix_secs: unix_timestamp_secs(),
        };
        state
            .reservations
            .insert(reservation.reservation_id.clone(), reservation.clone());
        Ok(reservation)
    }

    fn reservations(&self) -> Vec<ModelReservation> {
        let state = self.lock_state();
        state.reservations.values().cloned().collect()
    }

    fn release_reservation(&self, reservation_id: &str) -> Result<()> {
        let mut state = self.lock_state();
        if state.reservations.remove(reservation_id).is_some() {
            Ok(())
        } else {
            Err(ComputeError::ReservationNotFound(
                reservation_id.to_string(),
            ))
        }
    }

    fn openai_models(&self) -> OpenAiModelListResponse {
        OpenAiModelListResponse {
            object: "list".to_string(),
            data: self
                .local_models()
                .into_iter()
                .map(|model| OpenAiModelCard {
                    id: model.id,
                    object: "model".to_string(),
                    created: 0,
                    owned_by: "x0x-compute".to_string(),
                })
                .collect(),
        }
    }

    fn chat_completion(
        &self,
        request: OpenAiChatCompletionRequest,
    ) -> Result<OpenAiChatCompletionResponse> {
        let model = self.ensure_model_exists(&request.model)?;
        if request.stream {
            return Err(ComputeError::UnsupportedFeature(
                "streaming chat completions are not implemented yet".to_string(),
            ));
        }
        if request.messages.is_empty() {
            return Err(ComputeError::InvalidChatRequest(
                "messages must not be empty".to_string(),
            ));
        }

        let prompt = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.clone())
            .unwrap_or_else(|| {
                request
                    .messages
                    .last()
                    .map(|message| message.content.clone())
                    .unwrap_or_default()
            });

        let completion = format!("{} [{}] {}", self.response_prefix, model.id, prompt.trim())
            .trim()
            .to_string();

        let prompt_tokens = estimated_tokens(
            &request
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let completion_tokens = estimated_tokens(&completion);
        let total_tokens = prompt_tokens.saturating_add(completion_tokens);

        let mut state = self.lock_state();
        state.next_completion_id = state.next_completion_id.saturating_add(1);

        Ok(OpenAiChatCompletionResponse {
            id: format!("chatcmpl-{:06}", state.next_completion_id),
            object: "chat.completion".to_string(),
            created: unix_timestamp_secs(),
            model: model.id.clone(),
            choices: vec![OpenAiChatChoice {
                index: 0,
                message: OpenAiChatMessage {
                    role: "assistant".to_string(),
                    content: completion,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: OpenAiUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
            },
        })
    }
}

fn default_requested_slots() -> u16 {
    1
}

fn estimated_tokens(text: &str) -> u32 {
    let count = text.split_whitespace().count().max(1);
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn unix_timestamp_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::config::{MeshConfig, RuntimeConfig, X0xConfig};

    fn config() -> ComputeConfig {
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
                    id: "qwen3.5:32b".to_string(),
                    family: "qwen3.5".to_string(),
                    context_window_tokens: 131_072,
                    max_output_tokens: 8_192,
                    max_parallel_reservations: 2,
                    tags: vec!["coding".to_string(), "reasoning".to_string()],
                },
                ModelConfig {
                    id: "gemma-4:27b".to_string(),
                    family: "gemma-4".to_string(),
                    context_window_tokens: 65_536,
                    max_output_tokens: 4_096,
                    max_parallel_reservations: 1,
                    tags: vec!["fast".to_string()],
                },
            ],
            x0x: X0xConfig::default(),
        }
    }

    #[test]
    fn local_inventory_reflects_configured_models() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let models = runtime.local_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gemma-4:27b");
        assert_eq!(models[1].id, "qwen3.5:32b");
    }

    #[test]
    fn reservation_reduces_available_slots() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let reservation = runtime
            .create_reservation(CreateReservationRequest {
                model: "qwen3.5:32b".to_string(),
                consumer: "alice".to_string(),
                requested_slots: 1,
            })
            .expect("create reservation");

        let models = runtime.local_models();
        let qwen = models
            .iter()
            .find(|model| model.id == "qwen3.5:32b")
            .expect("qwen inventory");
        assert_eq!(reservation.model, "qwen3.5:32b");
        assert_eq!(qwen.reserved_slots, 1);
        assert_eq!(qwen.available_slots, 1);
    }

    #[test]
    fn reservation_rejects_capacity_overflow() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let error = runtime
            .create_reservation(CreateReservationRequest {
                model: "gemma-4:27b".to_string(),
                consumer: "alice".to_string(),
                requested_slots: 2,
            })
            .expect_err("expected capacity error");

        assert!(matches!(
            error,
            ComputeError::ModelCapacityExceeded {
                model,
                requested_slots: 2,
                available_slots: 1,
            } if model == "gemma-4:27b"
        ));
    }

    #[test]
    fn release_reservation_removes_it() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let reservation = runtime
            .create_reservation(CreateReservationRequest {
                model: "gemma-4:27b".to_string(),
                consumer: "alice".to_string(),
                requested_slots: 1,
            })
            .expect("create reservation");

        runtime
            .release_reservation(&reservation.reservation_id)
            .expect("release reservation");

        assert!(runtime.reservations().is_empty());
        let models = runtime.local_models();
        let gemma = models
            .iter()
            .find(|model| model.id == "gemma-4:27b")
            .expect("gemma inventory");
        assert_eq!(gemma.available_slots, 1);
    }

    #[test]
    fn openai_models_reflect_inventory() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let models = runtime.openai_models();
        assert_eq!(models.object, "list");
        assert_eq!(models.data.len(), 2);
        assert_eq!(models.data[0].object, "model");
    }

    #[test]
    fn chat_completion_returns_skeleton_response() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let response = runtime
            .chat_completion(OpenAiChatCompletionRequest {
                model: "qwen3.5:32b".to_string(),
                messages: vec![OpenAiChatMessage {
                    role: "user".to_string(),
                    content: "hello trusted mesh".to_string(),
                }],
                stream: false,
                max_tokens: None,
                temperature: None,
            })
            .expect("chat completion");

        assert_eq!(response.object, "chat.completion");
        assert_eq!(response.model, "qwen3.5:32b");
        assert!(response.choices[0]
            .message
            .content
            .contains("hello trusted mesh"));
        assert!(response.usage.total_tokens >= response.usage.prompt_tokens);
    }

    #[test]
    fn chat_completion_rejects_streaming_requests() {
        let runtime = SkeletonRuntimeAdapter::new(&config());
        let error = runtime
            .chat_completion(OpenAiChatCompletionRequest {
                model: "qwen3.5:32b".to_string(),
                messages: vec![OpenAiChatMessage {
                    role: "user".to_string(),
                    content: "hello".to_string(),
                }],
                stream: true,
                max_tokens: None,
                temperature: None,
            })
            .expect_err("expected streaming to be unsupported");

        assert!(matches!(error, ComputeError::UnsupportedFeature(_)));
    }
}
