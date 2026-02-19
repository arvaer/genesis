use anyhow::Result;
use substrate_core::effect::Effect;

/// Trait for executing effects after transaction commit.
pub trait EffectExecutor {
    fn execute(&self, effects: &[Effect]) -> Result<()>;
}

/// No-op executor for tests. Discards all effects.
pub struct NoopExecutor;

impl EffectExecutor for NoopExecutor {
    fn execute(&self, _effects: &[Effect]) -> Result<()> {
        Ok(())
    }
}
