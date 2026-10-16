use std::fmt;

use crate::ids::{DeviceId, SessionId};
use crate::session::{DisconnectReason, TerminationReason};

/// The four byte prefix shared by every complete protocol frame.
pub const MAGIC: [u8; 4] = *b"IDR0";
/// The fixed frame header is magic, version, kind, and a little-endian length.
pub const HEADER_SIZE: usize = 10;
/// A frame is deliberately bounded before any message-specific decoding occurs.
pub const MAX_PAYLOAD_SIZE: usize = 64 * 1024;
pub const MAX_FRAME_SIZE: usize = HEADER_SIZE + MAX_PAYLOAD_SIZE;
pub const MAX_PAIRING_NONCE_SIZE: usize = 64;
pub const MAX_ERROR_MESSAGE_SIZE: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProtocolVersion {
    #[default]
    V1,
}

impl ProtocolVersion {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::V1 => 1,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::V1),
            other => Err(ProtocolError::UnsupportedVersion(other)),
        }
    }

    pub fn try_from_u8(value: u8) -> Result<Self, ProtocolError> {
        Self::from_wire(value)
    }
}

/// Message kinds are stable wire tags.  The first six values are retained from
/// the original core protocol so existing adapters can continue to exchange
/// legacy lifecycle/data frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    Hello,
    Heartbeat,
    Disconnect,
    Reconnect,
    Terminate,
    Data,
    PairingInitiation,
    PairingCompletion,
    SessionEstablishment,
    DeviceStatus,
    TapInput,
    PointerMovement,
    KeyboardInput,
    PermissionChange,
    CaptureState,
    Error,
}

impl MessageKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::Heartbeat => 2,
            Self::Disconnect => 3,
            Self::Reconnect => 4,
            Self::Terminate => 5,
            Self::Data => 6,
            Self::PairingInitiation => 7,
            Self::PairingCompletion => 8,
            Self::SessionEstablishment => 9,
            Self::DeviceStatus => 10,
            Self::TapInput => 11,
            Self::PointerMovement => 12,
            Self::KeyboardInput => 13,
            Self::PermissionChange => 14,
            Self::CaptureState => 15,
            Self::Error => 16,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Heartbeat),
            3 => Ok(Self::Disconnect),
            4 => Ok(Self::Reconnect),
            5 => Ok(Self::Terminate),
            6 => Ok(Self::Data),
            7 => Ok(Self::PairingInitiation),
            8 => Ok(Self::PairingCompletion),
            9 => Ok(Self::SessionEstablishment),
            10 => Ok(Self::DeviceStatus),
            11 => Ok(Self::TapInput),
            12 => Ok(Self::PointerMovement),
            13 => Ok(Self::KeyboardInput),
            14 => Ok(Self::PermissionChange),
            15 => Ok(Self::CaptureState),
            16 => Ok(Self::Error),
            other => Err(ProtocolError::UnknownMessageType(other)),
        }
    }

    pub fn try_from_u8(value: u8) -> Result<Self, ProtocolError> {
        Self::from_wire(value)
    }

    // Short names are associated constants rather than additional wire kinds.
    // They make the public API convenient without creating incompatible tags.
    #[allow(non_upper_case_globals)]
    pub const PairingInit: Self = Self::PairingInitiation;
    #[allow(non_upper_case_globals)]
    pub const PairingComplete: Self = Self::PairingCompletion;
    #[allow(non_upper_case_globals)]
    pub const SessionEstablish: Self = Self::SessionEstablishment;
    #[allow(non_upper_case_globals)]
    pub const SessionStart: Self = Self::SessionEstablishment;
    #[allow(non_upper_case_globals)]
    pub const SessionTermination: Self = Self::Terminate;
    #[allow(non_upper_case_globals)]
    pub const SessionTerminate: Self = Self::Terminate;
    #[allow(non_upper_case_globals)]
    pub const SessionReconnection: Self = Self::Reconnect;
    #[allow(non_upper_case_globals)]
    pub const SessionReconnect: Self = Self::Reconnect;
    #[allow(non_upper_case_globals)]
    pub const Status: Self = Self::DeviceStatus;
    #[allow(non_upper_case_globals)]
    pub const Tap: Self = Self::TapInput;
    #[allow(non_upper_case_globals)]
    pub const PointerMove: Self = Self::PointerMovement;
    #[allow(non_upper_case_globals)]
    pub const Keyboard: Self = Self::KeyboardInput;
    #[allow(non_upper_case_globals)]
    pub const Permission: Self = Self::PermissionChange;
    #[allow(non_upper_case_globals)]
    pub const Capture: Self = Self::CaptureState;
    #[allow(non_upper_case_globals)]
    pub const ErrorMessage: Self = Self::Error;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceStatus {
    Online,
    Offline,
    Pairing,
    Busy,
    Unavailable,
}

impl DeviceStatus {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Online => 1,
            Self::Offline => 2,
            Self::Pairing => 3,
            Self::Busy => 4,
            Self::Unavailable => 5,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Online),
            2 => Some(Self::Offline),
            3 => Some(Self::Pairing),
            4 => Some(Self::Busy),
            5 => Some(Self::Unavailable),
            _ => None,
        }
    }

    #[allow(non_upper_case_globals)]
    pub const Available: Self = Self::Online;
    #[allow(non_upper_case_globals)]
    pub const Connected: Self = Self::Online;
    #[allow(non_upper_case_globals)]
    pub const Disconnected: Self = Self::Offline;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionKind {
    ScreenCapture,
    InputControl,
    Accessibility,
    Notifications,
    FileTransfer,
}

