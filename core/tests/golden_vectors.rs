//! Golden frames shared by every language binding.
//!
//! The fixture lives in `tests/protocol/control-plane-v2.txt` and is written by
//! hand, not generated at test time. These tests pin the exact byte layout and
//! then check that the encoder reproduces the same bytes, so an encoder change
//! that silently breaks Kotlin or Swift fails here first.

use idroid_core::{
    Bye, ControlErrorCode, ControlMessage, DeviceFingerprint, Hello, HelloAck, MessageBody,
    MessageKind, PeerRole, ProtocolMessage, ProtocolVersion, SessionId,
};

const FIXTURE: &str = include_str!("../../tests/protocol/control-plane-v2.txt");

fn vectors() -> Vec<(String, Vec<u8>)> {
    let mut vectors = Vec::new();
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, hex) = line
            .split_once(' ')
            .expect("name and hex separated by a space");
        assert_eq!(hex.len() % 2, 0, "{name} has an odd number of hex digits");
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex"))
            .collect::<Vec<_>>();
        vectors.push((name.to_owned(), bytes));
    }
    vectors
}

fn vector(name: &str) -> Vec<u8> {
    vectors()
        .into_iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, bytes)| bytes)
        .unwrap_or_else(|| panic!("missing vector {name}"))
}

fn fingerprint_ascending() -> DeviceFingerprint {
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = index as u8;
    }
    DeviceFingerprint::from_array(bytes)
}

#[test]
fn fixture_names_are_unique() {
    let vectors = vectors();
    for (index, (name, _)) in vectors.iter().enumerate() {
        assert!(
            vectors[..index].iter().all(|(other, _)| other != name),
            "duplicate vector {name}"
        );
    }
    assert!(vectors.len() >= 7, "expected the full v1 and v2 set");
}

#[test]
fn hello_android_v2_has_the_expected_bytes() {
    let frame = vector("hello.android.v2");
    assert_eq!(&frame[..4], b"IDR0");
    assert_eq!(frame[4], 2, "protocol version");
    assert_eq!(frame[5], MessageKind::ControlHello.wire_value());
    assert_eq!(
        u32::from_le_bytes([frame[6], frame[7], frame[8], frame[9]]) as usize,
        frame.len() - 10
    );
    assert_eq!(&frame[10..12], &1u16.to_le_bytes(), "version_min");
    assert_eq!(&frame[12..14], &2u16.to_le_bytes(), "version_max");
    assert_eq!(
        &frame[14..46],
        &fingerprint_ascending().as_bytes(),
        "fingerprint"
    );
    assert_eq!(frame[46], PeerRole::Android.wire_value());
    assert_eq!(&frame[47..49], &5u16.to_le_bytes(), "app_version_len");
    assert_eq!(&frame[49..54], b"0.1.0");
    assert_eq!(frame[54], 0, "no last session");

    let decoded = ProtocolMessage::decode(&frame).expect("decodes");
    assert_eq!(decoded.version(), ProtocolVersion::V2);
    let MessageBody::Control(ControlMessage::Hello(hello)) = decoded.body() else {
        panic!("expected a control hello");
    };
    assert_eq!(hello.version_min, 1);
    assert_eq!(hello.version_max, 2);
    assert_eq!(hello.device_id, fingerprint_ascending());
    assert_eq!(hello.role, PeerRole::Android);
    assert_eq!(hello.app_version, "0.1.0");
    assert_eq!(hello.last_session_id, None);
}

#[test]
fn hello_mac_v2_carries_the_previous_session() {
    let frame = vector("hello.mac.v2");
    assert_eq!(frame[4], 2);
    assert_eq!(frame[5], MessageKind::ControlHello.wire_value());
    assert_eq!(&frame[14..46], &[0xff; 32]);
    assert_eq!(frame[46], PeerRole::Mac.wire_value());
    assert_eq!(&frame[47..49], &5u16.to_le_bytes());
    assert_eq!(&frame[49..54], b"1.2.3");
    assert_eq!(frame[54], 1, "last session present");
    assert_eq!(&frame[55..63], &0x0102_0304_0506_0708u64.to_le_bytes());

    let decoded = ProtocolMessage::decode(&frame).expect("decodes");
    let MessageBody::Control(ControlMessage::Hello(hello)) = decoded.body() else {
        panic!("expected a control hello");
    };
    assert_eq!(hello.role, PeerRole::Mac);
    assert_eq!(
        hello.last_session_id,
        Some(SessionId::new(0x0102_0304_0506_0708))
    );
}

#[test]
fn control_frames_round_trip_to_the_same_bytes() {
    let cases = vec![
        (
            "hello.android.v2",
            ControlMessage::Hello(
                Hello::new(
                    idroid_core::VersionRange { min: 1, max: 2 },
                    fingerprint_ascending(),
                    PeerRole::Android,
                    "0.1.0",
                    None,
                )
                .expect("valid"),
            ),
        ),
        (
            "hello.mac.v2",
            ControlMessage::Hello(
                Hello::new(
                    idroid_core::VersionRange { min: 1, max: 2 },
                    DeviceFingerprint::from_array([0xff; 32]),
                    PeerRole::Mac,
                    "1.2.3",
                    Some(SessionId::new(0x0102_0304_0506_0708)),
                )
                .expect("valid"),
            ),
        ),
        (
            "hello_ack.v2",
            ControlMessage::HelloAck(HelloAck::new(2).expect("valid")),
        ),
        ("bye.normal.v2", ControlMessage::Bye(Bye::normal())),
        (
            "bye.timeout.v2",
            ControlMessage::Bye(Bye {
                code: Some(ControlErrorCode::Timeout),
                retryable: true,
                user_action: None,
            }),
        ),
    ];

    for (name, message) in cases {
        let encoded = ProtocolMessage::new(message)
            .expect("valid")
            .encode()
            .expect("encodes");
        assert_eq!(encoded, vector(name), "{name} encoder drift");
    }
}

#[test]
fn legacy_frames_remain_version_one() {
    let frame = vector("heartbeat.v1");
    assert_eq!(frame[4], 1);
    assert_eq!(frame[5], MessageKind::Heartbeat.wire_value());

    let decoded = ProtocolMessage::decode(&frame).expect("decodes");
    assert_eq!(decoded.version(), ProtocolVersion::V1);
    assert_eq!(
        decoded.body(),
        &MessageBody::Heartbeat {
            session_id: SessionId::new(2),
            sequence: 3,
        }
    );

    let data = vector("data.v1");
    assert_eq!(data[4], 1);
    assert_eq!(data[5], MessageKind::Data.wire_value());
}

#[test]
fn corrupt_vectors_are_rejected() {
    let frame = vector("hello.android.v2");

    let mut bad_magic = frame.clone();
    bad_magic[0] = b'X';
    assert!(ProtocolMessage::decode(&bad_magic).is_err());

    assert!(ProtocolMessage::decode(&frame[..frame.len() - 1]).is_err());

    let mut trailing = frame.clone();
    trailing.push(0);
    assert!(ProtocolMessage::decode(&trailing).is_err());

    let mut bad_version = frame.clone();
    bad_version[4] = 9;
    assert!(ProtocolMessage::decode(&bad_version).is_err());

    let mut bad_kind = frame.clone();
    bad_kind[5] = 0xff;
    assert!(ProtocolMessage::decode(&bad_kind).is_err());
}
