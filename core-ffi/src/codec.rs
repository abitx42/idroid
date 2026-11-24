//! Binary codec entry points for the platform adapters.
//!
//! The Android and macOS slices exchange complete frames (magic, version,
//! kind, little-endian length, payload) produced by `idroid-core`. These
//! functions are the only supported way to build or parse a frame from Swift
//! or Kotlin, so neither adapter can drift from the Rust implementation.
//!
//! Strings and byte slices are caller-owned. Encoding writes into a caller
//! buffer: pass `out = NULL` and `capacity = 0` to learn the required size,
//! which is always reported through `out_len`.

use idroid_core::{
    Bye, ControlErrorCode, ControlMessage, DeviceFingerprint, Hello, HelloAck, MessageBody,
    PeerRole, ProtocolMessage, SessionId, UserAction, VersionRange, MAX_APP_VERSION_BYTES,
};

/// The frame was written or parsed.
pub const IDROID_CODEC_OK: i32 = 0;
/// The bytes are not a valid frame, or the frame is internally inconsistent.
pub const IDROID_CODEC_INVALID_FRAME: i32 = -1;
/// `out` is too small; `out_len` holds the required size.
pub const IDROID_CODEC_BUFFER_TOO_SMALL: i32 = -2;
/// A pointer was null or an enum value was unknown.
pub const IDROID_CODEC_INVALID_ARGUMENT: i32 = -3;
/// The frame is valid but holds a different message than the caller asked for.
pub const IDROID_CODEC_WRONG_KIND: i32 = -4;

/// Decoded `hello` fields. `app_version` is a fixed buffer with an explicit
/// length so the struct has no pointers and can cross the ABI by value.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdroidHelloView {
    pub version_min: u16,
    pub version_max: u16,
    pub fingerprint: [u8; 32],
    pub role: u8,
    pub app_version_len: u16,
    pub app_version: [u8; MAX_APP_VERSION_BYTES],
    pub has_last_session: u8,
    pub last_session_id: u64,
}

/// Decoded `bye` fields. `code` and `user_action` are optional.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdroidByeView {
    pub has_code: u8,
    pub code: u8,
    pub retryable: u8,
    pub has_user_action: u8,
    pub user_action: u8,
}

/// # Safety
/// `fingerprint` must point to 32 readable bytes and `app_version` to
/// `app_version_len` readable bytes when that length is non-zero.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_encode_hello(
    version_min: u16,
    version_max: u16,
    fingerprint: *const u8,
    role: u8,
    app_version: *const u8,
    app_version_len: usize,
    has_last_session: bool,
    last_session_id: u64,
    out: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> i32 {
    let Some(fingerprint) = bytes(fingerprint, 32) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };
    let Some(app_version) = bytes(app_version, app_version_len) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };
    let Ok(app_version) = std::str::from_utf8(app_version) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };
    let Some(role) = PeerRole::from_wire(role) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };
    let Ok(fingerprint) = DeviceFingerprint::from_bytes(fingerprint) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };

    let Ok(hello) = Hello::new(
        VersionRange {
            min: version_min,
            max: version_max,
        },
        fingerprint,
        role,
        app_version,
        has_last_session.then(|| SessionId::new(last_session_id)),
    ) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };
    match ProtocolMessage::new(ControlMessage::Hello(hello)) {
        Ok(message) => encode_into(&message, out, out_capacity, out_len),
        Err(_) => IDROID_CODEC_INVALID_ARGUMENT,
    }
}

/// # Safety
/// `out` must point to a writable [`IdroidHelloView`].
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_decode_hello(
    frame: *const u8,
    frame_len: usize,
    out: *mut IdroidHelloView,
) -> i32 {
    if out.is_null() {
        return IDROID_CODEC_INVALID_ARGUMENT;
    }
    let message = match decode_frame(frame, frame_len) {
        Ok(message) => message,
        Err(code) => return code,
    };
    let MessageBody::Control(ControlMessage::Hello(hello)) = message.body() else {
        return IDROID_CODEC_WRONG_KIND;
    };

    let app_version_len = u16::try_from(hello.app_version.len()).unwrap_or(0);
    let mut view = IdroidHelloView {
        version_min: hello.version_min,
        version_max: hello.version_max,
        fingerprint: hello.device_id.as_bytes(),
        role: hello.role.wire_value(),
        app_version_len,
        app_version: [0; MAX_APP_VERSION_BYTES],
        has_last_session: u8::from(hello.last_session_id.is_some()),
        last_session_id: hello.last_session_id.map_or(0, SessionId::get),
    };
    view.app_version[..hello.app_version.len()].copy_from_slice(hello.app_version.as_bytes());
    out.write(view);
    IDROID_CODEC_OK
}

