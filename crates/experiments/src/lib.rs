use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use mpl_ir::{parse_sexpr, Expr};
use mpl_symbol::space::{
    build_integrable_basis_with_acceptor_with_stats, count_words_with_acceptor, Alphabet, Basis,
    BasisStats, ConstraintBudget, GenealogicalAcceptor, GenealogicalRule, KGramAcceptor, KGramMode,
    WordAcceptor, WordConstraints, WordConstraintsAcceptor,
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
    channel: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpecConstraints {
    first_entry: Option<Vec<String>>,
    adjacency_mode: Option<String>,
    adjacency_pairs: Option<Vec<[String; 2]>>,
    budget: Option<SpecConstraintBudget>,
    automaton: Option<SpecAutomaton>,
}

#[derive(Debug, Deserialize)]
struct SpecConstraintBudget {
    max_states: Option<usize>,
    max_transitions: Option<usize>,
    max_words: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SpecAutomaton {
    acceptors: Option<Vec<SpecAutomatonAcceptor>>,
}

#[derive(Debug, Deserialize)]
struct SpecGenealogicalRule {
    if_seen: String,
    forbid: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
enum SpecAutomatonAcceptor {
    #[serde(rename = "kgram")]
    KGram {
        k: usize,
        mode: String,
        triplets: Vec<[String; 3]>,
    },
    #[serde(rename = "genealogical")]
    Genealogical {
        seen: Option<String>,
        rules: Vec<SpecGenealogicalRule>,
    },
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
    ConstraintBudgetExceeded,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotImplemented => "NotImplemented",
            Self::Eval => "Eval",
            Self::InsufficientSamples => "InsufficientSamples",
            Self::FuelExhausted => "FuelExhausted",
            Self::ConstraintBudgetExceeded => "ConstraintBudgetExceeded",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomatonAcceptorRef {
    KGram(usize),
    Genealogical(usize),
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
    let mut channels: Vec<Option<String>> = Vec::with_capacity(spec.alphabet.letters.len());

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
        channels.push(letter.channel.clone());
    }

    let alphabet = Alphabet {
        name: spec.experiment.id.clone(),
        letters,
        letter_names: names,
    };

    let constraints = build_constraints(&spec.constraints, &name_to_idx)?;
    let constraint_budget = build_budget(&spec.constraints);
    let (genealogical_acceptors, kgram_acceptors, automaton_acceptors) =
        build_automaton_acceptors(&spec.constraints, &name_to_idx, &channels)?;

    Ok(ExperimentConfig {
        name: spec.experiment.id,
        out_dir: PathBuf::from(spec.experiment.out_dir),
        alphabet,
        constraints,
        genealogical_acceptors,
        kgram_acceptors,
        automaton_acceptors,
        constraint_budget,
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
    pub genealogical_acceptors: Vec<GenealogicalAcceptor>,
    pub kgram_acceptors: Vec<KGramAcceptor>,
    pub automaton_acceptors: Vec<AutomatonAcceptorRef>,
    pub constraint_budget: ConstraintBudget,
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
pub struct CountReport {
    pub name: String,
    pub weight_min: usize,
    pub weight_max: usize,
    pub summaries: Vec<CountSummary>,
}

#[derive(Clone, Debug)]
pub struct CountSummary {
    pub weight: usize,
    pub n_words_allowed: usize,
    pub status: Status,
    pub error_code: Option<ErrorCode>,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AutomatonState {
    KGram(<KGramAcceptor as WordAcceptor>::State),
    Genealogical(<GenealogicalAcceptor as WordAcceptor>::State),
}

struct CompositeAcceptor<'a> {
    base: WordConstraintsAcceptor<'a>,
    order: &'a [AutomatonAcceptorRef],
    kgrams: &'a [KGramAcceptor],
    genealogical: &'a [GenealogicalAcceptor],
}

impl<'a> CompositeAcceptor<'a> {
    fn new(
        constraints: &'a WordConstraints,
        order: &'a [AutomatonAcceptorRef],
        kgrams: &'a [KGramAcceptor],
        genealogical: &'a [GenealogicalAcceptor],
    ) -> Self {
        Self {
            base: WordConstraintsAcceptor::new(constraints),
            order,
            kgrams,
            genealogical,
        }
    }
}

impl WordAcceptor for CompositeAcceptor<'_> {
    type State = (Option<usize>, Vec<AutomatonState>);

    fn start(&self) -> Self::State {
        let mut states = Vec::with_capacity(self.order.len());
        for entry in self.order {
            match *entry {
                AutomatonAcceptorRef::KGram(idx) => {
                    let acceptor = match self.kgrams.get(idx) {
                        Some(acceptor) => acceptor,
                        None => return (self.base.start(), Vec::new()),
                    };
                    states.push(AutomatonState::KGram(acceptor.start()));
                }
                AutomatonAcceptorRef::Genealogical(idx) => {
                    let acceptor = match self.genealogical.get(idx) {
                        Some(acceptor) => acceptor,
                        None => return (self.base.start(), Vec::new()),
                    };
                    states.push(AutomatonState::Genealogical(acceptor.start()));
                }
            }
        }
        (self.base.start(), states)
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        let base = self.base.step(&state.0, next)?;
        if state.1.len() != self.order.len() {
            return None;
        }
        let mut states = Vec::with_capacity(self.order.len());
        for (entry, sub_state) in self.order.iter().zip(state.1.iter()) {
            let updated = match (entry, sub_state) {
                (AutomatonAcceptorRef::KGram(idx), AutomatonState::KGram(inner)) => {
                    let acceptor = self.kgrams.get(*idx)?;
                    AutomatonState::KGram(acceptor.step(inner, next)?)
                }
                (AutomatonAcceptorRef::Genealogical(idx), AutomatonState::Genealogical(inner)) => {
                    let acceptor = self.genealogical.get(*idx)?;
                    AutomatonState::Genealogical(acceptor.step(inner, next)?)
                }
                _ => return None,
            };
            states.push(updated);
        }
        Some((base, states))
    }

    fn is_accepting(&self, state: &Self::State, depth: usize) -> bool {
        if !self.base.is_accepting(&state.0, depth) {
            return false;
        }
        if state.1.len() != self.order.len() {
            return false;
        }
        for (entry, sub_state) in self.order.iter().zip(state.1.iter()) {
            let ok = match (entry, sub_state) {
                (AutomatonAcceptorRef::KGram(idx), AutomatonState::KGram(inner)) => {
                    let acceptor = match self.kgrams.get(*idx) {
                        Some(acceptor) => acceptor,
                        None => return false,
                    };
                    acceptor.is_accepting(inner, depth)
                }
                (AutomatonAcceptorRef::Genealogical(idx), AutomatonState::Genealogical(inner)) => {
                    let acceptor = match self.genealogical.get(*idx) {
                        Some(acceptor) => acceptor,
                        None => return false,
                    };
                    acceptor.is_accepting(inner, depth)
                }
                _ => return false,
            };
            if !ok {
                return false;
            }
        }
        true
    }
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
    validate_automaton_order(
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
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
    );
    let budget = cfg.constraint_budget;

    for weight in cfg.weight_min..=cfg.weight_max {
        let n_words_allowed =
            match count_allowed_words_with_acceptor(alpha_len, &acceptor, weight, Some(&budget)) {
                Ok(count) => count,
                Err(err) => {
                    let topology = compute_topology_metrics(alpha_len, &BTreeMap::new(), 0);
                    let error_code = error_code_from_symbol(&err);
                    summaries.push(WeightSummary {
                        weight,
                        stats: BasisStats::default(),
                        n_words_allowed: 0,
                        n_active_words: 0,
                        topology,
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
        triplets_total,
        triplets_by_weight,
    })
}

pub fn run_count_only(cfg: &ExperimentConfig) -> Result<CountReport, ExperimentError> {
    if cfg.weight_min > cfg.weight_max {
        return Err(ExperimentError::InvalidConfig(
            "weight_min must be <= weight_max".to_string(),
        ));
    }

    let (alphabet, constraints) = normalize_inputs(&cfg.alphabet, &cfg.constraints);
    validate_constraints(&alphabet, &constraints)?;
    validate_genealogical_acceptors(&alphabet, &cfg.genealogical_acceptors)?;
    validate_kgram_acceptors(&alphabet, &cfg.kgram_acceptors)?;
    validate_automaton_order(
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
    )?;

    let alpha_len = alphabet.letters.len();
    let acceptor = CompositeAcceptor::new(
        &constraints,
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
    );
    let budget = cfg.constraint_budget;

    let mut summaries = Vec::new();
    for weight in cfg.weight_min..=cfg.weight_max {
        match count_allowed_words_with_acceptor(alpha_len, &acceptor, weight, Some(&budget)) {
            Ok(count) => summaries.push(CountSummary {
                weight,
                n_words_allowed: count,
                status: Status::Ok,
                error_code: None,
            }),
            Err(err) => summaries.push(CountSummary {
                weight,
                n_words_allowed: 0,
                status: Status::Err,
                error_code: Some(error_code_from_symbol(&err)),
            }),
        }
    }

    Ok(CountReport {
        name: cfg.name.clone(),
        weight_min: cfg.weight_min,
        weight_max: cfg.weight_max,
        summaries,
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
    fs::write(out_dir.join("triplets.csv"), render_triplets(report))?;
    fs::write(
        out_dir.join("triplets_by_weight.csv"),
        render_triplets_by_weight(report),
    )?;
    fs::write(
        out_dir.join("topology_metrics.csv"),
        render_topology_metrics(report),
    )?;
    Ok(())
}

pub fn write_count_only(report: &CountReport, out_dir: &Path) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("counts_only.csv"), render_count_only(report))?;
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

pub fn render_count_only(report: &CountReport) -> String {
    let mut out = String::new();
    out.push_str("weight,n_words_allowed,status,error_code,error\n");
    for summary in &report.summaries {
        let status_field = summary.status.as_str();
        let error_code = summary.error_code.map(|code| code.as_str()).unwrap_or("");
        let error_code_field = escape_csv_field(error_code);
        let error_field = error_code_field.clone();
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            summary.weight,
            summary.n_words_allowed,
            escape_csv_field(status_field),
            error_code_field,
            error_field
        ));
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

pub fn render_triplets(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("a,b,c,count\n");

    let names = letter_display_names(&report.alphabet);
    for (&(a, b, c), count) in &report.triplets_total {
        let left = names.get(a).cloned().unwrap_or_else(|| a.to_string());
        let mid = names.get(b).cloned().unwrap_or_else(|| b.to_string());
        let right = names.get(c).cloned().unwrap_or_else(|| c.to_string());
        out.push_str(&format!(
            "{},{},{},{}\n",
            escape_csv_field(&left),
            escape_csv_field(&mid),
            escape_csv_field(&right),
            count
        ));
    }
    out
}

pub fn render_triplets_by_weight(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("weight,a,b,c,count\n");

    let names = letter_display_names(&report.alphabet);
    for (weight, triplets) in &report.triplets_by_weight {
        for (&(a, b, c), count) in triplets {
            let left = names.get(a).cloned().unwrap_or_else(|| a.to_string());
            let mid = names.get(b).cloned().unwrap_or_else(|| b.to_string());
            let right = names.get(c).cloned().unwrap_or_else(|| c.to_string());
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                weight,
                escape_csv_field(&left),
                escape_csv_field(&mid),
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

fn build_budget(spec: &SpecConstraints) -> ConstraintBudget {
    let mut budget = ConstraintBudget::default();
    if let Some(spec_budget) = &spec.budget {
        budget.max_states = spec_budget.max_states;
        budget.max_transitions = spec_budget.max_transitions;
        budget.max_words = spec_budget.max_words;
    }
    budget
}

type AutomatonBuild = (
    Vec<GenealogicalAcceptor>,
    Vec<KGramAcceptor>,
    Vec<AutomatonAcceptorRef>,
);

fn build_automaton_acceptors(
    spec: &SpecConstraints,
    name_to_idx: &BTreeMap<String, usize>,
    letter_channels: &[Option<String>],
) -> Result<AutomatonBuild, ExperimentError> {
    const INVALID_SPEC_MISSING_CHANNEL: &str = "InvalidSpecMissingChannel";
    const INVALID_SPEC_UNKNOWN_MODE: &str = "InvalidSpecUnknownGenealogicalMode";
    const INVALID_SPEC_UNKNOWN_CHANNEL: &str = "InvalidSpecUnknownChannel";
    const INVALID_SPEC_UNKNOWN_LETTER: &str = "InvalidSpecUnknownLetter";
    const INVALID_SPEC_DUPLICATE_RULE: &str = "InvalidSpecDuplicateRule";
    const INVALID_SPEC_DUPLICATE_FORBID: &str = "InvalidSpecDuplicateForbid";
    const INVALID_SPEC_EMPTY_ALLOW_LIST: &str = "InvalidSpecEmptyAllowList";

    let Some(automaton) = &spec.automaton else {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    };

    let mut genealogical = Vec::new();
    let mut kgrams = Vec::new();
    let mut order = Vec::new();
    let mut channel_cache: Option<(BTreeMap<String, usize>, Vec<usize>)> = None;

    let acceptors = automaton.acceptors.as_deref().unwrap_or(&[]);
    for acceptor in acceptors {
        match acceptor {
            SpecAutomatonAcceptor::Genealogical { seen, rules } => {
                let mode = seen.as_deref().unwrap_or("channel");
                let (letter_to_key, key_count, name_map) = match mode {
                    "channel" => {
                        if channel_cache.is_none() {
                            let mut channel_names = BTreeSet::new();
                            for channel in letter_channels {
                                let Some(name) = channel else {
                                    return Err(ExperimentError::InvalidConfig(format!(
                                        "{INVALID_SPEC_MISSING_CHANNEL}: missing channel on letter"
                                    )));
                                };
                                if name.is_empty() {
                                    return Err(ExperimentError::InvalidConfig(format!(
                                        "{INVALID_SPEC_MISSING_CHANNEL}: empty channel on letter"
                                    )));
                                }
                                channel_names.insert(name.clone());
                            }

                            let mut channel_map = BTreeMap::new();
                            for (idx, name) in channel_names.into_iter().enumerate() {
                                channel_map.insert(name, idx);
                            }

                            let mut letter_to_channel = Vec::with_capacity(letter_channels.len());
                            for channel in letter_channels {
                                let Some(name) = channel.as_ref() else {
                                    return Err(ExperimentError::InvalidConfig(format!(
                                        "{INVALID_SPEC_MISSING_CHANNEL}: missing channel on letter"
                                    )));
                                };
                                let idx = channel_map.get(name).ok_or_else(|| {
                                    ExperimentError::InvalidConfig(format!(
                                        "{INVALID_SPEC_UNKNOWN_CHANNEL}: {name}"
                                    ))
                                })?;
                                letter_to_channel.push(*idx);
                            }
                            channel_cache = Some((channel_map, letter_to_channel));
                        }
                        let (channel_map, letter_to_channel) = match channel_cache.as_ref() {
                            Some(value) => value,
                            None => {
                                return Err(ExperimentError::InvalidConfig(format!(
                                    "{INVALID_SPEC_MISSING_CHANNEL}: channel map missing"
                                )))
                            }
                        };
                        (letter_to_channel.clone(), channel_map.len(), channel_map)
                    }
                    "letter" => {
                        let mut letter_to_key = Vec::with_capacity(name_to_idx.len());
                        for idx in 0..name_to_idx.len() {
                            letter_to_key.push(idx);
                        }
                        (letter_to_key, name_to_idx.len(), name_to_idx)
                    }
                    other => {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "{INVALID_SPEC_UNKNOWN_MODE}: {other}"
                        )))
                    }
                };

                if rules.is_empty() {
                    continue;
                }

                let mut mapped_rules = Vec::with_capacity(rules.len());
                let mut seen_rules = BTreeSet::new();
                for rule in rules {
                    let if_seen = name_map.get(&rule.if_seen).copied().ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "{}: {}",
                            if mode == "channel" {
                                INVALID_SPEC_UNKNOWN_CHANNEL
                            } else {
                                INVALID_SPEC_UNKNOWN_LETTER
                            },
                            rule.if_seen
                        ))
                    })?;
                    let mut forbid = Vec::with_capacity(rule.forbid.len());
                    for name in &rule.forbid {
                        let idx = name_map.get(name).copied().ok_or_else(|| {
                            ExperimentError::InvalidConfig(format!(
                                "{}: {}",
                                if mode == "channel" {
                                    INVALID_SPEC_UNKNOWN_CHANNEL
                                } else {
                                    INVALID_SPEC_UNKNOWN_LETTER
                                },
                                name
                            ))
                        })?;
                        forbid.push(idx);
                    }
                    forbid.sort_unstable();
                    for window in forbid.windows(2) {
                        if window[0] == window[1] {
                            return Err(ExperimentError::InvalidConfig(format!(
                                "{INVALID_SPEC_DUPLICATE_FORBID}: {}",
                                rule.if_seen
                            )));
                        }
                    }
                    if !seen_rules.insert((if_seen, forbid.clone())) {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "{INVALID_SPEC_DUPLICATE_RULE}: {}",
                            rule.if_seen
                        )));
                    }
                    mapped_rules.push(GenealogicalRule { if_seen, forbid });
                }

                let acceptor = GenealogicalAcceptor::new(letter_to_key, key_count, mapped_rules)
                    .map_err(|err| {
                        ExperimentError::InvalidConfig(format!("genealogical error: {err}"))
                    })?;
                genealogical.push(acceptor);
                let idx = genealogical.len() - 1;
                order.push(AutomatonAcceptorRef::Genealogical(idx));
            }
            SpecAutomatonAcceptor::KGram { k, mode, triplets } => {
                if *k != 3 {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "kgram acceptor requires k=3 (got {k})"
                    )));
                }
                let mode = match mode.as_str() {
                    "allowed" => KGramMode::Allowed,
                    "forbidden" => KGramMode::Forbidden,
                    other => {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "unknown kgram mode: {other}"
                        )))
                    }
                };
                if mode == KGramMode::Allowed && triplets.is_empty() {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "{INVALID_SPEC_EMPTY_ALLOW_LIST}: kgram mode=allowed requires non-empty triplets"
                    )));
                }
                let mut ids = Vec::with_capacity(triplets.len());
                for triplet in triplets {
                    let a_name = &triplet[0];
                    let b_name = &triplet[1];
                    let c_name = &triplet[2];
                    let a = name_to_idx.get(a_name).ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "kgram triplet references unknown letter: {a_name}"
                        ))
                    })?;
                    let b = name_to_idx.get(b_name).ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "kgram triplet references unknown letter: {b_name}"
                        ))
                    })?;
                    let c = name_to_idx.get(c_name).ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "kgram triplet references unknown letter: {c_name}"
                        ))
                    })?;
                    ids.push([*a, *b, *c]);
                }
                let acceptor = KGramAcceptor::new(mode, ids).map_err(|err| {
                    ExperimentError::InvalidConfig(format!("kgram acceptor error: {err}"))
                })?;
                kgrams.push(acceptor);
                let idx = kgrams.len() - 1;
                order.push(AutomatonAcceptorRef::KGram(idx));
            }
        }
    }
    Ok((genealogical, kgrams, order))
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

