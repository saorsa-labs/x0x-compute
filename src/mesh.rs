use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use x0x::contacts::ContactStore;
use x0x::trust::{TrustContext, TrustDecision, TrustEvaluator};

use crate::capability::CapabilityAnnouncement;
use crate::config::ComputeConfig;
use crate::error::Result;
use crate::x0x_identity::{encode_agent_id, parse_machine_id_hex, AgentIdentitySnapshot};

/// Stored view of a trusted peer capability announcement.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct PeerCapabilityRecord {
    pub capability: CapabilityAnnouncement,
    pub trust_decision: TrustDecision,
    pub first_seen_unix_secs: u64,
    pub last_seen_unix_secs: u64,
}

/// Outcome of processing an incoming capability announcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityDisposition {
    AcceptedNewPeer,
    UpdatedPeer,
    IgnoredSelf,
    RejectedMissingSender,
    RejectedUnverifiedSender,
    RejectedInvalidPayload,
    RejectedUnsupportedProtocol,
    RejectedWrongMesh,
    RejectedSenderMismatch,
    RejectedInvalidMachineId,
    RejectedTrust(TrustDecision),
}

/// In-memory registry of trusted peer capabilities.
#[derive(Debug, Default)]
pub struct TrustedPeerRegistry {
    peers: BTreeMap<String, PeerCapabilityRecord>,
}

impl TrustedPeerRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a stable snapshot of the currently accepted peer capabilities.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PeerCapabilityRecord> {
        self.peers.values().cloned().collect()
    }

    /// Returns the number of accepted peers currently tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Returns whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Applies a single incoming x0x pub/sub message to the registry.
    pub fn apply_message(
        &mut self,
        config: &ComputeConfig,
        local_agent_id: &str,
        contacts: &ContactStore,
        message: &x0x::PubSubMessage,
    ) -> CapabilityDisposition {
        let sender = match message.sender {
            Some(sender) => sender,
            None => return CapabilityDisposition::RejectedMissingSender,
        };

        if !message.verified {
            return CapabilityDisposition::RejectedUnverifiedSender;
        }

        let sender_hex = encode_agent_id(&sender);
        if sender_hex == local_agent_id {
            return CapabilityDisposition::IgnoredSelf;
        }

        let capability = match serde_json::from_slice::<CapabilityAnnouncement>(&message.payload) {
            Ok(capability) => capability,
            Err(_) => return CapabilityDisposition::RejectedInvalidPayload,
        };

        if capability.protocol_version != crate::PROTOCOL_VERSION {
            return CapabilityDisposition::RejectedUnsupportedProtocol;
        }

        if capability.policy.mesh_name != config.mesh.name {
            return CapabilityDisposition::RejectedWrongMesh;
        }

        if capability.identity.agent_id != sender_hex {
            return CapabilityDisposition::RejectedSenderMismatch;
        }

        let machine_id = match parse_machine_id_hex(&capability.identity.machine_id) {
            Ok(machine_id) => machine_id,
            Err(_) => return CapabilityDisposition::RejectedInvalidMachineId,
        };

        let decision = TrustEvaluator::new(contacts).evaluate(&TrustContext {
            agent_id: &sender,
            machine_id: &machine_id,
        });

        if !allows_peer(config, decision) {
            return CapabilityDisposition::RejectedTrust(decision);
        }

        let now = unix_timestamp_secs();
        let peer_id = capability.identity.agent_id.clone();
        match self.peers.get_mut(&peer_id) {
            Some(existing) => {
                existing.capability = capability;
                existing.trust_decision = decision;
                existing.last_seen_unix_secs = now;
                CapabilityDisposition::UpdatedPeer
            }
            None => {
                self.peers.insert(
                    peer_id,
                    PeerCapabilityRecord {
                        capability,
                        trust_decision: decision,
                        first_seen_unix_secs: now,
                        last_seen_unix_secs: now,
                    },
                );
                CapabilityDisposition::AcceptedNewPeer
            }
        }
    }
}

/// Publishes the local capability announcement to the configured x0x topic.
pub async fn announce_local_capability(
    agent: &x0x::Agent,
    config: &ComputeConfig,
    identity: &AgentIdentitySnapshot,
) -> Result<CapabilityAnnouncement> {
    let capability = CapabilityAnnouncement::local(config, identity.clone());
    let payload = serde_json::to_vec(&capability)?;
    agent.publish(&config.capability_topic, payload).await?;
    Ok(capability)
}

/// Starts a background loop that subscribes to x0x capability gossip and keeps
/// the in-memory peer registry updated.
pub async fn start_capability_subscription_loop(
    agent: &x0x::Agent,
    config: ComputeConfig,
    local_agent_id: String,
    peers: Arc<RwLock<TrustedPeerRegistry>>,
) -> Result<tokio::task::JoinHandle<()>> {
    let contacts = agent.contacts().clone();
    let mut subscription = agent.subscribe(&config.capability_topic).await?;

    Ok(tokio::spawn(async move {
        while let Some(message) = subscription.recv().await {
            let disposition = {
                let contacts_guard = contacts.read().await;
                let mut peers_guard = peers.write().await;
                peers_guard.apply_message(&config, &local_agent_id, &contacts_guard, &message)
            };
            tracing::debug!(?disposition, topic = %message.topic, "processed capability message");
        }
    }))
}

