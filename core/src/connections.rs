//! Transport-independent arbitration shared by diallers and listeners.
use std::collections::BTreeMap;

use crate::control_plane::{ContractError, DeviceFingerprint};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConnectionId(pub [u8; 16]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerConnection {
    pub id: ConnectionId,
    pub initiator: DeviceFingerprint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionDecision {
    pub keep: ConnectionId,
    pub close: Option<ConnectionId>,
}

pub struct ConnectionArbiter {
    local: DeviceFingerprint,
    active: BTreeMap<DeviceFingerprint, PeerConnection>,
}

impl ConnectionArbiter {
    pub fn new(local: DeviceFingerprint) -> Result<Self, ContractError> {
        local.validate()?;
        Ok(Self {
            local,
            active: BTreeMap::new(),
        })
    }

    /// Call only after TLS identity and Hello are validated. For parallel paths
    /// with the same initiator the initiating endpoint selects the first and
    /// sends Activate only on that stream; listeners admit that selected stream.
    /// Opposite-direction simultaneous dials converge on the lower fingerprint.
    pub fn admit(
        &mut self,
        peer: DeviceFingerprint,
        incoming: PeerConnection,
    ) -> Result<ConnectionDecision, ContractError> {
        peer.validate()?;
        if peer == self.local
            || incoming.id.0 == [0; 16]
            || (incoming.initiator != self.local && incoming.initiator != peer)
        {
            return Err(ContractError::InvalidField("connection_identity"));
        }
        let Some(current) = self.active.get(&peer).copied() else {
            self.active.insert(peer, incoming);
            return Ok(ConnectionDecision {
                keep: incoming.id,
                close: None,
            });
        };
        if current == incoming {
            return Ok(ConnectionDecision {
                keep: current.id,
                close: None,
            });
        }
        if current.id == incoming.id {
            return Err(ContractError::InvalidField("connection_id_reused"));
        }
        if incoming.initiator < current.initiator {
            self.active.insert(peer, incoming);
            Ok(ConnectionDecision {
                keep: incoming.id,
                close: Some(current.id),
            })
        } else {
            Ok(ConnectionDecision {
                keep: current.id,
                close: Some(incoming.id),
            })
        }
    }

    pub fn active(&self, peer: DeviceFingerprint) -> Option<PeerConnection> {
        self.active.get(&peer).copied()
    }

    /// A cancelled/stale loser never removes the surviving connection.
    pub fn closed(&mut self, peer: DeviceFingerprint, id: ConnectionId) -> bool {
        if self.active(peer).is_some_and(|active| active.id == id) {
            self.active.remove(&peer);
            true
        } else {
            false
        }
    }
}
