use idroid_core::control_plane::*;
use idroid_core::{Clock, ManualClock, SessionId, Timestamp};

fn fingerprint(seed: u8) -> DeviceFingerprint {
    let mut bytes = [0_u8; KEY_FINGERPRINT_SIZE];
    bytes[0] = seed;
    DeviceFingerprint::from_array(bytes)
}

fn time(milliseconds: u64) -> Timestamp {
    Timestamp::from_millis(milliseconds)
}

fn ready_link(session_id: SessionId) -> LinkMachine {
    let mut link = LinkMachine::new(time(0));
    link.begin_connecting(time(0)).expect("connect");
    link.transport_connected(time(0)).expect("transport");
    link.handshake_ready(time(0), Some(session_id))
        .expect("ready");
    link
}

fn active_helper_session() -> MediaSessionMachine {
    let request = SessionRequest::new(1, Feature::Video, Some(Backend::Helper)).expect("request");
    let result = SessionResult::new(
        request.request_id,
        request.feature,
        SessionOutcome::Accepted {
            backend: Backend::Helper,
        },
    )
    .expect("result");
    let mut session = MediaSessionMachine::new();
    session.request(request).expect("start");
    session.apply_result(result).expect("active");
    session
}

fn capabilities_with_helper(helper_video_status: BackendCapability) -> Capabilities {
    Capabilities::new(
        FeatureCapabilities::new(
            Feature::Video,
            vec![
                helper_video_status,
                BackendCapability::needs_user_action(
                    Backend::MediaProjection,
                    CapabilityReason::MediaProjectionConsentRequired,
                ),
            ],
        )
        .expect("video capabilities"),
        FeatureCapabilities::new(
            Feature::Input,
            vec![
                BackendCapability::available(Backend::Helper),
                BackendCapability::needs_user_action(
                    Backend::Accessibility,
                    CapabilityReason::AccessibilityPermissionRequired,
                ),
            ],
        )
        .expect("input capabilities"),
    )
    .expect("capabilities")
}

#[test]
fn version_negotiation_picks_highest_common_version() {
    let device = fingerprint(1);
    let hello = Hello::new(
        VersionRange::new(1, 3).expect("range"),
        device,
        PeerRole::Android,
        "android-test",
        None,
    )
    .expect("hello");

    let acknowledgement =
        negotiate_versions(VersionRange::new(2, 4).expect("range"), &hello, device)
            .expect("overlapping version");

    assert_eq!(acknowledgement.version, 3);
}

#[test]
fn unsupported_version_and_identity_mismatch_fail_closed() {
    let device = fingerprint(2);
    let hello = Hello::new(
        VersionRange::new(1, 1).expect("range"),
        device,
        PeerRole::Android,
        "android-test",
        None,
    )
    .expect("hello");

    assert_eq!(
        negotiate_versions(VersionRange::new(2, 3).expect("range"), &hello, device)
            .expect_err("no overlap")
            .code,
        ControlErrorCode::VersionUnsupported
    );
    assert_eq!(
        negotiate_versions(
            VersionRange::new(1, 1).expect("range"),
            &hello,
            fingerprint(3),
        )
        .expect_err("identity mismatch")
        .code,
        ControlErrorCode::IdentityMismatch
    );
}

#[test]
fn helper_is_offered_first_and_capability_updates_are_live() {
    let capabilities = capabilities_with_helper(BackendCapability::available(Backend::Helper));
    assert_eq!(
        capabilities
            .select(Feature::Video, None)
            .expect("helper selection"),
        BackendDecision::Available(Backend::Helper)
    );

    let updated = capabilities_with_helper(BackendCapability::unavailable(
        Backend::Helper,
        CapabilityReason::HelperStopped,
    ));
    assert_eq!(
        ControlMessage::CapabilitiesUpdate(updated.clone()).kind(),
        ControlMessageKind::CapabilitiesUpdate
    );
    assert_eq!(
        updated
            .select(Feature::Video, None)
            .expect("projection fallback"),
        BackendDecision::NeedsUserAction {
            backend: Backend::MediaProjection,
            action: UserAction::GrantMediaProjection,
        }
    );
}

