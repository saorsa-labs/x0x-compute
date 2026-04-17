//! x0x-compute — trusted-friends compute mesh built on x0x.

pub mod capability;
pub mod config;
pub mod daemon;
pub mod error;
pub mod mesh;
pub mod runtime;
pub mod x0x_identity;

pub use capability::{CapabilityAnnouncement, HardwareProfile, TrustedMeshPolicy};
pub use config::{
    ComputeConfig, MeshConfig, ModelConfig, RuntimeBackend, RuntimeConfig, X0xConfig,
};
pub use daemon::ComputeDaemon;
pub use error::{ComputeError, Result};
pub use mesh::{
    announce_local_capability, start_capability_subscription_loop, CapabilityDisposition,
    PeerCapabilityRecord, TrustedPeerRegistry,
};
pub use runtime::{
    build_runtime, CreateReservationRequest, LocalModelInventory, ModelReservation,
    OpenAiChatChoice, OpenAiChatCompletionRequest, OpenAiChatCompletionResponse, OpenAiChatMessage,
    OpenAiModelCard, OpenAiModelListResponse, OpenAiUsage, RuntimeAdapter, SkeletonRuntimeAdapter,
};
pub use x0x_identity::{
    build_agent, encode_agent_id, encode_machine_id, encode_user_id, parse_agent_id_hex,
    parse_machine_id_hex, AgentIdentitySnapshot,
};

/// Reserved topic for compute capability advertisements.
pub const CAPABILITY_TOPIC: &str = "x0x.compute.capabilities.v1";

/// Initial wire protocol version for x0x-compute announcements.
pub const PROTOCOL_VERSION: u32 = 1;