fn allows_peer(config: &ComputeConfig, decision: TrustDecision) -> bool {
    match decision {
        TrustDecision::Accept => true,
        TrustDecision::AcceptWithFlag => {
            !config.mesh.trusted_friends_only && !config.mesh.require_trusted_contacts
        }
        TrustDecision::RejectMachineMismatch
        | TrustDecision::RejectBlocked
        | TrustDecision::Unknown => false,
    }
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
    use bytes::Bytes;
    use x0x::contacts::{Contact, IdentityType, MachineRecord, TrustLevel};

    use super::*;
    use crate::capability::{HardwareProfile, TrustedMeshPolicy};
    use crate::x0x_identity::{encode_machine_id, AgentIdentitySnapshot};

    fn config() -> ComputeConfig {
        ComputeConfig::default()
    }

    fn relaxed_config() -> ComputeConfig {
        let mut config = ComputeConfig::default();
        config.mesh.trusted_friends_only = false;
        config.mesh.require_trusted_contacts = false;
        config
    }

    fn store() -> ContactStore {
        let tempdir = tempfile::tempdir().expect("tempdir");
        ContactStore::new(tempdir.path().join("contacts.json"))
    }

    fn capability_for(
        agent_id: &x0x::identity::AgentId,
        machine_id: &x0x::identity::MachineId,
        mesh_name: &str,
        available_models: Vec<String>,
    ) -> CapabilityAnnouncement {
        CapabilityAnnouncement {
            protocol_version: crate::PROTOCOL_VERSION,
            identity: AgentIdentitySnapshot {
                machine_id: encode_machine_id(machine_id),
                agent_id: encode_agent_id(agent_id),
                user_id: None,
            },
            hardware: HardwareProfile {
                hostname: Some("friendbox".to_string()),
                cpu_brand: "Apple M4".to_string(),
                logical_cores: 10,
                total_memory_bytes: 64 * 1024,
                available_memory_bytes: 32 * 1024,
            },
            policy: TrustedMeshPolicy {
                mesh_name: mesh_name.to_string(),
                trusted_friends_only: true,
                require_trusted_contacts: true,
                prefer_user_identity: true,
            },
            available_models,
            observed_at_unix_secs: 1,
        }
    }

    fn message_for(
        sender: x0x::identity::AgentId,
        capability: &CapabilityAnnouncement,
    ) -> x0x::PubSubMessage {
        x0x::PubSubMessage {
            topic: crate::CAPABILITY_TOPIC.to_string(),
            payload: Bytes::from(serde_json::to_vec(capability).expect("serialize capability")),
            sender: Some(sender),
            sender_public_key: None,
            verified: true,
            trust_level: None,
        }
    }

    fn local_agent_id() -> x0x::identity::AgentId {
        x0x::identity::AgentKeypair::generate()
            .expect("local agent keypair")
            .agent_id()
    }

    fn contact(
        agent_id: x0x::identity::AgentId,
        trust_level: TrustLevel,
        identity_type: IdentityType,
        machines: Vec<MachineRecord>,
    ) -> Contact {
        Contact {
            agent_id,
            trust_level,
            label: Some("friend".to_string()),
            added_at: 1,
            last_seen: None,
            identity_type,
            machines,
        }
    }

    #[test]
    fn accepts_trusted_peer_with_matching_sender() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability =
            capability_for(&sender, &machine_id, "friends", vec!["qwen3.5:32b".into()]);
        let message = message_for(sender, &capability);

        let mut store = store();
        store.set_trust(&sender, TrustLevel::Trusted);

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::AcceptedNewPeer);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.snapshot()[0].capability, capability);
        assert_eq!(registry.snapshot()[0].trust_decision, TrustDecision::Accept);
    }

    #[test]
    fn rejects_sender_mismatch() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let different_agent = x0x::identity::AgentKeypair::generate()
            .expect("different keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&different_agent, &machine_id, "friends", Vec::new());
        let message = message_for(sender, &capability);

        let mut store = store();
        store.set_trust(&sender, TrustLevel::Trusted);

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::RejectedSenderMismatch);
        assert!(registry.is_empty());
    }

    #[test]
    fn rejects_unknown_peer_under_trusted_friends_policy() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &machine_id, "friends", Vec::new());
        let message = message_for(sender, &capability);
        let store = store();

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(
            disposition,
            CapabilityDisposition::RejectedTrust(TrustDecision::Unknown)
        );
        assert!(registry.is_empty());
    }

    #[test]
    fn accepts_known_peer_when_policy_is_relaxed() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &machine_id, "friends", vec!["gemma-4".into()]);
        let message = message_for(sender, &capability);

        let mut store = store();
        store.set_trust(&sender, TrustLevel::Known);

        let disposition = registry.apply_message(
            &relaxed_config(),
            &encode_agent_id(&local_agent),
            &store,
            &message,
        );

        assert_eq!(disposition, CapabilityDisposition::AcceptedNewPeer);
        assert_eq!(
            registry.snapshot()[0].trust_decision,
            TrustDecision::AcceptWithFlag
        );
    }

    #[test]
    fn rejects_known_peer_when_policy_requires_trusted_contacts() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &machine_id, "friends", Vec::new());
        let message = message_for(sender, &capability);

        let mut store = store();
        store.set_trust(&sender, TrustLevel::Known);

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(
            disposition,
            CapabilityDisposition::RejectedTrust(TrustDecision::AcceptWithFlag)
        );
        assert!(registry.is_empty());
    }

    #[test]
    fn rejects_pinned_machine_mismatch() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let pinned_machine = x0x::identity::MachineKeypair::generate()
            .expect("pinned machine keypair")
            .machine_id();
        let other_machine = x0x::identity::MachineKeypair::generate()
            .expect("other machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &other_machine, "friends", Vec::new());
        let message = message_for(sender, &capability);

        let mut record = MachineRecord::new(pinned_machine, Some("pinned".to_string()));
        record.pinned = true;

        let mut store = store();
        store.add(contact(
            sender,
            TrustLevel::Trusted,
            IdentityType::Pinned,
            vec![record],
        ));

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(
            disposition,
            CapabilityDisposition::RejectedTrust(TrustDecision::RejectMachineMismatch)
        );
        assert!(registry.is_empty());
    }

    #[test]
    fn accepts_pinned_machine_match() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let pinned_machine = x0x::identity::MachineKeypair::generate()
            .expect("pinned machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &pinned_machine, "friends", Vec::new());
        let message = message_for(sender, &capability);

        let mut record = MachineRecord::new(pinned_machine, Some("pinned".to_string()));
        record.pinned = true;

        let mut store = store();
        store.add(contact(
            sender,
            TrustLevel::Trusted,
            IdentityType::Pinned,
            vec![record],
        ));

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::AcceptedNewPeer);
        assert_eq!(registry.snapshot()[0].trust_decision, TrustDecision::Accept);
    }

    #[test]
    fn ignores_self_announcement() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&local_agent, &machine_id, "friends", Vec::new());
        let message = message_for(local_agent, &capability);
        let store = store();

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::IgnoredSelf);
        assert!(registry.is_empty());
    }

    #[test]
    fn rejects_wrong_mesh_name() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &machine_id, "other-mesh", Vec::new());
        let message = message_for(sender, &capability);

        let mut store = store();
        store.set_trust(&sender, TrustLevel::Trusted);

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::RejectedWrongMesh);
        assert!(registry.is_empty());
    }

    #[test]
    fn updates_existing_peer_record() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let first = capability_for(&sender, &machine_id, "friends", vec!["gemma-4".into()]);
        let second = capability_for(
            &sender,
            &machine_id,
            "friends",
            vec!["gemma-4".into(), "qwen3.5:32b".into()],
        );

        let mut store = store();
        store.set_trust(&sender, TrustLevel::Trusted);

        let first_disposition = registry.apply_message(
            &config(),
            &encode_agent_id(&local_agent),
            &store,
            &message_for(sender, &first),
        );
        let second_disposition = registry.apply_message(
            &config(),
            &encode_agent_id(&local_agent),
            &store,
            &message_for(sender, &second),
        );

        assert_eq!(first_disposition, CapabilityDisposition::AcceptedNewPeer);
        assert_eq!(second_disposition, CapabilityDisposition::UpdatedPeer);
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.snapshot()[0].capability.available_models,
            second.available_models
        );
    }

    #[test]
    fn rejects_invalid_payload() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let mut store = store();
        store.set_trust(&sender, TrustLevel::Trusted);

        let message = x0x::PubSubMessage {
            topic: crate::CAPABILITY_TOPIC.to_string(),
            payload: Bytes::from_static(b"not-json"),
            sender: Some(sender),
            sender_public_key: None,
            verified: true,
            trust_level: None,
        };

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::RejectedInvalidPayload);
        assert!(registry.is_empty());
    }

    #[test]
    fn rejects_unverified_sender() {
        let mut registry = TrustedPeerRegistry::new();
        let local_agent = local_agent_id();
        let sender = x0x::identity::AgentKeypair::generate()
            .expect("sender keypair")
            .agent_id();
        let machine_id = x0x::identity::MachineKeypair::generate()
            .expect("machine keypair")
            .machine_id();
        let capability = capability_for(&sender, &machine_id, "friends", Vec::new());
        let mut message = message_for(sender, &capability);
        let mut store = store();
        store.set_trust(&sender, TrustLevel::Trusted);
        message.verified = false;

        let disposition =
            registry.apply_message(&config(), &encode_agent_id(&local_agent), &store, &message);

        assert_eq!(disposition, CapabilityDisposition::RejectedUnverifiedSender);
        assert!(registry.is_empty());
    }
}