fn validate_genealogical_acceptors(
    alphabet: &Alphabet,
    acceptors: &[GenealogicalAcceptor],
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    for acceptor in acceptors {
        if acceptor.letter_count() != size {
            return Err(ExperimentError::InvalidConfig(
                "genealogical acceptor letter mapping mismatch".to_string(),
            ));
        }
        if acceptor.key_count() == 0 && size > 0 {
            return Err(ExperimentError::InvalidConfig(
                "genealogical acceptor has zero keys".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_automaton_order(
    order: &[AutomatonAcceptorRef],
    kgrams: &[KGramAcceptor],
    genealogical: &[GenealogicalAcceptor],
) -> Result<(), ExperimentError> {
    for entry in order {
        match *entry {
            AutomatonAcceptorRef::KGram(idx) => {
                if idx >= kgrams.len() {
                    return Err(ExperimentError::InvalidConfig(
                        "automaton order references missing kgram acceptor".to_string(),
                    ));
                }
            }
            AutomatonAcceptorRef::Genealogical(idx) => {
                if idx >= genealogical.len() {
                    return Err(ExperimentError::InvalidConfig(
                        "automaton order references missing genealogical acceptor".to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_kgram_acceptors(
    alphabet: &Alphabet,
    acceptors: &[KGramAcceptor],
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    for acceptor in acceptors {
        for triplet in acceptor.triplets() {
            if triplet.iter().any(|&idx| idx >= size) {
                return Err(ExperimentError::InvalidConfig(
                    "kgram triplet index out of range".to_string(),
                ));
            }
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

fn count_allowed_words_with_acceptor<A: WordAcceptor>(
    alpha_len: usize,
    acceptor: &A,
    weight: usize,
    budget: Option<&ConstraintBudget>,
) -> Result<usize, SymbolError> {
    let count = count_words_with_acceptor(alpha_len, acceptor, weight, budget)?;
    if count > (usize::MAX as u64) {
        return Err(SymbolError::NotImplemented(
            "word count exceeds usize".to_string(),
        ));
    }
    Ok(count as usize)
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
        SymbolError::ConstraintBudgetExceeded(_) => ErrorCode::ConstraintBudgetExceeded,
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

#[cfg(test)]
mod acceptor_tests {
    use super::*;
    use std::path::PathBuf;

    fn load_l1_a2_spec() -> ExperimentConfig {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("m1")
            .join("L1_A2_cluster.toml");
        load_spec(&path).expect("load L1_A2_cluster.toml")
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
        let topology =
            compute_topology_metrics(cfg.alphabet.letters.len(), &pair_counts, active_cols.len());
        let expected_pairs = report
            .pairs_by_weight
            .get(&weight)
            .cloned()
            .unwrap_or_default();

        assert_eq!(pair_counts, expected_pairs);
        assert_eq!(topology, summary.topology);
    }
}
