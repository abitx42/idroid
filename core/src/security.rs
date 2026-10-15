use std::collections::BTreeMap;
use std::fmt;

use crate::ids::DeviceId;

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
    Rejected,
    Unavailable,
}

impl fmt::Display for SecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentity => formatter.write_str("invalid device identity"),
            Self::Rejected => formatter.write_str("security check rejected the input"),
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
            Some(expected) if expected.as_slice() == presented_credential => Ok(VerifiedIdentity { device_id }),
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
            verifier.verify_identity(device, b"fixture-proof").expect("accepted").device_id(),
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
        assert!(authenticator.verify_message(b"message", b"fixture-tag").is_ok());
        assert_eq!(
            authenticator.verify_message(b"message", b"wrong"),
            Err(SecurityError::Rejected)
        );
    }
}
