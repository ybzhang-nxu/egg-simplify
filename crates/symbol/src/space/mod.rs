use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use mpl_ir::Expr;
use num_traits::{One, Zero};

use crate::error::{ConstraintBudgetKind, SymbolError};
use crate::integrability_utils::{build_envs, collect_vars, DlogCache};
use crate::{Coeff, Symbol, Word};

mod acceptor;
mod stats;
pub use acceptor::{
    And, ConstraintBudget, GenealogicalAcceptor, GenealogicalRule, KGramAcceptor, KGramMode,
    WordAcceptor, WordConstraintsAcceptor,
};
pub use stats::BasisStats;

#[derive(Clone, Debug)]
pub struct Alphabet {
    pub name: String,
    pub letters: Vec<Expr>,
    pub letter_names: Vec<String>,
}

impl Alphabet {
    pub fn new(name: String, letters: Vec<Expr>, letter_names: Vec<String>) -> Self {
        let letters = letters.into_iter().map(|expr| expr.normalize()).collect();
        Self {
            name,
            letters,
            letter_names,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WordConstraints {
    pub first_allowed: Option<BTreeSet<usize>>,
    pub allowed_pairs: Option<Vec<Vec<bool>>>,
}

impl WordConstraints {
    pub fn allow_step(&self, pos: usize, prev: Option<usize>, next: usize) -> bool {
        if pos == 0 {
            if let Some(first) = &self.first_allowed {
                if !first.contains(&next) {
                    return false;
                }
            }
        }

        if let Some(prev_idx) = prev {
            if let Some(pairs) = &self.allowed_pairs {
                if prev_idx >= pairs.len() {
                    return false;
                }
                let row = &pairs[prev_idx];
                if next >= row.len() {
                    return false;
                }
                if !row[next] {
                    return false;
                }
            }
        }

        true
    }
}

#[derive(Clone, Debug)]
pub struct Basis {
    pub words: Vec<Vec<usize>>,
    pub vectors: Vec<Vec<Coeff>>,
    free_cols: Vec<usize>,
    stats: BasisStats,
}

impl Basis {
    pub fn stats(&self) -> &BasisStats {
        &self.stats
    }
}

#[derive(Debug)]
pub struct BasisBuildError {
    pub err: SymbolError,
    pub stats: BasisStats,
}

impl fmt::Display for BasisBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (stats: {})", self.err, self.stats.one_line())
    }
}

impl std::error::Error for BasisBuildError {}

pub fn build_integrable_basis(
    alpha: &Alphabet,
    constraints: &WordConstraints,
    weight: usize,
) -> Result<Basis, SymbolError> {
    build_integrable_basis_with_stats(alpha, constraints, weight).map_err(|err| err.err)
}

#[allow(clippy::result_large_err)]
pub fn build_integrable_basis_with_stats(
    alpha: &Alphabet,
    constraints: &WordConstraints,
    weight: usize,
) -> Result<Basis, BasisBuildError> {
    if let Err(err) = validate_constraints(alpha, constraints) {
        return Err(BasisBuildError {
            err,
            stats: BasisStats::default(),
        });
    }

    let acceptor = WordConstraintsAcceptor::new(constraints);
    build_integrable_basis_with_acceptor_with_stats(alpha, &acceptor, weight, None)
}

pub fn build_integrable_basis_with_acceptor<A: WordAcceptor>(
    alpha: &Alphabet,
    acceptor: &A,
    weight: usize,
    budget: Option<&ConstraintBudget>,
) -> Result<Basis, SymbolError> {
    build_integrable_basis_with_acceptor_with_stats(alpha, acceptor, weight, budget)
        .map_err(|err| err.err)
}

#[allow(clippy::result_large_err)]
pub fn build_integrable_basis_with_acceptor_with_stats<A: WordAcceptor>(
    alpha: &Alphabet,
    acceptor: &A,
    weight: usize,
    budget: Option<&ConstraintBudget>,
) -> Result<Basis, BasisBuildError> {
    let words = enumerate_words_with_acceptor(alpha.letters.len(), acceptor, weight, budget)
        .map_err(|err| BasisBuildError {
            err,
            stats: BasisStats::default(),
        })?;
    let ncols = words.len();
    let letters = normalized_letters(alpha);
    let vars = collect_vars_from_letters(&letters);
    let mut stats = BasisStats {
        ncols,
        vars_count: vars.len(),
        ..Default::default()
    };

    if ncols == 0 {
        stats.dim = 0;
        stats.rank = 0;
        return Ok(Basis {
            words,
            vectors: Vec::new(),
            free_cols: Vec::new(),
            stats,
        });
    }

    if weight < 2 || vars.len() < 2 {
        let free_cols: Vec<usize> = (0..ncols).collect();
        let vectors = identity_basis(ncols);
        stats.dim = vectors.len();
        stats.rank = 0;
        return Ok(Basis {
            words,
            vectors,
            free_cols,
            stats,
        });
    }

    let envs = build_envs(&vars);
    let cache = DlogCache::new(&letters, &vars, &envs).map_err(|err| BasisBuildError {
        err,
        stats: stats.clone(),
    })?;
    let mut pivot_rows: BTreeMap<usize, SparseRow> = BTreeMap::new();
    stats.envs_total = envs.len();

    for k in 0..(weight - 1) {
        let contexts = build_contexts(&words, k);
        for (_context, cols) in contexts {
            for vi in 0..vars.len() {
                for vj in (vi + 1)..vars.len() {
                    let mut valid = 0;
                    for env_idx in 0..envs.len() {
                        stats.rows_attempted += 1;
                        let mut row = SparseRow::new();
                        let mut invalid = false;
                        for &col in &cols {
                            let word = &words[col];
                            let a = word[k];
                            let b = word[k + 1];
                            let wedge =
                                wedge_from_cache_stats(&cache, env_idx, a, b, vi, vj, &mut stats);
                            let wedge = match wedge {
                                Some(value) => value,
                                None => {
                                    invalid = true;
                                    break;
                                }
                            };
                            if !wedge.is_zero() {
                                row.insert(col, wedge);
                            }
                        }

                        if invalid {
                            stats.rows_skipped_singular += 1;
                            continue;
                        }
                        valid += 1;
                        stats.samples_used += 1;
                        if !row.is_empty() {
                            if let Some(nnz) = insert_row(&mut pivot_rows, row) {
                                stats.rows_inserted += 1;
                                stats.sum_row_nnz += nnz;
                                if nnz > stats.max_row_nnz {
                                    stats.max_row_nnz = nnz;
                                }
                            }
                        }
                    }

                    if valid < 2 {
                        stats.constraints_insufficient_samples += 1;
                        let _ = stats.constraints_insufficient_samples;
                        return Err(BasisBuildError {
                            err: SymbolError::InsufficientSamples,
                            stats,
                        });
                    }
                }
            }
        }
    }

    let pivot_cols: Vec<usize> = pivot_rows.keys().copied().collect();
    let free_cols = compute_free_cols(ncols, &pivot_cols);
    let vectors = build_nullspace_vectors(ncols, &pivot_rows, &free_cols);
    stats.rank = pivot_rows.len();
    stats.dim = vectors.len();

    Ok(Basis {
        words,
        vectors,
        free_cols,
        stats,
    })
}

pub fn reduce_to_basis(
    sym: &Symbol,
    basis: &Basis,
    alpha: &Alphabet,
) -> Result<(Vec<Coeff>, Symbol), SymbolError> {
    let weight = basis.words.first().map(|word| word.len()).unwrap_or(0);
    if basis.words.iter().any(|word| word.len() != weight) {
        return Err(SymbolError::NotImplemented(
            "basis has inconsistent word weights".to_string(),
        ));
    }

    let ncols = basis.words.len();
    let free_cols = if basis.free_cols.is_empty() && !basis.vectors.is_empty() {
        return Err(SymbolError::NotImplemented(
            "basis metadata missing free columns".to_string(),
        ));
    } else {
        basis.free_cols.clone()
    };

    let word_to_col = build_word_index(&basis.words);
    let letter_map = alphabet_letter_index_map(alpha);

    let mut vec = vec![Coeff::zero(); ncols];
    let mut residual = Symbol::zero();

    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        if word.letters().len() != weight {
            residual.add_term(word.clone(), *coeff);
            continue;
        }
        let ids = word_to_ids(word, &letter_map)?;
        let col = match word_to_col.get(&ids) {
            Some(index) => *index,
            None => {
                return Err(SymbolError::NotImplemented(
                    "symbol contains word not in basis".to_string(),
                ))
            }
        };
        vec[col] += *coeff;
    }

