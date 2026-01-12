use std::fmt;

use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintBudgetKind {
    States,
    Transitions,
    Words,
}

impl fmt::Display for ConstraintBudgetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::States => "max_states",
            Self::Transitions => "max_transitions",
            Self::Words => "max_words",
        };
        write!(f, "{label}")
    }
}

#[derive(Clone, Debug, Error)]
pub enum SymbolError {
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error(transparent)]
    Eval(#[from] EvalError),
    #[error("insufficient valid sample points for integrability check")]
    InsufficientSamples,
    #[error("fuel exhausted during shuffle expansion")]
    FuelExhausted,
    #[error("constraint budget exceeded: {0}")]
    ConstraintBudgetExceeded(ConstraintBudgetKind),
}

#[derive(Clone, Debug, Error)]
pub enum EvalError {
    #[error("unknown variable '{0}'")]
    UnknownVariable(String),
    #[error("negative exponent on zero")]
    NegativePowerOfZero,
    #[error("overflow while computing power")]
    PowerOverflow,
}