/// # Safety
/// `out` must be null or point to writable storage for `out_capacity` bytes,
/// and `out_len` to writable storage for one `usize`.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_encode_hello_ack(
    version: u16,
    out: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> i32 {
    let Ok(ack) = HelloAck::new(version) else {
        return IDROID_CODEC_INVALID_ARGUMENT;
    };
    match ProtocolMessage::new(ControlMessage::HelloAck(ack)) {
        Ok(message) => encode_into(&message, out, out_capacity, out_len),
        Err(_) => IDROID_CODEC_INVALID_ARGUMENT,
    }
}

/// # Safety
/// `version` must point to writable storage for one `u16`.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_decode_hello_ack(
    frame: *const u8,
    frame_len: usize,
    version: *mut u16,
) -> i32 {
    if version.is_null() {
        return IDROID_CODEC_INVALID_ARGUMENT;
    }
    let message = match decode_frame(frame, frame_len) {
        Ok(message) => message,
        Err(code) => return code,
    };
    let MessageBody::Control(ControlMessage::HelloAck(ack)) = message.body() else {
        return IDROID_CODEC_WRONG_KIND;
    };
    version.write(ack.version);
    IDROID_CODEC_OK
}

/// # Safety
/// `out` must be null or point to writable storage for `out_capacity` bytes,
/// and `out_len` to writable storage for one `usize`.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_encode_bye(
    has_code: bool,
    code: u8,
    retryable: bool,
    has_user_action: bool,
    user_action: u8,
    out: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> i32 {
    let code = if has_code {
        match ControlErrorCode::from_wire(code) {
            Some(code) => Some(code),
            None => return IDROID_CODEC_INVALID_ARGUMENT,
        }
    } else {
        None
    };
    let user_action = if has_user_action {
        match UserAction::from_wire(user_action) {
            Some(action) => Some(action),
            None => return IDROID_CODEC_INVALID_ARGUMENT,
        }
    } else {
        None
    };
    let bye = Bye {
        code,
        retryable,
        user_action,
    };
    match ProtocolMessage::new(ControlMessage::Bye(bye)) {
        Ok(message) => encode_into(&message, out, out_capacity, out_len),
        Err(_) => IDROID_CODEC_INVALID_ARGUMENT,
    }
}

/// # Safety
/// `out` must point to a writable [`IdroidByeView`].
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_decode_bye(
    frame: *const u8,
    frame_len: usize,
    out: *mut IdroidByeView,
) -> i32 {
    if out.is_null() {
        return IDROID_CODEC_INVALID_ARGUMENT;
    }
    let message = match decode_frame(frame, frame_len) {
        Ok(message) => message,
        Err(code) => return code,
    };
    let MessageBody::Control(ControlMessage::Bye(bye)) = message.body() else {
        return IDROID_CODEC_WRONG_KIND;
    };
    out.write(IdroidByeView {
        has_code: u8::from(bye.code.is_some()),
        code: bye.code.map_or(0, ControlErrorCode::wire_value),
        retryable: u8::from(bye.retryable),
        has_user_action: u8::from(bye.user_action.is_some()),
        user_action: bye.user_action.map_or(0, UserAction::wire_value),
    });
    IDROID_CODEC_OK
}

/// The wire kind of a frame, or a negative result code.
///
/// # Safety
/// `frame` must point to `frame_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_frame_kind(frame: *const u8, frame_len: usize) -> i32 {
    match decode_frame(frame, frame_len) {
        Ok(message) => i32::from(message.kind().wire_value()),
        Err(code) => code,
    }
}