    let mut coeffs = Vec::with_capacity(free_cols.len());
    for &col in &free_cols {
        coeffs.push(vec[col]);
    }

    let recon = reconstruct_from_basis(ncols, &basis.vectors, &coeffs);

    for col in 0..ncols {
        let diff = vec[col] - recon[col];
        if diff.is_zero() {
            continue;
        }
        let word = ids_to_word(&basis.words[col], alpha)?;
        residual.add_term(word, diff);
    }

    Ok((coeffs, residual))
}

pub fn check_integrable_n(sym: &Symbol) -> Result<bool, SymbolError> {
    let mut terms_by_weight: BTreeMap<usize, Vec<TermKey>> = BTreeMap::new();
    let mut letters_by_weight: BTreeMap<usize, BTreeMap<String, Expr>> = BTreeMap::new();

    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let weight = word.letters().len();
        let entry = terms_by_weight.entry(weight).or_default();
        let mut keys = Vec::with_capacity(weight);
        for letter in word.letters() {
            let normalized = letter.normalize();
            let key = normalized.to_canonical_string();
            letters_by_weight
                .entry(weight)
                .or_default()
                .entry(key.clone())
                .or_insert(normalized);
            keys.push(key);
        }
        entry.push(TermKey {
            letters: keys,
            coeff: *coeff,
        });
    }

    for (weight, terms) in terms_by_weight {
        if weight <= 1 {
            continue;
        }

        let letters_map = letters_by_weight.remove(&weight).unwrap_or_default();
        let (letters, letter_index) = build_letter_index(letters_map);
        let term_ids = map_terms_to_ids(&terms, &letter_index)?;

        if term_ids.is_empty() {
            continue;
        }

        let vars = collect_vars_from_letters(&letters);
        if vars.len() < 2 {
            continue;
        }
        let envs = build_envs(&vars);
        let cache = DlogCache::new(&letters, &vars, &envs)?;

        for k in 0..(weight - 1) {
            let contexts = build_contexts_terms(&term_ids, weight, k);
            for (_context, entries) in contexts {
                for vi in 0..vars.len() {
                    for vj in (vi + 1)..vars.len() {
                        let mut valid = 0;
                        for env_idx in 0..envs.len() {
                            let mut invalid = false;
                            let mut total = Coeff::zero();
                            for entry in &entries {
                                let wedge =
                                    wedge_from_cache(&cache, env_idx, entry.a, entry.b, vi, vj);
                                let wedge = match wedge {
                                    Some(value) => value,
                                    None => {
                                        invalid = true;
                                        break;
                                    }
                                };
                                total += entry.coeff * wedge;
                            }
                            if invalid {
                                continue;
                            }
                            valid += 1;
                            if !total.is_zero() {
                                return Ok(false);
                            }
                        }
                        if valid < 2 {
                            return Err(SymbolError::InsufficientSamples);
                        }
                    }
                }
            }
        }
    }

    Ok(true)
}

