use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use mpl_ir::{parse_sexpr, Expr};
use mpl_symbol::space::{
    build_integrable_basis_with_stats, Alphabet, Basis, BasisStats, WordConstraints,
};
use mpl_symbol::SymbolError;
use num_traits::Zero;
use serde::Deserialize;

#[derive(Debug)]
pub enum ExperimentError {
    InvalidConfig(String),
    Symbol(SymbolError),
    Io(std::io::Error),
}

#[derive(Debug, Deserialize)]
struct SpecFile {
    experiment: SpecExperiment,
    alphabet: SpecAlphabet,
    constraints: SpecConstraints,
    pairs: Option<SpecPairs>,
}

#[derive(Debug, Deserialize)]
struct SpecExperiment {
    id: String,
    #[allow(dead_code)]
    title: Option<String>,
    out_dir: String,
    w_min: usize,
    w_max: usize,
}

#[derive(Debug, Deserialize)]
struct SpecAlphabet {
    vars: Vec<String>,
    letters: Vec<SpecLetter>,
}

#[derive(Debug, Deserialize)]
struct SpecLetter {
    name: String,
    expr: String,
}

#[derive(Debug, Deserialize)]
struct SpecConstraints {
    first_entry: Option<Vec<String>>,
    adjacency_mode: Option<String>,
    adjacency_pairs: Option<Vec<[String; 2]>>,
}

#[derive(Debug, Deserialize)]
struct SpecPairs {
    count_mode: Option<String>,
}

impl fmt::Display for ExperimentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
            Self::Symbol(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ExperimentError {}

impl From<SymbolError> for ExperimentError {
    fn from(err: SymbolError) -> Self {
        Self::Symbol(err)
    }
}

impl From<std::io::Error> for ExperimentError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Err,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Err => "err",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    NotImplemented,
    Eval,
    InsufficientSamples,
    FuelExhausted,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotImplemented => "NotImplemented",
            Self::Eval => "Eval",
            Self::InsufficientSamples => "InsufficientSamples",
            Self::FuelExhausted => "FuelExhausted",
        }
    }
}

pub fn load_spec(path: &Path) -> Result<ExperimentConfig, ExperimentError> {
    let content = fs::read_to_string(path)?;
    parse_spec_str(&content)
        .map_err(|err| ExperimentError::InvalidConfig(format!("spec {}: {}", path.display(), err)))
}

pub fn parse_spec_str(input: &str) -> Result<ExperimentConfig, ExperimentError> {
    let spec: SpecFile = toml::from_str(input)
        .map_err(|err| ExperimentError::InvalidConfig(format!("toml parse error: {err}")))?;

    if spec.experiment.w_min > spec.experiment.w_max {
        return Err(ExperimentError::InvalidConfig(
            "w_min must be <= w_max".to_string(),
        ));
    }
    if spec.alphabet.letters.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "alphabet letters must be non-empty".to_string(),
        ));
    }

    if let Some(pairs) = &spec.pairs {
        if let Some(mode) = &pairs.count_mode {
            if mode != "active_word_positions" {
                return Err(ExperimentError::InvalidConfig(format!(
                    "unsupported pairs.count_mode: {mode}"
                )));
            }
        }
    }

    let mut letters = Vec::with_capacity(spec.alphabet.letters.len());
    let mut names = Vec::with_capacity(spec.alphabet.letters.len());
    let mut name_to_idx: BTreeMap<String, usize> = BTreeMap::new();

    for (idx, letter) in spec.alphabet.letters.iter().enumerate() {
        if name_to_idx.insert(letter.name.clone(), idx).is_some() {
            return Err(ExperimentError::InvalidConfig(format!(
                "duplicate letter name: {}",
                letter.name
            )));
        }
        let expr = parse_sexpr(&letter.expr).map_err(|err| {
            ExperimentError::InvalidConfig(format!("letter '{}' parse error: {}", letter.name, err))
        })?;
        letters.push(expr.normalize());
        names.push(letter.name.clone());
    }

    let alphabet = Alphabet {
        name: spec.experiment.id.clone(),
        letters,
        letter_names: names,
    };

    let constraints = build_constraints(&spec.constraints, &name_to_idx)?;

    Ok(ExperimentConfig {
        name: spec.experiment.id,
        out_dir: PathBuf::from(spec.experiment.out_dir),
        alphabet,
        constraints,
        weight_min: spec.experiment.w_min,
        weight_max: spec.experiment.w_max,
        vars: spec.alphabet.vars,
    })
}

