//! x0x-compute — trusted-friends compute mesh built on x0x.

pub mod capability;
pub mod config;
pub mod daemon;
pub mod error;
pub mod x0x_identity;

pub use capability::{CapabilityAnnouncement, HardwareProfile, TrustedMeshPolicy};
pub use config::{ComputeConfig, MeshConfig, X0xConfig};
pub use daemon::ComputeDaemon;
pub use error::{ComputeError, Result};
pub use x0x_identity::{build_agent, AgentIdentitySnapshot};

/// Reserved topic for compute capability advertisements.
pub const CAPABILITY_TOPIC: &str = "x0x.compute.capabilities.v1";

/// Initial wire protocol version for x0x-compute announcements.
pub const PROTOCOL_VERSION: u32 = 1;
