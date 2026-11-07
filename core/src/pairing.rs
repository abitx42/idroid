//! Deterministic pairing contract and trust-store boundary.
//!
//! The core validates the QR shape, token lifetime, attempt limits, and trust
//! transitions. It does not generate randomness, render QR codes, perform TLS,
//! or choose a platform key store.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::clock::Clock;
use crate::control_plane::{
    ControlErrorCode, ControlPlaneEventKind, ControlPlaneEventLog, DeviceFingerprint,
};
use crate::session::Timestamp;

pub const PAIRING_TOKEN_SIZE: usize = 16;
pub const PAIRING_LIFETIME_MS: u64 = 120_000;
pub const PAIRING_MAX_ATTEMPTS: u8 = 3;
pub const PAIRING_LOCKOUT_MS: u64 = 5_000;
const MAX_REJECTED_TOKENS: usize = 32;
pub const MAX_SERVICE_HINT_BYTES: usize = 128;
pub const MAX_PEER_NAME_BYTES: usize = 128;

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct PairingToken([u8; PAIRING_TOKEN_SIZE]);

impl fmt::Debug for PairingToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PairingToken(REDACTED)")
    }
}

impl PairingToken {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PairingError> {
        let value: [u8; PAIRING_TOKEN_SIZE] =
            bytes.try_into().map_err(|_| PairingError::InvalidToken)?;
        let token = Self(value);
        token.validate()?;
        Ok(token)
    }

    pub const fn from_array(value: [u8; PAIRING_TOKEN_SIZE]) -> Self {
        Self(value)
    }

    pub const fn as_bytes(&self) -> &[u8; PAIRING_TOKEN_SIZE] {
        &self.0
    }