type SparseRow = BTreeMap<usize, Coeff>;

#[derive(Clone, Debug)]
struct TermKey {
    letters: Vec<String>,
    coeff: Coeff,
}

#[derive(Clone, Debug)]
struct TermIds {
    ids: Vec<usize>,
    coeff: Coeff,
}

#[derive(Clone, Debug)]
struct TermEntry {
    a: usize,
    b: usize,
    coeff: Coeff,
}

fn validate_constraints(
    alpha: &Alphabet,
    constraints: &WordConstraints,
) -> Result<(), SymbolError> {
    let size = alpha.letters.len();
    if let Some(first) = &constraints.first_allowed {
        if first.iter().any(|&idx| idx >= size) {
            return Err(SymbolError::NotImplemented(
                "first-allowed constraint out of range".to_string(),
            ));
        }
    }
    if let Some(pairs) = &constraints.allowed_pairs {
        if pairs.len() != size || pairs.iter().any(|row| row.len() != size) {
            return Err(SymbolError::NotImplemented(
                "allowed-pairs adjacency matrix mismatch".to_string(),
            ));
        }
    }
    Ok(())
}

fn normalized_letters(alpha: &Alphabet) -> Vec<Expr> {
    alpha.letters.iter().map(|expr| expr.normalize()).collect()
}

