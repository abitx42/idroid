//! Candidate discovery contracts and a deterministic connection race.
//!
//! Real Bonjour, gateway probing, sockets, TLS, and Android multicast-lock
//! handling belong behind these traits. The portable core only validates
//! candidate hints, races authenticated results, and records measurable events.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use crate::clock::Clock;
use crate::control_plane::{
    CandidateSourceKind, ConnectFailureReason, ControlPlaneEventKind, ControlPlaneEventLog,
    DeviceFingerprint, LinkError, LinkMachine,
};
use crate::pairing::{authorize_reconnect, PairingError, PairingStore};
use crate::session::Timestamp;

pub const MAX_NETWORK_ID_BYTES: usize = 128;
pub const MAX_CANDIDATE_ADDRESS_BYTES: usize = 255;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NetworkIdentity(String);

impl NetworkIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self, DiscoveryError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_NETWORK_ID_BYTES {
            return Err(DiscoveryError::InvalidNetworkIdentity);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub address: String,
    pub port: u16,
    pub source: CandidateSourceKind,
    pub network: NetworkIdentity,
    pub expected_peer: Option<DeviceFingerprint>,
}

impl Candidate {
    pub fn new(
        address: impl Into<String>,
        port: u16,
        source: CandidateSourceKind,
        network: NetworkIdentity,
        expected_peer: Option<DeviceFingerprint>,
    ) -> Result<Self, DiscoveryError> {
        let candidate = Self {
            address: address.into(),
            port,
            source,
            network,
            expected_peer,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    pub fn validate(&self) -> Result<(), DiscoveryError> {
        if self.address.is_empty() || self.address.len() > MAX_CANDIDATE_ADDRESS_BYTES {
            return Err(DiscoveryError::InvalidCandidate);
        }
        if self.port == 0 {
            return Err(DiscoveryError::InvalidCandidate);
        }
        if let Some(peer) = self.expected_peer {
            peer.validate()
                .map_err(|_| DiscoveryError::InvalidCandidate)?;
        }
        Ok(())
    }

    fn key(&self) -> (String, u16) {
        (self.address.clone(), self.port)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateSourceError {
    Unavailable,
    Failed,
}

pub trait CandidateSource {
    fn kind(&self) -> CandidateSourceKind;
    fn discover(
        &mut self,
        network: &NetworkIdentity,
    ) -> Result<Vec<Candidate>, CandidateSourceError>;
}

#[derive(Clone, Debug)]
pub struct ScriptedCandidateSource {
    kind: CandidateSourceKind,
    responses: VecDeque<Result<Vec<Candidate>, CandidateSourceError>>,
    calls: Vec<NetworkIdentity>,
}

impl ScriptedCandidateSource {
    pub fn new(kind: CandidateSourceKind) -> Self {
        Self {
            kind,
            responses: VecDeque::new(),
            calls: Vec::new(),
        }
    }

    pub fn push_response(&mut self, response: Result<Vec<Candidate>, CandidateSourceError>) {
        self.responses.push_back(response);
    }

    pub fn calls(&self) -> &[NetworkIdentity] {
        &self.calls
    }
}

impl CandidateSource for ScriptedCandidateSource {
    fn kind(&self) -> CandidateSourceKind {
        self.kind
    }

    fn discover(
        &mut self,
        network: &NetworkIdentity,
    ) -> Result<Vec<Candidate>, CandidateSourceError> {
        self.calls.push(network.clone());
        self.responses.pop_front().unwrap_or(Ok(Vec::new()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedPeer {
    pub fingerprint: DeviceFingerprint,
}

impl AuthenticatedPeer {
    pub fn new(fingerprint: DeviceFingerprint) -> Result<Self, DiscoveryError> {
        fingerprint
            .validate()
            .map_err(|_| DiscoveryError::InvalidHandshake)?;
        Ok(Self { fingerprint })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConnectionAttempt {
    id: u64,
}

impl ConnectionAttempt {
    pub const fn id(self) -> u64 {
        self.id
    }
}

pub trait CandidateConnector {
    fn start(&mut self, candidate: &Candidate) -> Result<ConnectionAttempt, ConnectFailureReason>;
    fn next_completion(
        &mut self,
        pending: &[ConnectionAttempt],
    ) -> Option<(
        ConnectionAttempt,
        Result<AuthenticatedPeer, ConnectFailureReason>,
    )>;
    fn cancel(&mut self, attempt: ConnectionAttempt);
}

#[derive(Clone, Debug, Default)]
pub struct ScriptedCandidateConnector {
    outcomes: BTreeMap<(String, u16), VecDeque<Result<AuthenticatedPeer, ConnectFailureReason>>>,
    calls: Vec<Candidate>,
    pending: BTreeMap<u64, (Candidate, Result<AuthenticatedPeer, ConnectFailureReason>)>,
    next_id: u64,
    completion_order: VecDeque<(String, u16)>,
    cancelled: Vec<ConnectionAttempt>,
}

impl ScriptedCandidateConnector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_outcome(
        &mut self,
        address: impl Into<String>,
        port: u16,
        outcome: Result<AuthenticatedPeer, ConnectFailureReason>,
    ) {
        self.outcomes
            .entry((address.into(), port))
            .or_default()
            .push_back(outcome);
    }

    pub fn calls(&self) -> &[Candidate] {
        &self.calls
    }

    pub fn set_completion_order(&mut self, candidates: &[(String, u16)]) {
        self.completion_order = candidates.iter().cloned().collect();
    }

    pub fn cancelled(&self) -> &[ConnectionAttempt] {
        &self.cancelled
    }
}

impl CandidateConnector for ScriptedCandidateConnector {
    fn start(&mut self, candidate: &Candidate) -> Result<ConnectionAttempt, ConnectFailureReason> {
        self.calls.push(candidate.clone());
        let outcome = self
            .outcomes
            .get_mut(&candidate.key())
            .and_then(VecDeque::pop_front)
            .unwrap_or(Err(ConnectFailureReason::Timeout));
        let attempt = ConnectionAttempt { id: self.next_id };
        self.next_id = self.next_id.saturating_add(1).max(1);
        self.pending
            .insert(attempt.id, (candidate.clone(), outcome));
        Ok(attempt)
    }

    fn next_completion(
        &mut self,
        pending: &[ConnectionAttempt],
    ) -> Option<(
        ConnectionAttempt,
        Result<AuthenticatedPeer, ConnectFailureReason>,
    )> {
        let selected_id = self
            .completion_order
            .iter()
            .find_map(|key| {
                self.pending
                    .iter()
                    .find(|(_, (candidate, _))| candidate.key() == *key)
                    .map(|(id, _)| *id)
            })
            .or_else(|| {
                pending
                    .iter()
                    .map(|attempt| attempt.id)
                    .find(|id| self.pending.contains_key(id))
            })?;
        let attempt = ConnectionAttempt { id: selected_id };
        let (_, outcome) = self.pending.remove(&selected_id)?;
        if self.completion_order.front().is_some_and(|key| {
            self.pending
                .values()
                .all(|(candidate, _)| candidate.key() != *key)
        }) {
            self.completion_order.pop_front();
        }
        Some((attempt, outcome))
    }

    fn cancel(&mut self, attempt: ConnectionAttempt) {
        self.cancelled.push(attempt);
        self.pending.remove(&attempt.id);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RaceResult {
    pub generation: u64,
    pub candidate: Candidate,
    pub peer_fingerprint: DeviceFingerprint,
    pub source: CandidateSourceKind,
    pub time_to_first_candidate_ms: u64,
    pub time_to_authenticated_handshake_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    InvalidNetworkIdentity,
    InvalidCandidate,
    InvalidHandshake,
    NoCandidates,
    NoAuthenticatedPeer,
    StaleGeneration { expected: u64, received: u64 },
    Link(LinkError),
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNetworkIdentity => formatter.write_str("invalid network identity"),
            Self::InvalidCandidate => formatter.write_str("invalid discovery candidate"),
            Self::InvalidHandshake => formatter.write_str("invalid authenticated peer"),
            Self::NoCandidates => formatter.write_str("discovery produced no candidates"),
            Self::NoAuthenticatedPeer => {
                formatter.write_str("no candidate authenticated the target peer")
            }
            Self::StaleGeneration { expected, received } => write!(
                formatter,
                "stale discovery generation {received}; current generation is {expected}"
            ),
            Self::Link(error) => write!(formatter, "link state update failed: {error}"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

#[derive(Clone, Debug, Default)]
pub struct DiscoveryRace {
    generation: u64,
    network: Option<NetworkIdentity>,
    started_at: Option<Timestamp>,
    first_candidate_at: Option<Timestamp>,
    candidates: Vec<Candidate>,
    source_errors: Vec<(CandidateSourceKind, CandidateSourceError)>,
    winner: Option<RaceResult>,
}

impl DiscoveryRace {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn network(&self) -> Option<&NetworkIdentity> {
        self.network.as_ref()
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn source_errors(&self) -> &[(CandidateSourceKind, CandidateSourceError)] {
        &self.source_errors
    }

    pub fn winner(&self) -> Option<&RaceResult> {
        self.winner.as_ref()
    }

    /// Start all supplied sources. Platform adapters should pass cached and
    /// gateway sources first and start Bonjour in parallel; the scripted core
    /// keeps the same ordering while remaining synchronous and deterministic.
    pub fn start<C: Clock>(
        &mut self,
        clock: &C,
        network: NetworkIdentity,
        sources: &mut [&mut dyn CandidateSource],
        log: &mut ControlPlaneEventLog,
    ) -> Result<u64, DiscoveryError> {
        self.generation = self.generation.saturating_add(1).max(1);
        self.network = Some(network.clone());
        self.started_at = Some(clock.now());
        self.first_candidate_at = None;
        self.candidates.clear();
        self.source_errors.clear();
        self.winner = None;

        for source in sources {
            let kind = source.kind();
            match source.discover(&network) {
                Ok(candidates) => {
                    for candidate in candidates {
                        if candidate.source != kind || candidate.network != network {
                            return Err(DiscoveryError::InvalidCandidate);
                        }
                        candidate.validate()?;
                        if self.first_candidate_at.is_none() {
                            self.first_candidate_at = Some(clock.now());
                        }
                        log.record_event(
                            clock,
                            ControlPlaneEventKind::CandidateFound { source: kind },
                        );
                        self.candidates.push(candidate);
                    }
                }
                Err(error) => self.source_errors.push((kind, error)),
            }
        }
        Ok(self.generation)
    }

    pub fn network_changed<C: Clock>(
        &mut self,
        clock: &C,
        network: NetworkIdentity,
        log: &mut ControlPlaneEventLog,
    ) -> Result<u64, DiscoveryError> {
        self.generation = self.generation.saturating_add(1).max(1);
        self.network = Some(network);
        self.started_at = None;
        self.first_candidate_at = None;
        self.candidates.clear();
        self.source_errors.clear();
        self.winner = None;
        log.record_event(clock, ControlPlaneEventKind::NetworkChanged);
        Ok(self.generation)
    }

    pub fn try_connect<C: Clock, K: CandidateConnector>(
        &mut self,
        clock: &C,
        generation: u64,
        expected_peer: DeviceFingerprint,
        connector: &mut K,
        log: &mut ControlPlaneEventLog,
    ) -> Result<RaceResult, DiscoveryError> {
        self.try_connect_internal(clock, generation, expected_peer, connector, log, |_| Ok(()))
    }

    pub fn try_connect_authorized<C: Clock, K: CandidateConnector, S: PairingStore>(
        &mut self,
        clock: &C,
        generation: u64,
        expected_peer: DeviceFingerprint,
        connector: &mut K,
        store: &S,
        log: &mut ControlPlaneEventLog,
    ) -> Result<RaceResult, DiscoveryError> {
        self.try_connect_internal(
            clock,
            generation,
            expected_peer,
            connector,
            log,
            |fingerprint| {
                authorize_reconnect(store, fingerprint)
                    .map(|_| ())
                    .map_err(pairing_failure_reason)
            },
        )
    }

    /// Run the authenticated candidate race as the transport portion of the
    /// link state machine. A winner leaves the link in `Handshaking`; the
    /// control-plane adapter must still validate Hello/version/session data and
    /// call `LinkMachine::handshake_ready` before the link is `Ready`.
    pub fn try_connect_with_link<C: Clock, K: CandidateConnector>(
        &mut self,
        clock: &C,
        generation: u64,
        expected_peer: DeviceFingerprint,
        connector: &mut K,
        link: &mut LinkMachine,
        log: &mut ControlPlaneEventLog,
    ) -> Result<RaceResult, DiscoveryError> {
        self.try_connect_with_link_internal(
            clock,
            generation,
            expected_peer,
            connector,
            link,
            log,
            |_| Ok(()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_connect_authorized_with_link<C: Clock, K: CandidateConnector, S: PairingStore>(
        &mut self,
        clock: &C,
        generation: u64,
        expected_peer: DeviceFingerprint,
        connector: &mut K,
        store: &S,
        link: &mut LinkMachine,
        log: &mut ControlPlaneEventLog,
    ) -> Result<RaceResult, DiscoveryError> {
        self.try_connect_with_link_internal(
            clock,
            generation,
            expected_peer,
            connector,
            link,
            log,
            |fingerprint| {
                authorize_reconnect(store, fingerprint)
                    .map(|_| ())
                    .map_err(pairing_failure_reason)
            },
        )
    }

    fn try_connect_internal<C: Clock, K: CandidateConnector, F>(
        &mut self,
        clock: &C,
        generation: u64,
        expected_peer: DeviceFingerprint,
        connector: &mut K,
        log: &mut ControlPlaneEventLog,
        mut authorize: F,
    ) -> Result<RaceResult, DiscoveryError>
    where
        F: FnMut(DeviceFingerprint) -> Result<(), ConnectFailureReason>,
    {
        if generation != self.generation {
            return Err(DiscoveryError::StaleGeneration {
                expected: self.generation,
                received: generation,
            });
        }
        if self.candidates.is_empty() {
            return Err(DiscoveryError::NoCandidates);
        }
        let started_at = self.started_at.unwrap_or_else(|| clock.now());
        let first_candidate_at = self.first_candidate_at.unwrap_or(started_at);

        let mut pending = Vec::new();
        for candidate in self.candidates.clone() {
            log.record_event(clock, ControlPlaneEventKind::ConnectStarted);
            if candidate
                .expected_peer
                .is_some_and(|peer| peer != expected_peer)
            {
                log.record_event(
                    clock,
                    ControlPlaneEventKind::ConnectFailed {
                        reason: ConnectFailureReason::FingerprintMismatch,
                    },
                );
                continue;
            }
            match connector.start(&candidate) {
                Ok(attempt) => pending.push((candidate, attempt)),
                Err(reason) => {
                    log.record_event(clock, ControlPlaneEventKind::ConnectFailed { reason });
                }
            }
        }

        while !pending.is_empty() {
            let attempt_ids: Vec<ConnectionAttempt> =
                pending.iter().map(|(_, attempt)| *attempt).collect();
            let Some((attempt, completion)) = connector.next_completion(&attempt_ids) else {
                break;
            };
            let Some(index) = pending
                .iter()
                .position(|(_, pending_attempt)| *pending_attempt == attempt)
            else {
                continue;
            };
            let (candidate, _) = pending.remove(index);
            let authenticated = match completion {
                Ok(peer) => peer,
                Err(reason) => {
                    log.record_event(clock, ControlPlaneEventKind::ConnectFailed { reason });
                    continue;
                }
            };
            if authenticated.fingerprint != expected_peer {
                log.record_event(
                    clock,
                    ControlPlaneEventKind::ConnectFailed {
                        reason: ConnectFailureReason::FingerprintMismatch,
                    },
                );
                continue;
            }
            if let Err(reason) = authorize(authenticated.fingerprint) {
                log.record_event(clock, ControlPlaneEventKind::ConnectFailed { reason });
                continue;
            }
            let now = clock.now();
            for (_, pending_attempt) in pending.drain(..) {
                connector.cancel(pending_attempt);
            }
            log.record_event(clock, ControlPlaneEventKind::HandshakeOk);
            let result = RaceResult {
                generation,
                source: candidate.source,
                candidate,
                peer_fingerprint: authenticated.fingerprint,
                time_to_first_candidate_ms: elapsed_ms(first_candidate_at, started_at),
                time_to_authenticated_handshake_ms: elapsed_ms(now, started_at),
            };
            self.winner = Some(result.clone());
            return Ok(result);
        }
        Err(DiscoveryError::NoAuthenticatedPeer)
    }

    #[allow(clippy::too_many_arguments)]
    fn try_connect_with_link_internal<C: Clock, K: CandidateConnector, F>(
        &mut self,
        clock: &C,
        generation: u64,
        expected_peer: DeviceFingerprint,
        connector: &mut K,
        link: &mut LinkMachine,
        log: &mut ControlPlaneEventLog,
        authorize: F,
    ) -> Result<RaceResult, DiscoveryError>
    where
        F: FnMut(DeviceFingerprint) -> Result<(), ConnectFailureReason>,
    {
        if generation != self.generation {
            return Err(DiscoveryError::StaleGeneration {
                expected: self.generation,
                received: generation,
            });
        }
        if self.candidates.is_empty() {
            return Err(DiscoveryError::NoCandidates);
        }

        let transitions = link
            .begin_connecting(clock.now())
            .map_err(DiscoveryError::Link)?;
        log.record_link_transitions(clock, &transitions);

        let result =
            self.try_connect_internal(clock, generation, expected_peer, connector, log, authorize);
        match result {
            Ok(result) => {
                let transitions = link
                    .transport_connected(clock.now())
                    .map_err(DiscoveryError::Link)?;
                log.record_link_transitions(clock, &transitions);
                Ok(result)
            }
            Err(error) => {
                let transitions = link
                    .connection_failed(clock.now())
                    .map_err(DiscoveryError::Link)?;
                log.record_link_transitions(clock, &transitions);
                Err(error)
            }
        }
    }
}

fn elapsed_ms(later: Timestamp, earlier: Timestamp) -> u64 {
    later.as_millis().saturating_sub(earlier.as_millis())
}

fn pairing_failure_reason(error: PairingError) -> ConnectFailureReason {
    match error {
        PairingError::NotPaired => ConnectFailureReason::NotPaired,
        PairingError::Revoked => ConnectFailureReason::Revoked,
        _ => ConnectFailureReason::Transport,
    }
}
