use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolError {
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error(transparent)]
    Eval(#[from] EvalError),
    #[error("insufficient valid sample points for integrability check")]
    InsufficientSamples,
    #[error("fuel exhausted during shuffle expansion")]
    FuelExhausted,
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unknown variable '{0}'")]
    UnknownVariable(String),
    #[error("negative exponent on zero")]
    NegativePowerOfZero,
    #[error("overflow while computing power")]
    PowerOverflow,
}
