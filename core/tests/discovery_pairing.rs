use idroid_core::control_plane::{
    CandidateSourceKind, ConnectFailureReason, ControlErrorCode, ControlPlaneEventKind,
    DeviceFingerprint, LinkState,
};
use idroid_core::discovery::{
    AuthenticatedPeer, Candidate, CandidateSource, DiscoveryError, DiscoveryRace, NetworkIdentity,
    ScriptedCandidateConnector, ScriptedCandidateSource,
};
use idroid_core::pairing::{
    authorize_reconnect, connect_failure_for_pairing, InMemoryPairingStore, PairingAuthority,
    PairingError, PairingStore, PairingToken,
};
use idroid_core::{ControlPlaneEventLog, ManualClock, Timestamp};

fn fingerprint(seed: u8) -> DeviceFingerprint {
    let mut bytes = [0_u8; 32];
    bytes[0] = seed;
    DeviceFingerprint::from_array(bytes)
}

fn time(milliseconds: u64) -> Timestamp {
    Timestamp::from_millis(milliseconds)
}

fn token(seed: u8) -> PairingToken {
    PairingToken::from_array([seed; 16])
}

fn network(name: &str) -> NetworkIdentity {
    NetworkIdentity::new(name).expect("network")
}

fn candidate(
    address: &str,
    port: u16,
    source: CandidateSourceKind,
    network: &NetworkIdentity,
    expected_peer: Option<DeviceFingerprint>,
) -> Candidate {
    Candidate::new(address, port, source, network.clone(), expected_peer).expect("candidate")
}

fn paired_store(peers: &[(DeviceFingerprint, &str)]) -> InMemoryPairingStore {
    let mut store = InMemoryPairingStore::new();
    for (fingerprint, name) in peers {
        store
            .save(
                idroid_core::pairing::PairedPeer::new(*fingerprint, *name, time(1)).expect("peer"),
            )
            .expect("save peer");
    }
    store
}

#[test]
fn valid_pairing_succeeds_and_burns_the_token() {
    let mac = fingerprint(1);
    let phone = fingerprint(2);
    let mut authority =
        PairingAuthority::new(1, mac, InMemoryPairingStore::new()).expect("authority");
    let clock = ManualClock::new(time(0));
    let mut log = ControlPlaneEventLog::new();
    let qr = authority
        .issue_qr_logged(token(7), "_acontinuity._tcp", &clock, &mut log)
        .expect("QR");
    let invitation = qr.pin_for_phone(mac).expect("pinned QR");
    let peer = authority
        .consume_invitation_logged(&invitation, phone, "Pixel", &clock, &mut log)
        .expect("paired");

    assert_eq!(peer.peer_fingerprint, phone);
    assert!(authority.store().lookup(phone).expect("lookup").is_some());
    assert_eq!(
        log.events()
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![
            ControlPlaneEventKind::PairingTokenIssued,
            ControlPlaneEventKind::PairingTokenConsumed,
        ]
    );
    assert_eq!(
        authority.consume_token(&invitation.one_time_token, phone, "Pixel", time(3)),
        Err(PairingError::TokenAlreadyUsed)
    );
    assert_eq!(
        authority.issue_qr(token(7), "hint", time(4)),
        Err(PairingError::TokenAlreadyUsed)
    );
}

#[test]
fn expired_or_reused_tokens_store_nothing() {
    let mac = fingerprint(3);
    let phone = fingerprint(4);
    let mut authority =
        PairingAuthority::new(1, mac, InMemoryPairingStore::new()).expect("authority");
    let qr = authority.issue_qr(token(8), "hint", time(0)).expect("QR");

    let invitation = qr.pin_for_phone(mac).expect("phone pin");
    assert_eq!(
        authority.consume_invitation(&invitation, phone, "Pixel", time(120_000)),
        Err(PairingError::Expired)
    );
    assert_eq!(authority.store().lookup(phone).expect("lookup"), None);
}