#[derive(Clone, Debug)]
pub struct ExperimentConfig {
    pub name: String,
    pub out_dir: PathBuf,
    pub alphabet: Alphabet,
    pub constraints: WordConstraints,
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
}

#[derive(Clone, Debug)]
pub struct WeightSummary {
    pub weight: usize,
    pub stats: BasisStats,
    pub n_words_allowed: usize,
    pub n_active_words: usize,
    pub topology: TopologyMetrics,
    pub status: Status,
    pub error_code: Option<ErrorCode>,
}

#[derive(Clone, Debug)]
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
    let vars = if cfg.vars.is_empty() {
        collect_vars_from_letters(&alphabet.letters)
    } else {
        cfg.vars.clone()
    };

    let mut summaries = Vec::new();
    let mut pairs_total: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    let mut pairs_by_weight: BTreeMap<usize, BTreeMap<(usize, usize), u64>> = BTreeMap::new();
    let alpha_len = alphabet.letters.len();

    for weight in cfg.weight_min..=cfg.weight_max {
        let n_words_allowed = count_allowed_words(alpha_len, &constraints, weight);
        match build_integrable_basis_with_stats(&alphabet, &constraints, weight) {
            Ok(basis) => {
                let stats = basis.stats().clone();
                let active_cols = active_columns(&basis);
                let pair_counts = pair_counts_from_words(&basis.words, &active_cols);
                let topology = compute_topology_metrics(alpha_len, &pair_counts, active_cols.len());
                for ((a, b), count) in &pair_counts {
                    *pairs_total.entry((*a, *b)).or_insert(0) += *count;
                }
                pairs_by_weight.insert(weight, pair_counts);
                summaries.push(WeightSummary {
                    weight,
                    stats,
                    n_words_allowed,
                    n_active_words: active_cols.len(),
                    topology,
                    status: Status::Ok,
                    error_code: None,
                });
            }
            Err(err) => {
                let stats = err.stats;
                let topology = compute_topology_metrics(alpha_len, &BTreeMap::new(), 0);
                let error_code = error_code_from_symbol(&err.err);
                summaries.push(WeightSummary {
                    weight,
                    stats,
                    n_words_allowed,
                    n_active_words: 0,
                    topology,
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
    })
}

pub fn write_outputs(report: &ExperimentReport, out_dir: &Path) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("basis_stats.txt"), render_basis_stats(report))?;
    fs::write(out_dir.join("dim_vs_w.csv"), render_dim_vs_w(report))?;
    fs::write(out_dir.join("pairs.csv"), render_pairs(report))?;
    fs::write(
        out_dir.join("pairs_by_weight.csv"),
        render_pairs_by_weight(report),
    )?;
    fs::write(
        out_dir.join("topology_metrics.csv"),
        render_topology_metrics(report),
    )?;
    Ok(())
}

pub fn render_basis_stats(report: &ExperimentReport) -> String {
    let mut out = String::new();
    for summary in &report.summaries {
        out.push_str(&format!(
            "w={} {}",
            summary.weight,
            summary.stats.one_line()
        ));
        out.push_str(" status=");
        out.push_str(summary.status.as_str());
        if let Some(code) = summary.error_code {
            out.push_str(" error_code=");
            out.push_str(code.as_str());
        }
        out.push('\n');
    }
    out
}

