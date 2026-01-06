pub(crate) mod algebra;

use egg::{Analysis, Rewrite};

use crate::config::RewriteMode;
use crate::lang::Lang;

/// Build the rewrite ruleset for the selected mode.
pub fn rules_for_mode<N>(mode: RewriteMode) -> Vec<Rewrite<Lang, N>>
where
    N: Analysis<Lang> + 'static,
{
    let mut all_rules = Vec::new();
    all_rules.extend(algebra::safe_rules::<N>());
    if matches!(mode, RewriteMode::Aggressive) {
        all_rules.extend(algebra::aggressive_rules::<N>());
    }
    all_rules
}