#[test]
fn helper_loss_enters_switching_until_projection_consent_completes() {
    let request = SessionRequest::new(7, Feature::Video, Some(Backend::Helper)).expect("request");
    let accepted = SessionResult::new(
        request.request_id,
        request.feature,
        SessionOutcome::Accepted {
            backend: Backend::Helper,
        },
    )
    .expect("accepted result");
    let change = BackendChanged::new(
        Feature::Video,
        Backend::Helper,
        Backend::MediaProjection,
        BackendChangeReason::HelperLost,
    )
    .expect("backend change");

    let mut session = MediaSessionMachine::new();
    session.request(request).expect("start");
    session.apply_result(accepted).expect("active");
    session
        .begin_switch(change, Some(UserAction::GrantMediaProjection))
        .expect("switching");
    assert_eq!(
        session.state(),
        &MediaSessionState::Switching {
            request_id: 7,
            feature: Feature::Video,
            from: Some(Backend::Helper),
            to: Backend::MediaProjection,
            awaiting_user_action: Some(UserAction::GrantMediaProjection),
        }
    );

    session
        .user_action_completed(UserAction::GrantMediaProjection)
        .expect("consent");
    session
        .complete_switch(Backend::MediaProjection)
        .expect("projection active");
    assert_eq!(
        session.state(),
        &MediaSessionState::Active {
            request_id: 7,
            feature: Feature::Video,
            backend: Backend::MediaProjection,
        }
    );
}

#[test]
fn reconnect_uses_mac_clock_and_preserves_session_id() {
    let session_id = SessionId::new(41);
    let mut link = LinkMachine::new(time(0));
    link.begin_connecting(time(0)).expect("connect");
    link.transport_connected(time(1)).expect("transport");
    link.handshake_ready(time(2), Some(session_id))
        .expect("ready");

    let transitions = link.network_changed(time(10)).expect("network change");
    assert_eq!(
        transitions,
        vec![
            LinkTransition {
                from: LinkState::Ready,
                to: LinkState::Degraded,
                at: time(10),
            },
            LinkTransition {
                from: LinkState::Degraded,
                to: LinkState::Searching,
                at: time(10),
            },
        ]
    );
    link.begin_connecting(time(11)).expect("rediscover");
    link.transport_connected(time(12)).expect("reconnect");
    link.handshake_ready(time(14), Some(session_id))
        .expect("resume");

    assert_eq!(link.state(), LinkState::Ready);
    assert_eq!(link.session_id(), Some(session_id));
    assert_eq!(link.last_reconnect_time_ms(), Some(4));
}

#[test]
fn reconnect_time_includes_the_degraded_interval() {
    let session_id = SessionId::new(42);
    let mut link = ready_link(session_id);

    link.tick(time(DEGRADE_AFTER_MS)).expect("degrade");
    assert_eq!(link.state(), LinkState::Degraded);
    link.network_changed(time(8_000)).expect("network change");
    link.begin_connecting(time(8_001)).expect("rediscover");
    link.transport_connected(time(8_002)).expect("reconnect");
    link.handshake_ready(time(8_004), Some(session_id))
        .expect("resume");

    assert_eq!(link.last_reconnect_time_ms(), Some(5_004));
}

#[test]
fn network_change_from_ready_enters_searching_without_waiting_for_timeout() {
    let mut link = ready_link(SessionId::new(51));

    let transitions = link.network_changed(time(100)).expect("network change");

    assert_eq!(link.state(), LinkState::Searching);
    assert_eq!(transitions.len(), 2);
    assert_eq!(transitions[0].from, LinkState::Ready);
    assert_eq!(transitions[0].to, LinkState::Degraded);
    assert_eq!(transitions[1].from, LinkState::Degraded);
    assert_eq!(transitions[1].to, LinkState::Searching);
}

#[test]
fn link_and_backend_failures_are_independent() {
    let healthy_link = ready_link(SessionId::new(61));
    let mut helper_session = active_helper_session();
    let change = BackendChanged::new(
        Feature::Video,
        Backend::Helper,
        Backend::MediaProjection,
        BackendChangeReason::HelperLost,
    )
    .expect("backend change");

    helper_session
        .begin_switch(change, Some(UserAction::GrantMediaProjection))
        .expect("helper loss");
    assert_eq!(healthy_link.state(), LinkState::Ready);
    assert_eq!(helper_session.state().stage(), MediaSessionStage::Switching);

    let mut reconnecting_link = ready_link(SessionId::new(62));
    let still_active_session = active_helper_session();
    reconnecting_link
        .network_changed(time(100))
        .expect("network loss");
    assert_eq!(reconnecting_link.state(), LinkState::Searching);
    assert_eq!(
        still_active_session.state(),
        &MediaSessionState::Active {
            request_id: 1,
            feature: Feature::Video,
            backend: Backend::Helper,
        }
    );
}