fn collect_vars_from_letters(letters: &[Expr]) -> Vec<String> {
    let mut vars = BTreeSet::new();
    for letter in letters {
        collect_vars(letter, &mut vars);
    }
    vars.into_iter().collect()
}

fn build_letter_index(letters_map: BTreeMap<String, Expr>) -> (Vec<Expr>, BTreeMap<String, usize>) {
    let mut letters = Vec::with_capacity(letters_map.len());
    let mut index = BTreeMap::new();
    for (idx, (key, expr)) in letters_map.into_iter().enumerate() {
        letters.push(expr);
        index.insert(key, idx);
    }
    (letters, index)
}

fn map_terms_to_ids(
    terms: &[TermKey],
    letter_index: &BTreeMap<String, usize>,
) -> Result<Vec<TermIds>, SymbolError> {
    let mut term_ids = Vec::with_capacity(terms.len());
    for term in terms {
        let mut ids = Vec::with_capacity(term.letters.len());
        for key in &term.letters {
            let idx = match letter_index.get(key) {
                Some(idx) => *idx,
                None => {
                    return Err(SymbolError::NotImplemented(
                        "symbol contains unknown letter".to_string(),
                    ))
                }
            };
            ids.push(idx);
        }
        term_ids.push(TermIds {
            ids,
            coeff: term.coeff,
        });
    }
    Ok(term_ids)
}

struct AcceptorGraph<S> {
    states: Vec<S>,
    transitions: Vec<Vec<Option<usize>>>,
    start_state: usize,
}

pub fn count_words_with_acceptor<A: WordAcceptor>(
    alpha_len: usize,
    acceptor: &A,
    weight: usize,
    budget: Option<&ConstraintBudget>,
) -> Result<u64, SymbolError> {
    let graph = build_acceptor_graph(acceptor, alpha_len, weight, budget)?;
    let max_words = budget.and_then(|value| value.max_words);
    count_words_with_graph(&graph, acceptor, weight, max_words)
}

fn enumerate_words_with_acceptor<A: WordAcceptor>(
    alpha_len: usize,
    acceptor: &A,
    weight: usize,
    budget: Option<&ConstraintBudget>,
) -> Result<Vec<Vec<usize>>, SymbolError> {
    let graph = build_acceptor_graph(acceptor, alpha_len, weight, budget)?;
    let max_words = budget.and_then(|value| value.max_words);
    let _ = count_words_with_graph(&graph, acceptor, weight, max_words)?;
    let mut out = Vec::new();
    let mut current = Vec::with_capacity(weight);
    enumerate_words_rec(
        &graph,
        acceptor,
        weight,
        0,
        graph.start_state,
        &mut current,
        &mut out,
    );
    Ok(out)
}

fn enumerate_words_rec<A: WordAcceptor>(
    graph: &AcceptorGraph<A::State>,
    acceptor: &A,
    weight: usize,
    depth: usize,
    state_id: usize,
    current: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if depth == weight {
        if acceptor.is_accepting(&graph.states[state_id], depth) {
            out.push(current.clone());
        }
        return;
    }

    for (next_letter, next_state) in graph.transitions[state_id].iter().enumerate() {
        if let Some(next_id) = next_state {
            current.push(next_letter);
            enumerate_words_rec(graph, acceptor, weight, depth + 1, *next_id, current, out);
            current.pop();
        }
    }
}

