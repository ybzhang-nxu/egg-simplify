use std::collections::BTreeMap;

use mpl_symbol::space::{
    build_integrable_basis_with_acceptor_with_stats, count_words_with_acceptor, Alphabet, Basis,
    BasisStats, WordAcceptor, WordConstraints,
};
use mpl_symbol::SymbolError;
use num_traits::Zero;

use crate::analysis::skeleton2::{compute_skeleton2_metrics, Skeleton2Metrics};
use crate::build::acceptors::{
    validate_automaton_order, validate_channel_pairs_acceptors, validate_genealogical_acceptors,
    validate_kgram_acceptors, AutomatonAcceptorRef, CompositeAcceptor,
};
use crate::build::alphabet::{collect_vars_from_letters, normalize_inputs};
use crate::build::constraints::validate_constraints;
use crate::{ErrorCode, ExperimentError, Status};

#[derive(Clone, Debug)]
pub struct ExperimentConfig {
    pub name: String,
    pub out_dir: std::path::PathBuf,
    pub alphabet: Alphabet,
    pub constraints: WordConstraints,
    pub genealogical_acceptors: Vec<mpl_symbol::space::GenealogicalAcceptor>,
    pub kgram_acceptors: Vec<mpl_symbol::space::KGramAcceptor>,
    pub channel_pairs_acceptors: Vec<mpl_symbol::space::ChannelPairsAcceptor>,
    pub automaton_acceptors: Vec<AutomatonAcceptorRef>,
    pub constraint_budget: mpl_symbol::space::ConstraintBudget,
    pub weight_min: usize,
    pub weight_max: usize,
    pub vars: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ExperimentReport {
    pub name: String,
    pub alphabet: Alphabet,
    pub constraints: WordConstraints,
    pub weight_min: usize,
    pub weight_max: usize,
    pub vars: Vec<String>,
    pub summaries: Vec<WeightSummary>,
    pub pairs_total: BTreeMap<(usize, usize), u64>,
    pub pairs_by_weight: BTreeMap<usize, BTreeMap<(usize, usize), u64>>,
    pub triplets_total: BTreeMap<(usize, usize, usize), u64>,
    pub triplets_by_weight: BTreeMap<usize, BTreeMap<(usize, usize, usize), u64>>,
}

#[derive(Clone, Debug)]
pub struct WeightSummary {
    pub weight: usize,
    pub stats: BasisStats,
    pub n_words_allowed: usize,
    pub n_active_words: usize,
    pub topology: TopologyMetrics,
    pub skeleton2: Skeleton2Metrics,
    pub status: Status,
    pub error_code: Option<ErrorCode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyMetrics {
    pub n_vertices: usize,
    pub n_edges: usize,
    pub n_active_words: usize,
    pub weakly_connected_components: usize,
    pub strongly_connected_components: usize,
    pub max_out_degree: usize,
    pub density_num: u64,
    pub density_den: u64,
    pub avg_out_degree_num: u64,
    pub avg_out_degree_den: u64,
}

pub fn run_experiment(cfg: &ExperimentConfig) -> Result<ExperimentReport, ExperimentError> {
    if cfg.weight_min > cfg.weight_max {
        return Err(ExperimentError::InvalidConfig(
            "weight_min must be <= weight_max".to_string(),
        ));
    }

    let (alphabet, constraints) = normalize_inputs(&cfg.alphabet, &cfg.constraints);
    validate_constraints(&alphabet, &constraints)?;
    validate_genealogical_acceptors(&alphabet, &cfg.genealogical_acceptors)?;
    validate_kgram_acceptors(&alphabet, &cfg.kgram_acceptors)?;
    validate_channel_pairs_acceptors(&alphabet, &cfg.channel_pairs_acceptors)?;
    validate_automaton_order(
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    )?;
    let vars = if cfg.vars.is_empty() {
        collect_vars_from_letters(&alphabet.letters)
    } else {
        cfg.vars.clone()
    };

    let mut summaries = Vec::new();
    let mut pairs_total: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    let mut pairs_by_weight: BTreeMap<usize, BTreeMap<(usize, usize), u64>> = BTreeMap::new();
    let mut triplets_total: BTreeMap<(usize, usize, usize), u64> = BTreeMap::new();
    let mut triplets_by_weight: BTreeMap<usize, BTreeMap<(usize, usize, usize), u64>> =
        BTreeMap::new();
    let alpha_len = alphabet.letters.len();
    let acceptor = CompositeAcceptor::new(
        &constraints,
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    );
    let budget = cfg.constraint_budget;

    for weight in cfg.weight_min..=cfg.weight_max {
        let n_words_allowed =
            match count_allowed_words_with_acceptor(alpha_len, &acceptor, weight, Some(&budget)) {
                Ok(count) => count,
                Err(err) => {
                    let empty_pairs = BTreeMap::new();
                    let empty_triplets = BTreeMap::new();
                    let topology = compute_topology_metrics(alpha_len, &empty_pairs, 0);
                    let skeleton2 =
                        compute_skeleton2_metrics(alpha_len, &empty_pairs, &empty_triplets);
                    let error_code = error_code_from_symbol(&err);
                    summaries.push(WeightSummary {
                        weight,
                        stats: BasisStats::default(),
                        n_words_allowed: 0,
                        n_active_words: 0,
                        topology,
                        skeleton2,
                        status: Status::Err,
                        error_code: Some(error_code),
                    });
                    continue;
                }
            };
        match build_integrable_basis_with_acceptor_with_stats(
            &alphabet,
            &acceptor,
            weight,
            Some(&budget),
        ) {
            Ok(basis) => {
                let stats = basis.stats().clone();
                let active_cols = active_columns(&basis);
                let pair_counts = pair_counts_from_words(&basis.words, &active_cols);
                let triplet_counts = triplet_counts_from_words(&basis.words, &active_cols);
                let topology = compute_topology_metrics(alpha_len, &pair_counts, active_cols.len());
                let skeleton2 = compute_skeleton2_metrics(alpha_len, &pair_counts, &triplet_counts);
                for ((a, b), count) in &pair_counts {
                    *pairs_total.entry((*a, *b)).or_insert(0) += *count;
                }
                pairs_by_weight.insert(weight, pair_counts);
                for ((a, b, c), count) in &triplet_counts {
                    *triplets_total.entry((*a, *b, *c)).or_insert(0) += *count;
                }
                triplets_by_weight.insert(weight, triplet_counts);
                summaries.push(WeightSummary {
                    weight,
                    stats,
                    n_words_allowed,
                    n_active_words: active_cols.len(),
                    topology,
                    skeleton2,
                    status: Status::Ok,
                    error_code: None,
                });
            }
            Err(err) => {
                let stats = err.stats;
                let empty_pairs = BTreeMap::new();
                let empty_triplets = BTreeMap::new();
                let topology = compute_topology_metrics(alpha_len, &empty_pairs, 0);
                let skeleton2 = compute_skeleton2_metrics(alpha_len, &empty_pairs, &empty_triplets);
                let error_code = error_code_from_symbol(&err.err);
                summaries.push(WeightSummary {
                    weight,
                    stats,
                    n_words_allowed,
                    n_active_words: 0,
                    topology,
                    skeleton2,
                    status: Status::Err,
                    error_code: Some(error_code),
                });
            }
        }
    }

    Ok(ExperimentReport {
        name: cfg.name.clone(),
        alphabet,
        constraints,
        weight_min: cfg.weight_min,
        weight_max: cfg.weight_max,
        vars,
        summaries,
        pairs_total,
        pairs_by_weight,
        triplets_total,
        triplets_by_weight,
    })
}

pub(crate) fn count_allowed_words_with_acceptor<A: WordAcceptor>(
    alpha_len: usize,
    acceptor: &A,
    weight: usize,
    budget: Option<&mpl_symbol::space::ConstraintBudget>,
) -> Result<usize, SymbolError> {
    let count = count_words_with_acceptor(alpha_len, acceptor, weight, budget)?;
    if count > (usize::MAX as u64) {
        return Err(SymbolError::NotImplemented(
            "word count exceeds usize".to_string(),
        ));
    }
    Ok(count as usize)
}

pub(crate) fn error_code_from_symbol(err: &SymbolError) -> ErrorCode {
    match err {
        SymbolError::NotImplemented(_) => ErrorCode::NotImplemented,
        SymbolError::Eval(_) => ErrorCode::Eval,
        SymbolError::InsufficientSamples => ErrorCode::InsufficientSamples,
        SymbolError::FuelExhausted => ErrorCode::FuelExhausted,
        SymbolError::ConstraintBudgetExceeded(_) => ErrorCode::ConstraintBudgetExceeded,
    }
}

fn active_columns(basis: &Basis) -> Vec<usize> {
    // Active words are columns with nonzero coefficients in any nullspace basis vector.
    let ncols = basis.words.len();
    if ncols == 0 || basis.vectors.is_empty() {
        return Vec::new();
    }
    let mut active = vec![false; ncols];
    for vec in &basis.vectors {
        for (col, coeff) in vec.iter().enumerate() {
            if !coeff.is_zero() {
                active[col] = true;
            }
        }
    }
    let mut cols = Vec::new();
    for (col, is_active) in active.iter().enumerate() {
        if *is_active {
            cols.push(col);
        }
    }
    cols
}

fn pair_counts_from_words(
    words: &[Vec<usize>],
    active_cols: &[usize],
) -> BTreeMap<(usize, usize), u64> {
    // Count definition: sum over active words of all adjacent positions (w-1 per word).
    let mut counts = BTreeMap::new();
    for &col in active_cols {
        let word = match words.get(col) {
            Some(word) => word,
            None => continue,
        };
        if word.len() < 2 {
            continue;
        }
        for idx in 0..(word.len() - 1) {
            let a = word[idx];
            let b = word[idx + 1];
            *counts.entry((a, b)).or_insert(0) += 1;
        }
    }
    counts
}

fn triplet_counts_from_words(
    words: &[Vec<usize>],
    active_cols: &[usize],
) -> BTreeMap<(usize, usize, usize), u64> {
    // Count definition: sum over active words of all consecutive triplets (w-2 per word).
    let mut counts = BTreeMap::new();
    for &col in active_cols {
        let word = match words.get(col) {
            Some(word) => word,
            None => continue,
        };
        if word.len() < 3 {
            continue;
        }
        for idx in 0..(word.len() - 2) {
            let a = word[idx];
            let b = word[idx + 1];
            let c = word[idx + 2];
            *counts.entry((a, b, c)).or_insert(0) += 1;
        }
    }
    counts
}

fn compute_topology_metrics(
    n_vertices: usize,
    pair_counts: &BTreeMap<(usize, usize), u64>,
    n_active_words: usize,
) -> TopologyMetrics {
    let mut edges = Vec::new();
    for &(a, b) in pair_counts.keys() {
        if a >= n_vertices || b >= n_vertices {
            continue;
        }
        edges.push((a, b));
    }

    let mut adj = vec![Vec::new(); n_vertices];
    let mut rev = vec![Vec::new(); n_vertices];
    for (a, b) in &edges {
        adj[*a].push(*b);
        rev[*b].push(*a);
    }
    for neighbors in &mut adj {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    for neighbors in &mut rev {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let n_edges = edges.len();
    let weakly_connected_components = weakly_connected_components(n_vertices, &edges);
    let strongly_connected_components = strongly_connected_components(n_vertices, &adj, &rev);

    let max_out_degree = adj
        .iter()
        .map(|neighbors| neighbors.len())
        .max()
        .unwrap_or(0);
    let avg_out_degree_num = n_edges as u64;
    let avg_out_degree_den = if n_vertices == 0 {
        1
    } else {
        n_vertices as u64
    };
    let density_num = n_edges as u64;
    let density_den = if n_vertices == 0 {
        1
    } else {
        (n_vertices as u64) * (n_vertices as u64)
    };

    TopologyMetrics {
        n_vertices,
        n_edges,
        n_active_words,
        weakly_connected_components,
        strongly_connected_components,
        max_out_degree,
        density_num,
        density_den,
        avg_out_degree_num,
        avg_out_degree_den,
    }
}

fn weakly_connected_components(n_vertices: usize, edges: &[(usize, usize)]) -> usize {
    if n_vertices == 0 {
        return 0;
    }
    let mut adj = vec![Vec::new(); n_vertices];
    for (a, b) in edges {
        adj[*a].push(*b);
        adj[*b].push(*a);
    }
    for neighbors in &mut adj {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let mut visited = vec![false; n_vertices];
    let mut count = 0;
    for v in 0..n_vertices {
        if visited[v] {
            continue;
        }
        count += 1;
        let mut stack = vec![v];
        visited[v] = true;
        while let Some(node) = stack.pop() {
            for &next in &adj[node] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
    }
    count
}

fn strongly_connected_components(
    n_vertices: usize,
    adj: &[Vec<usize>],
    rev: &[Vec<usize>],
) -> usize {
    if n_vertices == 0 {
        return 0;
    }
    let mut visited = vec![false; n_vertices];
    let mut order = Vec::with_capacity(n_vertices);

    for v in 0..n_vertices {
        if visited[v] {
            continue;
        }
        let mut stack = vec![(v, 0usize)];
        visited[v] = true;
        while let Some((node, idx)) = stack.pop() {
            if idx < adj[node].len() {
                stack.push((node, idx + 1));
                let next = adj[node][idx];
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                order.push(node);
            }
        }
    }

    visited.fill(false);
    let mut count = 0;
    for &v in order.iter().rev() {
        if visited[v] {
            continue;
        }
        count += 1;
        let mut stack = vec![v];
        visited[v] = true;
        while let Some(node) = stack.pop() {
            for &next in &rev[node] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod acceptor_tests {
    use super::*;
    use mpl_symbol::space::WordConstraintsAcceptor;
    use std::path::PathBuf;

    fn load_l1_a2_spec() -> ExperimentConfig {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("m1")
            .join("L1_A2_cluster.toml");
        crate::load_spec(&path).expect("load L1_A2_cluster.toml")
    }

    #[test]
    fn l1_a2_acceptor_matches_runner_outputs() {
        let mut cfg = load_l1_a2_spec();
        cfg.weight_min = 3;
        cfg.weight_max = 3;
        let report = run_experiment(&cfg).expect("run experiment");
        let acceptor = WordConstraintsAcceptor::new(&cfg.constraints);
        let budget = cfg.constraint_budget;

        let summary = report.summaries.first().expect("summary");
        let weight = summary.weight;
        let basis = build_integrable_basis_with_acceptor_with_stats(
            &cfg.alphabet,
            &acceptor,
            weight,
            Some(&budget),
        )
        .expect("basis");
        assert_eq!(basis.words.len(), summary.n_words_allowed);
        assert_eq!(basis.stats().one_line(), summary.stats.one_line());

        let active_cols = active_columns(&basis);
        let pair_counts = pair_counts_from_words(&basis.words, &active_cols);
        let triplet_counts = triplet_counts_from_words(&basis.words, &active_cols);
        let topology =
            compute_topology_metrics(cfg.alphabet.letters.len(), &pair_counts, active_cols.len());
        let skeleton2 =
            compute_skeleton2_metrics(cfg.alphabet.letters.len(), &pair_counts, &triplet_counts);
        let expected_pairs = report
            .pairs_by_weight
            .get(&weight)
            .cloned()
            .unwrap_or_default();
        let expected_triplets = report
            .triplets_by_weight
            .get(&weight)
            .cloned()
            .unwrap_or_default();

        assert_eq!(pair_counts, expected_pairs);
        assert_eq!(triplet_counts, expected_triplets);
        assert_eq!(topology, summary.topology);
        assert_eq!(skeleton2, summary.skeleton2);
    }
}
