//! Orthogonal device state used by platform adapters and user-facing policy.
//!
//! A connection can be healthy while capture is paused, and a phone can be
//! nearby while permission is missing. Keeping these dimensions separate avoids
//! collapsing useful recovery information into one large enum.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    Nearby,
    Pairing,
    Connecting,
    Connected,
    Reconnecting,
    Offline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureHealth {
    Stopped,
    Starting,
    Active,
    Paused,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputHealth {
    Unavailable,
    Ready,
    Active,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionStatus {
    Unknown,
    Required,
    Granted,
    Denied,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceHeadline {
    Nearby,
    Pairing,
    Connecting,
    Connected,
    SharingPaused,
    Reconnecting,
    Offline,
    PermissionRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceState {
    connection: ConnectionState,
    capture: CaptureHealth,
    input: InputHealth,
    permission: PermissionStatus,
}

impl DeviceState {
    pub const fn new(
        connection: ConnectionState,
        capture: CaptureHealth,
        input: InputHealth,
        permission: PermissionStatus,
    ) -> Self {
        Self {
            connection,
            capture,
            input,
            permission,
        }
    }

    pub const fn connected() -> Self {
        Self::new(
            ConnectionState::Connected,
            CaptureHealth::Stopped,
            InputHealth::Ready,
            PermissionStatus::Granted,
        )
    }

    pub const fn connection(self) -> ConnectionState {
        self.connection
    }

    pub const fn capture(self) -> CaptureHealth {
        self.capture
    }

    pub const fn input(self) -> InputHealth {
        self.input
    }

    pub const fn permission(self) -> PermissionStatus {
        self.permission
    }

    /// Derive the one short status label shown by a surface that cannot show
    /// all dimensions. Permission and capture conditions take precedence over
    /// connectivity so the user sees the actionable cause.
    pub const fn headline(self) -> DeviceHeadline {
        if matches!(
            self.permission,
            PermissionStatus::Required | PermissionStatus::Denied | PermissionStatus::Revoked
        ) {
            return DeviceHeadline::PermissionRequired;
        }
        if matches!(self.capture, CaptureHealth::Paused) {
            return DeviceHeadline::SharingPaused;
        }
        match self.connection {
            ConnectionState::Nearby => DeviceHeadline::Nearby,
            ConnectionState::Pairing => DeviceHeadline::Pairing,
            ConnectionState::Connecting => DeviceHeadline::Connecting,
            ConnectionState::Connected => DeviceHeadline::Connected,
            ConnectionState::Reconnecting => DeviceHeadline::Reconnecting,
            ConnectionState::Offline => DeviceHeadline::Offline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_is_not_hidden_by_connection_state() {
        let state = DeviceState::new(
            ConnectionState::Connected,
            CaptureHealth::Stopped,
            InputHealth::Ready,
            PermissionStatus::Required,
        );

        assert_eq!(state.connection(), ConnectionState::Connected);
        assert_eq!(state.headline(), DeviceHeadline::PermissionRequired);
    }

    #[test]
    fn paused_capture_is_distinct_from_offline_connection() {
        let state = DeviceState::new(
            ConnectionState::Connected,
            CaptureHealth::Paused,
            InputHealth::Ready,
            PermissionStatus::Granted,
        );

        assert_eq!(state.headline(), DeviceHeadline::SharingPaused);
        assert_ne!(state.connection(), ConnectionState::Offline);
    }
}