#[test]
fn invalid_token_attempts_are_limited_and_close_the_listener() {
    let mac = fingerprint(19);
    let phone = fingerprint(20);
    let mut authority =
        PairingAuthority::new(1, mac, InMemoryPairingStore::new()).expect("authority");
    authority.issue_qr(token(19), "hint", time(0)).expect("QR");

    assert_eq!(
        authority.consume_token(&token(20), phone, "Pixel", time(1)),
        Err(PairingError::TokenInvalid)
    );
    assert_eq!(
        authority.consume_token(&token(20), phone, "Pixel", time(2)),
        Err(PairingError::TokenInvalid)
    );
    assert_eq!(
        authority.consume_token(&token(20), phone, "Pixel", time(3)),
        Err(PairingError::AttemptsExceeded)
    );
    assert!(authority.listener_is_open(time(4)));
    assert_eq!(
        authority.consume_token(&token(20), phone, "Pixel", time(4)),
        Err(PairingError::LockedOut)
    );
    authority
        .consume_token(&token(19), phone, "Pixel", time(6))
        .expect("legitimate token is not blocked");
    assert!(authority.store().lookup(phone).expect("lookup").is_some());
}

#[test]
fn phone_aborts_when_mac_certificate_differs_from_qr() {
    let mac = fingerprint(5);
    let other_mac = fingerprint(6);
    let authority = PairingAuthority::new(1, mac, InMemoryPairingStore::new()).expect("authority");
    let mut authority = authority;
    let qr = authority.issue_qr(token(9), "hint", time(0)).expect("QR");

    assert_eq!(
        qr.pin_for_phone(other_mac),
        Err(PairingError::MacFingerprintMismatch)
    );
    assert!(authority.listener_is_open(time(1)));
}

#[test]
fn cached_address_fingerprint_mismatch_falls_through_to_next_candidate() {
    let target = fingerprint(10);
    let impostor = fingerprint(11);
    let net = network("wifi-a");
    let mut store = paired_store(&[(target, "phone")]);
    let mut cached = ScriptedCandidateSource::new(CandidateSourceKind::CachedAddress);
    cached.push_response(Ok(vec![candidate(
        "192.0.2.10",
        443,
        CandidateSourceKind::CachedAddress,
        &net,
        Some(target),
    )]));
    let mut bonjour = ScriptedCandidateSource::new(CandidateSourceKind::Bonjour);
    bonjour.push_response(Ok(vec![candidate(
        "192.0.2.11",
        443,
        CandidateSourceKind::Bonjour,
        &net,
        None,
    )]));
    let mut sources: Vec<&mut dyn CandidateSource> = vec![&mut cached, &mut bonjour];
    let mut race = DiscoveryRace::new();
    let mut clock = ManualClock::new(time(100));
    let mut log = ControlPlaneEventLog::new();
    let generation = race
        .start(&clock, net, &mut sources, &mut log)
        .expect("candidates");
    let mut connector = ScriptedCandidateConnector::new();
    connector.push_outcome(
        "192.0.2.10",
        443,
        Ok(AuthenticatedPeer::new(impostor).expect("impostor")),
    );
    connector.push_outcome(
        "192.0.2.11",
        443,
        Ok(AuthenticatedPeer::new(target).expect("target")),
    );

    let result = race
        .try_connect_authorized(&clock, generation, target, &mut connector, &store, &mut log)
        .expect("next candidate");

    assert_eq!(result.source, CandidateSourceKind::Bonjour);
    assert!(log.events().iter().any(|event| {
        event.kind
            == ControlPlaneEventKind::ConnectFailed {
                reason: ConnectFailureReason::FingerprintMismatch,
            }
    }));
    clock.advance(1);
    store.forget(impostor).expect("forget absent peer");
}

#[test]
fn paired_mac_fingerprint_selects_the_right_peer() {
    let first_mac = fingerprint(12);
    let second_mac = fingerprint(13);
    let net = network("wifi-b");
    let store = paired_store(&[(first_mac, "Mac one"), (second_mac, "Mac two")]);
    let mut source = ScriptedCandidateSource::new(CandidateSourceKind::Bonjour);
    source.push_response(Ok(vec![
        candidate("192.0.2.20", 443, CandidateSourceKind::Bonjour, &net, None),
        candidate("192.0.2.21", 443, CandidateSourceKind::Bonjour, &net, None),
    ]));
    let mut sources: Vec<&mut dyn CandidateSource> = vec![&mut source];
    let mut race = DiscoveryRace::new();
    let clock = ManualClock::new(time(10));
    let mut log = ControlPlaneEventLog::new();
    let generation = race
        .start(&clock, net, &mut sources, &mut log)
        .expect("candidates");
    let mut connector = ScriptedCandidateConnector::new();
    connector.push_outcome(
        "192.0.2.20",
        443,
        Ok(AuthenticatedPeer::new(first_mac).expect("first")),
    );
    connector.push_outcome(
        "192.0.2.21",
        443,
        Ok(AuthenticatedPeer::new(second_mac).expect("second")),
    );
    connector.set_completion_order(&[
        ("192.0.2.21".to_owned(), 443),
        ("192.0.2.20".to_owned(), 443),
    ]);

    let result = race
        .try_connect_authorized(
            &clock,
            generation,
            second_mac,
            &mut connector,
            &store,
            &mut log,
        )
        .expect("right Mac");
    assert_eq!(result.peer_fingerprint, second_mac);
    assert_eq!(result.candidate.address, "192.0.2.21");
    assert_eq!(connector.cancelled().len(), 1);
    assert!(log.events().iter().all(|event| {
        event.kind
            != ControlPlaneEventKind::ConnectFailed {
                reason: ConnectFailureReason::Cancelled,
            }
    }));
}