/// The protocol version of a frame, or a negative result code.
///
/// # Safety
/// `frame` must point to `frame_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_frame_version(frame: *const u8, frame_len: usize) -> i32 {
    match decode_frame(frame, frame_len) {
        Ok(message) => i32::from(message.version().wire_value()),
        Err(code) => code,
    }
}

/// `IDROID_CODEC_OK` when the frame decodes and validates.
///
/// # Safety
/// `frame` must point to `frame_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn idroid_codec_validate(frame: *const u8, frame_len: usize) -> i32 {
    match decode_frame(frame, frame_len) {
        Ok(_) => IDROID_CODEC_OK,
        Err(code) => code,
    }
}

unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    if ptr.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(ptr, len))
    }
}

unsafe fn decode_frame(frame: *const u8, frame_len: usize) -> Result<ProtocolMessage, i32> {
    let Some(frame) = bytes(frame, frame_len) else {
        return Err(IDROID_CODEC_INVALID_ARGUMENT);
    };
    ProtocolMessage::decode(frame).map_err(|_| IDROID_CODEC_INVALID_FRAME)
}

unsafe fn encode_into(
    message: &ProtocolMessage,
    out: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> i32 {
    if out_len.is_null() {
        return IDROID_CODEC_INVALID_ARGUMENT;
    }
    let frame = match message.encode() {
        Ok(frame) => frame,
        Err(_) => return IDROID_CODEC_INVALID_FRAME,
    };
    out_len.write(frame.len());
    if out.is_null() || out_capacity < frame.len() {
        return IDROID_CODEC_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(frame.as_ptr(), out, frame.len());
    IDROID_CODEC_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint() -> [u8; 32] {
        let mut value = [0u8; 32];
        for (index, byte) in value.iter_mut().enumerate() {
            *byte = index as u8;
        }
        value
    }

    fn encode_hello_into(buffer: &mut [u8]) -> usize {
        let mut len = 0usize;
        let result = unsafe {
            idroid_codec_encode_hello(
                1,
                2,
                fingerprint().as_ptr(),
                PeerRole::Android.wire_value(),
                b"0.1.0".as_ptr(),
                5,
                true,
                42,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut len,
            )
        };
        assert_eq!(result, IDROID_CODEC_OK);
        len
    }

    #[test]
    fn hello_round_trips_through_the_c_abi() {
        let mut buffer = [0u8; 256];
        let len = encode_hello_into(&mut buffer);

        let mut view = IdroidHelloView {
            version_min: 0,
            version_max: 0,
            fingerprint: [0; 32],
            role: 0,
            app_version_len: 0,
            app_version: [0; MAX_APP_VERSION_BYTES],
            has_last_session: 0,
            last_session_id: 0,
        };
        let result = unsafe { idroid_codec_decode_hello(buffer.as_ptr(), len, &mut view) };
        assert_eq!(result, IDROID_CODEC_OK);
        assert_eq!(view.version_min, 1);
        assert_eq!(view.version_max, 2);
        assert_eq!(view.fingerprint, fingerprint());
        assert_eq!(view.role, PeerRole::Android.wire_value());
        assert_eq!(view.app_version_len, 5);
        assert_eq!(&view.app_version[..5], b"0.1.0");
        assert_eq!(view.has_last_session, 1);
        assert_eq!(view.last_session_id, 42);
    }

    #[test]
    fn buffer_too_small_reports_the_required_length() {
        let mut required = 0usize;
        let result = unsafe {
            idroid_codec_encode_hello(
                1,
                2,
                fingerprint().as_ptr(),
                PeerRole::Mac.wire_value(),
                b"0.1.0".as_ptr(),
                5,
                false,
                0,
                std::ptr::null_mut(),
                0,
                &mut required,
            )
        };
        assert_eq!(result, IDROID_CODEC_BUFFER_TOO_SMALL);
        assert!(required > 10);

        let mut small = [0u8; 4];
        let mut written = 0usize;
        let result = unsafe {
            idroid_codec_encode_hello(
                1,
                2,
                fingerprint().as_ptr(),
                PeerRole::Mac.wire_value(),
                b"0.1.0".as_ptr(),
                5,
                false,
                0,
                small.as_mut_ptr(),
                small.len(),
                &mut written,
            )
        };
        assert_eq!(result, IDROID_CODEC_BUFFER_TOO_SMALL);
        assert_eq!(written, required);
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        let mut len = 0usize;
        let result = unsafe {
            idroid_codec_encode_hello(
                1,
                2,
                std::ptr::null(),
                PeerRole::Mac.wire_value(),
                b"0.1.0".as_ptr(),
                5,
                false,
                0,
                std::ptr::null_mut(),
                0,
                &mut len,
            )
        };
        assert_eq!(result, IDROID_CODEC_INVALID_ARGUMENT);

        let result = unsafe {
            idroid_codec_encode_hello(
                1,
                2,
                fingerprint().as_ptr(),
                9,
                b"0.1.0".as_ptr(),
                5,
                false,
                0,
                std::ptr::null_mut(),
                0,
                &mut len,
            )
        };
        assert_eq!(result, IDROID_CODEC_INVALID_ARGUMENT);
    }

    #[test]
    fn truncated_frames_are_invalid() {
        let mut buffer = [0u8; 256];
        let len = encode_hello_into(&mut buffer);
        let mut view = unsafe { std::mem::zeroed::<IdroidHelloView>() };
        let result = unsafe { idroid_codec_decode_hello(buffer.as_ptr(), len - 1, &mut view) };
        assert_eq!(result, IDROID_CODEC_INVALID_FRAME);
        assert_eq!(
            unsafe { idroid_codec_validate(buffer.as_ptr(), len - 1) },
            IDROID_CODEC_INVALID_FRAME
        );
    }

    #[test]
    fn frame_inspection_reports_kind_and_version() {
        let mut buffer = [0u8; 256];
        let len = encode_hello_into(&mut buffer);
        assert_eq!(
            unsafe { idroid_codec_frame_kind(buffer.as_ptr(), len) },
            i32::from(idroid_core::MessageKind::ControlHello.wire_value())
        );
        assert_eq!(
            unsafe { idroid_codec_frame_version(buffer.as_ptr(), len) },
            2
        );
    }

    #[test]
    fn hello_ack_and_bye_round_trip() {
        let mut buffer = [0u8; 64];
        let mut len = 0usize;
        assert_eq!(
            unsafe {
                idroid_codec_encode_hello_ack(2, buffer.as_mut_ptr(), buffer.len(), &mut len)
            },
            IDROID_CODEC_OK
        );
        let mut version = 0u16;
        assert_eq!(
            unsafe { idroid_codec_decode_hello_ack(buffer.as_ptr(), len, &mut version) },
            IDROID_CODEC_OK
        );
        assert_eq!(version, 2);

        let mut bye_buffer = [0u8; 64];
        let mut bye_len = 0usize;
        assert_eq!(
            unsafe {
                idroid_codec_encode_bye(
                    true,
                    ControlErrorCode::Timeout.wire_value(),
                    true,
                    false,
                    0,
                    bye_buffer.as_mut_ptr(),
                    bye_buffer.len(),
                    &mut bye_len,
                )
            },
            IDROID_CODEC_OK
        );
        let mut view = IdroidByeView {
            has_code: 0,
            code: 0,
            retryable: 0,
            has_user_action: 0,
            user_action: 0,
        };
        assert_eq!(
            unsafe { idroid_codec_decode_bye(bye_buffer.as_ptr(), bye_len, &mut view) },
            IDROID_CODEC_OK
        );
        assert_eq!(view.has_code, 1);
        assert_eq!(view.code, ControlErrorCode::Timeout.wire_value());
        assert_eq!(view.retryable, 1);
        assert_eq!(view.has_user_action, 0);
    }

    #[test]
    fn wrong_kind_is_reported_separately() {
        let mut buffer = [0u8; 256];
        let len = encode_hello_into(&mut buffer);
        let mut version = 0u16;
        assert_eq!(
            unsafe { idroid_codec_decode_hello_ack(buffer.as_ptr(), len, &mut version) },
            IDROID_CODEC_WRONG_KIND
        );
    }
}
