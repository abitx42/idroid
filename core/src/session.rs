use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ids::{DeviceId, SessionId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Timestamp(u64);

impl Timestamp {
    pub const ZERO: Self = Self(0);

    pub const fn from_millis(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

impl From<u64> for Timestamp {
    fn from(value: u64) -> Self {
        Self::from_millis(value)
    }
}

impl From<Timestamp> for u64 {
    fn from(value: Timestamp) -> Self {
        value.as_millis()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionConfig {
    heartbeat_timeout_ms: u64,
    reconnect_grace_ms: u64,
    stale_after_ms: u64,
}

impl SessionConfig {
    pub const fn new(
        heartbeat_timeout_ms: u64,
        reconnect_grace_ms: u64,
        stale_after_ms: u64,
    ) -> Self {
        Self {
            heartbeat_timeout_ms,
            reconnect_grace_ms,
            stale_after_ms,
        }
    }

    pub const fn heartbeat_timeout_ms(self) -> u64 {
        self.heartbeat_timeout_ms
    }

    pub const fn reconnect_grace_ms(self) -> u64 {
        self.reconnect_grace_ms
    }

    pub const fn stale_after_ms(self) -> u64 {
        self.stale_after_ms
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self::new(30_000, 15_000, 300_000)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisconnectReason {
    Requested,
    TransportLost,
    HeartbeatTimeout,
}

impl DisconnectReason {
    pub(crate) const fn wire_value(self) -> u8 {
        match self {
            Self::Requested => 1,
            Self::TransportLost => 2,
            Self::HeartbeatTimeout => 3,
        }
    }

    pub(crate) fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Requested),
            2 => Some(Self::TransportLost),
            3 => Some(Self::HeartbeatTimeout),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationReason {
    Requested,
    Duplicate,
    Stale,
    ProtocolError,
}

impl TerminationReason {
    pub(crate) const fn wire_value(self) -> u8 {
        match self {
            Self::Requested => 1,
            Self::Duplicate => 2,
            Self::Stale => 3,
            Self::ProtocolError => 4,
        }
    }

    pub(crate) fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Requested),
            2 => Some(Self::Duplicate),
            3 => Some(Self::Stale),
            4 => Some(Self::ProtocolError),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Connected,
    Disconnected,
    Offline,
    Terminated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    id: SessionId,
    device_id: DeviceId,
    status: SessionStatus,
    connected_at: Timestamp,
    last_heartbeat: Timestamp,
    state_changed_at: Timestamp,
    disconnect_reason: Option<DisconnectReason>,
    termination_reason: Option<TerminationReason>,
}

impl SessionSnapshot {
    pub const fn id(&self) -> SessionId {
        self.id
    }

    pub const fn device_id(&self) -> DeviceId {
        self.device_id
    }

    pub const fn status(&self) -> SessionStatus {
        self.status
    }

    pub const fn connected_at(&self) -> Timestamp {
        self.connected_at
    }

    pub const fn last_heartbeat(&self) -> Timestamp {
        self.last_heartbeat
    }

    pub const fn state_changed_at(&self) -> Timestamp {
        self.state_changed_at
    }

    pub const fn disconnect_reason(&self) -> Option<DisconnectReason> {
        self.disconnect_reason
    }

    pub const fn termination_reason(&self) -> Option<TerminationReason> {
        self.termination_reason
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionStart {
    pub session_id: SessionId,
    pub superseded: Option<SessionId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupReport {
    pub heartbeat_timeouts: Vec<SessionId>,
    pub offline_sessions: Vec<SessionId>,
    pub removed_stale: Vec<SessionId>,
}

impl CleanupReport {
    fn new() -> Self {
        Self {
            heartbeat_timeouts: Vec::new(),
            offline_sessions: Vec::new(),
            removed_stale: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.heartbeat_timeouts.is_empty()
            && self.offline_sessions.is_empty()
            && self.removed_stale.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    InvalidDeviceId,
    InvalidSessionId,
    UnknownSession(SessionId),
    IdExhausted,
    AlreadyDisconnected(SessionId),
    NotConnected(SessionId),
    NotDisconnected(SessionId),
    AlreadyOffline(SessionId),
    AlreadyTerminated(SessionId),
    ReconnectWindowExpired(SessionId),
    ClockRewound {
        session_id: SessionId,
        last: Timestamp,
        now: Timestamp,
    },
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeviceId => formatter.write_str("device identifiers must be non-zero"),
            Self::InvalidSessionId => formatter.write_str("session identifiers must be non-zero"),
            Self::UnknownSession(id) => write!(formatter, "unknown session {id}"),
            Self::IdExhausted => formatter.write_str("session identifier space exhausted"),
            Self::AlreadyDisconnected(id) => {
                write!(formatter, "session {id} is already disconnected")
            }
            Self::NotConnected(id) => write!(formatter, "session {id} is not connected"),
            Self::NotDisconnected(id) => {
                write!(formatter, "session {id} is not temporarily disconnected")
            }
            Self::AlreadyOffline(id) => write!(formatter, "session {id} is already offline"),
            Self::AlreadyTerminated(id) => write!(formatter, "session {id} is already terminated"),
            Self::ReconnectWindowExpired(id) => {
                write!(formatter, "reconnect window expired for session {id}")
            }
            Self::ClockRewound {
                session_id,
                last,
                now,
            } => write!(
                formatter,
                "clock moved backwards for session {session_id}: {now:?} before {last:?}"
            ),
        }
    }
}

impl std::error::Error for SessionError {}

#[derive(Clone, Debug)]
struct SessionRecord {
    snapshot: SessionSnapshot,
}

#[derive(Clone, Debug)]
pub struct SessionManager {
    config: SessionConfig,
    next_id: u64,
    sessions: BTreeMap<SessionId, SessionRecord>,
    active_by_device: BTreeMap<DeviceId, SessionId>,
}

impl SessionManager {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            next_id: 1,
            sessions: BTreeMap::new(),
            active_by_device: BTreeMap::new(),
        }
    }

    pub const fn config(&self) -> SessionConfig {
        self.config
    }

    pub fn start_session(
        &mut self,
        device_id: DeviceId,
        now: Timestamp,
    ) -> Result<SessionStart, SessionError> {
        if !device_id.is_valid() {
            return Err(SessionError::InvalidDeviceId);
        }

        self.sweep(now);
        let superseded = self.active_by_device.get(&device_id).copied();
        if let Some(old_id) = superseded {
            self.transition_to_terminated(old_id, now, TerminationReason::Duplicate)?;
        }

        let session_id = self.allocate_id()?;
        let snapshot = SessionSnapshot {
            id: session_id,
            device_id,
            status: SessionStatus::Connected,
            connected_at: now,
            last_heartbeat: now,
            state_changed_at: now,
            disconnect_reason: None,
            termination_reason: None,
        };
        self.sessions.insert(session_id, SessionRecord { snapshot });
        self.active_by_device.insert(device_id, session_id);
        Ok(SessionStart {
            session_id,
            superseded,
        })
    }

    pub fn get(&self, session_id: SessionId) -> Option<&SessionSnapshot> {
        self.sessions
            .get(&session_id)
            .map(|record| &record.snapshot)
    }

    pub fn active_session(&self, device_id: DeviceId) -> Option<SessionId> {
        self.active_by_device.get(&device_id).copied()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn heartbeat(&mut self, session_id: SessionId, now: Timestamp) -> Result<(), SessionError> {
        let record = self.record_mut(session_id)?;
        ensure_forward(record.snapshot.id, record.snapshot.last_heartbeat, now)?;
        if record.snapshot.status != SessionStatus::Connected {
            return Err(SessionError::NotConnected(session_id));
        }
        record.snapshot.last_heartbeat = now;
        Ok(())
    }

    pub fn disconnect(
        &mut self,
        session_id: SessionId,
        now: Timestamp,
    ) -> Result<(), SessionError> {
        let device_id;
        {
            let record = self.record_mut(session_id)?;
            ensure_forward(record.snapshot.id, record.snapshot.state_changed_at, now)?;
            match record.snapshot.status {
                SessionStatus::Connected => {
                    record.snapshot.status = SessionStatus::Disconnected;
                    record.snapshot.state_changed_at = now;
                    record.snapshot.disconnect_reason = Some(DisconnectReason::Requested);
                    device_id = record.snapshot.device_id;
                }
                SessionStatus::Disconnected => {
                    return Err(SessionError::AlreadyDisconnected(session_id))
                }
                _ => return Err(SessionError::NotConnected(session_id)),
            }
        }
        self.active_by_device.insert(device_id, session_id);
        Ok(())
    }

    pub fn reconnect(&mut self, session_id: SessionId, now: Timestamp) -> Result<(), SessionError> {
        let device_id;
        let reconnect_grace_ms = self.config.reconnect_grace_ms();
        {
            let record = self.record_mut(session_id)?;
            ensure_forward(record.snapshot.id, record.snapshot.state_changed_at, now)?;
            if record.snapshot.status != SessionStatus::Disconnected {
                return Err(SessionError::NotDisconnected(session_id));
            }
            let elapsed = now.as_millis() - record.snapshot.state_changed_at.as_millis();
            if elapsed > reconnect_grace_ms {
                record.snapshot.status = SessionStatus::Offline;
                record.snapshot.state_changed_at = now;
                device_id = record.snapshot.device_id;
            } else {
                record.snapshot.status = SessionStatus::Connected;
                record.snapshot.last_heartbeat = now;
                record.snapshot.state_changed_at = now;
                record.snapshot.disconnect_reason = None;
                device_id = record.snapshot.device_id;
                self.active_by_device.insert(device_id, session_id);
                return Ok(());
            }
        }
        self.active_by_device.remove(&device_id);
        Err(SessionError::ReconnectWindowExpired(session_id))
    }

    pub fn mark_offline(
        &mut self,
        session_id: SessionId,
        now: Timestamp,
    ) -> Result<(), SessionError> {
        let device_id;
        {
            let record = self.record_mut(session_id)?;
            ensure_forward(record.snapshot.id, record.snapshot.state_changed_at, now)?;
            match record.snapshot.status {
                SessionStatus::Connected | SessionStatus::Disconnected => {
                    record.snapshot.status = SessionStatus::Offline;
                    record.snapshot.state_changed_at = now;
                    device_id = record.snapshot.device_id;
                }
                SessionStatus::Offline => return Err(SessionError::AlreadyOffline(session_id)),
                SessionStatus::Terminated => {
                    return Err(SessionError::AlreadyTerminated(session_id))
                }
            }
        }
        self.active_by_device.remove(&device_id);
        Ok(())
    }

    pub fn terminate(
        &mut self,
        session_id: SessionId,
        now: Timestamp,
        reason: TerminationReason,
    ) -> Result<(), SessionError> {
        let device_id = self.transition_to_terminated(session_id, now, reason)?;
        self.active_by_device.remove(&device_id);
        Ok(())
    }

    pub fn sweep(&mut self, now: Timestamp) -> CleanupReport {
        let ids: Vec<SessionId> = self.sessions.keys().copied().collect();
        let mut report = CleanupReport::new();
        let mut remove = BTreeSet::new();

        for session_id in ids {
            let Some(record) = self.sessions.get_mut(&session_id) else {
                continue;
            };
            let snapshot = &mut record.snapshot;
            match snapshot.status {
                SessionStatus::Connected => {
                    if elapsed(now, snapshot.last_heartbeat)
                        .is_some_and(|age| age >= self.config.heartbeat_timeout_ms())
                    {
                        snapshot.status = SessionStatus::Disconnected;
                        snapshot.state_changed_at = now;
                        snapshot.disconnect_reason = Some(DisconnectReason::HeartbeatTimeout);
                        self.active_by_device.insert(snapshot.device_id, session_id);
                        report.heartbeat_timeouts.push(session_id);
                    }
                }
                SessionStatus::Disconnected => {
                    if elapsed(now, snapshot.state_changed_at)
                        .is_some_and(|age| age >= self.config.reconnect_grace_ms())
                    {
                        snapshot.status = SessionStatus::Offline;
                        snapshot.state_changed_at = now;
                        self.active_by_device.remove(&snapshot.device_id);
                        report.offline_sessions.push(session_id);
                    }
                }
                SessionStatus::Offline | SessionStatus::Terminated => {
                    if elapsed(now, snapshot.state_changed_at)
                        .is_some_and(|age| age >= self.config.stale_after_ms())
                    {
                        remove.insert(session_id);
                    }
                }
            }
        }

        for session_id in remove {
            self.sessions.remove(&session_id);
            report.removed_stale.push(session_id);
        }
        report
    }

    fn allocate_id(&mut self) -> Result<SessionId, SessionError> {
        let value = self.next_id;
        if value == 0 {
            return Err(SessionError::IdExhausted);
        }
        self.next_id = self.next_id.checked_add(1).unwrap_or(0);
        Ok(SessionId::new(value))
    }

    fn record_mut(&mut self, session_id: SessionId) -> Result<&mut SessionRecord, SessionError> {
        if !session_id.is_valid() {
            return Err(SessionError::InvalidSessionId);
        }
        self.sessions
            .get_mut(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))
    }

    fn transition_to_terminated(
        &mut self,
        session_id: SessionId,
        now: Timestamp,
        reason: TerminationReason,
    ) -> Result<DeviceId, SessionError> {
        let record = self.record_mut(session_id)?;
        ensure_forward(record.snapshot.id, record.snapshot.state_changed_at, now)?;
        if record.snapshot.status == SessionStatus::Terminated {
            return Err(SessionError::AlreadyTerminated(session_id));
        }
        record.snapshot.status = SessionStatus::Terminated;
        record.snapshot.state_changed_at = now;
        record.snapshot.termination_reason = Some(reason);
        Ok(record.snapshot.device_id)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(SessionConfig::default())
    }
}

fn elapsed(now: Timestamp, then: Timestamp) -> Option<u64> {
    now.as_millis().checked_sub(then.as_millis())
}

fn ensure_forward(
    session_id: SessionId,
    last: Timestamp,
    now: Timestamp,
) -> Result<(), SessionError> {
    if now < last {
        Err(SessionError::ClockRewound {
            session_id,
            last,
            now,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(milliseconds: u64) -> Timestamp {
        Timestamp::from_millis(milliseconds)
    }

    #[test]
    fn duplicate_start_terminates_previous_session_and_keeps_ids_unique() {
        let mut manager = SessionManager::new(SessionConfig::new(100, 50, 500));
        let device = DeviceId::new(9);
        let first = manager.start_session(device, time(0)).expect("first");
        let second = manager.start_session(device, time(1)).expect("second");

        assert_ne!(first.session_id, second.session_id);
        assert_eq!(second.superseded, Some(first.session_id));
        assert_eq!(manager.active_session(device), Some(second.session_id));
        assert_eq!(
            manager.get(first.session_id).expect("old session").status(),
            SessionStatus::Terminated
        );
        assert_eq!(
            manager
                .get(first.session_id)
                .expect("old session")
                .termination_reason(),
            Some(TerminationReason::Duplicate)
        );
    }

    #[test]
    fn duplicate_start_also_supersedes_a_resumable_disconnect() {
        let mut manager = SessionManager::new(SessionConfig::new(100, 50, 500));
        let device = DeviceId::new(10);
        let first = manager
            .start_session(device, time(0))
            .expect("first")
            .session_id;
        manager.disconnect(first, time(1)).expect("disconnect");

        let second = manager.start_session(device, time(2)).expect("second");

        assert_eq!(second.superseded, Some(first));
        assert_eq!(manager.active_session(device), Some(second.session_id));
        assert_eq!(
            manager
                .get(first)
                .expect("old session")
                .termination_reason(),
            Some(TerminationReason::Duplicate)
        );
    }

    #[test]
    fn heartbeat_timeout_then_reconnect_window_then_stale_cleanup() {
        let mut manager = SessionManager::new(SessionConfig::new(10, 20, 30));
        let id = manager
            .start_session(DeviceId::new(3), time(0))
            .expect("session")
            .session_id;

        assert!(manager.sweep(time(9)).is_empty());
        let timeout = manager.sweep(time(10));
        assert_eq!(timeout.heartbeat_timeouts, vec![id]);
        assert_eq!(
            manager.get(id).expect("session").status(),
            SessionStatus::Disconnected
        );

        manager.reconnect(id, time(20)).expect("within grace");
        manager.disconnect(id, time(21)).expect("disconnect");
        let offline = manager.sweep(time(41));
        assert_eq!(offline.offline_sessions, vec![id]);
        assert_eq!(
            manager.get(id).expect("session").status(),
            SessionStatus::Offline
        );
        assert_eq!(
            manager.reconnect(id, time(42)),
            Err(SessionError::NotDisconnected(id))
        );

        let removed = manager.sweep(time(71));
        assert_eq!(removed.removed_stale, vec![id]);
        assert!(manager.get(id).is_none());
    }

    #[test]
    fn explicit_termination_is_final() {
        let mut manager = SessionManager::default();
        let id = manager
            .start_session(DeviceId::new(8), time(100))
            .expect("session")
            .session_id;

        manager
            .terminate(id, time(101), TerminationReason::Requested)
            .expect("terminate");
        assert_eq!(
            manager.heartbeat(id, time(102)),
            Err(SessionError::NotConnected(id))
        );
        assert_eq!(
            manager.reconnect(id, time(102)),
            Err(SessionError::NotDisconnected(id))
        );
        assert_eq!(
            manager.terminate(id, time(103), TerminationReason::Requested),
            Err(SessionError::AlreadyTerminated(id))
        );
    }
}
