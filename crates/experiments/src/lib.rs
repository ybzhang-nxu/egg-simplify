use std::fmt;

use mpl_symbol::SymbolError;

mod analysis;
mod build;
mod ladder_gen;
mod output;
mod pentaladder_gen;
mod run;
mod spec;
mod util;

#[derive(Debug)]
pub enum ExperimentError {
    InvalidConfig(String),
    Symbol(SymbolError),
    Io(std::io::Error),
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
    pub(crate) fn as_str(self) -> &'static str {
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
    NonDeterministicOutput,
}

impl ErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotImplemented => "NotImplemented",
            Self::Eval => "Eval",
            Self::InsufficientSamples => "InsufficientSamples",
            Self::FuelExhausted => "FuelExhausted",
            Self::ConstraintBudgetExceeded => "ConstraintBudgetExceeded",
            Self::NonDeterministicOutput => "NonDeterministicOutput",
        }
    }
}

pub use analysis::cross_loop::{
    express_images_in_lower_space, image_rank, prefix_from_names, run_cross_loop,
    run_cross_loop_scan, CrossLoopOptions, CrossLoopReport, CrossLoopScanOptions,
    CrossLoopScanReport, ImageRankReport, MappingReport, RowFilter, SuffixSpec,
};
pub use analysis::esymb_hankel_subblock::{
    run_esymb_hankel_subblock, EsymbHankelSubblockConfig, HankelDependency, HankelSubblockReport,
    HankelSubblockStats,
};
pub use analysis::esymb_rank_scan::{
    render_esymb_rank_scan_outputs, run_esymb_rank_scan, AlphabetMode, EsymbRankScanConfig,
    EsymbRankScanReport, NormalizeChoice, NormalizeMode, PairsMode, ScreenStatus,
};
pub use analysis::esymb_span_deps::{
    run_esymb_span_deps, CoefSet, EsymbSpanDepsConfig, SpanDepsReport, SpanFamilyFilter,
};
pub use analysis::path1_toy::{run_path1_toy, Path1Mode, Path1ToyConfig, Path1ToyReport};
pub use analysis::skeleton2::Skeleton2Metrics;
pub use build::acceptors::AutomatonAcceptorRef;
pub use build::alphabet::{alphabet_from_file, toy_alphabet_xy, toy_alphabet_xyz};
pub use ladder_gen::{
    ladder_de_down, ladder_marginal_count, ladder_symbol_bruteforce, ladder_symbol_combinatorial,
    run_ladder_gen, LadderFamily, LadderGenConfig, LadderGenReport,
};
pub use output::cross_loop::{
    render_cross_loop_report, render_cross_loop_scan_csv, render_cross_loop_scan_index,
    write_cross_loop_outputs, write_cross_loop_scan_outputs,
};
pub use output::filtration::{
    render_filtration_summary_csv, render_filtration_summary_md, write_filtration_summary,
};
pub use output::single::{
    render_basis_stats, render_count_only, render_dim_vs_w, render_forbidden_pairs,
    render_genealogical_rules, render_pairs, render_pairs_by_weight, render_skeleton2_metrics,
    render_topology_metrics, render_triplets, render_triplets_by_weight, write_count_only,
    write_outputs,
};
pub use pentaladder_gen::{
    pentaladder_alphabet, run_pentaladder_gen, symbol_psi, symbol_psi1_golden, symbol_psi2_blocks,
    symbol_psi2_from_recursion, symbol_psi2_golden, symbol_psi_with_psi2_source,
    symbol_q_blocks_expanded, trace_psi2_origin_details, trace_psi2_origin_report,
    trace_psi2_origin_terms, OriginDetail, OriginKind, OriginTraceReport, OriginTraceTerm,
    PentaladderFamily, PentaladderGenConfig, PentaladderGenReport, Psi2Blocks, Psi2Source,
    QBlocksExpanded,
};
pub use run::count::{run_count_only, CountReport, CountSummary};
pub use run::filtration::{
    run_filtration, FiltrationLayer, FiltrationLayerInfo, FiltrationMode, FiltrationReport,
    FiltrationSpec, FiltrationSummaryRow,
};
pub use run::single::{
    run_experiment, ExperimentConfig, ExperimentReport, GenealogicalKind, GenealogicalReport,
    TopologyMetrics, WeightSummary,
};
pub use spec::m2::{load_spec, parse_spec_str};
pub use spec::m6::{load_filtration_spec, parse_filtration_spec_str};
