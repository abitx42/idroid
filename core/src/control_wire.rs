//! V2 wire encoding for the Android/macOS control plane.
//!
//! Framing (magic, version, kind, little-endian length) is shared with
//! [`crate::protocol`]; this module owns only the message payloads. Every
//! integer is little-endian, strings are UTF-8 prefixed with a `u16` length,
//! and optional values are a one-byte present flag (`0` absent, `1` present)
//! followed by the value. Tags are never reused: new variants append.
//!
//! The payload for a message is always exactly the bytes the decoder consumes;
//! trailing bytes are rejected so a peer cannot smuggle data past a message.

use crate::control_plane::{
    Backend, BackendCapability, BackendChangeReason, BackendChanged, BackendSelection, Bye,
    Capabilities, CapabilityReason, CapabilityStatus, ControlError, ControlErrorCode,
    ControlMessage, DeviceFingerprint, Feature, FeatureCapabilities, Heartbeat, Hello, HelloAck,
    LinkState, PeerRole, SessionOutcome, SessionRequest, SessionResult, UserAction, VersionRange,
    KEY_FINGERPRINT_SIZE, MAX_APP_VERSION_BYTES, MAX_FEATURE_BACKENDS,
};
use crate::ids::SessionId;
use crate::protocol::{map_contract_error, MessageKind, ProtocolError, MAX_PAYLOAD_SIZE};

const ABSENT: u8 = 0;
const PRESENT: u8 = 1;

pub(crate) fn encode(message: &ControlMessage) -> Result<Vec<u8>, ProtocolError> {
    message.validate().map_err(map_contract_error)?;
    let mut out = Vec::new();
    match message {
        ControlMessage::Hello(message) => {
            put_u16(&mut out, message.version_min);
            put_u16(&mut out, message.version_max);
            out.extend_from_slice(&message.device_id.as_bytes());
            out.push(message.role.wire_value());
            put_string(
                &mut out,
                "app_version",
                &message.app_version,
                MAX_APP_VERSION_BYTES,
            )?;
            put_optional_session(&mut out, message.last_session_id);
        }
        ControlMessage::HelloAck(message) => put_u16(&mut out, message.version),
        ControlMessage::Capabilities(message) | ControlMessage::CapabilitiesUpdate(message) => {
            put_u8(
                &mut out,
                u8::try_from(message.features.len())
                    .map_err(|_| ProtocolError::InvalidField("features"))?,
            );
            for feature in &message.features {
                out.push(feature.feature.wire_value());
                put_u8(
                    &mut out,
                    u8::try_from(feature.backends.len())
                        .map_err(|_| ProtocolError::InvalidField("backends"))?,
                );
                for backend in &feature.backends {
                    out.push(backend.backend.wire_value());
                    out.push(backend.status.wire_value());
                    put_optional_reason(&mut out, backend.reason);
                }
            }
        }
        ControlMessage::SessionRequest(message) => {
            put_u64(&mut out, message.request_id);
            out.push(message.feature.wire_value());
            put_optional_backend(&mut out, message.preferred_backend);
        }
        ControlMessage::SessionResult(message) => {
            put_u64(&mut out, message.request_id);
            out.push(message.feature.wire_value());
            match message.outcome {
                SessionOutcome::Accepted { backend } => {
                    out.push(1);
                    out.push(backend.wire_value());
                }
                SessionOutcome::NeedsUserAction { action } => {
                    out.push(2);
                    out.push(action.wire_value());
                }
                SessionOutcome::Rejected { code } => {
                    out.push(3);
                    out.push(code.wire_value());
                }
            }
        }
        ControlMessage::Heartbeat(message) => {
            put_u64(&mut out, message.seq);
            out.push(message.link_state.wire_value());
            put_optional_selection(&mut out, message.active_backend);
        }
        ControlMessage::BackendChanged(message) => {
            out.push(message.feature.wire_value());
            out.push(message.from.wire_value());
            out.push(message.to.wire_value());
            out.push(message.reason.wire_value());
        }
        ControlMessage::Error(message) => {
            out.push(message.code.wire_value());
            put_bool(&mut out, message.retryable);
            put_optional_action(&mut out, message.user_action);
        }
        ControlMessage::Bye(message) => {
            put_optional_code(&mut out, message.code);
            put_bool(&mut out, message.retryable);
            put_optional_action(&mut out, message.user_action);
        }
    }

    if out.len() > MAX_PAYLOAD_SIZE {
        return Err(ProtocolError::PayloadTooLarge {
            actual: out.len(),
            maximum: MAX_PAYLOAD_SIZE,
        });
    }
    Ok(out)
}

