use std::fmt;

use crate::ids::{DeviceId, SessionId};
use crate::session::{DisconnectReason, TerminationReason};

pub const MAGIC: [u8; 4] = *b"IDR0";
pub const HEADER_SIZE: usize = 10;
pub const MAX_PAYLOAD_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolVersion {
    V1,
}

impl ProtocolVersion {
    const fn wire_value(self) -> u8 {
        match self {
            Self::V1 => 1,
        }
    }

    fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::V1),
            other => Err(ProtocolError::UnsupportedVersion(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    Hello,
    Heartbeat,
    Disconnect,
    Reconnect,
    Terminate,
    Data,
}

impl MessageKind {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::Heartbeat => 2,
            Self::Disconnect => 3,
            Self::Reconnect => 4,
            Self::Terminate => 5,
            Self::Data => 6,
        }
    }

    fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Heartbeat),
            3 => Ok(Self::Disconnect),
            4 => Ok(Self::Reconnect),
            5 => Ok(Self::Terminate),
            6 => Ok(Self::Data),
            other => Err(ProtocolError::UnknownMessageType(other)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageBody {
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
        }
    }

    fn validate(&self) -> Result<(), ProtocolError> {
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
                if payload.is_empty() {
                    return Err(ProtocolError::InvalidField("payload"));
                }
                if payload.len() > MAX_PAYLOAD_SIZE {
                    return Err(ProtocolError::PayloadTooLarge {
                        actual: payload.len(),
                        maximum: MAX_PAYLOAD_SIZE,
                    });
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolMessage {
    version: ProtocolVersion,
    body: MessageBody,
}

impl ProtocolMessage {
    pub fn new(body: MessageBody) -> Result<Self, ProtocolError> {
        Self::with_version(ProtocolVersion::V1, body)
    }

    pub fn with_version(version: ProtocolVersion, body: MessageBody) -> Result<Self, ProtocolError> {
        let message = Self { version, body };
        message.validate()?;
        Ok(message)
    }

    pub const fn version(&self) -> ProtocolVersion {
        self.version
    }

    pub const fn body(&self) -> &MessageBody {
        &self.body
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
        }

        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_PAYLOAD_SIZE,
            });
        }

        let payload_len = u32::try_from(payload.len()).map_err(|_| ProtocolError::LengthOverflow)?;
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
    Truncated { expected: usize, actual: usize },
    TrailingBytes { expected: usize, actual: usize },
    UnsupportedVersion(u8),
    UnknownMessageType(u8),
    InvalidPayloadLength {
        kind: MessageKind,
        expected: usize,
        actual: usize,
    },
    InvalidField(&'static str),
    InvalidReason {
        field: &'static str,
        value: u8,
    },
    PayloadTooLarge { actual: usize, maximum: usize },
    LengthOverflow,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => formatter.write_str("invalid protocol magic"),
            Self::Truncated { expected, actual } => {
                write!(formatter, "truncated frame: expected {expected} bytes, got {actual}")
            }
            Self::TrailingBytes { expected, actual } => {
                write!(formatter, "trailing frame bytes: expected {expected}, got {actual}")
            }
            Self::UnsupportedVersion(version) => write!(formatter, "unsupported protocol version {version}"),
            Self::UnknownMessageType(kind) => write!(formatter, "unknown message type {kind}"),
            Self::InvalidPayloadLength {
                kind,
                expected,
                actual,
            } => write!(formatter, "invalid {kind:?} payload length: expected {expected}, got {actual}"),
            Self::InvalidField(field) => write!(formatter, "invalid {field}"),
            Self::InvalidReason { field, value } => write!(formatter, "invalid {field} value {value}"),
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

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u64(payload: &[u8], offset: usize) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&payload[offset..offset + 8]);
    u64::from_le_bytes(bytes)
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
            let reason = DisconnectReason::from_wire(payload[8]).ok_or(ProtocolError::InvalidReason {
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
            let reason = TerminationReason::from_wire(payload[8]).ok_or(ProtocolError::InvalidReason {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_version_and_body() {
        let message = ProtocolMessage::new(MessageBody::Data {
            session_id: SessionId::new(42),
            payload: vec![1, 2, 3],
        })
        .expect("valid message");

        let decoded = ProtocolMessage::decode(&message.encode().expect("encodes")).expect("decodes");

        assert_eq!(decoded, message);
        assert_eq!(decoded.version(), ProtocolVersion::V1);
    }

    #[test]
    fn malformed_frames_are_rejected_without_panicking() {
        let message = ProtocolMessage::new(MessageBody::Reconnect {
            session_id: SessionId::new(4),
        })
        .expect("valid message");
        let frame = message.encode().expect("encodes");

        assert_eq!(ProtocolMessage::decode(&frame[..HEADER_SIZE - 1]), Err(ProtocolError::Truncated {
            expected: HEADER_SIZE,
            actual: HEADER_SIZE - 1,
        }));

        let mut bad_magic = frame.clone();
        bad_magic[0] = 0;
        assert_eq!(ProtocolMessage::decode(&bad_magic), Err(ProtocolError::InvalidMagic));

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
    }
}
