use crate::value::Value;
use serde::{Deserialize, Serialize};

/// Opaque capability identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub u64);

/// A single effect record produced during evaluation.
/// Effects are data — they are never executed during eval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effect {
    pub cap_id: CapabilityId,
    pub op: String,
    pub args: Vec<Value>,
}

/// Evaluation result carrying an ordered effect log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eff<T> {
    pub value: T,
    pub effects: Vec<Effect>,
}

impl<T> Eff<T> {
    /// Wrap a pure value with no effects.
    pub fn pure(value: T) -> Self {
        Eff {
            value,
            effects: Vec::new(),
        }
    }

    /// Transform the value, keeping effects unchanged.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Eff<U> {
        Eff {
            value: f(self.value),
            effects: self.effects,
        }
    }

    /// Monadic bind: run `f` on the value, then concatenate effect logs (preserving order).
    pub fn bind<U, F: FnOnce(T) -> Eff<U>>(self, f: F) -> Eff<U> {
        let mut effects = self.effects;
        let next = f(self.value);
        effects.extend(next.effects);
        Eff {
            value: next.value,
            effects,
        }
    }
}