pub(crate) fn decode(kind: MessageKind, payload: &[u8]) -> Result<ControlMessage, ProtocolError> {
    let mut reader = Reader::new(payload);
    let message = match kind {
        MessageKind::ControlHello => {
            let version_min = reader.u16()?;
            let version_max = reader.u16()?;
            let fingerprint = reader.fingerprint()?;
            let role = reader.peer_role()?;
            let app_version = reader.string("app_version", MAX_APP_VERSION_BYTES)?;
            let last_session_id = reader.optional_session()?;
            ControlMessage::Hello(
                Hello::new(
                    VersionRange {
                        min: version_min,
                        max: version_max,
                    },
                    fingerprint,
                    role,
                    app_version,
                    last_session_id,
                )
                .map_err(map_contract_error)?,
            )
        }
        MessageKind::ControlHelloAck => {
            ControlMessage::HelloAck(HelloAck::new(reader.u16()?).map_err(map_contract_error)?)
        }
        MessageKind::Capabilities | MessageKind::CapabilitiesUpdate => {
            let capabilities = decode_capabilities(&mut reader)?;
            if kind == MessageKind::Capabilities {
                ControlMessage::Capabilities(capabilities)
            } else {
                ControlMessage::CapabilitiesUpdate(capabilities)
            }
        }
        MessageKind::SessionRequest => {
            let request_id = reader.u64()?;
            let feature = reader.feature()?;
            let preferred_backend = reader.optional_backend()?;
            ControlMessage::SessionRequest(
                SessionRequest::new(request_id, feature, preferred_backend)
                    .map_err(map_contract_error)?,
            )
        }
        MessageKind::SessionResult => {
            let request_id = reader.u64()?;
            let feature = reader.feature()?;
            let outcome = match reader.u8()? {
                1 => SessionOutcome::Accepted {
                    backend: reader.backend()?,
                },
                2 => SessionOutcome::NeedsUserAction {
                    action: reader.user_action()?,
                },
                3 => SessionOutcome::Rejected {
                    code: reader.control_error_code()?,
                },
                value => {
                    return Err(ProtocolError::InvalidEnum {
                        field: "session_outcome",
                        value,
                    });
                }
            };
            ControlMessage::SessionResult(
                SessionResult::new(request_id, feature, outcome).map_err(map_contract_error)?,
            )
        }
        MessageKind::ControlHeartbeat => {
            let seq = reader.u64()?;
            let link_state = reader.link_state()?;
            let active_backend = reader.optional_selection()?;
            ControlMessage::Heartbeat(
                Heartbeat::new(seq, link_state, active_backend).map_err(map_contract_error)?,
            )
        }
        MessageKind::BackendChanged => {
            let feature = reader.feature()?;
            let from = reader.backend()?;
            let to = reader.backend()?;
            let reason = reader.backend_change_reason()?;
            ControlMessage::BackendChanged(
                BackendChanged::new(feature, from, to, reason).map_err(map_contract_error)?,
            )
        }
        MessageKind::ControlError => {
            let code = reader.control_error_code()?;
            let retryable = reader.bool()?;
            let user_action = reader.optional_action()?;
            let error = ControlError::new(code, retryable, user_action);
            error.validate().map_err(map_contract_error)?;
            ControlMessage::Error(error)
        }
        MessageKind::Bye => {
            let bye = Bye {
                code: reader.optional_code()?,
                retryable: reader.bool()?,
                user_action: reader.optional_action()?,
            };
            bye.validate().map_err(map_contract_error)?;
            ControlMessage::Bye(bye)
        }
        other => {
            let _ = other;
            return Err(ProtocolError::InvalidField("control_message"));
        }
    };

    reader.finish(kind)?;
    message.validate().map_err(map_contract_error)?;
    Ok(message)
}

fn decode_capabilities(reader: &mut Reader<'_>) -> Result<Capabilities, ProtocolError> {
    let feature_count = usize::from(reader.u8()?);
    if feature_count != Feature::ALL.len() {
        return Err(ProtocolError::InvalidField("features"));
    }
    let mut features = Vec::with_capacity(feature_count);
    for _ in 0..feature_count {
        let feature = reader.feature()?;
        let backend_count = usize::from(reader.u8()?);
        if backend_count == 0 || backend_count > MAX_FEATURE_BACKENDS {
            return Err(ProtocolError::InvalidField("backends"));
        }
        let mut backends = Vec::with_capacity(backend_count);
        for _ in 0..backend_count {
            backends.push(BackendCapability {
                backend: reader.backend()?,
                status: reader.capability_status()?,
                reason: reader.optional_reason()?,
            });
        }
        features.push(FeatureCapabilities { feature, backends });
    }
    let capabilities = Capabilities { features };
    capabilities.validate().map_err(map_contract_error)?;
    Ok(capabilities)
}