fn build_acceptor_graph<A: WordAcceptor>(
    acceptor: &A,
    alpha_len: usize,
    weight: usize,
    budget: Option<&ConstraintBudget>,
) -> Result<AcceptorGraph<A::State>, SymbolError> {
    let max_states = budget.and_then(|value| value.max_states);
    let max_transitions = budget.and_then(|value| value.max_transitions);

    let start_state = acceptor.start();
    let mut seen: BTreeSet<A::State> = BTreeSet::new();
    let mut frontier: BTreeSet<A::State> = BTreeSet::new();
    seen.insert(start_state.clone());
    frontier.insert(start_state.clone());

    if let Some(limit) = max_states {
        if seen.len() > limit {
            return Err(SymbolError::ConstraintBudgetExceeded(
                ConstraintBudgetKind::States,
            ));
        }
    }

    for _depth in 0..weight {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = BTreeSet::new();
        for state in &frontier {
            for next in 0..alpha_len {
                if let Some(next_state) = acceptor.step(state, next) {
                    if seen.contains(&next_state) {
                        continue;
                    }
                    seen.insert(next_state.clone());
                    if let Some(limit) = max_states {
                        if seen.len() > limit {
                            return Err(SymbolError::ConstraintBudgetExceeded(
                                ConstraintBudgetKind::States,
                            ));
                        }
                    }
                    next_frontier.insert(next_state);
                }
            }
        }
        frontier = next_frontier;
    }

    let mut state_ids = BTreeMap::new();
    let mut states = Vec::with_capacity(seen.len());
    for (idx, state) in seen.into_iter().enumerate() {
        state_ids.insert(state.clone(), idx);
        states.push(state);
    }
    let start_state = match state_ids.get(&start_state) {
        Some(id) => *id,
        None => {
            return Err(SymbolError::NotImplemented(
                "acceptor start state missing".to_string(),
            ))
        }
    };

    let mut transitions = Vec::with_capacity(states.len());
    let mut transition_count: usize = 0;
    for state in &states {
        let mut row = Vec::with_capacity(alpha_len);
        for next in 0..alpha_len {
            let next_state = acceptor.step(state, next);
            if let Some(next_state) = next_state {
                if let Some(next_id) = state_ids.get(&next_state) {
                    row.push(Some(*next_id));
                    transition_count += 1;
                    if let Some(limit) = max_transitions {
                        if transition_count > limit {
                            return Err(SymbolError::ConstraintBudgetExceeded(
                                ConstraintBudgetKind::Transitions,
                            ));
                        }
                    }
                } else {
                    row.push(None);
                }
            } else {
                row.push(None);
            }
        }
        transitions.push(row);
    }

    Ok(AcceptorGraph {
        states,
        transitions,
        start_state,
    })
}

fn count_words_with_graph<A: WordAcceptor>(
    graph: &AcceptorGraph<A::State>,
    acceptor: &A,
    weight: usize,
    max_words: Option<u64>,
) -> Result<u64, SymbolError> {
    if graph.states.is_empty() {
        return Ok(0);
    }

    if weight == 0 {
        let start = &graph.states[graph.start_state];
        return Ok(if acceptor.is_accepting(start, 0) {
            1
        } else {
            0
        });
    }

    let n_states = graph.states.len();
    let mut current = vec![0u64; n_states];
    current[graph.start_state] = 1;

    for _depth in 0..weight {
        let mut next = vec![0u64; n_states];
        for (state_id, count) in current.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            for next_id in graph.transitions[state_id].iter().flatten() {
                let updated = next[*next_id].saturating_add(*count);
                next[*next_id] = updated;
            }
        }
        current = next;
    }

    let mut total = 0u64;
    for (state_id, count) in current.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        if !acceptor.is_accepting(&graph.states[state_id], weight) {
            continue;
        }
        total = total.saturating_add(*count);
        if let Some(limit) = max_words {
            if total > limit {
                return Err(SymbolError::ConstraintBudgetExceeded(
                    ConstraintBudgetKind::Words,
                ));
            }
        }
    }

    Ok(total)
}

