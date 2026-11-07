#![forbid(unsafe_code)]

pub mod clock;
pub mod connections;
pub mod control_plane;
pub mod device;
pub mod device_state;
pub mod discovery;
pub mod ids;
pub mod pairing;
pub mod protocol;
pub mod security;
pub mod session;

pub use clock::{Clock, ManualClock};
pub use control_plane::*;
pub use device::{DeviceCapabilities, DeviceKind, DeviceProfile, DeviceRegistry};
pub use device_state::{
    CaptureHealth, ConnectionState, DeviceHeadline, DeviceState, InputHealth, PermissionStatus,
};
pub use discovery::*;
pub use ids::{DeviceId, IdError, SessionId};
pub use pairing::*;
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
