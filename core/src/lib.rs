#![forbid(unsafe_code)]

pub mod device;
pub mod device_state;
pub mod ids;
pub mod protocol;
pub mod security;
pub mod session;

pub use device::{DeviceCapabilities, DeviceKind, DeviceProfile, DeviceRegistry};
pub use device_state::{
    CaptureHealth, ConnectionState, DeviceHeadline, DeviceState, InputHealth, PermissionStatus,
};
pub use ids::{DeviceId, IdError, SessionId};
pub use protocol::{
    Capture, CaptureState, CaptureStateMessage, DeviceStatus, DeviceStatusMessage, ErrorCode,
    ErrorMessage, HeartbeatMessage, KeyAction, KeyboardInput, MessageBody, MessageKind,
    PairingComplete, PairingCompletion, PairingInit, PairingInitiation, PermissionChange,
    PermissionKind, PointerMove, PointerMovement, ProtocolError, ProtocolMessage, ProtocolVersion,
    SessionEstablish, SessionEstablishment, SessionReconnection, SessionTermination, TapInput,
    MAX_ERROR_MESSAGE_SIZE, MAX_FRAME_SIZE, MAX_PAIRING_NONCE_SIZE, MAX_PAYLOAD_SIZE,
};
pub use security::{
    AuthenticatedSession, DeviceIdentityStore, PairingNonce, PairingProvider, ReplayGuard,
    SessionAuthenticator, VerificationCode, PAIRING_NONCE_SIZE, VERIFICATION_CODE_DIGITS,
};
pub use session::{
    CleanupReport, DisconnectReason, SessionConfig, SessionError, SessionManager, SessionSnapshot,
    SessionStart, SessionStatus, TerminationReason, Timestamp,
};