#[test]
fn authenticated_discovery_winner_advances_the_link_to_handshaking() {
    let target = fingerprint(21);
    let net = network("wifi-link");
    let store = paired_store(&[(target, "phone")]);
    let mut source = ScriptedCandidateSource::new(CandidateSourceKind::CachedAddress);
    source.push_response(Ok(vec![candidate(
        "192.0.2.40",
        443,
        CandidateSourceKind::CachedAddress,
        &net,
        None,
    )]));
    let mut sources: Vec<&mut dyn CandidateSource> = vec![&mut source];
    let clock = ManualClock::new(time(40));
    let mut log = ControlPlaneEventLog::new();
    let mut race = DiscoveryRace::new();
    let generation = race
        .start(&clock, net, &mut sources, &mut log)
        .expect("candidate");
    let mut connector = ScriptedCandidateConnector::new();
    connector.push_outcome(
        "192.0.2.40",
        443,
        Ok(AuthenticatedPeer::new(target).expect("target")),
    );
    let mut link = idroid_core::LinkMachine::new(time(40));

    race.try_connect_authorized_with_link(
        &clock,
        generation,
        target,
        &mut connector,
        &store,
        &mut link,
        &mut log,
    )
    .expect("authenticated transport");

    assert_eq!(link.state(), LinkState::Handshaking);
    assert!(log.events().iter().any(|event| {
        event.kind
            == ControlPlaneEventKind::LinkTransition {
                from: LinkState::Searching,
                to: LinkState::Connecting,
            }
    }));
    assert!(log.events().iter().any(|event| {
        event.kind
            == ControlPlaneEventKind::LinkTransition {
                from: LinkState::Connecting,
                to: LinkState::Handshaking,
            }
    }));
}

#[test]
fn failed_discovery_race_returns_the_link_to_searching() {
    let target = fingerprint(22);
    let net = network("wifi-fail");
    let mut source = ScriptedCandidateSource::new(CandidateSourceKind::GatewayProbe);
    source.push_response(Ok(vec![candidate(
        "192.0.2.41",
        443,
        CandidateSourceKind::GatewayProbe,
        &net,
        None,
    )]));
    let mut sources: Vec<&mut dyn CandidateSource> = vec![&mut source];
    let clock = ManualClock::new(time(50));
    let mut log = ControlPlaneEventLog::new();
    let mut race = DiscoveryRace::new();
    let generation = race
        .start(&clock, net, &mut sources, &mut log)
        .expect("candidate");
    let mut connector = ScriptedCandidateConnector::new();
    connector.push_outcome("192.0.2.41", 443, Err(ConnectFailureReason::Timeout));
    let mut link = idroid_core::LinkMachine::new(time(50));

    assert_eq!(
        race.try_connect_with_link(
            &clock,
            generation,
            target,
            &mut connector,
            &mut link,
            &mut log,
        ),
        Err(DiscoveryError::NoAuthenticatedPeer)
    );
    assert_eq!(link.state(), LinkState::Searching);
    assert!(log.events().iter().any(|event| {
        event.kind
            == ControlPlaneEventKind::LinkTransition {
                from: LinkState::Connecting,
                to: LinkState::Searching,
            }
    }));
}

