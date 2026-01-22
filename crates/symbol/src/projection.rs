use mpl_ir::Expr;
use num_traits::Zero;

use crate::{Symbol, Word};

/// Apply suffix projection T_s: keep words ending in suffix and drop the suffix.
pub fn apply_suffix_projection(sym: &Symbol, suffix: &[Expr]) -> Symbol {
    if suffix.is_empty() {
        return sym.clone();
    }
    let suffix_keys = suffix_keys(suffix);
    apply_suffix_projection_with_keys(sym, &suffix_keys)
}

/// Apply suffix projection to each basis element.
pub fn apply_suffix_projection_to_basis(basis: &[Symbol], suffix: &[Expr]) -> Vec<Symbol> {
    if suffix.is_empty() {
        return basis.to_vec();
    }
    let suffix_keys = suffix_keys(suffix);
    basis
        .iter()
        .map(|sym| apply_suffix_projection_with_keys(sym, &suffix_keys))
        .collect()
}

fn apply_suffix_projection_with_keys(sym: &Symbol, suffix_keys: &[String]) -> Symbol {
    let suffix_len = suffix_keys.len();
    let mut out = Symbol::zero();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let letters = word.letters();
        if letters.len() < suffix_len {
            continue;
        }
        if !suffix_matches(letters, suffix_keys) {
            continue;
        }
        let prefix_len = letters.len() - suffix_len;
        let truncated = Word(letters[..prefix_len].to_vec());
        out.add_term(truncated, *coeff);
    }
    out
}

fn suffix_matches(letters: &[Expr], suffix_keys: &[String]) -> bool {
    let offset = letters.len().saturating_sub(suffix_keys.len());
    for (idx, key) in suffix_keys.iter().enumerate() {
        if letters[offset + idx].to_canonical_string() != *key {
            return false;
        }
    }
    true
}

fn suffix_keys(suffix: &[Expr]) -> Vec<String> {
    suffix
        .iter()
        .map(|expr| expr.normalize().to_canonical_string())
        .collect()
}
