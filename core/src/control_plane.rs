//! Typed contract and deterministic state machines for the Android/macOS
//! control plane.
//!
//! This module deliberately stops before wire encoding, cryptography, sockets,
//! and platform APIs. It gives both adapters one vocabulary for the authenticated
//! channel, capability negotiation, link health, and media-backend changes.

use std::fmt;

use crate::clock::Clock;
use crate::ids::SessionId;
use crate::session::Timestamp;

pub const CONTROL_PROTOCOL_VERSION_V1: u16 = 1;
pub const KEY_FINGERPRINT_SIZE: usize = 32;
pub const MAX_APP_VERSION_BYTES: usize = 64;
pub const MAX_FEATURE_BACKENDS: usize = 2;
pub const HEARTBEAT_INTERVAL_MS: u64 = 1_000;
pub const DEGRADE_AFTER_MS: u64 = 3_000;
pub const LOSE_AFTER_MS: u64 = 6_000;

/// A public, non-secret fingerprint of a device's long-term transport key.
///
/// The transport owns the actual key and authentication. The control plane only
/// compares this fingerprint with the authenticated transport identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceFingerprint([u8; KEY_FINGERPRINT_SIZE]);

pub type KeyFingerprint = DeviceFingerprint;

impl DeviceFingerprint {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ContractError> {
        let value: [u8; KEY_FINGERPRINT_SIZE] = bytes
            .try_into()
            .map_err(|_| ContractError::InvalidField("device_id"))?;
        let fingerprint = Self(value);
        fingerprint.validate()?;
        Ok(fingerprint)
    }

    pub const fn from_array(value: [u8; KEY_FINGERPRINT_SIZE]) -> Self {
        Self(value)
    }

    pub const fn as_bytes(self) -> [u8; KEY_FINGERPRINT_SIZE] {
        self.0
    }

    pub const fn is_valid(self) -> bool {
        !self.is_zero()
    }