fn build_contexts(words: &[Vec<usize>], k: usize) -> BTreeMap<Vec<usize>, Vec<usize>> {
    let mut contexts: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
    for (col, word) in words.iter().enumerate() {
        let mut context = Vec::with_capacity(word.len().saturating_sub(2));
        for (idx, &letter) in word.iter().enumerate() {
            if idx != k && idx != k + 1 {
                context.push(letter);
            }
        }
        contexts.entry(context).or_default().push(col);
    }
    contexts
}

fn build_contexts_terms(
    terms: &[TermIds],
    weight: usize,
    k: usize,
) -> BTreeMap<Vec<usize>, Vec<TermEntry>> {
    let mut contexts: BTreeMap<Vec<usize>, Vec<TermEntry>> = BTreeMap::new();
    for term in terms {
        let ids = &term.ids;
        if ids.len() != weight {
            continue;
        }
        let mut context = Vec::with_capacity(weight.saturating_sub(2));
        for (idx, &letter) in ids.iter().enumerate() {
            if idx != k && idx != k + 1 {
                context.push(letter);
            }
        }
        contexts.entry(context).or_default().push(TermEntry {
            a: ids[k],
            b: ids[k + 1],
            coeff: term.coeff,
        });
    }
    contexts
}

fn wedge_from_cache(
    cache: &DlogCache,
    env_idx: usize,
    a: usize,
    b: usize,
    vi: usize,
    vj: usize,
) -> Option<Coeff> {
    let a_vi = cache.get(env_idx, a, vi)?;
    let b_vj = cache.get(env_idx, b, vj)?;
    let a_vj = cache.get(env_idx, a, vj)?;
    let b_vi = cache.get(env_idx, b, vi)?;
    Some(a_vi * b_vj - a_vj * b_vi)
}

fn wedge_from_cache_stats(
    cache: &DlogCache,
    env_idx: usize,
    a: usize,
    b: usize,
    vi: usize,
    vj: usize,
    stats: &mut BasisStats,
) -> Option<Coeff> {
    let a_vi = cached_dlog_value(cache, env_idx, a, vi, stats)?;
    let b_vj = cached_dlog_value(cache, env_idx, b, vj, stats)?;
    let a_vj = cached_dlog_value(cache, env_idx, a, vj, stats)?;
    let b_vi = cached_dlog_value(cache, env_idx, b, vi, stats)?;
    stats.wedge_cache_hits += 1;
    Some(a_vi * b_vj - a_vj * b_vi)
}

fn cached_dlog_value(
    cache: &DlogCache,
    env_idx: usize,
    letter_idx: usize,
    var_idx: usize,
    stats: &mut BasisStats,
) -> Option<Coeff> {
    match cache.get(env_idx, letter_idx, var_idx) {
        Some(value) => {
            stats.dlog_cache_hits += 1;
            Some(value)
        }
        None => {
            stats.dlog_cache_misses += 1;
            stats.wedge_cache_misses += 1;
            None
        }
    }
}

fn insert_row(pivot_rows: &mut BTreeMap<usize, SparseRow>, mut row: SparseRow) -> Option<usize> {
    loop {
        let pivot_col = row.keys().next().copied()?;

        if let Some(existing) = pivot_rows.get(&pivot_col) {
            let factor = *row.get(&pivot_col).unwrap();
            let existing = existing.clone();
            add_scaled_row(&mut row, factor, &existing);
            continue;
        }

        let pivot = row[&pivot_col];
        if !pivot.is_zero() {
            let inv = Coeff::one() / pivot;
            scale_row(&mut row, inv);
        }

        pivot_rows.insert(pivot_col, row);
        return Some(pivot_rows[&pivot_col].len());
    }
}

fn add_scaled_row(row: &mut SparseRow, factor: Coeff, other: &SparseRow) {
    if factor.is_zero() {
        return;
    }
    for (col, value) in other {
        let updated = row.get(col).copied().unwrap_or_else(Coeff::zero) - factor * *value;
        if updated.is_zero() {
            row.remove(col);
        } else {
            row.insert(*col, updated);
        }
    }
}