impl PermissionKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::ScreenCapture => 1,
            Self::InputControl => 2,
            Self::Accessibility => 3,
            Self::Notifications => 4,
            Self::FileTransfer => 5,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::ScreenCapture),
            2 => Some(Self::InputControl),
            3 => Some(Self::Accessibility),
            4 => Some(Self::Notifications),
            5 => Some(Self::FileTransfer),
            _ => None,
        }
    }

    #[allow(non_upper_case_globals)]
    pub const Input: Self = Self::InputControl;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureState {
    Stopped,
    Starting,
    Active,
    Paused,
    Failed,
}

impl CaptureState {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Stopped => 1,
            Self::Starting => 2,
            Self::Active => 3,
            Self::Paused => 4,
            Self::Failed => 5,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Stopped),
            2 => Some(Self::Starting),
            3 => Some(Self::Active),
            4 => Some(Self::Paused),
            5 => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyAction {
    Press,
    Release,
    Repeat,
}

impl KeyAction {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Press => 1,
            Self::Release => 2,
            Self::Repeat => 3,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Press),
            2 => Some(Self::Release),
            3 => Some(Self::Repeat),
            _ => None,
        }
    }

    #[allow(non_upper_case_globals)]
    pub const Down: Self = Self::Press;
    #[allow(non_upper_case_globals)]
    pub const Up: Self = Self::Release;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    MalformedMessage,
    InvalidField,
    UnsupportedVersion,
    Unauthorized,
    PermissionDenied,
    PairingFailed,
    SessionNotFound,
    Internal,
}

impl ErrorCode {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::MalformedMessage => 1,
            Self::InvalidField => 2,
            Self::UnsupportedVersion => 3,
            Self::Unauthorized => 4,
            Self::PermissionDenied => 5,
            Self::PairingFailed => 6,
            Self::SessionNotFound => 7,
            Self::Internal => 8,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.wire_value()
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MalformedMessage),
            2 => Some(Self::InvalidField),
            3 => Some(Self::UnsupportedVersion),
            4 => Some(Self::Unauthorized),
            5 => Some(Self::PermissionDenied),
            6 => Some(Self::PairingFailed),
            7 => Some(Self::SessionNotFound),
            8 => Some(Self::Internal),
            _ => None,
        }
    }

    #[allow(non_upper_case_globals)]
    pub const InvalidMessage: Self = Self::MalformedMessage;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageBody {
    // Legacy V1 bodies retained for source and wire compatibility.
    Hello {
        device_id: DeviceId,
    },
    Heartbeat {
        session_id: SessionId,
        sequence: u64,
    },
    Disconnect {
        session_id: SessionId,
        reason: DisconnectReason,
    },
    Reconnect {
        session_id: SessionId,
    },
    Terminate {
        session_id: SessionId,
        reason: TerminationReason,
    },
    Data {
        session_id: SessionId,
        payload: Vec<u8>,
    },
    PairingInitiation {
        device_id: DeviceId,
        nonce: Vec<u8>,
    },
    PairingCompletion {
        device_id: DeviceId,
        accepted: bool,
    },
    SessionEstablishment {
        device_id: DeviceId,
        session_id: SessionId,
    },
    DeviceStatus {
        device_id: DeviceId,
        status: DeviceStatus,
        capabilities: u32,
    },
    TapInput {
        session_id: SessionId,
        x: i32,
        y: i32,
    },
    PointerMovement {
        session_id: SessionId,
        x: i32,
        y: i32,
    },
    KeyboardInput {
        session_id: SessionId,
        key_code: u32,
        action: KeyAction,
        modifiers: u32,
    },
    PermissionChange {
        device_id: DeviceId,
        permission: PermissionKind,
        granted: bool,
    },
    CaptureState {
        session_id: SessionId,
        state: CaptureState,
    },
    Error {
        session_id: Option<SessionId>,
        code: ErrorCode,
        message: String,
    },
}

impl MessageBody {
    pub const fn kind(&self) -> MessageKind {
        match self {
            Self::Hello { .. } => MessageKind::Hello,
            Self::Heartbeat { .. } => MessageKind::Heartbeat,
            Self::Disconnect { .. } => MessageKind::Disconnect,
            Self::Reconnect { .. } => MessageKind::Reconnect,
            Self::Terminate { .. } => MessageKind::Terminate,
            Self::Data { .. } => MessageKind::Data,
            Self::PairingInitiation { .. } => MessageKind::PairingInitiation,
            Self::PairingCompletion { .. } => MessageKind::PairingCompletion,
            Self::SessionEstablishment { .. } => MessageKind::SessionEstablishment,
            Self::DeviceStatus { .. } => MessageKind::DeviceStatus,
            Self::TapInput { .. } => MessageKind::TapInput,
            Self::PointerMovement { .. } => MessageKind::PointerMovement,
            Self::KeyboardInput { .. } => MessageKind::KeyboardInput,
            Self::PermissionChange { .. } => MessageKind::PermissionChange,
            Self::CaptureState { .. } => MessageKind::CaptureState,
            Self::Error { .. } => MessageKind::Error,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Hello { device_id } => require_id(device_id.is_valid(), "device_id"),
            Self::Heartbeat {
                session_id,
                sequence,
            } => {
                require_id(session_id.is_valid(), "session_id")?;
                require_id(*sequence != 0, "sequence")
            }
            Self::Disconnect { session_id, .. }
            | Self::Reconnect { session_id }
            | Self::Terminate { session_id, .. } => require_id(session_id.is_valid(), "session_id"),
            Self::Data {
                session_id,
                payload,
            } => {
                require_id(session_id.is_valid(), "session_id")?;
                validate_non_empty_bounded(payload, "payload", MAX_PAYLOAD_SIZE - 8)
            }
            Self::PairingInitiation { device_id, nonce } => {
                require_id(device_id.is_valid(), "device_id")?;
                validate_non_empty_bounded(nonce, "nonce", MAX_PAIRING_NONCE_SIZE)
            }
            Self::PairingCompletion { device_id, .. } => {
                require_id(device_id.is_valid(), "device_id")
            }
            Self::SessionEstablishment {
                device_id,
                session_id,
            } => {
                require_id(device_id.is_valid(), "device_id")?;
                require_id(session_id.is_valid(), "session_id")
            }
            Self::DeviceStatus { device_id, .. } => require_id(device_id.is_valid(), "device_id"),
            Self::TapInput { session_id, x, y } => {
                require_id(session_id.is_valid(), "session_id")?;
                require_id(*x >= 0, "x")?;
                require_id(*y >= 0, "y")
            }
            Self::PointerMovement { session_id, .. } => {
                require_id(session_id.is_valid(), "session_id")
            }
            Self::KeyboardInput {
                session_id,
                key_code,
                ..
            } => {
                require_id(session_id.is_valid(), "session_id")?;
                require_id(*key_code != 0, "key_code")
            }
            Self::PermissionChange { device_id, .. } => {
                require_id(device_id.is_valid(), "device_id")
            }
            Self::CaptureState { session_id, .. } => {
                require_id(session_id.is_valid(), "session_id")
            }
            Self::Error {
                session_id,
                message,
                ..
            } => {
                if let Some(session_id) = session_id {
                    require_id(session_id.is_valid(), "session_id")?;
                }
                validate_text(message, "message", MAX_ERROR_MESSAGE_SIZE)
            }
        }
    }

