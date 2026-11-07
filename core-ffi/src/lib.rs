//! Small C ABI for the real platform adapters.
//!
//! The ABI deliberately exposes only the link state machine. Discovery,
//! certificate verification, sockets, and UI stay in platform code; both
//! adapters can nevertheless drive the exact `idroid-core` implementation.

#![allow(unsafe_code)]

use idroid_core::{LinkMachine, LinkState, SessionId, Timestamp};

#[repr(C)]
pub struct IdroidLink {
    machine: LinkMachine,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdroidLinkState {
    Searching = 0,
    Connecting = 1,
    Handshaking = 2,
    Ready = 3,
    Degraded = 4,
    Lost = 5,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdroidLinkResult {
    Ok = 0,
    InvalidHandle = 1,
    InvalidTransition = 2,
    InvalidSession = 3,
    SessionChanged = 4,
    ClockRewound = 5,
}

#[no_mangle]
pub extern "C" fn idroid_core_ffi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn idroid_link_new(now_ms: u64) -> *mut IdroidLink {
    Box::into_raw(Box::new(IdroidLink {
        machine: LinkMachine::new(Timestamp::from_millis(now_ms)),
    }))
}

/// # Safety
/// `link` must be a pointer returned by [`idroid_link_new`] and must not have
/// been freed already.
#[no_mangle]
pub unsafe extern "C" fn idroid_link_free(link: *mut IdroidLink) {
    if !link.is_null() {
        drop(Box::from_raw(link));
    }
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`].
#[no_mangle]
pub unsafe extern "C" fn idroid_link_state(link: *const IdroidLink) -> IdroidLinkState {
    if link.is_null() {
        return IdroidLinkState::Searching;
    }
    state((*link).machine.state())
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`].
#[no_mangle]
pub unsafe extern "C" fn idroid_link_begin_connecting(
    link: *mut IdroidLink,
    now_ms: u64,
) -> IdroidLinkResult {
    let Some(machine) = machine_mut(link) else {
        return IdroidLinkResult::InvalidHandle;
    };
    map_result(machine.begin_connecting(timestamp(now_ms)))
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`].
#[no_mangle]
pub unsafe extern "C" fn idroid_link_transport_connected(
    link: *mut IdroidLink,
    now_ms: u64,
) -> IdroidLinkResult {
    let Some(machine) = machine_mut(link) else {
        return IdroidLinkResult::InvalidHandle;
    };
    map_result(machine.transport_connected(timestamp(now_ms)))
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`].
#[no_mangle]
pub unsafe extern "C" fn idroid_link_connection_failed(
    link: *mut IdroidLink,
    now_ms: u64,
) -> IdroidLinkResult {
    let Some(machine) = machine_mut(link) else {
        return IdroidLinkResult::InvalidHandle;
    };
    map_result(machine.connection_failed(timestamp(now_ms)))
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`].
#[no_mangle]
pub unsafe extern "C" fn idroid_link_network_changed(
    link: *mut IdroidLink,
    now_ms: u64,
) -> IdroidLinkResult {
    let Some(machine) = machine_mut(link) else {
        return IdroidLinkResult::InvalidHandle;
    };
    map_result(machine.network_changed(timestamp(now_ms)))
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`].
#[no_mangle]
pub unsafe extern "C" fn idroid_link_handshake_ready(
    link: *mut IdroidLink,
    now_ms: u64,
    has_session: bool,
    session_id: u64,
) -> IdroidLinkResult {
    let Some(machine) = machine_mut(link) else {
        return IdroidLinkResult::InvalidHandle;
    };
    let session = if has_session {
        Some(SessionId::new(session_id))
    } else {
        None
    };
    map_result(machine.handshake_ready(timestamp(now_ms), session))
}

/// # Safety
/// `link` must be a valid live handle returned by [`idroid_link_new`], and
/// `value` must point to writable storage for one `u64`.
#[no_mangle]
pub unsafe extern "C" fn idroid_link_reconnect_time_ms(
    link: *const IdroidLink,
    now_ms: u64,
    value: *mut u64,
) -> IdroidLinkResult {
    if link.is_null() || value.is_null() {
        return IdroidLinkResult::InvalidHandle;
    }
    match (*link).machine.reconnect_time_ms(timestamp(now_ms)) {
        Ok(Some(reconnect_ms)) => {
            *value = reconnect_ms;
            IdroidLinkResult::Ok
        }
        Ok(None) => {
            *value = 0;
            IdroidLinkResult::Ok
        }
        Err(error) => map_error(error),
    }
}

fn timestamp(now_ms: u64) -> Timestamp {
    Timestamp::from_millis(now_ms)
}

unsafe fn machine_mut<'a>(link: *mut IdroidLink) -> Option<&'a mut LinkMachine> {
    link.as_mut().map(|value| &mut value.machine)
}

fn state(value: LinkState) -> IdroidLinkState {
    match value {
        LinkState::Searching => IdroidLinkState::Searching,
        LinkState::Connecting => IdroidLinkState::Connecting,
        LinkState::Handshaking => IdroidLinkState::Handshaking,
        LinkState::Ready => IdroidLinkState::Ready,
        LinkState::Degraded => IdroidLinkState::Degraded,
        LinkState::Lost => IdroidLinkState::Lost,
    }
}

fn map_result<T>(result: Result<T, idroid_core::LinkError>) -> IdroidLinkResult {
    match result {
        Ok(_) => IdroidLinkResult::Ok,
        Err(error) => map_error(error),
    }
}

fn map_error(error: idroid_core::LinkError) -> IdroidLinkResult {
    match error {
        idroid_core::LinkError::ClockRewound { .. } => IdroidLinkResult::ClockRewound,
        idroid_core::LinkError::InvalidTransition { .. } => IdroidLinkResult::InvalidTransition,
        idroid_core::LinkError::InvalidSessionId => IdroidLinkResult::InvalidSession,
        idroid_core::LinkError::SessionChanged { .. } => IdroidLinkResult::SessionChanged,
        idroid_core::LinkError::InvalidHeartbeat
        | idroid_core::LinkError::HeartbeatSequenceRewound { .. } => {
            IdroidLinkResult::InvalidTransition
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_abi_drives_the_same_link_state_machine() {
        unsafe {
            let link = idroid_link_new(0);
            assert_eq!(idroid_link_state(link), IdroidLinkState::Searching);
            assert_eq!(idroid_link_begin_connecting(link, 1), IdroidLinkResult::Ok);
            assert_eq!(
                idroid_link_transport_connected(link, 2),
                IdroidLinkResult::Ok
            );
            assert_eq!(idroid_link_state(link), IdroidLinkState::Handshaking);
            assert_eq!(
                idroid_link_handshake_ready(link, 3, true, 7),
                IdroidLinkResult::Ok
            );
            assert_eq!(idroid_link_state(link), IdroidLinkState::Ready);
            idroid_link_free(link);
        }
    }
}