fn put_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bool(out: &mut Vec<u8>, value: bool) {
    out.push(u8::from(value));
}

fn put_string(
    out: &mut Vec<u8>,
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), ProtocolError> {
    if value.len() > maximum {
        return Err(ProtocolError::FieldTooLarge {
            field,
            actual: value.len(),
            maximum,
        });
    }
    put_u16(
        out,
        u16::try_from(value.len()).map_err(|_| ProtocolError::LengthOverflow)?,
    );
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_optional_session(out: &mut Vec<u8>, value: Option<SessionId>) {
    match value {
        Some(value) => {
            out.push(PRESENT);
            put_u64(out, value.get());
        }
        None => out.push(ABSENT),
    }
}

fn put_optional_backend(out: &mut Vec<u8>, value: Option<Backend>) {
    match value {
        Some(value) => {
            out.push(PRESENT);
            out.push(value.wire_value());
        }
        None => out.push(ABSENT),
    }
}

fn put_optional_selection(out: &mut Vec<u8>, value: Option<BackendSelection>) {
    match value {
        Some(value) => {
            out.push(PRESENT);
            out.push(value.feature.wire_value());
            out.push(value.backend.wire_value());
        }
        None => out.push(ABSENT),
    }
}

fn put_optional_reason(out: &mut Vec<u8>, value: Option<CapabilityReason>) {
    match value {
        Some(value) => {
            out.push(PRESENT);
            out.push(value.wire_value());
        }
        None => out.push(ABSENT),
    }
}

fn put_optional_action(out: &mut Vec<u8>, value: Option<UserAction>) {
    match value {
        Some(value) => {
            out.push(PRESENT);
            out.push(value.wire_value());
        }
        None => out.push(ABSENT),
    }
}

fn put_optional_code(out: &mut Vec<u8>, value: Option<ControlErrorCode>) {
    match value {
        Some(value) => {
            out.push(PRESENT);
            out.push(value.wire_value());
        }
        None => out.push(ABSENT),
    }
}

struct Reader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(ProtocolError::LengthOverflow)?;
        if end > self.payload.len() {
            return Err(ProtocolError::Truncated {
                expected: end,
                actual: self.payload.len(),
            });
        }
        let bytes = &self.payload[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn bool(&mut self) -> Result<bool, ProtocolError> {
        let value = self.u8()?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(ProtocolError::InvalidEnum {
                field: "bool",
                value,
            }),
        }
    }

    fn fingerprint(&mut self) -> Result<DeviceFingerprint, ProtocolError> {
        let bytes = self.take(KEY_FINGERPRINT_SIZE)?;
        DeviceFingerprint::from_bytes(bytes).map_err(map_contract_error)
    }

    fn string(&mut self, field: &'static str, maximum: usize) -> Result<String, ProtocolError> {
        let length = usize::from(self.u16()?);
        if length > maximum {
            return Err(ProtocolError::FieldTooLarge {
                field,
                actual: length,
                maximum,
            });
        }
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ProtocolError::InvalidField(field))
    }

    fn feature(&mut self) -> Result<Feature, ProtocolError> {
        let value = self.u8()?;
        Feature::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "feature",
            value,
        })
    }

    fn backend(&mut self) -> Result<Backend, ProtocolError> {
        let value = self.u8()?;
        Backend::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "backend",
            value,
        })
    }

    fn peer_role(&mut self) -> Result<PeerRole, ProtocolError> {
        let value = self.u8()?;
        PeerRole::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "peer_role",
            value,
        })
    }

    fn link_state(&mut self) -> Result<LinkState, ProtocolError> {
        let value = self.u8()?;
        LinkState::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "link_state",
            value,
        })
    }

    fn capability_status(&mut self) -> Result<CapabilityStatus, ProtocolError> {
        let value = self.u8()?;
        CapabilityStatus::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "capability_status",
            value,
        })
    }

    fn capability_reason(&mut self) -> Result<CapabilityReason, ProtocolError> {
        let value = self.u8()?;
        CapabilityReason::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "capability_reason",
            value,
        })
    }

    fn user_action(&mut self) -> Result<UserAction, ProtocolError> {
        let value = self.u8()?;
        UserAction::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "user_action",
            value,
        })
    }

    fn control_error_code(&mut self) -> Result<ControlErrorCode, ProtocolError> {
        let value = self.u8()?;
        ControlErrorCode::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "control_error_code",
            value,
        })
    }

    fn backend_change_reason(&mut self) -> Result<BackendChangeReason, ProtocolError> {
        let value = self.u8()?;
        BackendChangeReason::from_wire(value).ok_or(ProtocolError::InvalidEnum {
            field: "change_reason",
            value,
        })
    }

    fn optional_session(&mut self) -> Result<Option<SessionId>, ProtocolError> {
        match self.u8()? {
            ABSENT => Ok(None),
            PRESENT => Ok(Some(SessionId::new(self.u64()?))),
            value => Err(ProtocolError::InvalidEnum {
                field: "last_session_id",
                value,
            }),
        }
    }

    fn optional_backend(&mut self) -> Result<Option<Backend>, ProtocolError> {
        match self.u8()? {
            ABSENT => Ok(None),
            PRESENT => Ok(Some(self.backend()?)),
            value => Err(ProtocolError::InvalidEnum {
                field: "preferred_backend",
                value,
            }),
        }
    }

    fn optional_selection(&mut self) -> Result<Option<BackendSelection>, ProtocolError> {
        match self.u8()? {
            ABSENT => Ok(None),
            PRESENT => {
                let feature = self.feature()?;
                let backend = self.backend()?;
                Ok(Some(
                    BackendSelection::new(feature, backend).map_err(map_contract_error)?,
                ))
            }
            value => Err(ProtocolError::InvalidEnum {
                field: "active_backend",
                value,
            }),
        }
    }

    fn optional_reason(&mut self) -> Result<Option<CapabilityReason>, ProtocolError> {
        match self.u8()? {
            ABSENT => Ok(None),
            PRESENT => Ok(Some(self.capability_reason()?)),
            value => Err(ProtocolError::InvalidEnum {
                field: "capability_reason",
                value,
            }),
        }
    }

    fn optional_action(&mut self) -> Result<Option<UserAction>, ProtocolError> {
        match self.u8()? {
            ABSENT => Ok(None),
            PRESENT => Ok(Some(self.user_action()?)),
            value => Err(ProtocolError::InvalidEnum {
                field: "user_action",
                value,
            }),
        }
    }

    fn optional_code(&mut self) -> Result<Option<ControlErrorCode>, ProtocolError> {
        match self.u8()? {
            ABSENT => Ok(None),
            PRESENT => Ok(Some(self.control_error_code()?)),
            value => Err(ProtocolError::InvalidEnum {
                field: "control_error_code",
                value,
            }),
        }
    }

    fn finish(&self, kind: MessageKind) -> Result<(), ProtocolError> {
        if self.offset == self.payload.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidPayloadLength {
                kind,
                expected: self.offset,
                actual: self.payload.len(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::{BackendCapability, CapabilityReason, PeerRole, UserAction};

    fn hello() -> Hello {
        Hello::new(
            VersionRange { min: 1, max: 2 },
            DeviceFingerprint::from_array([7; 32]),
            PeerRole::Android,
            "0.1.0",
            Some(SessionId::new(9)),
        )
        .expect("valid hello")
    }

    fn capabilities() -> Capabilities {
        Capabilities::new(
            FeatureCapabilities::new(
                Feature::Video,
                [
                    BackendCapability::available(Backend::Helper),
                    BackendCapability::needs_user_action(
                        Backend::MediaProjection,
                        CapabilityReason::MediaProjectionConsentRequired,
                    ),
                ],
            )
            .expect("valid video capabilities"),
            FeatureCapabilities::new(
                Feature::Input,
                [
                    BackendCapability::available(Backend::Helper),
                    BackendCapability::unavailable(
                        Backend::Accessibility,
                        CapabilityReason::AccessibilityPermissionRequired,
                    ),
                ],
            )
            .expect("valid input capabilities"),
        )
        .expect("valid capabilities")
    }

    fn kind_of(message: &ControlMessage) -> MessageKind {
        match message {
            ControlMessage::Hello(_) => MessageKind::ControlHello,
            ControlMessage::HelloAck(_) => MessageKind::ControlHelloAck,
            ControlMessage::Capabilities(_) => MessageKind::Capabilities,
            ControlMessage::CapabilitiesUpdate(_) => MessageKind::CapabilitiesUpdate,
            ControlMessage::SessionRequest(_) => MessageKind::SessionRequest,
            ControlMessage::SessionResult(_) => MessageKind::SessionResult,
            ControlMessage::Heartbeat(_) => MessageKind::ControlHeartbeat,
            ControlMessage::BackendChanged(_) => MessageKind::BackendChanged,
            ControlMessage::Error(_) => MessageKind::ControlError,
            ControlMessage::Bye(_) => MessageKind::Bye,
        }
    }

    fn round_trip(message: ControlMessage) {
        let payload = encode(&message).expect("encodes");
        assert_eq!(
            decode(kind_of(&message), &payload).expect("decodes"),
            message
        );
    }

    #[test]
    fn every_control_message_round_trips() {
        round_trip(ControlMessage::Hello(hello()));
        round_trip(ControlMessage::HelloAck(
            HelloAck::new(2).expect("valid ack"),
        ));
        round_trip(ControlMessage::Capabilities(capabilities()));
        round_trip(ControlMessage::CapabilitiesUpdate(capabilities()));
        round_trip(ControlMessage::SessionRequest(
            SessionRequest::new(1, Feature::Video, Some(Backend::Helper)).expect("valid request"),
        ));
        round_trip(ControlMessage::SessionResult(
            SessionResult::new(
                1,
                Feature::Input,
                SessionOutcome::NeedsUserAction {
                    action: UserAction::GrantAccessibility,
                },
            )
            .expect("valid result"),
        ));
        round_trip(ControlMessage::Heartbeat(
            Heartbeat::new(
                4,
                LinkState::Ready,
                Some(
                    BackendSelection::new(Feature::Video, Backend::Helper)
                        .expect("valid selection"),
                ),
            )
            .expect("valid heartbeat"),
        ));
        round_trip(ControlMessage::BackendChanged(
            BackendChanged::new(
                Feature::Video,
                Backend::Helper,
                Backend::MediaProjection,
                BackendChangeReason::HelperLost,
            )
            .expect("valid change"),
        ));
        round_trip(ControlMessage::Error(ControlError::new(
            ControlErrorCode::UserActionRequired,
            true,
            Some(UserAction::GrantMediaProjection),
        )));
        round_trip(ControlMessage::Bye(Bye::normal()));
    }

    #[test]
    fn hello_carries_the_transport_fingerprint() {
        let payload = encode(&ControlMessage::Hello(hello())).expect("encodes");
        let decoded = decode(MessageKind::ControlHello, &payload).expect("decodes");
        let ControlMessage::Hello(decoded) = decoded else {
            panic!("expected hello");
        };
        assert_eq!(decoded.device_id.as_bytes(), [7; 32]);
        assert_eq!(decoded.device_id.as_bytes().len(), KEY_FINGERPRINT_SIZE);
        assert_eq!(
            decoded.verify_transport_identity(DeviceFingerprint::from_array([7; 32])),
            Ok(())
        );
        assert!(decoded
            .verify_transport_identity(DeviceFingerprint::from_array([8; 32]))
            .is_err());
    }

    #[test]
    fn malformed_control_payloads_are_rejected() {
        let payload = encode(&ControlMessage::Hello(hello())).expect("encodes");
        assert!(decode(MessageKind::ControlHello, &payload[..payload.len() - 1]).is_err());

        let mut trailing = payload.clone();
        trailing.push(0);
        assert!(decode(MessageKind::ControlHello, &trailing).is_err());

        let mut bad_role = payload.clone();
        bad_role[36] = 0xff;
        assert!(matches!(
            decode(MessageKind::ControlHello, &bad_role),
            Err(ProtocolError::InvalidEnum {
                field: "peer_role",
                ..
            })
        ));
    }

    #[test]
    fn wrong_kind_for_payload_is_rejected() {
        let payload = encode(&ControlMessage::HelloAck(
            HelloAck::new(2).expect("valid ack"),
        ))
        .expect("encodes");
        assert!(decode(MessageKind::ControlHello, &payload).is_err());
        assert!(decode(MessageKind::Bye, &payload).is_err());
    }

    #[test]
    fn capabilities_require_both_features() {
        let payload = encode(&ControlMessage::Capabilities(capabilities())).expect("encodes");
        let mut only_video = Vec::new();
        put_u8(&mut only_video, 1);
        only_video.extend_from_slice(&payload[1..]);
        assert!(matches!(
            decode(MessageKind::Capabilities, &only_video),
            Err(ProtocolError::InvalidField("features"))
        ));
    }

    #[test]
    fn identical_messages_encode_identically() {
        let first = encode(&ControlMessage::Capabilities(capabilities())).expect("encodes");
        let second = encode(&ControlMessage::Capabilities(capabilities())).expect("encodes");
        assert_eq!(first, second);
    }
}