pub fn render_dim_vs_w(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("weight,n_words_allowed,dim,rank,rows_attempted,rows_inserted,samples_used,envs_total,rows_skipped_singular,constraints_insufficient_samples,vars,max_row_nnz,avg_row_nnz,status,error_code,error\n");
    let vars_value = vars_csv(&report.vars);
    let vars_field = escape_csv_field(&vars_value);
    for summary in &report.summaries {
        let stats = &summary.stats;
        let avg_row_nnz = if stats.rows_inserted == 0 {
            0
        } else {
            stats.sum_row_nnz / stats.rows_inserted
        };
        let status_field = summary.status.as_str();
        let error_code = summary.error_code.map(|code| code.as_str()).unwrap_or("");
        let error_code_field = escape_csv_field(error_code);
        let error_field = error_code_field.clone();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            summary.weight,
            summary.n_words_allowed,
            stats.dim,
            stats.rank,
            stats.rows_attempted,
            stats.rows_inserted,
            stats.samples_used,
            stats.envs_total,
            stats.rows_skipped_singular,
            stats.constraints_insufficient_samples,
            &vars_field,
            stats.max_row_nnz,
            avg_row_nnz,
            escape_csv_field(status_field),
            error_code_field,
            error_field
        ));
        out.push('\n');
    }
    out
}

pub fn render_pairs(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("a,b,count\n");

    let names = letter_display_names(&report.alphabet);
    for (&(a, b), count) in &report.pairs_total {
        let left = names.get(a).cloned().unwrap_or_else(|| a.to_string());
        let right = names.get(b).cloned().unwrap_or_else(|| b.to_string());
        out.push_str(&format!(
            "{},{},{}\n",
            escape_csv_field(&left),
            escape_csv_field(&right),
            count
        ));
    }
    out
}

pub fn render_pairs_by_weight(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("weight,a,b,count\n");

    let names = letter_display_names(&report.alphabet);
    for (weight, pairs) in &report.pairs_by_weight {
        for (&(a, b), count) in pairs {
            let left = names.get(a).cloned().unwrap_or_else(|| a.to_string());
            let right = names.get(b).cloned().unwrap_or_else(|| b.to_string());
            out.push_str(&format!(
                "{},{},{},{}\n",
                weight,
                escape_csv_field(&left),
                escape_csv_field(&right),
                count
            ));
        }
    }
    out
}

pub fn render_topology_metrics(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("weight,n_vertices,n_edges,n_active_words,weakly_connected_components,strongly_connected_components,density_num,density_den,max_out_degree,avg_out_degree_num,avg_out_degree_den,status,error_code,error\n");
    for summary in &report.summaries {
        let topo = &summary.topology;
        let status_field = summary.status.as_str();
        let error_code = summary.error_code.map(|code| code.as_str()).unwrap_or("");
        let error_code_field = escape_csv_field(error_code);
        let error_field = error_code_field.clone();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            summary.weight,
            topo.n_vertices,
            topo.n_edges,
            topo.n_active_words,
            topo.weakly_connected_components,
            topo.strongly_connected_components,
            topo.density_num,
            topo.density_den,
            topo.max_out_degree,
            topo.avg_out_degree_num,
            topo.avg_out_degree_den,
            escape_csv_field(status_field),
            error_code_field,
            error_field
        ));
    }
    out
}

pub fn toy_alphabet_xy() -> Alphabet {
    Alphabet {
        name: "toy_xy".to_string(),
        letters: vec![var("x"), var("y")],
        letter_names: vec!["x".to_string(), "y".to_string()],
    }
}

pub fn toy_alphabet_xyz() -> Alphabet {
    Alphabet {
        name: "toy_xyz".to_string(),
        letters: vec![var("x"), var("y"), var("z")],
        letter_names: vec!["x".to_string(), "y".to_string(), "z".to_string()],
    }
}

