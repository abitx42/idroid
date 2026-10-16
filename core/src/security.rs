use std::collections::BTreeMap;
use std::fmt;

use crate::ids::DeviceId;
use crate::ids::SessionId;

pub const PAIRING_NONCE_SIZE: usize = 32;
pub const VERIFICATION_CODE_DIGITS: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedIdentity {
    device_id: DeviceId,
}

impl VerifiedIdentity {
    pub const fn device_id(self) -> DeviceId {
        self.device_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecurityError {
    InvalidIdentity,
    InvalidNonce,
    InvalidVerificationCode,
    Rejected,
    Revoked,
    Replay,
    Unavailable,
}

impl fmt::Display for SecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentity => formatter.write_str("invalid device identity"),
            Self::InvalidNonce => formatter.write_str("invalid pairing nonce"),
            Self::InvalidVerificationCode => formatter.write_str("invalid verification code"),
            Self::Rejected => formatter.write_str("security check rejected the input"),
            Self::Revoked => formatter.write_str("device identity has been revoked"),
            Self::Replay => formatter.write_str("message replay was rejected"),
            Self::Unavailable => formatter.write_str("security provider is unavailable"),
        }
    }
}

impl std::error::Error for SecurityError {}

/// A production identity verifier can bind a device identity to an external trust system.
/// The core crate deliberately does not prescribe a cryptographic protocol.
pub trait IdentityVerifier {
    fn verify_identity(
        &self,
        device_id: DeviceId,
        presented_credential: &[u8],
    ) -> Result<VerifiedIdentity, SecurityError>;
}

/// A production message verifier can validate a frame using the application's security layer.
/// `tag` is opaque to the core crate; no cryptographic implementation is provided here.
pub trait MessageAuthenticator {
    fn verify_message(&self, message: &[u8], tag: &[u8]) -> Result<(), SecurityError>;
}

/// Opaque pairing nonce. The core validates shape only; generation and
/// randomness belong to a platform security provider.
#[derive(Clone, Eq, PartialEq)]
pub struct PairingNonce([u8; PAIRING_NONCE_SIZE]);

impl std::fmt::Debug for PairingNonce {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PairingNonce(REDACTED)")
    }
}

impl PairingNonce {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        let value: [u8; PAIRING_NONCE_SIZE] =
            bytes.try_into().map_err(|_| SecurityError::InvalidNonce)?;
        Ok(Self(value))
    }

    pub fn as_bytes(&self) -> &[u8; PAIRING_NONCE_SIZE] {
        &self.0
    }
}

/// User-visible verification code derived by a trusted pairing provider.
/// Keeping it as a validated value prevents accidental logging of arbitrary
/// credentials while leaving the derivation algorithm out of the portable core.
#[derive(Clone, Eq, PartialEq)]
pub struct VerificationCode(String);

impl std::fmt::Debug for VerificationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("VerificationCode")
            .field(&"REDACTED")
            .finish()
    }
}