    pub fn pairing_initiation(
        device_id: DeviceId,
        nonce: impl Into<Vec<u8>>,
    ) -> Result<Self, ProtocolError> {
        let body = Self::PairingInitiation {
            device_id,
            nonce: nonce.into(),
        };
        body.validate()?;
        Ok(body)
    }

    pub fn pairing_completion(device_id: DeviceId, accepted: bool) -> Result<Self, ProtocolError> {
        let body = Self::PairingCompletion {
            device_id,
            accepted,
        };
        body.validate()?;
        Ok(body)
    }

    pub fn session_establishment(
        device_id: DeviceId,
        session_id: SessionId,
    ) -> Result<Self, ProtocolError> {
        let body = Self::SessionEstablishment {
            device_id,
            session_id,
        };
        body.validate()?;
        Ok(body)
    }

    pub fn session_termination(
        session_id: SessionId,
        reason: TerminationReason,
    ) -> Result<Self, ProtocolError> {
        let body = Self::Terminate { session_id, reason };
        body.validate()?;
        Ok(body)
    }

    pub fn session_reconnection(session_id: SessionId) -> Result<Self, ProtocolError> {
        let body = Self::Reconnect { session_id };
        body.validate()?;
        Ok(body)
    }
}

/// A validated pairing initiation payload that can be converted into a body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingInitiation {
    pub device_id: DeviceId,
    pub nonce: Vec<u8>,
}