#[test]
fn capability_update_is_live_without_restarting_a_ready_link() {
    let link = ready_link(SessionId::new(71));
    let capabilities = capabilities_with_helper(BackendCapability::unavailable(
        Backend::Helper,
        CapabilityReason::HelperStopped,
    ));
    let message = ControlMessage::CapabilitiesUpdate(capabilities);
    let clock = ManualClock::new(time(100));
    let mut log = ControlPlaneEventLog::new();

    log.record_message(&clock, &message)
        .expect("capability update");

    assert_eq!(link.state(), LinkState::Ready);
    assert_eq!(log.events()[0].at, Clock::now(&clock));
    assert_eq!(
        log.events()[0].kind,
        ControlPlaneEventKind::Message {
            kind: ControlMessageKind::CapabilitiesUpdate,
        }
    );
    assert_eq!(link.state(), LinkState::Ready);
}

#[test]
fn event_log_uses_the_injected_host_clock() {
    let mut clock = ManualClock::new(time(10));
    let mut link = LinkMachine::new_with_clock(&clock);
    let mut log = ControlPlaneEventLog::new();

    let transitions = link.begin_connecting_with_clock(&clock).expect("connect");
    log.record_link_transitions(&clock, &transitions);
    clock.advance(5);
    let transitions = link
        .transport_connected_with_clock(&clock)
        .expect("transport");
    log.record_link_transitions(&clock, &transitions);
    clock.advance(5);
    let transitions = link
        .handshake_ready_with_clock(&clock, Some(SessionId::new(81)))
        .expect("handshake");
    log.record_link_transitions(&clock, &transitions);

    assert_eq!(
        log.events()
            .iter()
            .map(|event| event.at)
            .collect::<Vec<_>>(),
        vec![time(10), time(15), time(20)]
    );
    assert_eq!(
        log.to_ndjson(),
        concat!(
            "{\"at_ms\":10,\"event\":\"link_transition\",\"from\":\"searching\",\"to\":\"connecting\"}\n",
            "{\"at_ms\":15,\"event\":\"link_transition\",\"from\":\"connecting\",\"to\":\"handshaking\"}\n",
            "{\"at_ms\":20,\"event\":\"link_transition\",\"from\":\"handshaking\",\"to\":\"ready\"}\n",
        )
    );
}

#[test]
fn missed_heartbeats_degrade_then_mark_link_lost() {
    let mut link = LinkMachine::new(time(0));
    link.begin_connecting(time(0)).expect("connect");
    link.transport_connected(time(0)).expect("transport");
    link.handshake_ready(time(0), Some(SessionId::new(1)))
        .expect("ready");

    assert_eq!(
        link.tick(time(DEGRADE_AFTER_MS))
            .expect("degrade")
            .as_slice(),
        &[LinkTransition {
            from: LinkState::Ready,
            to: LinkState::Degraded,
            at: time(DEGRADE_AFTER_MS),
        }]
    );
    assert_eq!(link.state(), LinkState::Degraded);
    link.tick(time(LOSE_AFTER_MS)).expect("lose");
    assert_eq!(link.state(), LinkState::Lost);
}

#[test]
fn all_control_messages_validate_and_have_stable_kinds() {
    let device = fingerprint(9);
    let hello = Hello::new(
        VersionRange::new(1, 1).expect("range"),
        device,
        PeerRole::Mac,
        "mac-test",
        Some(SessionId::new(3)),
    )
    .expect("hello");
    let capabilities = capabilities_with_helper(BackendCapability::available(Backend::Helper));
    let request = SessionRequest::new(1, Feature::Video, None).expect("request");
    let result = SessionResult::new(
        1,
        Feature::Video,
        SessionOutcome::NeedsUserAction {
            action: UserAction::GrantMediaProjection,
        },
    )
    .expect("result");
    let heartbeat = Heartbeat::new(1, LinkState::Ready, None).expect("heartbeat");
    let changed = BackendChanged::new(
        Feature::Video,
        Backend::Helper,
        Backend::MediaProjection,
        BackendChangeReason::HelperLost,
    )
    .expect("change");
    let messages = vec![
        ControlMessage::Hello(hello),
        ControlMessage::HelloAck(HelloAck::new(1).expect("ack")),
        ControlMessage::Capabilities(capabilities.clone()),
        ControlMessage::CapabilitiesUpdate(capabilities),
        ControlMessage::SessionRequest(request),
        ControlMessage::SessionResult(result),
        ControlMessage::Heartbeat(heartbeat),
        ControlMessage::BackendChanged(changed),
        ControlMessage::Error(ControlError::new(
            ControlErrorCode::PermissionDenied,
            true,
            Some(UserAction::GrantMediaProjection),
        )),
        ControlMessage::Bye(Bye::normal()),
    ];

    for (index, message) in messages.iter().enumerate() {
        message.validate().expect("valid control message");
        assert_eq!(message.kind().wire_value(), (index + 1) as u8);
    }
}