impl VerificationCode {
    pub fn new(value: impl Into<String>) -> Result<Self, SecurityError> {
        let value = value.into();
        if value.len() != VERIFICATION_CODE_DIGITS
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(SecurityError::InvalidVerificationCode);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedSession {
    device_id: DeviceId,
    session_id: SessionId,
}

impl AuthenticatedSession {
    pub const fn new(device_id: DeviceId, session_id: SessionId) -> Result<Self, SecurityError> {
        if !device_id.is_valid() || !session_id.is_valid() {
            return Err(SecurityError::InvalidIdentity);
        }
        Ok(Self {
            device_id,
            session_id,
        })
    }

    pub const fn device_id(self) -> DeviceId {
        self.device_id
    }

    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
}

/// Platform-owned persistent identity and revocation boundary. A production
/// implementation must use Keychain/Keystore or an equivalent protected store.
pub trait DeviceIdentityStore {
    fn load_or_create(&mut self, device_id: DeviceId) -> Result<Vec<u8>, SecurityError>;
    fn revoke(&mut self, device_id: DeviceId) -> Result<(), SecurityError>;
    fn is_revoked(&self, device_id: DeviceId) -> Result<bool, SecurityError>;
}

/// Pairing provider owns nonce generation, verification-code derivation, and
/// the user-consent flow. The returned bytes are opaque to the core.
pub trait PairingProvider {
    fn create_nonce(&mut self) -> Result<PairingNonce, SecurityError>;
    fn verification_code(
        &self,
        device_id: DeviceId,
        nonce: &PairingNonce,
    ) -> Result<VerificationCode, SecurityError>;
}

/// Establishes an authenticated session after identity, user consent, and
/// transport binding have been checked by the platform integration.
pub trait SessionAuthenticator {
    fn authenticate_session(
        &self,
        identity: VerifiedIdentity,
        session_id: SessionId,
        transcript: &[u8],
        tag: &[u8],
    ) -> Result<AuthenticatedSession, SecurityError>;
}

/// Replay protection is deliberately stateful and session-scoped. The real
/// implementation must bind counters/nonces to authenticated identity and
/// session epochs; this trait does not provide that algorithm.
pub trait ReplayGuard {
    fn accept(
        &mut self,
        identity: DeviceId,
        session: SessionId,
        sequence: u64,
    ) -> Result<(), SecurityError>;
}

/// Deterministic identity fixture for unit and integration tests.
/// It performs exact byte comparison only and is not cryptographic authentication.
#[derive(Clone, Debug, Default)]
pub struct FixtureIdentityVerifier {
    credentials: BTreeMap<DeviceId, Vec<u8>>,
}

impl FixtureIdentityVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow(&mut self, device_id: DeviceId, credential: impl Into<Vec<u8>>) -> &mut Self {
        self.credentials.insert(device_id, credential.into());
        self
    }
}

impl IdentityVerifier for FixtureIdentityVerifier {
    fn verify_identity(
        &self,
        device_id: DeviceId,
        presented_credential: &[u8],
    ) -> Result<VerifiedIdentity, SecurityError> {
        if !device_id.is_valid() {
            return Err(SecurityError::InvalidIdentity);
        }

        match self.credentials.get(&device_id) {
            Some(expected) if expected.as_slice() == presented_credential => {
                Ok(VerifiedIdentity { device_id })
            }
            _ => Err(SecurityError::Rejected),
        }
    }
}

/// Deterministic message fixture for tests. It compares an expected opaque tag and does not
/// derive, encrypt, or otherwise claim to protect the message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureMessageAuthenticator {
    expected_tag: Vec<u8>,
}

impl FixtureMessageAuthenticator {
    pub fn new(expected_tag: impl Into<Vec<u8>>) -> Self {
        Self {
            expected_tag: expected_tag.into(),
        }
    }
}

impl MessageAuthenticator for FixtureMessageAuthenticator {
    fn verify_message(&self, _message: &[u8], tag: &[u8]) -> Result<(), SecurityError> {
        if self.expected_tag.as_slice() == tag {
            Ok(())
        } else {
            Err(SecurityError::Rejected)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_identity_verifier_is_explicitly_replaceable() {
        let device = DeviceId::new(12);
        let mut verifier = FixtureIdentityVerifier::new();
        verifier.allow(device, b"fixture-proof".to_vec());

        assert_eq!(
            verifier
                .verify_identity(device, b"fixture-proof")
                .expect("accepted")
                .device_id(),
            device
        );
        assert_eq!(
            verifier.verify_identity(device, b"wrong"),
            Err(SecurityError::Rejected)
        );
    }

    #[test]
    fn fixture_message_authenticator_does_not_invent_cryptography() {
        let authenticator = FixtureMessageAuthenticator::new(b"fixture-tag".to_vec());
        assert!(authenticator
            .verify_message(b"message", b"fixture-tag")
            .is_ok());
        assert_eq!(
            authenticator.verify_message(b"message", b"wrong"),
            Err(SecurityError::Rejected)
        );
    }

    #[test]
    fn security_values_validate_shape_and_redact_debug_output() {
        let nonce = PairingNonce::from_bytes(&[7; PAIRING_NONCE_SIZE]).expect("nonce shape");
        assert_eq!(nonce.as_bytes(), &[7; PAIRING_NONCE_SIZE]);
        assert!(PairingNonce::from_bytes(&[7; PAIRING_NONCE_SIZE - 1]).is_err());

        let code = VerificationCode::new("123456").expect("code shape");
        assert_eq!(code.as_str(), "123456");
        assert!(VerificationCode::new("secret").is_err());
        assert!(!format!("{nonce:?}").contains('7'));
        assert!(!format!("{code:?}").contains("123456"));
    }

    #[test]
    fn authenticated_session_rejects_zero_identifiers() {
        assert!(AuthenticatedSession::new(DeviceId::new(1), crate::ids::SessionId::new(2)).is_ok());
        assert!(
            AuthenticatedSession::new(DeviceId::new(0), crate::ids::SessionId::new(2)).is_err()
        );
    }
}