impl PairingInitiation {
    pub fn new(device_id: DeviceId, nonce: impl Into<Vec<u8>>) -> Result<Self, ProtocolError> {
        let message = Self {
            device_id,
            nonce: nonce.into(),
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::PairingInitiation {
            device_id: self.device_id,
            nonce: self.nonce.clone(),
        }
        .validate()
    }
}

impl From<PairingInitiation> for MessageBody {
    fn from(message: PairingInitiation) -> Self {
        Self::PairingInitiation {
            device_id: message.device_id,
            nonce: message.nonce,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PairingCompletion {
    pub device_id: DeviceId,
    pub accepted: bool,
}

impl PairingCompletion {
    pub fn new(device_id: DeviceId, accepted: bool) -> Result<Self, ProtocolError> {
        let message = Self {
            device_id,
            accepted,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::PairingCompletion {
            device_id: self.device_id,
            accepted: self.accepted,
        }
        .validate()
    }
}

impl From<PairingCompletion> for MessageBody {
    fn from(message: PairingCompletion) -> Self {
        Self::PairingCompletion {
            device_id: message.device_id,
            accepted: message.accepted,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionEstablishment {
    pub device_id: DeviceId,
    pub session_id: SessionId,
}

impl SessionEstablishment {
    pub fn new(device_id: DeviceId, session_id: SessionId) -> Result<Self, ProtocolError> {
        let message = Self {
            device_id,
            session_id,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::SessionEstablishment {
            device_id: self.device_id,
            session_id: self.session_id,
        }
        .validate()
    }
}

impl From<SessionEstablishment> for MessageBody {
    fn from(message: SessionEstablishment) -> Self {
        Self::SessionEstablishment {
            device_id: message.device_id,
            session_id: message.session_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeartbeatMessage {
    pub session_id: SessionId,
    pub sequence: u64,
}

impl HeartbeatMessage {
    pub fn new(session_id: SessionId, sequence: u64) -> Result<Self, ProtocolError> {
        let message = Self {
            session_id,
            sequence,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::Heartbeat {
            session_id: self.session_id,
            sequence: self.sequence,
        }
        .validate()
    }
}

impl From<HeartbeatMessage> for MessageBody {
    fn from(message: HeartbeatMessage) -> Self {
        Self::Heartbeat {
            session_id: message.session_id,
            sequence: message.sequence,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionTermination {
    pub session_id: SessionId,
    pub reason: TerminationReason,
}

impl SessionTermination {
    pub fn new(session_id: SessionId, reason: TerminationReason) -> Result<Self, ProtocolError> {
        let message = Self { session_id, reason };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::Terminate {
            session_id: self.session_id,
            reason: self.reason,
        }
        .validate()
    }
}

impl From<SessionTermination> for MessageBody {
    fn from(message: SessionTermination) -> Self {
        Self::Terminate {
            session_id: message.session_id,
            reason: message.reason,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionReconnection {
    pub session_id: SessionId,
}

impl SessionReconnection {
    pub fn new(session_id: SessionId) -> Result<Self, ProtocolError> {
        let message = Self { session_id };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::Reconnect {
            session_id: self.session_id,
        }
        .validate()
    }
}

impl From<SessionReconnection> for MessageBody {
    fn from(message: SessionReconnection) -> Self {
        Self::Reconnect {
            session_id: message.session_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TapInput {
    pub session_id: SessionId,
    pub x: i32,
    pub y: i32,
}

impl TapInput {
    pub fn new(session_id: SessionId, x: i32, y: i32) -> Result<Self, ProtocolError> {
        let message = Self { session_id, x, y };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::TapInput {
            session_id: self.session_id,
            x: self.x,
            y: self.y,
        }
        .validate()
    }
}

impl From<TapInput> for MessageBody {
    fn from(message: TapInput) -> Self {
        Self::TapInput {
            session_id: message.session_id,
            x: message.x,
            y: message.y,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerMovement {
    pub session_id: SessionId,
    pub x: i32,
    pub y: i32,
}

impl PointerMovement {
    pub fn new(session_id: SessionId, x: i32, y: i32) -> Result<Self, ProtocolError> {
        let message = Self { session_id, x, y };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::PointerMovement {
            session_id: self.session_id,
            x: self.x,
            y: self.y,
        }
        .validate()
    }
}

impl From<PointerMovement> for MessageBody {
    fn from(message: PointerMovement) -> Self {
        Self::PointerMovement {
            session_id: message.session_id,
            x: message.x,
            y: message.y,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyboardInput {
    pub session_id: SessionId,
    pub key_code: u32,
    pub action: KeyAction,
    pub modifiers: u32,
}

impl KeyboardInput {
    pub fn new(
        session_id: SessionId,
        key_code: u32,
        action: KeyAction,
        modifiers: u32,
    ) -> Result<Self, ProtocolError> {
        let message = Self {
            session_id,
            key_code,
            action,
            modifiers,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::KeyboardInput {
            session_id: self.session_id,
            key_code: self.key_code,
            action: self.action,
            modifiers: self.modifiers,
        }
        .validate()
    }
}

impl From<KeyboardInput> for MessageBody {
    fn from(message: KeyboardInput) -> Self {
        Self::KeyboardInput {
            session_id: message.session_id,
            key_code: message.key_code,
            action: message.action,
            modifiers: message.modifiers,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermissionChange {
    pub device_id: DeviceId,
    pub permission: PermissionKind,
    pub granted: bool,
}

impl PermissionChange {
    pub fn new(
        device_id: DeviceId,
        permission: PermissionKind,
        granted: bool,
    ) -> Result<Self, ProtocolError> {
        let message = Self {
            device_id,
            permission,
            granted,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::PermissionChange {
            device_id: self.device_id,
            permission: self.permission,
            granted: self.granted,
        }
        .validate()
    }
}

impl From<PermissionChange> for MessageBody {
    fn from(message: PermissionChange) -> Self {
        Self::PermissionChange {
            device_id: message.device_id,
            permission: message.permission,
            granted: message.granted,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureStateMessage {
    pub session_id: SessionId,
    pub state: CaptureState,
}

impl CaptureStateMessage {
    pub fn new(session_id: SessionId, state: CaptureState) -> Result<Self, ProtocolError> {
        let message = Self { session_id, state };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::CaptureState {
            session_id: self.session_id,
            state: self.state,
        }
        .validate()
    }
}

impl From<CaptureStateMessage> for MessageBody {
    fn from(message: CaptureStateMessage) -> Self {
        Self::CaptureState {
            session_id: message.session_id,
            state: message.state,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceStatusMessage {
    pub device_id: DeviceId,
    pub status: DeviceStatus,
    pub capabilities: u32,
}

impl DeviceStatusMessage {
    pub fn new(
        device_id: DeviceId,
        status: DeviceStatus,
        capabilities: u32,
    ) -> Result<Self, ProtocolError> {
        let message = Self {
            device_id,
            status,
            capabilities,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::DeviceStatus {
            device_id: self.device_id,
            status: self.status,
            capabilities: self.capabilities,
        }
        .validate()
    }
}

impl From<DeviceStatusMessage> for MessageBody {
    fn from(message: DeviceStatusMessage) -> Self {
        Self::DeviceStatus {
            device_id: message.device_id,
            status: message.status,
            capabilities: message.capabilities,
        }
    }
}

pub type PairingInit = PairingInitiation;
pub type PairingComplete = PairingCompletion;
pub type SessionEstablish = SessionEstablishment;
pub type PointerMove = PointerMovement;
pub type Capture = CaptureStateMessage;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorMessage {
    pub session_id: Option<SessionId>,
    pub code: ErrorCode,
    pub message: String,
}

impl ErrorMessage {
    pub fn new(
        session_id: Option<SessionId>,
        code: ErrorCode,
        message: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        let message = Self {
            session_id,
            code,
            message: message.into(),
        };
        message.validate()?;
        Ok(message)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        MessageBody::Error {
            session_id: self.session_id,
            code: self.code,
            message: self.message.clone(),
        }
        .validate()
    }
}

impl From<ErrorMessage> for MessageBody {
    fn from(message: ErrorMessage) -> Self {
        Self::Error {
            session_id: message.session_id,
            code: message.code,
            message: message.message,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolMessage {
    version: ProtocolVersion,
    body: MessageBody,
}

impl ProtocolMessage {
    pub fn new(body: impl Into<MessageBody>) -> Result<Self, ProtocolError> {
        Self::with_version(ProtocolVersion::V1, body)
    }

    pub fn with_version(
        version: ProtocolVersion,
        body: impl Into<MessageBody>,
    ) -> Result<Self, ProtocolError> {
        let message = Self {
            version,
            body: body.into(),
        };
        message.validate()?;
        Ok(message)
    }

    pub const fn version(&self) -> ProtocolVersion {
        self.version
    }

    pub const fn body(&self) -> &MessageBody {
        &self.body
    }

    pub const fn kind(&self) -> MessageKind {
        self.body.kind()
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.body.validate()
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let mut payload = Vec::new();
        match &self.body {
            MessageBody::Hello { device_id } => put_u64(&mut payload, device_id.get()),
            MessageBody::Heartbeat {
                session_id,
                sequence,
            } => {
                put_u64(&mut payload, session_id.get());
                put_u64(&mut payload, *sequence);
            }
            MessageBody::Disconnect { session_id, reason } => {
                put_u64(&mut payload, session_id.get());
                payload.push(reason.wire_value());
            }
            MessageBody::Reconnect { session_id } => put_u64(&mut payload, session_id.get()),
            MessageBody::Terminate { session_id, reason } => {
                put_u64(&mut payload, session_id.get());
                payload.push(reason.wire_value());
            }
            MessageBody::Data {
                session_id,
                payload: data,
            } => {
                put_u64(&mut payload, session_id.get());
                payload.extend_from_slice(data);
            }
            MessageBody::PairingInitiation { device_id, nonce } => {
                put_u64(&mut payload, device_id.get());
                put_u16(&mut payload, nonce.len());
                payload.extend_from_slice(nonce);
            }
            MessageBody::PairingCompletion {
                device_id,
                accepted,
            } => {
                put_u64(&mut payload, device_id.get());
                put_bool(&mut payload, *accepted);
            }
            MessageBody::SessionEstablishment {
                device_id,
                session_id,
            } => {
                put_u64(&mut payload, device_id.get());
                put_u64(&mut payload, session_id.get());
            }
            MessageBody::DeviceStatus {
                device_id,
                status,
                capabilities,
            } => {
                put_u64(&mut payload, device_id.get());
                payload.push(status.wire_value());
                put_u32(&mut payload, *capabilities);
            }
            MessageBody::TapInput { session_id, x, y }
            | MessageBody::PointerMovement { session_id, x, y } => {
                put_u64(&mut payload, session_id.get());
                put_i32(&mut payload, *x);
                put_i32(&mut payload, *y);
            }
            MessageBody::KeyboardInput {
                session_id,
                key_code,
                action,
                modifiers,
            } => {
                put_u64(&mut payload, session_id.get());
                put_u32(&mut payload, *key_code);
                payload.push(action.wire_value());
                put_u32(&mut payload, *modifiers);
            }
            MessageBody::PermissionChange {
                device_id,
                permission,
                granted,
            } => {
                put_u64(&mut payload, device_id.get());
                payload.push(permission.wire_value());
                put_bool(&mut payload, *granted);
            }
            MessageBody::CaptureState { session_id, state } => {
                put_u64(&mut payload, session_id.get());
                payload.push(state.wire_value());
            }
            MessageBody::Error {
                session_id,
                code,
                message,
            } => {
                put_u64(&mut payload, session_id.map_or(0, SessionId::get));
                payload.push(code.wire_value());
                put_u16(&mut payload, message.len());
                payload.extend_from_slice(message.as_bytes());
            }
        }

        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_PAYLOAD_SIZE,
            });
        }

        let payload_len =
            u32::try_from(payload.len()).map_err(|_| ProtocolError::LengthOverflow)?;
        let mut frame = Vec::with_capacity(HEADER_SIZE + payload.len());
        frame.extend_from_slice(&MAGIC);
        frame.push(self.version.wire_value());
        frame.push(self.body.kind().wire_value());
        frame.extend_from_slice(&payload_len.to_le_bytes());
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    pub fn decode(frame: &[u8]) -> Result<Self, ProtocolError> {
        if frame.len() < HEADER_SIZE {
            return Err(ProtocolError::Truncated {
                expected: HEADER_SIZE,
                actual: frame.len(),
            });
        }
        if frame[..MAGIC.len()] != MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }

        let version = ProtocolVersion::from_wire(frame[4])?;
        let kind = MessageKind::from_wire(frame[5])?;
        let payload_len = u32::from_le_bytes([frame[6], frame[7], frame[8], frame[9]]) as usize;
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload_len,
                maximum: MAX_PAYLOAD_SIZE,
            });
        }

        let expected = HEADER_SIZE + payload_len;
        if frame.len() < expected {
            return Err(ProtocolError::Truncated {
                expected,
                actual: frame.len(),
            });
        }
        if frame.len() > expected {
            return Err(ProtocolError::TrailingBytes {
                expected,
                actual: frame.len(),
            });
        }

        let payload = &frame[HEADER_SIZE..expected];
        let body = decode_body(kind, payload)?;
        Self::with_version(version, body)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidMagic,
    Truncated {
        expected: usize,
        actual: usize,
    },
    TrailingBytes {
        expected: usize,
        actual: usize,
    },
    UnsupportedVersion(u8),
    UnknownMessageType(u8),
    InvalidPayloadLength {
        kind: MessageKind,
        expected: usize,
        actual: usize,
    },
    InvalidField(&'static str),
    InvalidEnum {
        field: &'static str,
        value: u8,
    },
    InvalidReason {
        field: &'static str,
        value: u8,
    },
    FieldTooLarge {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    LengthOverflow,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => formatter.write_str("invalid protocol magic"),
            Self::Truncated { expected, actual } => {
                write!(
                    formatter,
                    "truncated frame: expected {expected} bytes, got {actual}"
                )
            }
            Self::TrailingBytes { expected, actual } => {
                write!(
                    formatter,
                    "trailing frame bytes: expected {expected}, got {actual}"
                )
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported protocol version {version}")
            }
            Self::UnknownMessageType(kind) => write!(formatter, "unknown message type {kind}"),
            Self::InvalidPayloadLength {
                kind,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid {kind:?} payload length: expected {expected}, got {actual}"
            ),
            Self::InvalidField(field) => write!(formatter, "invalid {field}"),
            Self::InvalidEnum { field, value } => {
                write!(formatter, "invalid {field} value {value}")
            }
            Self::InvalidReason { field, value } => {
                write!(formatter, "invalid {field} value {value}")
            }
            Self::FieldTooLarge {
                field,
                actual,
                maximum,
            } => write!(formatter, "{field} is {actual} bytes, maximum is {maximum}"),
            Self::PayloadTooLarge { actual, maximum } => {
                write!(formatter, "payload is {actual} bytes, maximum is {maximum}")
            }
            Self::LengthOverflow => formatter.write_str("payload length cannot be represented"),
        }
    }
}

impl std::error::Error for ProtocolError {}

fn require_id(valid: bool, field: &'static str) -> Result<(), ProtocolError> {
    if valid {
        Ok(())
    } else {
        Err(ProtocolError::InvalidField(field))
    }
}

fn validate_non_empty_bounded(
    value: &[u8],
    field: &'static str,
    maximum: usize,
) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Err(ProtocolError::InvalidField(field));
    }
    if value.len() > maximum {
        return Err(ProtocolError::FieldTooLarge {
            field,
            actual: value.len(),
            maximum,
        });
    }
    Ok(())
}

fn validate_text(value: &str, field: &'static str, maximum: usize) -> Result<(), ProtocolError> {
    validate_non_empty_bounded(value.as_bytes(), field, maximum)
}

fn put_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}

fn put_u16(output: &mut Vec<u8>, value: usize) {
    output.extend_from_slice(&(value as u16).to_le_bytes());
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(payload: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([payload[offset], payload[offset + 1]])
}

fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

fn read_i32(payload: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

fn read_u64(payload: &[u8], offset: usize) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&payload[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}

fn read_bool(field: &'static str, value: u8) -> Result<bool, ProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(ProtocolError::InvalidEnum {
            field,
            value: other,
        }),
    }
}

fn exact_length(kind: MessageKind, payload: &[u8], expected: usize) -> Result<(), ProtocolError> {
    if payload.len() == expected {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPayloadLength {
            kind,
            expected,
            actual: payload.len(),
        })
    }
}

fn decode_variable_field<'a>(
    kind: MessageKind,
    payload: &'a [u8],
    offset: usize,
    field: &'static str,
    maximum: usize,
) -> Result<&'a [u8], ProtocolError> {
    if payload.len() < offset + 2 {
        return Err(ProtocolError::InvalidPayloadLength {
            kind,
            expected: offset + 2,
            actual: payload.len(),
        });
    }
    let length = usize::from(read_u16(payload, offset));
    if length > maximum {
        return Err(ProtocolError::FieldTooLarge {
            field,
            actual: length,
            maximum,
        });
    }
    let start = offset + 2;
    let expected = start
        .checked_add(length)
        .ok_or(ProtocolError::LengthOverflow)?;
    exact_length(kind, payload, expected)?;
    Ok(&payload[start..expected])
}

fn decode_body(kind: MessageKind, payload: &[u8]) -> Result<MessageBody, ProtocolError> {
    match kind {
        MessageKind::Hello => {
            exact_length(kind, payload, 8)?;
            Ok(MessageBody::Hello {
                device_id: DeviceId::new(read_u64(payload, 0)),
            })
        }
        MessageKind::Heartbeat => {
            exact_length(kind, payload, 16)?;
            Ok(MessageBody::Heartbeat {
                session_id: SessionId::new(read_u64(payload, 0)),
                sequence: read_u64(payload, 8),
            })
        }
        MessageKind::Disconnect => {
            exact_length(kind, payload, 9)?;
            let reason =
                DisconnectReason::from_wire(payload[8]).ok_or(ProtocolError::InvalidReason {
                    field: "disconnect_reason",
                    value: payload[8],
                })?;
            Ok(MessageBody::Disconnect {
                session_id: SessionId::new(read_u64(payload, 0)),
                reason,
            })
        }
        MessageKind::Reconnect => {
            exact_length(kind, payload, 8)?;
            Ok(MessageBody::Reconnect {
                session_id: SessionId::new(read_u64(payload, 0)),
            })
        }
        MessageKind::Terminate => {
            exact_length(kind, payload, 9)?;
            let reason =
                TerminationReason::from_wire(payload[8]).ok_or(ProtocolError::InvalidReason {
                    field: "termination_reason",
                    value: payload[8],
                })?;
            Ok(MessageBody::Terminate {
                session_id: SessionId::new(read_u64(payload, 0)),
                reason,
            })
        }
        MessageKind::Data => {
            if payload.len() < 9 {
                return Err(ProtocolError::InvalidPayloadLength {
                    kind,
                    expected: 9,
                    actual: payload.len(),
                });
            }
            Ok(MessageBody::Data {
                session_id: SessionId::new(read_u64(payload, 0)),
                payload: payload[8..].to_vec(),
            })
        }
        MessageKind::PairingInitiation => {
            if payload.len() < 10 {
                return Err(ProtocolError::InvalidPayloadLength {
                    kind,
                    expected: 10,
                    actual: payload.len(),
                });
            }
            let nonce = decode_variable_field(kind, payload, 8, "nonce", MAX_PAIRING_NONCE_SIZE)?;
            Ok(MessageBody::PairingInitiation {
                device_id: DeviceId::new(read_u64(payload, 0)),
                nonce: nonce.to_vec(),
            })
        }
        MessageKind::PairingCompletion => {
            exact_length(kind, payload, 9)?;
            Ok(MessageBody::PairingCompletion {
                device_id: DeviceId::new(read_u64(payload, 0)),
                accepted: read_bool("accepted", payload[8])?,
            })
        }
        MessageKind::SessionEstablishment => {
            exact_length(kind, payload, 16)?;
            Ok(MessageBody::SessionEstablishment {
                device_id: DeviceId::new(read_u64(payload, 0)),
                session_id: SessionId::new(read_u64(payload, 8)),
            })
        }
        MessageKind::DeviceStatus => {
            exact_length(kind, payload, 13)?;
            let status = DeviceStatus::from_wire(payload[8]).ok_or(ProtocolError::InvalidEnum {
                field: "device_status",
                value: payload[8],
            })?;
            Ok(MessageBody::DeviceStatus {
                device_id: DeviceId::new(read_u64(payload, 0)),
                status,
                capabilities: read_u32(payload, 9),
            })
        }
        MessageKind::TapInput => {
            exact_length(kind, payload, 16)?;
            Ok(MessageBody::TapInput {
                session_id: SessionId::new(read_u64(payload, 0)),
                x: read_i32(payload, 8),
                y: read_i32(payload, 12),
            })
        }
        MessageKind::PointerMovement => {
            exact_length(kind, payload, 16)?;
            Ok(MessageBody::PointerMovement {
                session_id: SessionId::new(read_u64(payload, 0)),
                x: read_i32(payload, 8),
                y: read_i32(payload, 12),
            })
        }
        MessageKind::KeyboardInput => {
            exact_length(kind, payload, 17)?;
            let action = KeyAction::from_wire(payload[12]).ok_or(ProtocolError::InvalidEnum {
                field: "keyboard_action",
                value: payload[12],
            })?;
            Ok(MessageBody::KeyboardInput {
                session_id: SessionId::new(read_u64(payload, 0)),
                key_code: read_u32(payload, 8),
                action,
                modifiers: read_u32(payload, 13),
            })
        }
        MessageKind::PermissionChange => {
            exact_length(kind, payload, 10)?;
            let permission =
                PermissionKind::from_wire(payload[8]).ok_or(ProtocolError::InvalidEnum {
                    field: "permission",
                    value: payload[8],
                })?;
            Ok(MessageBody::PermissionChange {
                device_id: DeviceId::new(read_u64(payload, 0)),
                permission,
                granted: read_bool("granted", payload[9])?,
            })
        }
        MessageKind::CaptureState => {
            exact_length(kind, payload, 9)?;
            let state = CaptureState::from_wire(payload[8]).ok_or(ProtocolError::InvalidEnum {
                field: "capture_state",
                value: payload[8],
            })?;
            Ok(MessageBody::CaptureState {
                session_id: SessionId::new(read_u64(payload, 0)),
                state,
            })
        }
        MessageKind::Error => {
            if payload.len() < 11 {
                return Err(ProtocolError::InvalidPayloadLength {
                    kind,
                    expected: 11,
                    actual: payload.len(),
                });
            }
            let code = ErrorCode::from_wire(payload[8]).ok_or(ProtocolError::InvalidEnum {
                field: "error_code",
                value: payload[8],
            })?;
            let message_bytes =
                decode_variable_field(kind, payload, 9, "message", MAX_ERROR_MESSAGE_SIZE)?;
            let message = String::from_utf8(message_bytes.to_vec())
                .map_err(|_| ProtocolError::InvalidField("message"))?;
            Ok(MessageBody::Error {
                session_id: match read_u64(payload, 0) {
                    0 => None,
                    value => Some(SessionId::new(value)),
                },
                code,
                message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(message: MessageBody) {
        let message = ProtocolMessage::new(message).expect("valid message");
        let decoded =
            ProtocolMessage::decode(&message.encode().expect("encodes")).expect("decodes");
        assert_eq!(decoded, message);
        assert_eq!(decoded.version(), ProtocolVersion::V1);
        assert_eq!(decoded.kind(), decoded.body().kind());
    }

    #[test]
    fn legacy_round_trip_preserves_version_and_body() {
        round_trip(MessageBody::Data {
            session_id: SessionId::new(42),
            payload: vec![1, 2, 3],
        });
    }

    #[test]
    fn new_message_kinds_round_trip() {
        round_trip(MessageBody::PairingInitiation {
            device_id: DeviceId::new(1),
            nonce: vec![9, 8, 7],
        });
        round_trip(MessageBody::PairingCompletion {
            device_id: DeviceId::new(1),
            accepted: true,
        });
        round_trip(MessageBody::SessionEstablishment {
            device_id: DeviceId::new(1),
            session_id: SessionId::new(2),
        });
        round_trip(MessageBody::DeviceStatus {
            device_id: DeviceId::new(1),
            status: DeviceStatus::Online,
            capabilities: 0x0f,
        });
        round_trip(MessageBody::Heartbeat {
            session_id: SessionId::new(2),
            sequence: 3,
        });
        round_trip(MessageBody::Terminate {
            session_id: SessionId::new(2),
            reason: TerminationReason::Requested,
        });
        round_trip(MessageBody::Reconnect {
            session_id: SessionId::new(2),
        });
        round_trip(MessageBody::TapInput {
            session_id: SessionId::new(2),
            x: 100,
            y: 200,
        });
        round_trip(MessageBody::PointerMovement {
            session_id: SessionId::new(2),
            x: -4,
            y: 8,
        });
        round_trip(MessageBody::KeyboardInput {
            session_id: SessionId::new(2),
            key_code: 30,
            action: KeyAction::Press,
            modifiers: 1,
        });
        round_trip(MessageBody::PermissionChange {
            device_id: DeviceId::new(1),
            permission: PermissionKind::ScreenCapture,
            granted: false,
        });
        round_trip(MessageBody::CaptureState {
            session_id: SessionId::new(2),
            state: CaptureState::Active,
        });
        round_trip(MessageBody::Error {
            session_id: Some(SessionId::new(2)),
            code: ErrorCode::PermissionDenied,
            message: "capture permission denied".to_owned(),
        });
    }

    #[test]
    fn constructors_validate_fields() {
        assert_eq!(
            MessageBody::pairing_initiation(DeviceId::new(1), Vec::<u8>::new()),
            Err(ProtocolError::InvalidField("nonce"))
        );
        assert_eq!(
            ProtocolMessage::new(MessageBody::TapInput {
                session_id: SessionId::new(1),
                x: -1,
                y: 0,
            }),
            Err(ProtocolError::InvalidField("x"))
        );
        assert_eq!(
            ProtocolMessage::new(MessageBody::KeyboardInput {
                session_id: SessionId::new(1),
                key_code: 0,
                action: KeyAction::Press,
                modifiers: 0,
            }),
            Err(ProtocolError::InvalidField("key_code"))
        );
        assert_eq!(
            ProtocolMessage::new(MessageBody::Error {
                session_id: None,
                code: ErrorCode::Internal,
                message: String::new(),
            }),
            Err(ProtocolError::InvalidField("message"))
        );
    }

    #[test]
    fn malformed_frames_are_rejected_without_panicking() {
        let message = ProtocolMessage::new(MessageBody::Reconnect {
            session_id: SessionId::new(4),
        })
        .expect("valid message");
        let frame = message.encode().expect("encodes");

        assert_eq!(
            ProtocolMessage::decode(&frame[..HEADER_SIZE - 1]),
            Err(ProtocolError::Truncated {
                expected: HEADER_SIZE,
                actual: HEADER_SIZE - 1,
            })
        );

        let mut bad_magic = frame.clone();
        bad_magic[0] = 0;
        assert_eq!(
            ProtocolMessage::decode(&bad_magic),
            Err(ProtocolError::InvalidMagic)
        );

        let mut bad_length = frame.clone();
        bad_length[6] = 20;
        assert_eq!(
            ProtocolMessage::decode(&bad_length),
            Err(ProtocolError::Truncated {
                expected: HEADER_SIZE + 20,
                actual: frame.len(),
            })
        );

        let mut trailing = frame.clone();
        trailing.push(0xff);
        assert_eq!(
            ProtocolMessage::decode(&trailing),
            Err(ProtocolError::TrailingBytes {
                expected: frame.len(),
                actual: frame.len() + 1,
            })
        );

        let mut unknown_kind = frame;
        unknown_kind[5] = 0xff;
        assert_eq!(
            ProtocolMessage::decode(&unknown_kind),
            Err(ProtocolError::UnknownMessageType(0xff))
        );
    }

    #[test]
    fn malformed_message_fields_are_rejected() {
        let message = ProtocolMessage::new(MessageBody::PairingCompletion {
            device_id: DeviceId::new(4),
            accepted: true,
        })
        .expect("valid message");
        let mut invalid_bool = message.encode().expect("encodes");
        invalid_bool[HEADER_SIZE + 8] = 2;
        assert_eq!(
            ProtocolMessage::decode(&invalid_bool),
            Err(ProtocolError::InvalidEnum {
                field: "accepted",
                value: 2,
            })
        );

        let message = ProtocolMessage::new(MessageBody::Error {
            session_id: None,
            code: ErrorCode::Internal,
            message: "bad".to_owned(),
        })
        .expect("valid message");
        let mut invalid_utf8 = message.encode().expect("encodes");
        invalid_utf8[HEADER_SIZE + 11] = 0xff;
        assert_eq!(
            ProtocolMessage::decode(&invalid_utf8),
            Err(ProtocolError::InvalidField("message"))
        );

        let mut oversized = vec![0_u8; HEADER_SIZE];
        oversized[..MAGIC.len()].copy_from_slice(&MAGIC);
        oversized[4] = ProtocolVersion::V1.wire_value();
        oversized[5] = MessageKind::Data.wire_value();
        oversized[6..10].copy_from_slice(&((MAX_PAYLOAD_SIZE as u32) + 1).to_le_bytes());
        assert_eq!(
            ProtocolMessage::decode(&oversized),
            Err(ProtocolError::PayloadTooLarge {
                actual: MAX_PAYLOAD_SIZE + 1,
                maximum: MAX_PAYLOAD_SIZE,
            })
        );
    }

    #[test]
    fn unknown_enum_values_are_rejected_for_each_enum_message() {
        let message = ProtocolMessage::new(MessageBody::DeviceStatus {
            device_id: DeviceId::new(1),
            status: DeviceStatus::Online,
            capabilities: 0,
        })
        .expect("valid status");
        let mut invalid_status = message.encode().expect("encodes");
        invalid_status[HEADER_SIZE + 8] = 0xff;
        assert_eq!(
            ProtocolMessage::decode(&invalid_status),
            Err(ProtocolError::InvalidEnum {
                field: "device_status",
                value: 0xff,
            })
        );

        let message = ProtocolMessage::new(MessageBody::KeyboardInput {
            session_id: SessionId::new(1),
            key_code: 1,
            action: KeyAction::Press,
            modifiers: 0,
        })
        .expect("valid keyboard input");
        let mut invalid_action = message.encode().expect("encodes");
        invalid_action[HEADER_SIZE + 12] = 0xff;
        assert_eq!(
            ProtocolMessage::decode(&invalid_action),
            Err(ProtocolError::InvalidEnum {
                field: "keyboard_action",
                value: 0xff,
            })
        );

        let message = ProtocolMessage::new(MessageBody::CaptureState {
            session_id: SessionId::new(1),
            state: CaptureState::Active,
        })
        .expect("valid capture state");
        let mut invalid_capture = message.encode().expect("encodes");
        invalid_capture[HEADER_SIZE + 8] = 0xff;
        assert_eq!(
            ProtocolMessage::decode(&invalid_capture),
            Err(ProtocolError::InvalidEnum {
                field: "capture_state",
                value: 0xff,
            })
        );
    }
}