pub fn alphabet_from_file(path: &Path) -> Result<Alphabet, ExperimentError> {
    let content = fs::read_to_string(path)?;
    let mut letters = Vec::new();
    let mut names = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (name_opt, expr_str) = split_name_expr(trimmed);
        if expr_str.is_empty() {
            return Err(ExperimentError::InvalidConfig(format!(
                "empty expression at line {} in {}",
                idx + 1,
                path.display()
            )));
        }
        let expr = parse_sexpr(expr_str).map_err(|err| {
            ExperimentError::InvalidConfig(format!(
                "alphabet parse error at line {}: {}",
                idx + 1,
                err
            ))
        })?;
        let expr = expr.normalize();
        let name = match name_opt {
            Some(name) if !name.is_empty() => name,
            _ => expr.to_canonical_string(),
        };
        letters.push(expr);
        names.push(name);
    }

    if letters.is_empty() {
        return Err(ExperimentError::InvalidConfig(format!(
            "alphabet file '{}' has no letters",
            path.display()
        )));
    }

    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("alphabet")
        .to_string();

    Ok(Alphabet {
        name,
        letters,
        letter_names: names,
    })
}

fn var(name: &str) -> Expr {
    Expr::Var(name.to_string()).normalize()
}

fn split_name_expr(line: &str) -> (Option<String>, &str) {
    if let Some((name, expr)) = line.split_once('=') {
        return (Some(name.trim().to_string()), expr.trim());
    }
    if let Some((name, expr)) = line.split_once(':') {
        return (Some(name.trim().to_string()), expr.trim());
    }
    (None, line)
}

fn normalize_inputs(
    alphabet: &Alphabet,
    constraints: &WordConstraints,
) -> (Alphabet, WordConstraints) {
    let letters: Vec<Expr> = alphabet.letters.iter().map(|e| e.normalize()).collect();
    let names = if alphabet.letter_names.len() == letters.len() {
        alphabet.letter_names.clone()
    } else {
        letters
            .iter()
            .map(|expr| expr.to_canonical_string())
            .collect()
    };

    (
        Alphabet {
            name: alphabet.name.clone(),
            letters,
            letter_names: names,
        },
        constraints.clone(),
    )
}

fn build_constraints(
    spec: &SpecConstraints,
    name_to_idx: &BTreeMap<String, usize>,
) -> Result<WordConstraints, ExperimentError> {
    let size = name_to_idx.len();
    let first_allowed = match &spec.first_entry {
        Some(entries) => {
            let mut set = BTreeSet::new();
            for name in entries {
                let idx = name_to_idx.get(name).ok_or_else(|| {
                    ExperimentError::InvalidConfig(format!(
                        "first_entry references unknown letter: {name}"
                    ))
                })?;
                set.insert(*idx);
            }
            Some(set)
        }
        None => None,
    };

    let mode = spec.adjacency_mode.as_deref().ok_or_else(|| {
        ExperimentError::InvalidConfig("constraints.adjacency_mode is required".to_string())
    })?;
    let pairs = spec.adjacency_pairs.clone().unwrap_or_default();
    if mode == "allow" && pairs.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "adjacency_mode=allow requires non-empty adjacency_pairs".to_string(),
        ));
    }
    let mut allowed_pairs = match mode {
        "allow" => vec![vec![false; size]; size],
        "forbid" => vec![vec![true; size]; size],
        other => {
            return Err(ExperimentError::InvalidConfig(format!(
                "unknown adjacency_mode: {other}"
            )))
        }
    };

    for pair in pairs {
        let a_name = &pair[0];
        let b_name = &pair[1];
        let a = name_to_idx.get(a_name).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "adjacency_pairs references unknown letter: {a_name}"
            ))
        })?;
        let b = name_to_idx.get(b_name).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "adjacency_pairs references unknown letter: {b_name}"
            ))
        })?;
        allowed_pairs[*a][*b] = mode == "allow";
    }

    Ok(WordConstraints {
        first_allowed,
        allowed_pairs: Some(allowed_pairs),
    })
}