    pub fn validate(&self) -> Result<(), PairingError> {
        if self.0.iter().all(|byte| *byte == 0) {
            Err(PairingError::InvalidToken)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingQr {
    pub version: u16,
    pub mac_cert_fingerprint: DeviceFingerprint,
    pub one_time_token: PairingToken,
    pub expires_at: Timestamp,
    pub service_hint: String,
}

impl PairingQr {
    pub fn new(
        version: u16,
        mac_cert_fingerprint: DeviceFingerprint,
        one_time_token: PairingToken,
        expires_at: Timestamp,
        service_hint: impl Into<String>,
    ) -> Result<Self, PairingError> {
        let qr = Self {
            version,
            mac_cert_fingerprint,
            one_time_token,
            expires_at,
            service_hint: service_hint.into(),
        };
        qr.validate()?;
        Ok(qr)
    }

    pub fn validate(&self) -> Result<(), PairingError> {
        if self.version == 0 {
            return Err(PairingError::InvalidQr);
        }
        self.mac_cert_fingerprint
            .validate()
            .map_err(|_| PairingError::InvalidQr)?;
        self.one_time_token.validate()?;
        validate_text(
            &self.service_hint,
            MAX_SERVICE_HINT_BYTES,
            PairingError::InvalidQr,
        )?;
        Ok(())
    }

    pub fn pin_for_phone(
        &self,
        observed_mac_fingerprint: DeviceFingerprint,
    ) -> Result<PairingInvitation, PairingError> {
        self.validate()?;
        if observed_mac_fingerprint != self.mac_cert_fingerprint {
            return Err(PairingError::MacFingerprintMismatch);
        }
        Ok(PairingInvitation {
            version: self.version,
            mac_cert_fingerprint: self.mac_cert_fingerprint,
            one_time_token: self.one_time_token.clone(),
            expires_at: self.expires_at,
            service_hint: self.service_hint.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingInvitation {
    pub version: u16,
    pub mac_cert_fingerprint: DeviceFingerprint,
    pub one_time_token: PairingToken,
    pub expires_at: Timestamp,
    pub service_hint: String,
}

impl PairingInvitation {
    pub fn validate(&self) -> Result<(), PairingError> {
        PairingQr::new(
            self.version,
            self.mac_cert_fingerprint,
            self.one_time_token.clone(),
            self.expires_at,
            self.service_hint.clone(),
        )
        .map(|_| ())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairedPeer {
    pub peer_fingerprint: DeviceFingerprint,
    pub name: String,
    pub paired_at: Timestamp,
    pub revoked: bool,
}

impl PairedPeer {
    pub fn new(
        peer_fingerprint: DeviceFingerprint,
        name: impl Into<String>,
        paired_at: Timestamp,
    ) -> Result<Self, PairingError> {
        peer_fingerprint
            .validate()
            .map_err(|_| PairingError::InvalidPeer)?;
        let name = name.into();
        validate_text(&name, MAX_PEER_NAME_BYTES, PairingError::InvalidPeer)?;
        Ok(Self {
            peer_fingerprint,
            name,
            paired_at,
            revoked: false,
        })
    }
}

pub trait PairingStore {
    fn lookup(
        &self,
        peer_fingerprint: DeviceFingerprint,
    ) -> Result<Option<PairedPeer>, PairingStoreError>;
    fn save(&mut self, peer: PairedPeer) -> Result<(), PairingStoreError>;
    fn forget(&mut self, peer_fingerprint: DeviceFingerprint) -> Result<(), PairingStoreError>;
    fn revoke(&mut self, peer_fingerprint: DeviceFingerprint) -> Result<(), PairingStoreError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairingStoreError {
    Unavailable,
    InvalidRecord,
}

impl fmt::Display for PairingStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("pairing store unavailable"),
            Self::InvalidRecord => formatter.write_str("invalid pairing record"),
        }
    }
}

impl std::error::Error for PairingStoreError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryPairingStore {
    peers: BTreeMap<DeviceFingerprint, PairedPeer>,
}

impl InMemoryPairingStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn peers(&self) -> impl Iterator<Item = &PairedPeer> {
        self.peers.values()
    }
}

impl PairingStore for InMemoryPairingStore {
    fn lookup(
        &self,
        peer_fingerprint: DeviceFingerprint,
    ) -> Result<Option<PairedPeer>, PairingStoreError> {
        Ok(self.peers.get(&peer_fingerprint).cloned())
    }

    fn save(&mut self, peer: PairedPeer) -> Result<(), PairingStoreError> {
        peer.peer_fingerprint
            .validate()
            .map_err(|_| PairingStoreError::InvalidRecord)?;
        validate_text(&peer.name, MAX_PEER_NAME_BYTES, PairingError::InvalidPeer)
            .map_err(|_| PairingStoreError::InvalidRecord)?;
        self.peers.insert(peer.peer_fingerprint, peer);
        Ok(())
    }

    fn forget(&mut self, peer_fingerprint: DeviceFingerprint) -> Result<(), PairingStoreError> {
        self.peers.remove(&peer_fingerprint);
        Ok(())
    }

    fn revoke(&mut self, peer_fingerprint: DeviceFingerprint) -> Result<(), PairingStoreError> {
        if let Some(peer) = self.peers.get_mut(&peer_fingerprint) {
            peer.revoked = true;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairingError {
    InvalidQr,
    InvalidToken,
    InvalidPeer,
    Expired,
    TokenAlreadyUsed,
    TokenInvalid,
    AttemptsExceeded,
    LockedOut,
    ListenerClosed,
    MacFingerprintMismatch,
    NotPaired,
    Revoked,
    Store(PairingStoreError),
}

impl fmt::Display for PairingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQr => formatter.write_str("invalid pairing QR"),
            Self::InvalidToken => formatter.write_str("invalid pairing token"),
            Self::InvalidPeer => formatter.write_str("invalid peer record"),
            Self::Expired => formatter.write_str("pairing token expired"),
            Self::TokenAlreadyUsed => formatter.write_str("pairing token already used"),
            Self::TokenInvalid => formatter.write_str("pairing token rejected"),
            Self::AttemptsExceeded => formatter.write_str("pairing attempts exceeded"),
            Self::LockedOut => formatter.write_str("presented token temporarily locked out"),
            Self::ListenerClosed => formatter.write_str("pairing listener is closed"),
            Self::MacFingerprintMismatch => {
                formatter.write_str("mac certificate fingerprint differs from QR")
            }
            Self::NotPaired => formatter.write_str("peer is not paired"),
            Self::Revoked => formatter.write_str("peer is revoked"),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PairingError {}

#[derive(Clone, Debug)]
struct ActivePairing {
    token: PairingToken,
    expires_at: Timestamp,
    rejected: BTreeMap<PairingToken, RejectedToken>,
}

#[derive(Clone, Debug)]
struct RejectedToken {
    attempts: u8,
    retry_at: Timestamp,
}

pub struct PairingAuthority<S: PairingStore> {
    version: u16,
    mac_cert_fingerprint: DeviceFingerprint,
    store: S,
    active: Option<ActivePairing>,
    consumed_tokens: BTreeSet<PairingToken>,
}

impl<S: PairingStore> PairingAuthority<S> {
    pub fn new(
        version: u16,
        mac_cert_fingerprint: DeviceFingerprint,
        store: S,
    ) -> Result<Self, PairingError> {
        if version == 0 {
            return Err(PairingError::InvalidQr);
        }
        mac_cert_fingerprint
            .validate()
            .map_err(|_| PairingError::InvalidQr)?;
        Ok(Self {
            version,
            mac_cert_fingerprint,
            store,
            active: None,
            consumed_tokens: BTreeSet::new(),
        })
    }

    pub const fn mac_cert_fingerprint(&self) -> DeviceFingerprint {
        self.mac_cert_fingerprint
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    pub fn into_store(self) -> S {
        self.store
    }

    pub fn issue_qr(
        &mut self,
        one_time_token: PairingToken,
        service_hint: impl Into<String>,
        now: Timestamp,
    ) -> Result<PairingQr, PairingError> {
        one_time_token.validate()?;
        if self.consumed_tokens.contains(&one_time_token) {
            return Err(PairingError::TokenAlreadyUsed);
        }
        let expires_at =
            Timestamp::from_millis(now.as_millis().saturating_add(PAIRING_LIFETIME_MS));
        let qr = PairingQr::new(
            self.version,
            self.mac_cert_fingerprint,
            one_time_token.clone(),
            expires_at,
            service_hint,
        )?;
        self.active = Some(ActivePairing {
            token: one_time_token,
            expires_at,
            rejected: BTreeMap::new(),
        });
        Ok(qr)
    }

    pub fn listener_is_open(&self, now: Timestamp) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| now < active.expires_at)
    }

    /// Close the pairing listener when the Mac QR screen is dismissed. The
    /// platform UI must call this even before the two-minute expiry.
    pub fn close_listener(&mut self) {
        self.active = None;
    }

    pub fn expire(&mut self, now: Timestamp) -> bool {
        if self
            .active
            .as_ref()
            .is_some_and(|active| now >= active.expires_at)
        {
            self.active = None;
            true
        } else {
            false
        }
    }

    pub fn consume_invitation(
        &mut self,
        invitation: &PairingInvitation,
        phone_fingerprint: DeviceFingerprint,
        name: impl Into<String>,
        now: Timestamp,
    ) -> Result<PairedPeer, PairingError> {
        invitation.validate()?;
        if invitation.mac_cert_fingerprint != self.mac_cert_fingerprint {
            return Err(PairingError::MacFingerprintMismatch);
        }
        self.consume_token(&invitation.one_time_token, phone_fingerprint, name, now)
    }

    pub fn consume_token(
        &mut self,
        token: &PairingToken,
        phone_fingerprint: DeviceFingerprint,
        name: impl Into<String>,
        now: Timestamp,
    ) -> Result<PairedPeer, PairingError> {
        if self.expire(now) {
            return Err(PairingError::Expired);
        }
        if self.consumed_tokens.contains(token) {
            return Err(PairingError::TokenAlreadyUsed);
        }
        if self.active.is_none() {
            return Err(PairingError::ListenerClosed);
        }

        if self
            .active
            .as_ref()
            .is_some_and(|active| active.token != *token)
        {
            let active = self.active.as_mut().expect("checked above");
            // Only the presented incorrect value is throttled. LAN callers
            // cannot spend the QR owner's budget or close its listener.
            if !active.rejected.contains_key(token) && active.rejected.len() >= MAX_REJECTED_TOKENS
            {
                if let Some(oldest) = active.rejected.keys().next().cloned() {
                    active.rejected.remove(&oldest);
                }
            }
            let rejected = active
                .rejected
                .entry(token.clone())
                .or_insert(RejectedToken {
                    attempts: 0,
                    retry_at: now,
                });
            if now < rejected.retry_at {
                return Err(PairingError::LockedOut);
            }
            if rejected.attempts >= PAIRING_MAX_ATTEMPTS {
                rejected.attempts = 0;
            }
            rejected.attempts += 1;
            if rejected.attempts >= PAIRING_MAX_ATTEMPTS {
                rejected.retry_at =
                    Timestamp::from_millis(now.as_millis().saturating_add(PAIRING_LOCKOUT_MS));
                return Err(PairingError::AttemptsExceeded);
            }
            return Err(PairingError::TokenInvalid);
        }

        let peer = PairedPeer::new(phone_fingerprint, name, now)?;
        self.store.save(peer.clone()).map_err(PairingError::Store)?;
        self.consumed_tokens.insert(token.clone());
        self.active = None;
        Ok(peer)
    }
}

pub fn authorize_reconnect<S: PairingStore>(
    store: &S,
    peer_fingerprint: DeviceFingerprint,
) -> Result<PairedPeer, PairingError> {
    match store
        .lookup(peer_fingerprint)
        .map_err(PairingError::Store)?
    {
        None => Err(PairingError::NotPaired),
        Some(peer) if peer.revoked => Err(PairingError::Revoked),
        Some(peer) => Ok(peer),
    }
}

pub fn connect_failure_for_pairing(error: &PairingError) -> ControlErrorCode {
    match error {
        PairingError::NotPaired => ControlErrorCode::NotPaired,
        PairingError::Revoked => ControlErrorCode::NotPaired,
        _ => ControlErrorCode::Internal,
    }
}

fn validate_text<E>(value: &str, maximum: usize, error: E) -> Result<(), E> {
    if value.is_empty() || value.len() > maximum {
        Err(error)
    } else {
        Ok(())
    }
}

impl<S: PairingStore> PairingAuthority<S> {
    pub fn issue_qr_with_clock<C: Clock>(
        &mut self,
        one_time_token: PairingToken,
        service_hint: impl Into<String>,
        clock: &C,
    ) -> Result<PairingQr, PairingError> {
        self.issue_qr(one_time_token, service_hint, clock.now())
    }

    pub fn issue_qr_logged<C: Clock>(
        &mut self,
        one_time_token: PairingToken,
        service_hint: impl Into<String>,
        clock: &C,
        log: &mut ControlPlaneEventLog,
    ) -> Result<PairingQr, PairingError> {
        let qr = self.issue_qr(one_time_token, service_hint, clock.now())?;
        log.record_event(clock, ControlPlaneEventKind::PairingTokenIssued);
        Ok(qr)
    }

    pub fn expire_logged<C: Clock>(&mut self, clock: &C, log: &mut ControlPlaneEventLog) -> bool {
        let expired = self.expire(clock.now());
        if expired {
            log.record_event(clock, ControlPlaneEventKind::PairingTokenExpired);
        }
        expired
    }

    pub fn consume_invitation_logged<C: Clock>(
        &mut self,
        invitation: &PairingInvitation,
        phone_fingerprint: DeviceFingerprint,
        name: impl Into<String>,
        clock: &C,
        log: &mut ControlPlaneEventLog,
    ) -> Result<PairedPeer, PairingError> {
        let result = self.consume_invitation(invitation, phone_fingerprint, name, clock.now());
        match &result {
            Ok(_) => log.record_event(clock, ControlPlaneEventKind::PairingTokenConsumed),
            Err(PairingError::Expired) => {
                log.record_event(clock, ControlPlaneEventKind::PairingTokenExpired)
            }
            Err(_) => {}
        }
        result
    }
}
