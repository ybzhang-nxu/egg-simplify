use std::collections::BTreeMap;

use thiserror::Error;

/// Errors returned by the symbol-aware rewrite pipeline.
#[derive(Debug, Error)]
pub enum RewriteSymbolError {
    /// Underlying rewrite error.
    #[error("rewrite error: {0}")]
    Rewrite(#[from] mpl_rewrite::RewriteError),
    /// Pattern parsing failed.
    #[error("pattern parse: {0}")]
    Pattern(String),
    /// RHS instantiation failed.
    #[error("rhs instantiation: {0}")]
    RhsInstantiation(String),
    /// Internal fingerprinting error.
    #[error("fingerprint internal: {0}")]
    FingerprintInternal(String),
}

/// Reasons a fingerprint is unavailable.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnknownReason {
    /// Symbolization is not implemented for this expression.
    SymbolNotImplemented,
    /// Symbol evaluation failed.
    SymbolEval,
    /// Not enough valid samples to evaluate constraints.
    InsufficientSamples,
    /// Budget or limits were exhausted.
    BudgetExhausted,
    /// Exponent is invalid or unsupported.
    InvalidExponent,
    /// Operator arity is invalid or unsupported.
    InvalidArity,
    /// Expression contains an unsupported node.
    UnsupportedNode,
}

/// Fingerprint results for an expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fingerprint {
    /// Per-weight fingerprint results.
    ByWeight(BTreeMap<usize, WeightFingerprint>),
    /// Unknown fingerprint with reason and expr hash.
    Unknown {
        reason: UnknownReason,
        expr_hash: u64,
    },
    /// Conflict between two incompatible fingerprints.
    Conflict { left_digest: u64, right_digest: u64 },
}

/// Fingerprint for a specific weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeightFingerprint {
    /// Symbol lies in the integrable subspace.
    Integrable {
        weight: usize,
        basis_id: u64,
        coords_hash: u64,
        resid_hash: u64,
    },
    /// Symbol lies outside the integrable subspace.
    NonIntegrable {
        weight: usize,
        basis_id: u64,
        coords_hash: u64,
        resid_hash: u64,
    },
    /// Fingerprint is unknown for this weight.
    Unknown {
        weight: usize,
        reason: UnknownReason,
        expr_hash: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::{Fingerprint, UnknownReason, WeightFingerprint};

    #[test]
    fn unknown_fingerprint_compares() {
        let left = Fingerprint::Unknown {
            reason: UnknownReason::UnsupportedNode,
            expr_hash: 1,
        };
        let right = Fingerprint::Unknown {
            reason: UnknownReason::UnsupportedNode,
            expr_hash: 2,
        };
        assert_ne!(left, right);
    }

    #[test]
    fn weight_fingerprint_roundtrips() {
        let fp = WeightFingerprint::Integrable {
            weight: 2,
            basis_id: 3,
            coords_hash: 4,
            resid_hash: 5,
        };
        let cloned = fp.clone();
        assert_eq!(fp, cloned);
    }
}
