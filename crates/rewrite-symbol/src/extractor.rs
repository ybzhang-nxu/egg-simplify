use egg::{CostFunction, EGraph, Id, Language};
use mpl_rewrite::lang::Lang;

use crate::analysis::SymbolAnalysis;
use crate::error::{Fingerprint, WeightFingerprint};
use crate::hash::StableHasher;

/// Penalty configuration for symbol-aware extraction.
#[derive(Clone, Debug)]
pub struct PenaltyConfig {
    /// Penalty for unknown fingerprint states.
    pub unknown_penalty: u64,
    /// Penalty for non-integrable symbols.
    pub non_integrable_penalty: u64,
    /// Penalty for conflicting fingerprints.
    pub conflict_penalty: u64,
}

impl Default for PenaltyConfig {
    fn default() -> Self {
        Self {
            unknown_penalty: 10,
            non_integrable_penalty: 100,
            conflict_penalty: 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SymbolCost {
    pub(crate) penalty: u64,
    pub(crate) size: usize,
    pub(crate) tie: u64,
}

impl Ord for SymbolCost {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.penalty
            .cmp(&other.penalty)
            .then(self.size.cmp(&other.size))
            .then(self.tie.cmp(&other.tie))
    }
}

impl PartialOrd for SymbolCost {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) struct SymbolCostFn<'a> {
    egraph: &'a EGraph<Lang, SymbolAnalysis>,
    penalty_cfg: PenaltyConfig,
}

impl<'a> SymbolCostFn<'a> {
    pub(crate) fn new(
        egraph: &'a EGraph<Lang, SymbolAnalysis>,
        penalty_cfg: PenaltyConfig,
    ) -> Self {
        Self {
            egraph,
            penalty_cfg,
        }
    }
}

impl<'a> CostFunction<Lang> for SymbolCostFn<'a> {
    type Cost = SymbolCost;

    fn cost<C>(&mut self, enode: &Lang, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost,
    {
        let mut child_costs = Vec::new();
        for &child in enode.children() {
            child_costs.push(costs(child));
        }
        let child_penalty = child_costs
            .iter()
            .map(|cost| cost.penalty)
            .max()
            .unwrap_or(0);
        let size = 1 + child_costs.iter().map(|cost| cost.size).sum::<usize>();
        let local_penalty = local_penalty(self.egraph, enode, &self.penalty_cfg);
        let penalty = child_penalty.max(local_penalty);
        let tie = tie_hash(enode, &child_costs);

        SymbolCost { penalty, size, tie }
    }
}

fn local_penalty(egraph: &EGraph<Lang, SymbolAnalysis>, enode: &Lang, cfg: &PenaltyConfig) -> u64 {
    let id = egraph.lookup(enode.clone());
    let Some(id) = id else {
        return cfg.unknown_penalty;
    };
    penalty_from_fingerprint(&egraph[id].data.fingerprint, cfg)
}

fn penalty_from_fingerprint(fp: &Fingerprint, cfg: &PenaltyConfig) -> u64 {
    match fp {
        Fingerprint::Unknown { .. } => cfg.unknown_penalty,
        Fingerprint::Conflict { .. } => cfg.conflict_penalty,
        Fingerprint::ByWeight(weights) => weights
            .values()
            .map(|wf| match wf {
                WeightFingerprint::Integrable { .. } => 0,
                WeightFingerprint::NonIntegrable { .. } => cfg.non_integrable_penalty,
                WeightFingerprint::Unknown { .. } => cfg.unknown_penalty,
            })
            .max()
            .unwrap_or(0),
    }
}

fn tie_hash(enode: &Lang, costs: &[SymbolCost]) -> u64 {
    let mut hasher = StableHasher::new();
    match enode {
        Lang::Num(value) => {
            hasher.update_str("num");
            hasher.update_i64(*value.numer());
            hasher.update_i64(*value.denom());
        }
        Lang::Var(sym) => {
            hasher.update_str("var");
            hasher.update_str(sym.as_str());
        }
        Lang::Add(_) => {
            hasher.update_str("add");
            hash_children(&mut hasher, costs, false);
        }
        Lang::Mul(_) => {
            hasher.update_str("mul");
            hash_children(&mut hasher, costs, false);
        }
        Lang::Pow(_) => {
            hasher.update_str("pow");
            hash_children(&mut hasher, costs, false);
        }
        Lang::Log(_) => {
            hasher.update_str("log");
            hash_children(&mut hasher, costs, false);
        }
        Lang::Li2(_) => {
            hasher.update_str("li2");
            hash_children(&mut hasher, costs, false);
        }
    }
    hasher.finish()
}

fn hash_children(hasher: &mut StableHasher, costs: &[SymbolCost], commutative: bool) {
    let mut ties: Vec<u64> = costs.iter().map(|cost| cost.tie).collect();
    if commutative {
        ties.sort();
    }
    hasher.update_u64(ties.len() as u64);
    for tie in ties {
        hasher.update_u64(tie);
    }
}

#[cfg(test)]
mod tests {
    use super::SymbolCost;

    #[test]
    fn symbol_cost_orders_by_penalty_then_size_then_tie() {
        let a = SymbolCost {
            penalty: 1,
            size: 2,
            tie: 3,
        };
        let b = SymbolCost {
            penalty: 2,
            size: 1,
            tie: 1,
        };
        assert!(a < b);
    }
}
