pub(crate) mod algebra;

use egg::Rewrite;

use crate::config::RewriteMode;
use crate::lang::{ConstFold, Lang};

pub(crate) fn rules(mode: RewriteMode) -> Vec<Rewrite<Lang, ConstFold>> {
    let mut all_rules = Vec::new();
    all_rules.extend(algebra::safe_rules());
    if matches!(mode, RewriteMode::Aggressive) {
        all_rules.extend(algebra::aggressive_rules());
    }
    all_rules
}
