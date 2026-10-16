use std::collections::BTreeMap;
use std::fmt;

use crate::ids::DeviceId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeviceKind {
    Phone,
    Tablet,
    Emulator,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeviceCapabilities(u32);

impl DeviceCapabilities {
    pub const NONE: Self = Self(0);
    pub const DISPLAY: Self = Self(1 << 0);
    pub const INPUT: Self = Self(1 << 1);
    pub const FILE_TRANSFER: Self = Self(1 << 2);
    pub const DEBUG_BRIDGE: Self = Self(1 << 3);

    pub const fn empty() -> Self {
        Self::NONE
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, capability: Self) -> bool {
        self.0 & capability.0 == capability.0
    }

    pub const fn with(self, capability: Self) -> Self {
        Self(self.0 | capability.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceProfile {
    id: DeviceId,
    kind: DeviceKind,
    capabilities: DeviceCapabilities,
}

impl DeviceProfile {
    pub fn new(
        id: DeviceId,
        kind: DeviceKind,
        capabilities: DeviceCapabilities,
    ) -> Result<Self, DeviceError> {
        if !id.is_valid() {
            return Err(DeviceError::InvalidId);
        }

        Ok(Self {
            id,
            kind,
            capabilities,
        })
    }

    pub const fn id(&self) -> DeviceId {
        self.id
    }

    pub const fn kind(&self) -> DeviceKind {
        self.kind
    }

    pub const fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceError {
    InvalidId,
    AlreadyRegistered(DeviceId),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => formatter.write_str("device identifiers must be non-zero"),
            Self::AlreadyRegistered(id) => write!(formatter, "device {id} is already registered"),
        }
    }
}

impl std::error::Error for DeviceError {}

#[derive(Clone, Debug, Default)]
pub struct DeviceRegistry {
    devices: BTreeMap<DeviceId, DeviceProfile>,
}

impl DeviceRegistry {
    pub fn register(&mut self, profile: DeviceProfile) -> Result<(), DeviceError> {
        if self.devices.contains_key(&profile.id()) {
            return Err(DeviceError::AlreadyRegistered(profile.id()));
        }

        self.devices.insert(profile.id(), profile);
        Ok(())
    }

    pub fn get(&self, id: DeviceId) -> Option<&DeviceProfile> {
        self.devices.get(&id)
    }

    pub fn remove(&mut self, id: DeviceId) -> Option<DeviceProfile> {
        self.devices.remove(&id)
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_and_registry_have_no_session_state() {
        let id = DeviceId::new(7);
        let profile = DeviceProfile::new(
            id,
            DeviceKind::Phone,
            DeviceCapabilities::DISPLAY.with(DeviceCapabilities::INPUT),
        )
        .expect("valid device");
        let mut registry = DeviceRegistry::default();

        registry
            .register(profile.clone())
            .expect("first registration");

        assert_eq!(registry.get(id), Some(&profile));
        assert_eq!(registry.len(), 1);
        assert!(profile.capabilities().contains(DeviceCapabilities::DISPLAY));
        assert!(!profile
            .capabilities()
            .contains(DeviceCapabilities::FILE_TRANSFER));
    }
}