fn scale_row(row: &mut SparseRow, factor: Coeff) {
    if factor.is_one() {
        return;
    }
    let keys: Vec<usize> = row.keys().copied().collect();
    for col in keys {
        let updated = row[&col] * factor;
        if updated.is_zero() {
            row.remove(&col);
        } else {
            row.insert(col, updated);
        }
    }
}

fn compute_free_cols(ncols: usize, pivot_cols: &[usize]) -> Vec<usize> {
    let mut free_cols = Vec::new();
    let mut pivot_iter = pivot_cols.iter().copied().peekable();
    for col in 0..ncols {
        if pivot_iter.peek() == Some(&col) {
            pivot_iter.next();
        } else {
            free_cols.push(col);
        }
    }
    free_cols
}

fn build_nullspace_vectors(
    ncols: usize,
    pivot_rows: &BTreeMap<usize, SparseRow>,
    free_cols: &[usize],
) -> Vec<Vec<Coeff>> {
    let mut vectors = Vec::with_capacity(free_cols.len());
    let pivot_cols: Vec<usize> = pivot_rows.keys().copied().collect();
    let mut pivot_cols_desc = pivot_cols.clone();
    pivot_cols_desc.reverse();
    for &free in free_cols {
        let mut vec = vec![Coeff::zero(); ncols];
        if free < ncols {
            vec[free] = Coeff::one();
        }
        for pivot in &pivot_cols_desc {
            let row = &pivot_rows[pivot];
            let mut sum = Coeff::zero();
            for (col, value) in row {
                if *col == *pivot {
                    continue;
                }
                sum += *value * vec[*col];
            }
            vec[*pivot] = -sum;
        }
        vectors.push(vec);
    }
    vectors
}

fn identity_basis(ncols: usize) -> Vec<Vec<Coeff>> {
    let mut vectors = Vec::with_capacity(ncols);
    for col in 0..ncols {
        let mut vec = vec![Coeff::zero(); ncols];
        vec[col] = Coeff::one();
        vectors.push(vec);
    }
    vectors
}

fn alphabet_letter_index_map(alpha: &Alphabet) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for (idx, letter) in alpha.letters.iter().enumerate() {
        let key = letter.normalize().to_canonical_string();
        map.insert(key, idx);
    }
    map
}

fn build_word_index(words: &[Vec<usize>]) -> BTreeMap<Vec<usize>, usize> {
    let mut map = BTreeMap::new();
    for (idx, word) in words.iter().enumerate() {
        map.insert(word.clone(), idx);
    }
    map
}

fn word_to_ids(
    word: &Word,
    letter_map: &BTreeMap<String, usize>,
) -> Result<Vec<usize>, SymbolError> {
    let mut ids = Vec::with_capacity(word.letters().len());
    for letter in word.letters() {
        let key = letter.normalize().to_canonical_string();
        let idx = match letter_map.get(&key) {
            Some(index) => *index,
            None => {
                return Err(SymbolError::NotImplemented(
                    "symbol contains letter not in alphabet".to_string(),
                ))
            }
        };
        ids.push(idx);
    }
    Ok(ids)
}

fn ids_to_word(ids: &[usize], alpha: &Alphabet) -> Result<Word, SymbolError> {
    let mut letters = Vec::with_capacity(ids.len());
    for &id in ids {
        let letter = alpha.letters.get(id).cloned().ok_or_else(|| {
            SymbolError::NotImplemented("basis refers to missing alphabet letter".to_string())
        })?;
        letters.push(letter);
    }
    Ok(Word(letters))
}

fn reconstruct_from_basis(ncols: usize, vectors: &[Vec<Coeff>], coeffs: &[Coeff]) -> Vec<Coeff> {
    let mut out = vec![Coeff::zero(); ncols];
    for (idx, vec) in vectors.iter().enumerate() {
        let coeff = coeffs[idx];
        if coeff.is_zero() {
            continue;
        }
        for col in 0..ncols {
            let add = coeff * vec[col];
            if !add.is_zero() {
                out[col] += add;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests_weight_n;
