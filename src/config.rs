use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ComputeError, Result};

/// x0x-compute configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeConfig {
    pub api_bind: SocketAddr,
    pub data_dir: PathBuf,
    pub capability_topic: String,
    pub join_network_on_start: bool,
    pub announce_on_start: bool,
    pub require_user_id: bool,
    pub resource_announce_interval_secs: u64,
    pub mesh: MeshConfig,
    pub x0x: X0xConfig,
}

/// Mesh-level policy for trusted friend groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    pub name: String,
    pub trusted_friends_only: bool,
    pub require_trusted_contacts: bool,
    pub prefer_user_identity: bool,
}

/// Optional overrides for x0x identity and contact-store paths.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct X0xConfig {
    pub machine_key_path: Option<PathBuf>,
    pub agent_key_path: Option<PathBuf>,
    pub user_key_path: Option<PathBuf>,
    pub contact_store_path: Option<PathBuf>,
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            api_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12_800),
            data_dir: default_data_dir(),
            capability_topic: crate::CAPABILITY_TOPIC.to_string(),
            join_network_on_start: false,
            announce_on_start: false,
            require_user_id: false,
            resource_announce_interval_secs: 60,
            mesh: MeshConfig::default(),
            x0x: X0xConfig::default(),
        }
    }
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            name: "friends".to_string(),
            trusted_friends_only: true,
            require_trusted_contacts: true,
            prefer_user_identity: true,
        }
    }
}

impl ComputeConfig {
    /// Returns the default configuration path.
    #[must_use]
    pub fn default_path() -> PathBuf {
        default_config_dir().join("config.toml")
    }

    /// Loads configuration from `path`, or returns defaults when the file does not exist.
    pub fn load_or_default(path: Option<&Path>) -> Result<Self> {
        let config_path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(Self::default_path);

        if config_path.exists() {
            let raw = std::fs::read_to_string(&config_path)?;
            toml::from_str(&raw).map_err(ComputeError::from)
        } else {
            Ok(Self::default())
        }
    }

    /// Writes the current configuration to disk.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| ComputeError::MissingParentDirectory(path.to_path_buf()))?;
        std::fs::create_dir_all(parent)?;
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Writes a default config to `path` and returns the resolved path.
    pub fn write_default(path: Option<&Path>) -> Result<PathBuf> {
        let config = Self::default();
        let resolved = path
            .map(Path::to_path_buf)
            .unwrap_or_else(Self::default_path);
        config.write_to_path(&resolved)?;
        Ok(resolved)
    }
}

#[must_use]
pub fn default_config_dir() -> PathBuf {
    match dirs::config_dir() {
        Some(path) => path.join("x0x-compute"),
        None => fallback_base_dir().join(".config").join("x0x-compute"),
    }
}

#[must_use]
pub fn default_data_dir() -> PathBuf {
    match dirs::data_dir() {
        Some(path) => path.join("x0x-compute"),
        None => fallback_base_dir()
            .join(".local")
            .join("share")
            .join("x0x-compute"),
    }
}

#[must_use]
fn fallback_base_dir() -> PathBuf {
    match std::env::current_dir() {
        Ok(path) => path,
        Err(_) => PathBuf::from("."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_trusted_friends_policy() {
        let config = ComputeConfig::default();
        assert!(config.mesh.trusted_friends_only);
        assert!(config.mesh.require_trusted_contacts);
        assert!(config.mesh.prefer_user_identity);
    }

    #[test]
    fn default_path_ends_with_config_toml() {
        let path = ComputeConfig::default_path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("config.toml")
        );
    }
}
