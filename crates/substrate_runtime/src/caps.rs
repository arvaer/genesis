use substrate_core::effect::CapabilityId;

/// Capability registry. In v0, just provides a root capability.
pub struct CapabilityRegistry {
    next_id: u64,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        CapabilityRegistry { next_id: 1 }
    }

    /// Create a new capability. Only the kernel should call this.
    pub fn create_capability(&mut self) -> CapabilityId {
        let id = CapabilityId(self.next_id);
        self.next_id += 1;
        id
    }

    /// The root capability (ID 0), always available.
    pub fn root_capability() -> CapabilityId {
        CapabilityId(0)
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}