fn validate_constraints(
    alphabet: &Alphabet,
    constraints: &WordConstraints,
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    if let Some(first) = &constraints.first_allowed {
        if first.iter().any(|&idx| idx >= size) {
            return Err(ExperimentError::InvalidConfig(
                "first_allowed constraint out of range".to_string(),
            ));
        }
    }
    if let Some(pairs) = &constraints.allowed_pairs {
        if pairs.len() != size || pairs.iter().any(|row| row.len() != size) {
            return Err(ExperimentError::InvalidConfig(
                "allowed_pairs adjacency matrix mismatch".to_string(),
            ));
        }
    }
    Ok(())
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

fn count_allowed_words(alpha_len: usize, constraints: &WordConstraints, weight: usize) -> usize {
    if weight == 0 {
        return 1;
    }
    if alpha_len == 0 {
        return 0;
    }
    let sentinel = alpha_len;
    let mut memo = vec![vec![None; alpha_len + 1]; weight + 1];

    fn rec(
        alpha_len: usize,
        constraints: &WordConstraints,
        weight: usize,
        pos: usize,
        prev_idx: usize,
        memo: &mut [Vec<Option<usize>>],
    ) -> usize {
        if pos == weight {
            return 1;
        }
        if let Some(value) = memo[pos][prev_idx] {
            return value;
        }
        let prev = if prev_idx == alpha_len {
            None
        } else {
            Some(prev_idx)
        };
        let mut total = 0;
        for next in 0..alpha_len {
            if constraints.allow_step(pos, prev, next) {
                total += rec(alpha_len, constraints, weight, pos + 1, next, memo);
            }
        }
        memo[pos][prev_idx] = Some(total);
        total
    }

    rec(alpha_len, constraints, weight, 0, sentinel, &mut memo)
}

fn compute_topology_metrics(
    n_vertices: usize,
    pair_counts: &BTreeMap<(usize, usize), u64>,
    n_active_words: usize,
) -> TopologyMetrics {
    let mut edges = Vec::new();
    for (&(a, b), _count) in pair_counts {
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

fn letter_display_names(alpha: &Alphabet) -> Vec<String> {
    if alpha.letter_names.len() == alpha.letters.len() {
        return alpha.letter_names.clone();
    }
    alpha
        .letters
        .iter()
        .map(|expr| expr.normalize().to_canonical_string())
        .collect()
}

fn error_code_from_symbol(err: &SymbolError) -> ErrorCode {
    match err {
        SymbolError::NotImplemented(_) => ErrorCode::NotImplemented,
        SymbolError::Eval(_) => ErrorCode::Eval,
        SymbolError::InsufficientSamples => ErrorCode::InsufficientSamples,
        SymbolError::FuelExhausted => ErrorCode::FuelExhausted,
    }
}

fn collect_vars_from_letters(letters: &[Expr]) -> Vec<String> {
    let mut vars = BTreeSet::new();
    for letter in letters {
        collect_vars(letter, &mut vars);
    }
    vars.into_iter().collect()
}

fn collect_vars(expr: &Expr, vars: &mut BTreeSet<String>) {
    match expr {
        Expr::Var(name) => {
            vars.insert(name.clone());
        }
        Expr::Add(children) | Expr::Mul(children) => {
            for child in children {
                collect_vars(child, vars);
            }
        }
        Expr::Neg(inner) => collect_vars(inner, vars),
        Expr::Pow(base, _) => collect_vars(base, vars),
        Expr::Rational(_) => {}
        Expr::Log(_) | Expr::Li2(_) => {}
    }
}

fn vars_csv(vars: &[String]) -> String {
    if vars.is_empty() {
        return String::new();
    }
    vars.join(",")
}

fn escape_csv_field(value: &str) -> String {
    let needs_quotes = value.contains(',') || value.contains('"') || value.contains('\n');
    if !needs_quotes {
        return value.to_string();
    }
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
