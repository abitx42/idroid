use idroid_core::security::{
    FixtureIdentityVerifier, FixtureMessageAuthenticator, IdentityVerifier, MessageAuthenticator,
};
use idroid_core::{
    CaptureState, DeviceCapabilities, DeviceId, DeviceKind, DeviceProfile, DeviceRegistry,
    DeviceStatus, ErrorCode, KeyAction, MessageBody, MessageKind, PermissionKind, ProtocolError,
    ProtocolMessage, SessionConfig, SessionId, SessionManager, SessionStatus, TerminationReason,
    Timestamp,
};

fn at(milliseconds: u64) -> Timestamp {
    Timestamp::from_millis(milliseconds)
}

#[test]
fn device_and_session_dimensions_remain_independent() {
    let device_id = DeviceId::new(100);
    let profile = DeviceProfile::new(
        device_id,
        DeviceKind::Phone,
        DeviceCapabilities::DISPLAY.with(DeviceCapabilities::DEBUG_BRIDGE),
    )
    .expect("valid device profile");
    let mut devices = DeviceRegistry::default();
    devices.register(profile).expect("register device");

    let mut sessions = SessionManager::new(SessionConfig::new(100, 50, 500));
    let first = sessions
        .start_session(device_id, at(0))
        .expect("first session");
    let second = sessions
        .start_session(device_id, at(1))
        .expect("replacement session");

    assert_eq!(
        devices.get(device_id).expect("device").kind(),
        DeviceKind::Phone
    );
    assert_ne!(first.session_id, second.session_id);
    assert_eq!(
        sessions
            .get(second.session_id)
            .expect("session")
            .device_id(),
        device_id
    );
    assert_eq!(
        sessions
            .get(first.session_id)
            .expect("superseded session")
            .termination_reason(),
        Some(TerminationReason::Duplicate)
    );
}

#[test]
fn protocol_round_trip_and_malformed_input_are_observable() {
    let message = ProtocolMessage::new(MessageBody::Heartbeat {
        session_id: SessionId::new(55),
        sequence: 7,
    })
    .expect("valid heartbeat");
    let frame = message.encode().expect("encode");
    assert_eq!(ProtocolMessage::decode(&frame).expect("decode"), message);

    assert!(matches!(
        ProtocolMessage::decode(&frame[..frame.len() - 1]),
        Err(ProtocolError::Truncated { .. })
    ));

    let mut unsupported_version = frame.clone();
    unsupported_version[4] = 99;
    assert_eq!(
        ProtocolMessage::decode(&unsupported_version),
        Err(ProtocolError::UnsupportedVersion(99))
    );

    let mut invalid_id = frame;
    invalid_id[10..18].copy_from_slice(&0_u64.to_le_bytes());
    assert_eq!(
        ProtocolMessage::decode(&invalid_id),
        Err(ProtocolError::InvalidField("session_id"))
    );
}

#[test]
fn every_extended_protocol_kind_round_trips_through_the_public_api() {
    let cases = vec![
        MessageBody::PairingInitiation {
            device_id: DeviceId::new(7),
            nonce: vec![1, 2, 3],
        },
        MessageBody::PairingCompletion {
            device_id: DeviceId::new(7),
            accepted: true,
        },
        MessageBody::SessionEstablishment {
            device_id: DeviceId::new(7),
            session_id: SessionId::new(8),
        },
        MessageBody::DeviceStatus {
            device_id: DeviceId::new(7),
            status: DeviceStatus::Online,
            capabilities: 0x03,
        },
        MessageBody::Terminate {
            session_id: SessionId::new(8),
            reason: TerminationReason::Requested,
        },
        MessageBody::Reconnect {
            session_id: SessionId::new(8),
        },
        MessageBody::TapInput {
            session_id: SessionId::new(8),
            x: 10,
            y: 20,
        },
        MessageBody::PointerMovement {
            session_id: SessionId::new(8),
            x: -2,
            y: 4,
        },
        MessageBody::KeyboardInput {
            session_id: SessionId::new(8),
            key_code: 30,
            action: KeyAction::Press,
            modifiers: 0,
        },
        MessageBody::PermissionChange {
            device_id: DeviceId::new(7),
            permission: PermissionKind::ScreenCapture,
            granted: true,
        },
        MessageBody::CaptureState {
            session_id: SessionId::new(8),
            state: CaptureState::Active,
        },
        MessageBody::Error {
            session_id: Some(SessionId::new(8)),
            code: ErrorCode::PermissionDenied,
            message: "permission denied".to_owned(),
        },
    ];

    for body in cases {
        let expected_kind = body.kind();
        let message = ProtocolMessage::new(body).expect("valid message");
        assert_eq!(message.kind(), expected_kind);
        let frame = message.encode().expect("encode");
        assert_eq!(frame[5], expected_kind.as_u8());
        assert_eq!(ProtocolMessage::decode(&frame).expect("decode"), message);
    }

    assert_eq!(MessageKind::PairingInit, MessageKind::PairingInitiation);
    assert_eq!(MessageKind::SessionTermination, MessageKind::Terminate);
    assert_eq!(MessageKind::SessionReconnection, MessageKind::Reconnect);
}

#[test]
fn lifecycle_supports_temporary_reconnect_and_offline_termination() {
    let mut manager = SessionManager::new(SessionConfig::new(10, 20, 30));
    let session_id = manager
        .start_session(DeviceId::new(3), at(0))
        .expect("session")
        .session_id;

    manager.sweep(at(10));
    assert_eq!(
        manager.get(session_id).expect("timed out").status(),
        SessionStatus::Disconnected
    );
    manager
        .reconnect(session_id, at(20))
        .expect("reconnect in grace");
    manager
        .disconnect(session_id, at(21))
        .expect("temporary disconnect");
    manager.sweep(at(41));
    assert_eq!(
        manager.get(session_id).expect("offline").status(),
        SessionStatus::Offline
    );

    manager
        .terminate(session_id, at(42), TerminationReason::Requested)
        .expect("terminate offline session");
    assert_eq!(
        manager.get(session_id).expect("terminated").status(),
        SessionStatus::Terminated
    );
    assert!(manager.sweep(at(72)).removed_stale.contains(&session_id));
}

#[test]
fn security_is_injected_and_fixtures_are_not_crypto_claims() {
    let device_id = DeviceId::new(88);
    let mut identity = FixtureIdentityVerifier::new();
    identity.allow(device_id, b"test-credential".to_vec());
    assert_eq!(
        identity
            .verify_identity(device_id, b"test-credential")
            .expect("fixture identity")
            .device_id(),
        device_id
    );

    let message_authenticator = FixtureMessageAuthenticator::new(b"test-tag".to_vec());
    assert!(message_authenticator
        .verify_message(b"opaque message", b"test-tag")
        .is_ok());
}