    const fn is_zero(self) -> bool {
        let mut index = 0;
        while index < self.0.len() {
            if self.0[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(ContractError::InvalidField("device_id"))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionRange {
    pub min: u16,
    pub max: u16,
}

impl VersionRange {
    pub fn new(min: u16, max: u16) -> Result<Self, ContractError> {
        let range = Self { min, max };
        range.validate()?;
        Ok(range)
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.min == 0 || self.max == 0 || self.min > self.max {
            return Err(ContractError::InvalidField("version_range"));
        }
        Ok(())
    }

    pub const fn highest_common(self, other: Self) -> Option<u16> {
        let minimum = if self.min > other.min {
            self.min
        } else {
            other.min
        };
        let maximum = if self.max < other.max {
            self.max
        } else {
            other.max
        };
        if minimum <= maximum {
            Some(maximum)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerRole {
    Android,
    Mac,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Feature {
    Video,
    Input,
}

impl Feature {
    pub const ALL: [Self; 2] = [Self::Video, Self::Input];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    Helper,
    MediaProjection,
    Accessibility,
}

impl Backend {
    pub const fn supports(self, feature: Feature) -> bool {
        matches!(
            (feature, self),
            (Feature::Video, Self::Helper | Self::MediaProjection)
                | (Feature::Input, Self::Helper | Self::Accessibility)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateSourceKind {
    CachedAddress,
    Bonjour,
    GatewayProbe,
    Ble,
}

impl CandidateSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CachedAddress => "cached_address",
            Self::Bonjour => "bonjour",
            Self::GatewayProbe => "gateway_probe",
            Self::Ble => "ble",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectFailureReason {
    Timeout,
    Transport,
    FingerprintMismatch,
    NotPaired,
    Revoked,
    Cancelled,
    NetworkChanged,
    ListenerClosed,
    TokenInvalid,
    AttemptsExceeded,
}

impl ConnectFailureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::FingerprintMismatch => "fingerprint_mismatch",
            Self::NotPaired => "not_paired",
            Self::Revoked => "revoked",
            Self::Cancelled => "cancelled",
            Self::NetworkChanged => "network_changed",
            Self::ListenerClosed => "listener_closed",
            Self::TokenInvalid => "token_invalid",
            Self::AttemptsExceeded => "attempts_exceeded",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityStatus {
    Available,
    NeedsUserAction,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityReason {
    MediaProjectionConsentRequired,
    AccessibilityPermissionRequired,
    WirelessDebuggingRequired,
    HelperStopped,
    HelperNotInstalled,
    PermissionDenied,
    UnsupportedDevice,
    BackgroundRestricted,
    Unknown,
}

impl CapabilityReason {
    pub const fn user_action(self) -> Option<UserAction> {
        match self {
            Self::MediaProjectionConsentRequired => Some(UserAction::GrantMediaProjection),
            Self::AccessibilityPermissionRequired => Some(UserAction::GrantAccessibility),
            Self::WirelessDebuggingRequired => Some(UserAction::EnableWirelessDebugging),
            Self::HelperStopped => Some(UserAction::StartHelper),
            Self::HelperNotInstalled => Some(UserAction::InstallHelper),
            Self::PermissionDenied
            | Self::UnsupportedDevice
            | Self::BackgroundRestricted
            | Self::Unknown => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserAction {
    GrantMediaProjection,
    GrantAccessibility,
    EnableWirelessDebugging,
    StartHelper,
    InstallHelper,
    UnlockDevice,
    PairDevice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendCapability {
    pub backend: Backend,
    pub status: CapabilityStatus,
    pub reason: Option<CapabilityReason>,
}

impl BackendCapability {
    pub const fn available(backend: Backend) -> Self {
        Self {
            backend,
            status: CapabilityStatus::Available,
            reason: None,
        }
    }

    pub const fn needs_user_action(backend: Backend, reason: CapabilityReason) -> Self {
        Self {
            backend,
            status: CapabilityStatus::NeedsUserAction,
            reason: Some(reason),
        }
    }

    pub const fn unavailable(backend: Backend, reason: CapabilityReason) -> Self {
        Self {
            backend,
            status: CapabilityStatus::Unavailable,
            reason: Some(reason),
        }
    }

    pub fn validate(self, feature: Feature) -> Result<(), ContractError> {
        if !self.backend.supports(feature) {
            return Err(ContractError::IncompatibleBackend {
                feature,
                backend: self.backend,
            });
        }

        match (self.status, self.reason) {
            (CapabilityStatus::Available, None) => Ok(()),
            (CapabilityStatus::Available, Some(_)) => {
                Err(ContractError::InvalidField("available_reason"))
            }
            (CapabilityStatus::NeedsUserAction, Some(reason)) if reason.user_action().is_some() => {
                Ok(())
            }
            (CapabilityStatus::NeedsUserAction, _) => {
                Err(ContractError::InvalidField("needs_user_action_reason"))
            }
            (CapabilityStatus::Unavailable, Some(_)) => Ok(()),
            (CapabilityStatus::Unavailable, None) => {
                Err(ContractError::InvalidField("unavailable_reason"))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureCapabilities {
    pub feature: Feature,
    pub backends: Vec<BackendCapability>,
}

impl FeatureCapabilities {
    pub fn new(
        feature: Feature,
        backends: impl Into<Vec<BackendCapability>>,
    ) -> Result<Self, ContractError> {
        let value = Self {
            feature,
            backends: backends.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.backends.is_empty() || self.backends.len() > MAX_FEATURE_BACKENDS {
            return Err(ContractError::InvalidField("backends"));
        }
        for (index, capability) in self.backends.iter().enumerate() {
            capability.validate(self.feature)?;
            if self
                .backends
                .iter()
                .take(index)
                .any(|previous| previous.backend == capability.backend)
            {
                return Err(ContractError::DuplicateBackend(capability.backend));
            }
        }
        Ok(())
    }

    pub fn find(&self, backend: Backend) -> Option<BackendCapability> {
        self.backends
            .iter()
            .copied()
            .find(|candidate| candidate.backend == backend)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capabilities {
    pub features: Vec<FeatureCapabilities>,
}

impl Capabilities {
    pub fn new(
        video: FeatureCapabilities,
        input: FeatureCapabilities,
    ) -> Result<Self, ContractError> {
        let value = Self {
            features: vec![video, input],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.features.len() != Feature::ALL.len() {
            return Err(ContractError::InvalidField("features"));
        }
        for (index, feature) in self.features.iter().enumerate() {
            feature.validate()?;
            if self
                .features
                .iter()
                .take(index)
                .any(|previous| previous.feature == feature.feature)
            {
                return Err(ContractError::DuplicateFeature(feature.feature));
            }
        }
        for expected in Feature::ALL {
            if !self
                .features
                .iter()
                .any(|feature| feature.feature == expected)
            {
                return Err(ContractError::MissingFeature(expected));
            }
        }
        Ok(())
    }

    pub fn for_feature(&self, feature: Feature) -> Option<&FeatureCapabilities> {
        self.features
            .iter()
            .find(|capabilities| capabilities.feature == feature)
    }

    /// Select an available backend, or report the user action needed to make a
    /// backend available. A preferred backend is tried first, but an already
    /// available fallback wins over a preferred backend that needs a prompt.
    pub fn select(
        &self,
        feature: Feature,
        preferred: Option<Backend>,
    ) -> Result<BackendDecision, ContractError> {
        self.validate()?;
        let capabilities = self
            .for_feature(feature)
            .ok_or(ContractError::MissingFeature(feature))?;
        if let Some(backend) = preferred {
            if !backend.supports(feature) {
                return Err(ContractError::IncompatibleBackend { feature, backend });
            }
        }

        let mut ordered = Vec::with_capacity(capabilities.backends.len());
        if let Some(preferred) = preferred {
            if let Some(candidate) = capabilities.find(preferred) {
                ordered.push(candidate);
            }
        }
        for candidate in &capabilities.backends {
            if !ordered
                .iter()
                .any(|selected| selected.backend == candidate.backend)
            {
                ordered.push(*candidate);
            }
        }

        if let Some(candidate) = ordered
            .iter()
            .find(|candidate| candidate.status == CapabilityStatus::Available)
        {
            return Ok(BackendDecision::Available(candidate.backend));
        }
        if let Some(candidate) = ordered
            .iter()
            .find(|candidate| candidate.status == CapabilityStatus::NeedsUserAction)
        {
            let reason = candidate.reason.expect("validated user-action capability");
            return Ok(BackendDecision::NeedsUserAction {
                backend: candidate.backend,
                action: reason.user_action().expect("validated user action"),
            });
        }
        let reason = ordered
            .first()
            .and_then(|candidate| candidate.reason)
            .expect("validated unavailable capability");
        Ok(BackendDecision::Unavailable { reason })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendDecision {
    Available(Backend),
    NeedsUserAction {
        backend: Backend,
        action: UserAction,
    },
    Unavailable {
        reason: CapabilityReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hello {
    pub version_min: u16,
    pub version_max: u16,
    pub device_id: DeviceFingerprint,
    pub role: PeerRole,
    pub app_version: String,
    pub last_session_id: Option<SessionId>,
}

impl Hello {
    pub fn new(
        versions: VersionRange,
        device_id: DeviceFingerprint,
        role: PeerRole,
        app_version: impl Into<String>,
        last_session_id: Option<SessionId>,
    ) -> Result<Self, ContractError> {
        let hello = Self {
            version_min: versions.min,
            version_max: versions.max,
            device_id,
            role,
            app_version: app_version.into(),
            last_session_id,
        };
        hello.validate()?;
        Ok(hello)
    }

    pub fn versions(&self) -> VersionRange {
        VersionRange {
            min: self.version_min,
            max: self.version_max,
        }
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        self.versions().validate()?;
        self.device_id.validate()?;
        validate_text(&self.app_version, "app_version", MAX_APP_VERSION_BYTES)?;
        if self
            .last_session_id
            .is_some_and(|session_id| !session_id.is_valid())
        {
            return Err(ContractError::InvalidField("last_session_id"));
        }
        Ok(())
    }

    pub fn verify_transport_identity(
        &self,
        authenticated_fingerprint: DeviceFingerprint,
    ) -> Result<(), ControlError> {
        if self.device_id == authenticated_fingerprint {
            Ok(())
        } else {
            Err(ControlError::identity_mismatch())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelloAck {
    pub version: u16,
}

impl HelloAck {
    pub const fn new(version: u16) -> Result<Self, ContractError> {
        if version == 0 {
            Err(ContractError::InvalidField("version"))
        } else {
            Ok(Self { version })
        }
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.version == 0 {
            Err(ContractError::InvalidField("version"))
        } else {
            Ok(())
        }
    }
}

/// Negotiate the highest common version after the caller has authenticated the
/// transport. A mismatch must be sent as `Error(version_unsupported)` and the
/// channel must then close.
pub fn negotiate_versions(
    local: VersionRange,
    hello: &Hello,
    authenticated_fingerprint: DeviceFingerprint,
) -> Result<HelloAck, ControlError> {
    if local.validate().is_err() || hello.validate().is_err() {
        return Err(ControlError::internal());
    }
    if hello
        .verify_transport_identity(authenticated_fingerprint)
        .is_err()
    {
        return Err(ControlError::identity_mismatch());
    }
    let version = local.highest_common(hello.versions());
    match version {
        Some(version) => Ok(HelloAck { version }),
        None => Err(ControlError::version_unsupported()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRequest {
    pub request_id: u64,
    pub feature: Feature,
    pub preferred_backend: Option<Backend>,
}

impl SessionRequest {
    pub fn new(
        request_id: u64,
        feature: Feature,
        preferred_backend: Option<Backend>,
    ) -> Result<Self, ContractError> {
        let request = Self {
            request_id,
            feature,
            preferred_backend,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.request_id == 0 {
            return Err(ContractError::InvalidField("request_id"));
        }
        if let Some(backend) = self.preferred_backend {
            if !backend.supports(self.feature) {
                return Err(ContractError::IncompatibleBackend {
                    feature: self.feature,
                    backend,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionOutcome {
    Accepted { backend: Backend },
    NeedsUserAction { action: UserAction },
    Rejected { code: ControlErrorCode },
}

impl SessionOutcome {
    pub fn validate(self, feature: Feature) -> Result<(), ContractError> {
        match self {
            Self::Accepted { backend } if backend.supports(feature) => Ok(()),
            Self::Accepted { backend } => {
                Err(ContractError::IncompatibleBackend { feature, backend })
            }
            Self::NeedsUserAction { .. } | Self::Rejected { .. } => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionResult {
    pub request_id: u64,
    pub feature: Feature,
    pub outcome: SessionOutcome,
}

impl SessionResult {
    pub fn new(
        request_id: u64,
        feature: Feature,
        outcome: SessionOutcome,
    ) -> Result<Self, ContractError> {
        let result = Self {
            request_id,
            feature,
            outcome,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.request_id == 0 {
            return Err(ContractError::InvalidField("request_id"));
        }
        self.outcome.validate(self.feature)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkState {
    Searching,
    Connecting,
    Handshaking,
    Ready,
    Degraded,
    Lost,
}

impl LinkState {
    const fn reconnect_measured(self) -> bool {
        matches!(
            self,
            Self::Degraded | Self::Lost | Self::Searching | Self::Connecting | Self::Handshaking
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Searching => "searching",
            Self::Connecting => "connecting",
            Self::Handshaking => "handshaking",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Lost => "lost",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendSelection {
    pub feature: Feature,
    pub backend: Backend,
}

impl BackendSelection {
    pub const fn new(feature: Feature, backend: Backend) -> Result<Self, ContractError> {
        if backend.supports(feature) {
            Ok(Self { feature, backend })
        } else {
            Err(ContractError::IncompatibleBackend { feature, backend })
        }
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.backend.supports(self.feature) {
            Ok(())
        } else {
            Err(ContractError::IncompatibleBackend {
                feature: self.feature,
                backend: self.backend,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Heartbeat {
    pub seq: u64,
    pub link_state: LinkState,
    pub active_backend: Option<BackendSelection>,
}

impl Heartbeat {
    pub fn new(
        seq: u64,
        link_state: LinkState,
        active_backend: Option<BackendSelection>,
    ) -> Result<Self, ContractError> {
        let heartbeat = Self {
            seq,
            link_state,
            active_backend,
        };
        heartbeat.validate()?;
        Ok(heartbeat)
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.seq == 0 {
            return Err(ContractError::InvalidField("seq"));
        }
        if let Some(selection) = self.active_backend {
            selection.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendChangeReason {
    HelperLost,
    HelperRecovered,
    UserSelected,
    PermissionGranted,
    CapabilityChanged,
    ThermalLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendChanged {
    pub feature: Feature,
    pub from: Backend,
    pub to: Backend,
    pub reason: BackendChangeReason,
}

impl BackendChanged {
    pub fn new(
        feature: Feature,
        from: Backend,
        to: Backend,
        reason: BackendChangeReason,
    ) -> Result<Self, ContractError> {
        let change = Self {
            feature,
            from,
            to,
            reason,
        };
        change.validate()?;
        Ok(change)
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.from == self.to {
            return Err(ContractError::InvalidField("backend_change"));
        }
        if !self.from.supports(self.feature) {
            return Err(ContractError::IncompatibleBackend {
                feature: self.feature,
                backend: self.from,
            });
        }
        if !self.to.supports(self.feature) {
            return Err(ContractError::IncompatibleBackend {
                feature: self.feature,
                backend: self.to,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlErrorCode {
    VersionUnsupported,
    IdentityMismatch,
    NotPaired,
    BackendUnavailable,
    UserActionRequired,
    PermissionDenied,
    HelperLost,
    Timeout,
    Internal,
}

impl ControlErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VersionUnsupported => "version_unsupported",
            Self::IdentityMismatch => "identity_mismatch",
            Self::NotPaired => "not_paired",
            Self::BackendUnavailable => "backend_unavailable",
            Self::UserActionRequired => "user_action_required",
            Self::PermissionDenied => "permission_denied",
            Self::HelperLost => "helper_lost",
            Self::Timeout => "timeout",
            Self::Internal => "internal",
        }
    }
}

impl fmt::Display for ControlErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlError {
    pub code: ControlErrorCode,
    pub retryable: bool,
    pub user_action: Option<UserAction>,
}

impl ControlError {
    pub const fn new(
        code: ControlErrorCode,
        retryable: bool,
        user_action: Option<UserAction>,
    ) -> Self {
        Self {
            code,
            retryable,
            user_action,
        }
    }

    pub const fn identity_mismatch() -> Self {
        Self::new(ControlErrorCode::IdentityMismatch, false, None)
    }

    pub const fn version_unsupported() -> Self {
        Self::new(ControlErrorCode::VersionUnsupported, false, None)
    }

    pub const fn internal() -> Self {
        Self::new(ControlErrorCode::Internal, false, None)
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.code == ControlErrorCode::UserActionRequired && self.user_action.is_none() {
            return Err(ContractError::InvalidField("user_action"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bye {
    pub code: Option<ControlErrorCode>,
    pub retryable: bool,
    pub user_action: Option<UserAction>,
}

impl Bye {
    pub const fn normal() -> Self {
        Self {
            code: None,
            retryable: false,
            user_action: None,
        }
    }

    pub const fn from_error(error: ControlError) -> Self {
        Self {
            code: Some(error.code),
            retryable: error.retryable,
            user_action: error.user_action,
        }
    }

    pub fn validate(self) -> Result<(), ContractError> {
        if self.code.is_none() && (self.retryable || self.user_action.is_some()) {
            return Err(ContractError::InvalidField("bye_code"));
        }
        if self.code == Some(ControlErrorCode::UserActionRequired) && self.user_action.is_none() {
            return Err(ContractError::InvalidField("user_action"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlMessageKind {
    Hello,
    HelloAck,
    Capabilities,
    CapabilitiesUpdate,
    SessionRequest,
    SessionResult,
    Heartbeat,
    BackendChanged,
    Error,
    Bye,
}

impl ControlMessageKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::HelloAck => 2,
            Self::Capabilities => 3,
            Self::CapabilitiesUpdate => 4,
            Self::SessionRequest => 5,
            Self::SessionResult => 6,
            Self::Heartbeat => 7,
            Self::BackendChanged => 8,
            Self::Error => 9,
            Self::Bye => 10,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::HelloAck => "hello_ack",
            Self::Capabilities => "capabilities",
            Self::CapabilitiesUpdate => "capabilities_update",
            Self::SessionRequest => "session_request",
            Self::SessionResult => "session_result",
            Self::Heartbeat => "heartbeat",
            Self::BackendChanged => "backend_changed",
            Self::Error => "error",
            Self::Bye => "bye",
        }
    }
}

/// The v1 control-plane messages. Direction is checked by the platform adapter
/// because this portable contract is shared by both peers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlMessage {
    Hello(Hello),
    HelloAck(HelloAck),
    Capabilities(Capabilities),
    CapabilitiesUpdate(Capabilities),
    SessionRequest(SessionRequest),
    SessionResult(SessionResult),
    Heartbeat(Heartbeat),
    BackendChanged(BackendChanged),
    Error(ControlError),
    Bye(Bye),
}

impl ControlMessage {
    pub const fn kind(&self) -> ControlMessageKind {
        match self {
            Self::Hello(_) => ControlMessageKind::Hello,
            Self::HelloAck(_) => ControlMessageKind::HelloAck,
            Self::Capabilities(_) => ControlMessageKind::Capabilities,
            Self::CapabilitiesUpdate(_) => ControlMessageKind::CapabilitiesUpdate,
            Self::SessionRequest(_) => ControlMessageKind::SessionRequest,
            Self::SessionResult(_) => ControlMessageKind::SessionResult,
            Self::Heartbeat(_) => ControlMessageKind::Heartbeat,
            Self::BackendChanged(_) => ControlMessageKind::BackendChanged,
            Self::Error(_) => ControlMessageKind::Error,
            Self::Bye(_) => ControlMessageKind::Bye,
        }
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Hello(message) => message.validate(),
            Self::HelloAck(message) => message.validate(),
            Self::Capabilities(message) | Self::CapabilitiesUpdate(message) => message.validate(),
            Self::SessionRequest(message) => message.validate(),
            Self::SessionResult(message) => message.validate(),
            Self::Heartbeat(message) => message.validate(),
            Self::BackendChanged(message) => message.validate(),
            Self::Error(message) => message.validate(),
            Self::Bye(message) => message.validate(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    InvalidField(&'static str),
    FieldTooLarge {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    IncompatibleBackend {
        feature: Feature,
        backend: Backend,
    },
    DuplicateBackend(Backend),
    DuplicateFeature(Feature),
    MissingFeature(Feature),
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(formatter, "invalid {field}"),
            Self::FieldTooLarge {
                field,
                actual,
                maximum,
            } => write!(formatter, "{field} is {actual} bytes, maximum is {maximum}"),
            Self::IncompatibleBackend { feature, backend } => {
                write!(formatter, "backend {backend:?} cannot provide {feature:?}")
            }
            Self::DuplicateBackend(backend) => write!(formatter, "duplicate backend {backend:?}"),
            Self::DuplicateFeature(feature) => write!(formatter, "duplicate feature {feature:?}"),
            Self::MissingFeature(feature) => write!(formatter, "missing feature {feature:?}"),
        }
    }
}

impl std::error::Error for ContractError {}

fn validate_text(value: &str, field: &'static str, maximum: usize) -> Result<(), ContractError> {
    if value.is_empty() {
        return Err(ContractError::InvalidField(field));
    }
    if value.len() > maximum {
        return Err(ContractError::FieldTooLarge {
            field,
            actual: value.len(),
            maximum,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkTransition {
    pub from: LinkState,
    pub to: LinkState,
    pub at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlPlaneEventKind {
    LinkTransition { from: LinkState, to: LinkState },
    Message { kind: ControlMessageKind },
    NetworkChanged,
    CandidateFound { source: CandidateSourceKind },
    ConnectStarted,
    ConnectFailed { reason: ConnectFailureReason },
    HandshakeOk,
    PairingTokenIssued,
    PairingTokenConsumed,
    PairingTokenExpired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlPlaneEvent {
    pub at: Timestamp,
    pub kind: ControlPlaneEventKind,
}

impl ControlPlaneEvent {
    pub fn to_ndjson(&self) -> String {
        match self.kind {
            ControlPlaneEventKind::LinkTransition { from, to } => format!(
                "{{\"at_ms\":{},\"event\":\"link_transition\",\"from\":\"{}\",\"to\":\"{}\"}}",
                self.at.as_millis(),
                from.as_str(),
                to.as_str()
            ),
            ControlPlaneEventKind::Message { kind } => format!(
                "{{\"at_ms\":{},\"event\":\"message\",\"kind\":\"{}\"}}",
                self.at.as_millis(),
                kind.as_str()
            ),
            ControlPlaneEventKind::NetworkChanged => {
                format!(
                    "{{\"at_ms\":{},\"event\":\"network_changed\"}}",
                    self.at.as_millis()
                )
            }
            ControlPlaneEventKind::CandidateFound { source } => format!(
                "{{\"at_ms\":{},\"event\":\"candidate_found\",\"source\":\"{}\"}}",
                self.at.as_millis(),
                source.as_str()
            ),
            ControlPlaneEventKind::ConnectStarted => {
                format!(
                    "{{\"at_ms\":{},\"event\":\"connect_started\"}}",
                    self.at.as_millis()
                )
            }
            ControlPlaneEventKind::ConnectFailed { reason } => format!(
                "{{\"at_ms\":{},\"event\":\"connect_failed\",\"reason\":\"{}\"}}",
                self.at.as_millis(),
                reason.as_str()
            ),
            ControlPlaneEventKind::HandshakeOk => {
                format!(
                    "{{\"at_ms\":{},\"event\":\"handshake_ok\"}}",
                    self.at.as_millis()
                )
            }
            ControlPlaneEventKind::PairingTokenIssued => format!(
                "{{\"at_ms\":{},\"event\":\"pairing_token_issued\"}}",
                self.at.as_millis()
            ),
            ControlPlaneEventKind::PairingTokenConsumed => format!(
                "{{\"at_ms\":{},\"event\":\"pairing_token_consumed\"}}",
                self.at.as_millis()
            ),
            ControlPlaneEventKind::PairingTokenExpired => format!(
                "{{\"at_ms\":{},\"event\":\"pairing_token_expired\"}}",
                self.at.as_millis()
            ),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ControlPlaneEventLog {
    events: Vec<ControlPlaneEvent>,
}

impl ControlPlaneEventLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[ControlPlaneEvent] {
        &self.events
    }

    pub fn to_ndjson(&self) -> String {
        self.events
            .iter()
            .map(ControlPlaneEvent::to_ndjson)
            .collect::<Vec<_>>()
            .join("\n")
            + if self.events.is_empty() { "" } else { "\n" }
    }

    pub fn record_event<C: Clock>(&mut self, clock: &C, kind: ControlPlaneEventKind) {
        self.events.push(ControlPlaneEvent {
            at: clock.now(),
            kind,
        });
    }

    /// Record every transition returned by a link operation using the same
    /// host clock that drove that operation. The event log intentionally omits
    /// message payloads and records only stable message kinds.
    pub fn record_link_transitions<C: Clock>(&mut self, clock: &C, transitions: &[LinkTransition]) {
        let at = clock.now();
        self.events
            .extend(transitions.iter().map(|transition| ControlPlaneEvent {
                at,
                kind: ControlPlaneEventKind::LinkTransition {
                    from: transition.from,
                    to: transition.to,
                },
            }));
    }

    pub fn record_message<C: Clock>(
        &mut self,
        clock: &C,
        message: &ControlMessage,
    ) -> Result<(), ContractError> {
        message.validate()?;
        self.record_event(
            clock,
            ControlPlaneEventKind::Message {
                kind: message.kind(),
            },
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkSnapshot {
    pub state: LinkState,
    pub state_since: Timestamp,
    pub last_heartbeat_at: Option<Timestamp>,
    pub missed_heartbeats: u64,
    pub session_id: Option<SessionId>,
    pub reconnect_time_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkError {
    ClockRewound {
        last: Timestamp,
        now: Timestamp,
    },
    InvalidTransition {
        state: LinkState,
        operation: &'static str,
    },
    InvalidHeartbeat,
    HeartbeatSequenceRewound {
        last: u64,
        received: u64,
    },
    InvalidSessionId,
    SessionChanged {
        existing: SessionId,
        received: SessionId,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClockRewound { last, now } => {
                write!(
                    formatter,
                    "link clock moved backwards: {now:?} before {last:?}"
                )
            }
            Self::InvalidTransition { state, operation } => {
                write!(formatter, "cannot {operation} while link is {state:?}")
            }
            Self::InvalidHeartbeat => formatter.write_str("invalid heartbeat"),
            Self::HeartbeatSequenceRewound { last, received } => {
                write!(
                    formatter,
                    "heartbeat sequence {received} is not newer than {last}"
                )
            }
            Self::InvalidSessionId => formatter.write_str("session identifier must be non-zero"),
            Self::SessionChanged { existing, received } => write!(
                formatter,
                "reconnect changed session from {existing} to {received}"
            ),
        }
    }
}

impl std::error::Error for LinkError {}

/// Deterministic link state machine. `Timestamp` is supplied by the Mac host,
/// so reconnect measurements never require clock synchronization with Android.
#[derive(Clone, Debug)]
pub struct LinkMachine {
    state: LinkState,
    state_since: Timestamp,
    last_observed_at: Timestamp,
    last_heartbeat_at: Option<Timestamp>,
    last_heartbeat_seq: u64,
    missed_heartbeats: u64,
    session_id: Option<SessionId>,
    reconnect_started: bool,
    reconnect_accumulated_ms: u64,
    last_reconnect_time_ms: Option<u64>,
}

impl LinkMachine {
    pub const fn new(now: Timestamp) -> Self {
        Self {
            state: LinkState::Searching,
            state_since: now,
            last_observed_at: now,
            last_heartbeat_at: None,
            last_heartbeat_seq: 0,
            missed_heartbeats: 0,
            session_id: None,
            reconnect_started: false,
            reconnect_accumulated_ms: 0,
            last_reconnect_time_ms: None,
        }
    }

    pub fn new_with_clock<C: Clock>(clock: &C) -> Self {
        Self::new(clock.now())
    }

    pub const fn state(&self) -> LinkState {
        self.state
    }

    pub const fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    pub const fn last_reconnect_time_ms(&self) -> Option<u64> {
        self.last_reconnect_time_ms
    }

    pub fn snapshot(&self, now: Timestamp) -> Result<LinkSnapshot, LinkError> {
        self.check_clock(now)?;
        Ok(LinkSnapshot {
            state: self.state,
            state_since: self.state_since,
            last_heartbeat_at: self.last_heartbeat_at,
            missed_heartbeats: self.missed_heartbeats,
            session_id: self.session_id,
            reconnect_time_ms: self.reconnect_time_ms(now)?,
        })
    }

    pub fn snapshot_with_clock<C: Clock>(&self, clock: &C) -> Result<LinkSnapshot, LinkError> {
        self.snapshot(clock.now())
    }

    /// Returns the current reconnect interval. The v1 metric includes time in
    /// `Degraded`, `Lost`, `Searching`, `Connecting`, and `Handshaking`.
    pub fn reconnect_time_ms(&self, now: Timestamp) -> Result<Option<u64>, LinkError> {
        self.check_clock(now)?;
        if !self.reconnect_started {
            return Ok(self.last_reconnect_time_ms);
        }
        let current = (if self.state.reconnect_measured() {
            elapsed(now, self.state_since)
        } else {
            Some(0)
        })
        .ok_or(LinkError::ClockRewound {
            last: self.state_since,
            now,
        })?;
        Ok(Some(self.reconnect_accumulated_ms.saturating_add(current)))
    }

    pub fn reconnect_time_ms_with_clock<C: Clock>(
        &self,
        clock: &C,
    ) -> Result<Option<u64>, LinkError> {
        self.reconnect_time_ms(clock.now())
    }

    pub fn begin_connecting(&mut self, now: Timestamp) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        if self.state != LinkState::Searching {
            return Err(self.invalid_transition("begin connecting"));
        }
        Ok(self.transition_to(LinkState::Connecting, now))
    }

    pub fn begin_connecting_with_clock<C: Clock>(
        &mut self,
        clock: &C,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.begin_connecting(clock.now())
    }

    pub fn transport_connected(
        &mut self,
        now: Timestamp,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        if self.state != LinkState::Connecting {
            return Err(self.invalid_transition("mark transport connected"));
        }
        Ok(self.transition_to(LinkState::Handshaking, now))
    }

    pub fn transport_connected_with_clock<C: Clock>(
        &mut self,
        clock: &C,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.transport_connected(clock.now())
    }

    /// Return an unsuccessful transport attempt to discovery. The next race
    /// starts from `Searching`; control-plane handshake failures use the same
    /// boundary after the transport has entered `Handshaking`.
    pub fn connection_failed(&mut self, now: Timestamp) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        if !matches!(self.state, LinkState::Connecting | LinkState::Handshaking) {
            return Err(self.invalid_transition("mark connection failed"));
        }
        Ok(self.transition_to(LinkState::Searching, now))
    }

    pub fn connection_failed_with_clock<C: Clock>(
        &mut self,
        clock: &C,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.connection_failed(clock.now())
    }

    pub fn handshake_ready(
        &mut self,
        now: Timestamp,
        session_id: Option<SessionId>,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        if self.state != LinkState::Handshaking {
            return Err(self.invalid_transition("complete handshake"));
        }
        if session_id.is_some_and(|id| !id.is_valid()) {
            return Err(LinkError::InvalidSessionId);
        }
        if let (Some(existing), Some(received)) = (self.session_id, session_id) {
            if existing != received {
                return Err(LinkError::SessionChanged { existing, received });
            }
        }
        if session_id.is_some() {
            self.session_id = session_id;
        }
        self.last_heartbeat_at = Some(now);
        self.last_heartbeat_seq = 0;
        self.missed_heartbeats = 0;
        Ok(self.transition_to(LinkState::Ready, now))
    }

    pub fn handshake_ready_with_clock<C: Clock>(
        &mut self,
        clock: &C,
        session_id: Option<SessionId>,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.handshake_ready(clock.now(), session_id)
    }

    pub fn receive_heartbeat(
        &mut self,
        now: Timestamp,
        heartbeat: &Heartbeat,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        if heartbeat.validate().is_err() {
            return Err(LinkError::InvalidHeartbeat);
        }
        if !matches!(self.state, LinkState::Ready | LinkState::Degraded) {
            return Err(self.invalid_transition("receive heartbeat"));
        }
        if heartbeat.seq <= self.last_heartbeat_seq {
            return Err(LinkError::HeartbeatSequenceRewound {
                last: self.last_heartbeat_seq,
                received: heartbeat.seq,
            });
        }
        self.last_heartbeat_at = Some(now);
        self.last_heartbeat_seq = heartbeat.seq;
        self.missed_heartbeats = 0;
        if self.state == LinkState::Degraded {
            return Ok(self.transition_to(LinkState::Ready, now));
        }
        Ok(Vec::new())
    }

    pub fn receive_heartbeat_with_clock<C: Clock>(
        &mut self,
        clock: &C,
        heartbeat: &Heartbeat,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.receive_heartbeat(clock.now(), heartbeat)
    }

    /// A path change forces discovery immediately. When the link was ready,
    /// the intermediate `Degraded` transition remains observable to the UI.
    pub fn network_changed(&mut self, now: Timestamp) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        let mut transitions = Vec::new();
        if self.state == LinkState::Ready {
            transitions.extend(self.transition_to(LinkState::Degraded, now));
        }
        if self.state != LinkState::Searching {
            transitions.extend(self.transition_to(LinkState::Searching, now));
        }
        Ok(transitions)
    }

    pub fn network_changed_with_clock<C: Clock>(
        &mut self,
        clock: &C,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.network_changed(clock.now())
    }

    /// Apply heartbeat deadlines. A late tick can return both transitions so
    /// observers do not lose the required `Ready -> Degraded -> Lost` history.
    pub fn tick(&mut self, now: Timestamp) -> Result<Vec<LinkTransition>, LinkError> {
        self.observe(now)?;
        let Some(last_heartbeat_at) = self.last_heartbeat_at else {
            return Ok(Vec::new());
        };
        if !matches!(self.state, LinkState::Ready | LinkState::Degraded) {
            return Ok(Vec::new());
        }
        let age = elapsed(now, last_heartbeat_at).ok_or(LinkError::ClockRewound {
            last: last_heartbeat_at,
            now,
        })?;
        self.missed_heartbeats = age / HEARTBEAT_INTERVAL_MS;
        let mut transitions = Vec::new();
        if age >= DEGRADE_AFTER_MS && self.state == LinkState::Ready {
            transitions.extend(self.transition_to(LinkState::Degraded, now));
        }
        if age >= LOSE_AFTER_MS && self.state == LinkState::Degraded {
            transitions.extend(self.transition_to(LinkState::Lost, now));
        }
        Ok(transitions)
    }

    pub fn tick_with_clock<C: Clock>(
        &mut self,
        clock: &C,
    ) -> Result<Vec<LinkTransition>, LinkError> {
        self.tick(clock.now())
    }

    fn observe(&mut self, now: Timestamp) -> Result<(), LinkError> {
        self.check_clock(now)?;
        self.last_observed_at = now;
        Ok(())
    }

    fn check_clock(&self, now: Timestamp) -> Result<(), LinkError> {
        if now < self.last_observed_at {
            Err(LinkError::ClockRewound {
                last: self.last_observed_at,
                now,
            })
        } else {
            Ok(())
        }
    }

    fn invalid_transition(&self, operation: &'static str) -> LinkError {
        LinkError::InvalidTransition {
            state: self.state,
            operation,
        }
    }

    fn transition_to(&mut self, next: LinkState, now: Timestamp) -> Vec<LinkTransition> {
        if self.state == next {
            return Vec::new();
        }

        if self.reconnect_started && self.state.reconnect_measured() {
            self.reconnect_accumulated_ms = self
                .reconnect_accumulated_ms
                .saturating_add(elapsed(now, self.state_since).unwrap_or(0));
        }

        if matches!(
            next,
            LinkState::Degraded | LinkState::Lost | LinkState::Searching
        ) && !self.reconnect_started
        {
            self.reconnect_started = true;
            self.reconnect_accumulated_ms = 0;
            self.last_reconnect_time_ms = None;
        }

        let transition = LinkTransition {
            from: self.state,
            to: next,
            at: now,
        };
        self.state = next;
        self.state_since = now;

        if next == LinkState::Ready && self.reconnect_started {
            self.last_reconnect_time_ms = Some(self.reconnect_accumulated_ms);
            self.reconnect_started = false;
        }
        vec![transition]
    }
}

fn elapsed(now: Timestamp, then: Timestamp) -> Option<u64> {
    now.as_millis().checked_sub(then.as_millis())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaSessionState {
    Idle,
    Starting {
        request_id: u64,
        feature: Feature,
        awaiting_user_action: Option<UserAction>,
    },
    Active {
        request_id: u64,
        feature: Feature,
        backend: Backend,
    },
    Switching {
        request_id: u64,
        feature: Feature,
        from: Option<Backend>,
        to: Backend,
        awaiting_user_action: Option<UserAction>,
    },
    Ended {
        request_id: Option<u64>,
        feature: Option<Feature>,
        error: Option<ControlErrorCode>,
    },
}

impl MediaSessionState {
    pub const fn stage(&self) -> MediaSessionStage {
        match self {
            Self::Idle => MediaSessionStage::Idle,
            Self::Starting { .. } => MediaSessionStage::Starting,
            Self::Active { .. } => MediaSessionStage::Active,
            Self::Switching { .. } => MediaSessionStage::Switching,
            Self::Ended { .. } => MediaSessionStage::Ended,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaSessionStage {
    Idle,
    Starting,
    Active,
    Switching,
    Ended,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MediaSessionError {
    InvalidTransition(MediaSessionStage),
    RequestMismatch {
        expected: u64,
        received: u64,
    },
    FeatureMismatch {
        expected: Feature,
        received: Feature,
    },
    BackendMismatch {
        expected: Backend,
        received: Backend,
    },
    InvalidBackend(ContractError),
}

impl fmt::Display for MediaSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition(stage) => {
                write!(formatter, "invalid media-session transition from {stage:?}")
            }
            Self::RequestMismatch { expected, received } => {
                write!(formatter, "request {received} does not match {expected}")
            }
            Self::FeatureMismatch { expected, received } => {
                write!(
                    formatter,
                    "feature {received:?} does not match {expected:?}"
                )
            }
            Self::BackendMismatch { expected, received } => {
                write!(
                    formatter,
                    "backend {received:?} does not match {expected:?}"
                )
            }
            Self::InvalidBackend(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for MediaSessionError {}

/// State machine for one requested feature. The Android app owns this state;
/// the helper is never a peer or a second identity in the control plane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSessionMachine {
    state: MediaSessionState,
}

impl Default for MediaSessionMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaSessionMachine {
    pub const fn new() -> Self {
        Self {
            state: MediaSessionState::Idle,
        }
    }

    pub const fn state(&self) -> &MediaSessionState {
        &self.state
    }

    pub fn request(&mut self, request: SessionRequest) -> Result<(), MediaSessionError> {
        request
            .validate()
            .map_err(MediaSessionError::InvalidBackend)?;
        if self.state.stage() != MediaSessionStage::Idle {
            return Err(MediaSessionError::InvalidTransition(self.state.stage()));
        }
        self.state = MediaSessionState::Starting {
            request_id: request.request_id,
            feature: request.feature,
            awaiting_user_action: None,
        };
        Ok(())
    }

    pub fn apply_result(&mut self, result: SessionResult) -> Result<(), MediaSessionError> {
        result
            .validate()
            .map_err(MediaSessionError::InvalidBackend)?;
        let (request_id, feature) = match self.state {
            MediaSessionState::Starting {
                request_id,
                feature,
                ..
            } => (request_id, feature),
            _ => return Err(MediaSessionError::InvalidTransition(self.state.stage())),
        };
        ensure_request(request_id, result.request_id)?;
        ensure_feature(feature, result.feature)?;
        match result.outcome {
            SessionOutcome::Accepted { backend } => {
                self.state = MediaSessionState::Active {
                    request_id,
                    feature,
                    backend,
                };
            }
            SessionOutcome::NeedsUserAction { action } => {
                self.state = MediaSessionState::Starting {
                    request_id,
                    feature,
                    awaiting_user_action: Some(action),
                };
            }
            SessionOutcome::Rejected { code } => {
                self.state = MediaSessionState::Ended {
                    request_id: Some(request_id),
                    feature: Some(feature),
                    error: Some(code),
                };
            }
        }
        Ok(())
    }

    pub fn begin_switch(
        &mut self,
        change: BackendChanged,
        awaiting_user_action: Option<UserAction>,
    ) -> Result<(), MediaSessionError> {
        change
            .validate()
            .map_err(MediaSessionError::InvalidBackend)?;
        let (request_id, feature, backend) = match self.state {
            MediaSessionState::Active {
                request_id,
                feature,
                backend,
            } => (request_id, feature, backend),
            _ => return Err(MediaSessionError::InvalidTransition(self.state.stage())),
        };
        ensure_feature(feature, change.feature)?;
        if backend != change.from {
            return Err(MediaSessionError::BackendMismatch {
                expected: backend,
                received: change.from,
            });
        }
        self.state = MediaSessionState::Switching {
            request_id,
            feature,
            from: Some(change.from),
            to: change.to,
            awaiting_user_action,
        };
        Ok(())
    }

    pub fn complete_switch(&mut self, backend: Backend) -> Result<(), MediaSessionError> {
        let (request_id, feature, expected, awaiting_user_action) = match self.state {
            MediaSessionState::Switching {
                request_id,
                feature,
                to,
                awaiting_user_action,
                ..
            } => (request_id, feature, to, awaiting_user_action),
            _ => return Err(MediaSessionError::InvalidTransition(self.state.stage())),
        };
        if expected != backend {
            return Err(MediaSessionError::BackendMismatch {
                expected,
                received: backend,
            });
        }
        if awaiting_user_action.is_some() {
            return Err(MediaSessionError::InvalidTransition(
                MediaSessionStage::Switching,
            ));
        }
        self.state = MediaSessionState::Active {
            request_id,
            feature,
            backend,
        };
        Ok(())
    }

    pub fn user_action_completed(&mut self, action: UserAction) -> Result<(), MediaSessionError> {
        let (request_id, feature, from, to, awaiting_user_action) = match self.state {
            MediaSessionState::Switching {
                request_id,
                feature,
                from,
                to,
                awaiting_user_action,
            } => (request_id, feature, from, to, awaiting_user_action),
            _ => return Err(MediaSessionError::InvalidTransition(self.state.stage())),
        };
        if awaiting_user_action != Some(action) {
            return Err(MediaSessionError::InvalidTransition(
                MediaSessionStage::Switching,
            ));
        }
        self.state = MediaSessionState::Switching {
            request_id,
            feature,
            from,
            to,
            awaiting_user_action: None,
        };
        Ok(())
    }

    pub fn end(&mut self, error: Option<ControlErrorCode>) -> Result<(), MediaSessionError> {
        let (request_id, feature) = match self.state {
            MediaSessionState::Starting {
                request_id,
                feature,
                ..
            }
            | MediaSessionState::Active {
                request_id,
                feature,
                ..
            }
            | MediaSessionState::Switching {
                request_id,
                feature,
                ..
            } => (Some(request_id), Some(feature)),
            _ => return Err(MediaSessionError::InvalidTransition(self.state.stage())),
        };
        self.state = MediaSessionState::Ended {
            request_id,
            feature,
            error,
        };
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), MediaSessionError> {
        if self.state.stage() != MediaSessionStage::Ended {
            return Err(MediaSessionError::InvalidTransition(self.state.stage()));
        }
        self.state = MediaSessionState::Idle;
        Ok(())
    }
}

fn ensure_request(expected: u64, received: u64) -> Result<(), MediaSessionError> {
    if expected == received {
        Ok(())
    } else {
        Err(MediaSessionError::RequestMismatch { expected, received })
    }
}

fn ensure_feature(expected: Feature, received: Feature) -> Result<(), MediaSessionError> {
    if expected == received {
        Ok(())
    } else {
        Err(MediaSessionError::FeatureMismatch { expected, received })
    }
}