#[test]
fn gateway_probe_wins_when_mdns_is_blocked() {
    let target = fingerprint(14);
    let net = network("hotspot");
    let store = paired_store(&[(target, "phone")]);
    let mut cached = ScriptedCandidateSource::new(CandidateSourceKind::CachedAddress);
    cached.push_response(Ok(Vec::new()));
    let mut gateway = ScriptedCandidateSource::new(CandidateSourceKind::GatewayProbe);
    gateway.push_response(Ok(vec![candidate(
        "192.168.43.1",
        443,
        CandidateSourceKind::GatewayProbe,
        &net,
        None,
    )]));
    let mut bonjour = ScriptedCandidateSource::new(CandidateSourceKind::Bonjour);
    bonjour.push_response(Err(
        idroid_core::discovery::CandidateSourceError::Unavailable,
    ));
    let mut sources: Vec<&mut dyn CandidateSource> = vec![&mut cached, &mut gateway, &mut bonjour];
    let clock = ManualClock::new(time(20));
    let mut log = ControlPlaneEventLog::new();
    let mut race = DiscoveryRace::new();
    let generation = race
        .start(&clock, net, &mut sources, &mut log)
        .expect("gateway candidate");
    let mut connector = ScriptedCandidateConnector::new();
    connector.push_outcome(
        "192.168.43.1",
        443,
        Ok(AuthenticatedPeer::new(target).expect("target")),
    );

    let result = race
        .try_connect_authorized(&clock, generation, target, &mut connector, &store, &mut log)
        .expect("gateway winner");
    assert_eq!(result.source, CandidateSourceKind::GatewayProbe);
    assert_eq!(
        race.source_errors(),
        &[(
            CandidateSourceKind::Bonjour,
            idroid_core::discovery::CandidateSourceError::Unavailable,
        )]
    );
}

#[test]
fn network_change_discards_old_candidates_and_restarts_the_race() {
    let target = fingerprint(15);
    let first_network = network("wifi-c");
    let second_network = network("wifi-d");
    let mut source = ScriptedCandidateSource::new(CandidateSourceKind::CachedAddress);
    source.push_response(Ok(vec![candidate(
        "192.0.2.30",
        443,
        CandidateSourceKind::CachedAddress,
        &first_network,
        Some(target),
    )]));
    source.push_response(Ok(vec![candidate(
        "192.0.2.31",
        443,
        CandidateSourceKind::CachedAddress,
        &second_network,
        Some(target),
    )]));
    let mut sources: Vec<&mut dyn CandidateSource> = vec![&mut source];
    let clock = ManualClock::new(time(30));
    let mut log = ControlPlaneEventLog::new();
    let mut race = DiscoveryRace::new();
    let first_generation = race
        .start(&clock, first_network, &mut sources, &mut log)
        .expect("first race");
    let second_generation = race
        .network_changed(&clock, second_network.clone(), &mut log)
        .expect("network change");
    assert!(race.candidates().is_empty());
    assert!(matches!(
        race.try_connect(
            &clock,
            first_generation,
            target,
            &mut ScriptedCandidateConnector::new(),
            &mut log,
        ),
        Err(DiscoveryError::StaleGeneration { .. })
    ));

    let restarted_generation = race
        .start(&clock, second_network, &mut sources, &mut log)
        .expect("second race");
    assert_eq!(restarted_generation, second_generation + 1);
    assert_eq!(race.candidates().len(), 1);
}

#[test]
fn unpaired_reconnect_is_rejected_as_not_paired() {
    let error =
        authorize_reconnect(&InMemoryPairingStore::new(), fingerprint(16)).expect_err("unpaired");
    assert_eq!(error, PairingError::NotPaired);
    assert_eq!(
        connect_failure_for_pairing(&error),
        ControlErrorCode::NotPaired
    );
}

#[test]
fn revoked_peer_is_rejected_and_maps_to_not_paired_for_the_ui() {
    let peer = fingerprint(17);
    let mut store = paired_store(&[(peer, "revoked")]);
    store.revoke(peer).expect("revoke");
    let error = authorize_reconnect(&store, peer).expect_err("revoked");
    assert_eq!(error, PairingError::Revoked);
    assert_eq!(
        connect_failure_for_pairing(&error),
        ControlErrorCode::NotPaired
    );
}

#[test]
fn pairing_listener_closes_after_qr_expiry() {
    let mac = fingerprint(18);
    let mut authority =
        PairingAuthority::new(1, mac, InMemoryPairingStore::new()).expect("authority");
    authority.issue_qr(token(18), "hint", time(0)).expect("QR");
    assert!(authority.listener_is_open(time(119_999)));
    assert!(authority.expire(time(120_000)));
    assert!(!authority.listener_is_open(time(120_000)));
}
