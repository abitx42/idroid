#![forbid(unsafe_code)]

pub mod device;
pub mod ids;
pub mod protocol;
pub mod security;
pub mod session;

pub use device::{DeviceCapabilities, DeviceKind, DeviceProfile, DeviceRegistry};
pub use ids::{DeviceId, IdError, SessionId};
pub use protocol::{MessageBody, MessageKind, ProtocolError, ProtocolMessage, ProtocolVersion};
pub use session::{
    CleanupReport, DisconnectReason, SessionConfig, SessionError, SessionManager, SessionSnapshot,
    SessionStart, SessionStatus, TerminationReason, Timestamp,
};
