use idroid_core::connections::{
    ConnectionArbiter, ConnectionDecision, ConnectionId, PeerConnection,
};
use idroid_core::control_plane::DeviceFingerprint;

fn fingerprint(seed: u8) -> DeviceFingerprint {
    let mut bytes = [0_u8; 32];
    bytes[0] = seed;
    DeviceFingerprint::from_array(bytes)
}

fn id(seed: u8) -> ConnectionId {
    ConnectionId([seed; 16])
}

#[test]
fn simultaneous_opposite_dials_keep_the_connection_started_by_lower_fingerprint() {
    let local = fingerprint(2);
    let peer = fingerprint(1);
    let mut arbiter = ConnectionArbiter::new(local).expect("arbiter");

    let first = arbiter
        .admit(
            peer,
            PeerConnection {
                id: id(2),
                initiator: local,
            },
        )
        .expect("first connection");
    assert_eq!(
        first,
        ConnectionDecision {
            keep: id(2),
            close: None
        }
    );

    let second = arbiter
        .admit(
            peer,
            PeerConnection {
                id: id(1),
                initiator: peer,
            },
        )
        .expect("simultaneous connection");
    assert_eq!(
        second,
        ConnectionDecision {
            keep: id(1),
            close: Some(id(2))
        }
    );
    assert_eq!(arbiter.active(peer).expect("active").id, id(1));
}

#[test]
fn duplicate_paths_from_one_initiator_keep_the_first_connection() {
    let local = fingerprint(1);
    let peer = fingerprint(2);
    let mut arbiter = ConnectionArbiter::new(local).expect("arbiter");

    arbiter
        .admit(
            peer,
            PeerConnection {
                id: id(3),
                initiator: local,
            },
        )
        .expect("first path");
    let duplicate = arbiter
        .admit(
            peer,
            PeerConnection {
                id: id(4),
                initiator: local,
            },
        )
        .expect("duplicate path");

    assert_eq!(
        duplicate,
        ConnectionDecision {
            keep: id(3),
            close: Some(id(4))
        }
    );
    assert_eq!(arbiter.active(peer).expect("active").id, id(3));
    assert!(!arbiter.closed(peer, id(4)));
    assert!(arbiter.closed(peer, id(3)));
}

#[test]
fn a_stale_loser_close_cannot_remove_the_survivor() {
    let local = fingerprint(1);
    let peer = fingerprint(2);
    let mut arbiter = ConnectionArbiter::new(local).expect("arbiter");
    arbiter
        .admit(
            peer,
            PeerConnection {
                id: id(8),
                initiator: local,
            },
        )
        .expect("active");
    assert!(!arbiter.closed(peer, id(9)));
    assert_eq!(arbiter.active(peer).expect("survivor").id, id(8));
}
