use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use mpl_ir::Expr;

use crate::error::Fingerprint;
use crate::hash::stable_hash_str;

/// Canonical expression key with stable hash.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExprKey {
    /// Canonical string form.
    pub canon: Arc<str>,
    /// Stable hash of the canonical string.
    pub hash: u64,
}

/// Cache key for fingerprint entries.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FingerprintKey {
    /// Expression identity.
    pub expr: ExprKey,
    /// Stable hash of the fingerprint configuration.
    pub cfg_hash: u64,
}

/// Cache key for integrable bases.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BasisKey {
    /// Symbol weight.
    pub weight: usize,
    /// Stable hash of the alphabet letters.
    pub alphabet_hash: u64,
    /// Stable hash of the word constraints.
    pub constraints_hash: u64,
}

/// Shared cache for fingerprinting and symbolization.
#[derive(Debug)]
pub struct FingerprintCache {
    /// Interned canonical strings.
    pub interner: RwLock<BTreeMap<String, Arc<str>>>,
    /// Fingerprints keyed by expression + config hash.
    pub expr_fp: RwLock<BTreeMap<FingerprintKey, Fingerprint>>,
    /// Cached symbols keyed by expression.
    pub symbol: RwLock<BTreeMap<ExprKey, mpl_symbol::Symbol>>,
    /// Cached bases keyed by alphabet/constraints/weight.
    pub basis: RwLock<BTreeMap<BasisKey, Arc<mpl_symbol::space::Basis>>>,
}

impl FingerprintCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            interner: RwLock::new(BTreeMap::new()),
            expr_fp: RwLock::new(BTreeMap::new()),
            symbol: RwLock::new(BTreeMap::new()),
            basis: RwLock::new(BTreeMap::new()),
        }
    }

    /// Intern a string and return a shared `Arc<str>`.
    pub fn intern(&self, value: &str) -> Arc<str> {
        let read_guard = match self.interner.read() {
            Ok(guard) => guard,
            Err(poison) => poison.into_inner(),
        };
        if let Some(interned) = read_guard.get(value) {
            return interned.clone();
        }
        drop(read_guard);

        let mut write_guard = match self.interner.write() {
            Ok(guard) => guard,
            Err(poison) => poison.into_inner(),
        };
        if let Some(interned) = write_guard.get(value) {
            return interned.clone();
        }
        let arc: Arc<str> = Arc::from(value.to_string());
        write_guard.insert(value.to_string(), arc.clone());
        arc
    }

    /// Build a stable key for a canonical expression.
    pub fn expr_key(&self, expr: &Expr) -> ExprKey {
        let canon = expr.to_canonical_string();
        ExprKey {
            canon: self.intern(&canon),
            hash: stable_hash_str(&canon),
        }
    }
}

impl Default for FingerprintCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mpl_ir::parse_sexpr;

    use super::FingerprintCache;

    #[test]
    fn interner_reuses_arc() {
        let cache = FingerprintCache::new();
        let a = cache.intern("x");
        let b = cache.intern("x");
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn expr_key_is_stable() {
        let cache = FingerprintCache::new();
        let expr = parse_sexpr("(+ x 0)").unwrap().normalize();
        let k1 = cache.expr_key(&expr);
        let k2 = cache.expr_key(&expr);
        assert_eq!(k1, k2);
    }
}
