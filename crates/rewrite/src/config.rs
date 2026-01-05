use thiserror::Error;

#[derive(Debug, Error)]
pub enum RewriteError {
    #[error("invalid arity: {0}")]
    InvalidArity(String),
    #[error("invalid exponent: {0}")]
    InvalidExponent(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewriteMode {
    Safe,
    Aggressive,
}

#[derive(Clone, Debug)]
pub struct RewriteConfig {
    pub iters: usize,
    pub node_limit: usize,
    pub time_limit_ms: u64,
    pub mode: RewriteMode,
}

impl Default for RewriteConfig {
    fn default() -> Self {
        Self {
            iters: 20,
            node_limit: 50_000,
            time_limit_ms: 300,
            mode: RewriteMode::Safe,
        }
    }
}
