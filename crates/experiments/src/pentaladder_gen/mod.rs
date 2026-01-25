use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use mpl_ir::Expr;
use mpl_symbol::space::{check_integrable_n, Alphabet};
use mpl_symbol::{Coeff, Symbol, Word};
use num_traits::{One, Zero};
use serde::Serialize;

use crate::analysis::esymb_rank_scan::observables::{
    render_marginals_matrix_rank_csv, render_marginals_observables_csv, MarginalCollector,
    MarginalCollectorConfig,
};
use crate::ExperimentError;

const LETTER_NAMES: [&str; 9] = [
    "u",
    "v",
    "1-u",
    "1-v",
    "1-w",
    "w",
    "1-uw",
    "1-vw",
    "Delta",
];
const LAST_ENTRY_NAMES: [&str; 5] = ["u", "1-u", "v", "1-v", "1-w"];
const DEFAULT_MAX_TERMS: u64 = 5_000_000;
const DEFAULT_VALIDATE_INTEGRABILITY_MAX_LOOP: usize = 4;
const DEFAULT_PRIMES: [i64; 3] = [1000003, 1000033, 1000037];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PentaladderFamily {
    Prefix,
    Suffix,
    PrefixSuffix,
}

impl PentaladderFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
            Self::PrefixSuffix => "prefix-suffix",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "prefix" => Some(Self::Prefix),
            "suffix" => Some(Self::Suffix),
            "prefix-suffix" => Some(Self::PrefixSuffix),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PentaladderGenConfig {
    pub out_dir: PathBuf,
    pub loops: Vec<usize>,
    pub prefix_len: usize,
    pub suffix_len: usize,
    pub family: PentaladderFamily,
    pub stats_only: bool,
    pub emit_jsonl: bool,
    pub max_terms: u64,
    pub data_dir: Option<PathBuf>,
    pub validate: bool,
    pub validate_integrability_max_loop: usize,
    pub matrix_rank: bool,
}

impl Default for PentaladderGenConfig {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::new(),
            loops: Vec::new(),
            prefix_len: 2,
            suffix_len: 2,
            family: PentaladderFamily::PrefixSuffix,
            stats_only: false,
            emit_jsonl: false,
            max_terms: DEFAULT_MAX_TERMS,
            data_dir: None,
            validate: true,
            validate_integrability_max_loop: DEFAULT_VALIDATE_INTEGRABILITY_MAX_LOOP,
            matrix_rank: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PentaladderGenReport {
    pub out_dir: PathBuf,
    pub loops: Vec<usize>,
    pub data_dir: Option<PathBuf>,
    pub wrote_jsonl: bool,
}

#[derive(Clone, Debug)]
struct AlphabetSpec {
    names: Vec<String>,
    letters: Vec<Expr>,
    map: BTreeMap<String, usize>,
    letter_polys: Vec<LetterPoly>,
}

#[derive(Clone, Debug)]
struct LetterPoly {
    expr: Expr,
    poly: Poly,
}

impl AlphabetSpec {
    fn name_for_expr(&self, expr: &Expr) -> Result<String, ExperimentError> {
        let key = expr.normalize().to_canonical_string();
        let idx = self.map.get(&key).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("unknown alphabet letter: {key}"))
        })?;
        Ok(self.names[*idx].clone())
    }
}

#[derive(Clone, Debug)]
struct PentaContext {
    u: Expr,
    v: Expr,
    w: Expr,
    t: Expr,
    one: Expr,
    zero: Expr,
    one_minus_u: Expr,
    one_minus_v: Expr,
    one_minus_w: Expr,
    uw: Expr,
    vw: Expr,
    one_minus_uw: Expr,
    one_minus_vw: Expr,
    delta: Expr,
}

impl PentaContext {
    fn new() -> Self {
        let u = expr_var("u");
        let v = expr_var("v");
        let w = expr_var("w");
        let t = expr_var("t");
        let one = expr_one();
        let zero = expr_zero();
        let one_minus_u = expr_one_minus(u.clone());
        let one_minus_v = expr_one_minus(v.clone());
        let one_minus_w = expr_one_minus(w.clone());
        let uw = expr_mul(vec![u.clone(), w.clone()]);
        let vw = expr_mul(vec![v.clone(), w.clone()]);
        let one_minus_uw = expr_one_minus(uw.clone());
        let one_minus_vw = expr_one_minus(vw.clone());
        let delta = expr_add(vec![
            one.clone(),
            expr_neg(u.clone()),
            expr_neg(v.clone()),
            expr_mul(vec![u.clone(), v.clone(), w.clone()]),
        ]);
        Self {
            u,
            v,
            w,
            t,
            one,
            zero,
            one_minus_u,
            one_minus_v,
            one_minus_w,
            uw,
            vw,
            one_minus_uw,
            one_minus_vw,
            delta,
        }
    }

    fn alphabet_spec(&self) -> AlphabetSpec {
        let letters = vec![
            self.u.clone(),
            self.v.clone(),
            self.one_minus_u.clone(),
            self.one_minus_v.clone(),
            self.one_minus_w.clone(),
            self.w.clone(),
            self.one_minus_uw.clone(),
            self.one_minus_vw.clone(),
            self.delta.clone(),
        ];
        let names = LETTER_NAMES.iter().map(|name| name.to_string()).collect();
        let mut map = BTreeMap::new();
        for (idx, expr) in letters.iter().enumerate() {
            map.insert(expr.to_canonical_string(), idx);
        }
        let letter_polys = build_letter_polys(&letters);
        AlphabetSpec {
            names,
            letters,
            map,
            letter_polys,
        }
    }
}

pub fn pentaladder_alphabet() -> Alphabet {
    let ctx = PentaContext::new();
    let spec = ctx.alphabet_spec();
    Alphabet::new("He2020PentaLadder".to_string(), spec.letters, spec.names)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Psi2Source {
    #[default]
    Golden,
    Recursive,
}

/// He 2020 penta-ladder recursion: eq. (3.8), (3.9) with base eq. (2.9).
pub fn symbol_psi(loop_value: usize) -> Result<Symbol, ExperimentError> {
    symbol_psi_with_psi2_source(loop_value, Psi2Source::default())
}

pub fn symbol_psi_with_psi2_source(
    loop_value: usize,
    psi2_source: Psi2Source,
) -> Result<Symbol, ExperimentError> {
    let raw = symbol_psi_raw_with_source(loop_value, psi2_source)?;
    let alpha = PentaContext::new().alphabet_spec();
    expand_symbol_to_alphabet(&raw, &alpha)
}

/// He 2020 eq. (2.9) base symbol, expanded to the 9-letter alphabet.
pub fn symbol_psi1_golden() -> Result<Symbol, ExperimentError> {
    let ctx = PentaContext::new();
    let raw = symbol_psi1_raw(&ctx);
    let alpha = ctx.alphabet_spec();
    expand_symbol_to_alphabet(&raw, &alpha)
}

/// He 2020 eq. (3.15) with Appendix A (A.1-A.3), expanded to the 9-letter alphabet.
pub fn symbol_psi2_golden() -> Result<Symbol, ExperimentError> {
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    let raw = symbol_psi2_golden_raw(&ctx);
    expand_symbol_to_alphabet(&raw, &alpha)
}

pub fn symbol_psi2_from_recursion() -> Result<Symbol, ExperimentError> {
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    let raw = symbol_psi2_from_recursion_raw(&ctx, &alpha)?;
    expand_symbol_to_alphabet(&raw, &alpha)
}

#[derive(Clone, Debug)]
pub struct Psi2Blocks {
    pub e1: Symbol,
    pub e2: Symbol,
    pub e3: Symbol,
}

#[derive(Clone, Debug)]
pub struct QBlocksExpanded {
    pub q_uv: Symbol,
    pub q_u_over_v: Symbol,
    pub q_w: Symbol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginKind {
    EndpointUpper,
    EndpointLower,
    LastEntryConst,
    LastEntryLinear,
}

impl OriginKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::EndpointUpper => "endpoint_upper",
            Self::EndpointLower => "endpoint_lower",
            Self::LastEntryConst => "last_entry_const",
            Self::LastEntryLinear => "last_entry_linear",
        }
    }
}

impl fmt::Display for OriginKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct OriginTraceTerm {
    pub word: Word,
    pub coeff: Coeff,
    pub origin: OriginKind,
}

#[derive(Clone, Debug)]
pub struct OriginDetail {
    pub word: Word,
    pub coeff: Coeff,
    pub origin: OriginKind,
    pub stage: String,
    pub kernel: String,
    pub source_last: Option<String>,
    pub normalized_last: Option<String>,
    pub last: Option<String>,
    pub d: Option<String>,
    pub diff: Option<String>,
    pub atoms: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct OriginTraceReport {
    pub raw_terms: usize,
    pub expanded_terms: usize,
    pub terms: Vec<OriginTraceTerm>,
}

pub fn trace_psi2_origin_terms() -> Result<Vec<OriginTraceTerm>, ExperimentError> {
    let report = trace_psi2_origin_report()?;
    Ok(report.terms)
}

pub fn trace_psi2_origin_report() -> Result<OriginTraceReport, ExperimentError> {
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    let base = symbol_psi1_raw(&ctx);
    let half = psi_step_x(&base, &ctx, &alpha)?;
    let mut subst = BTreeMap::new();
    subst.insert("v".to_string(), v_y_expr(&ctx));
    subst.insert("w".to_string(), w_y_expr(&ctx));
    let substituted = substitute_symbol(&half, &subst);
    let raw =
        integrate_symbol_simple_trace(&substituted, &ctx.one, &ctx, &alpha, IntegrationStage::Y)?;
    let terms = expand_trace_terms(&raw, &alpha)?;
    Ok(OriginTraceReport {
        raw_terms: raw.len(),
        expanded_terms: terms.len(),
        terms,
    })
}

pub fn trace_psi2_origin_details(
    targets: &BTreeSet<Word>,
) -> Result<BTreeMap<Word, Vec<OriginDetail>>, ExperimentError> {
    if targets.is_empty() {
        return Ok(BTreeMap::new());
    }
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    let base = symbol_psi1_raw(&ctx);
    let half = psi_step_x(&base, &ctx, &alpha)?;
    let mut subst = BTreeMap::new();
    subst.insert("v".to_string(), v_y_expr(&ctx));
    subst.insert("w".to_string(), w_y_expr(&ctx));
    let substituted = substitute_symbol(&half, &subst);
    let raw =
        integrate_symbol_simple_trace(&substituted, &ctx.one, &ctx, &alpha, IntegrationStage::Y)?;
    expand_trace_details(&raw, &alpha, targets)
}

#[derive(Clone, Debug)]
struct RawTraceTerm {
    word: Vec<Expr>,
    coeff: Coeff,
    origin: OriginKind,
    meta: TraceMeta,
}

#[derive(Clone, Debug)]
struct TraceMeta {
    stage: IntegrationStage,
    kernel: Expr,
    source_last: Option<Expr>,
    normalized_last: Option<Expr>,
    last: Option<Expr>,
    d: Option<Expr>,
    diff: Option<Expr>,
}

fn make_meta(
    stage: IntegrationStage,
    kernel: &Expr,
    source_last: Option<&Expr>,
    normalized_last: Option<&Expr>,
    last: Option<&Expr>,
    d: Option<&Expr>,
    diff: Option<&Expr>,
) -> TraceMeta {
    TraceMeta {
        stage,
        kernel: kernel.clone(),
        source_last: source_last.cloned(),
        normalized_last: normalized_last.cloned(),
        last: last.cloned(),
        d: d.cloned(),
        diff: diff.cloned(),
    }
}

fn integrate_symbol_simple_trace(
    sym: &Symbol,
    kernel_c: &Expr,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
    stage: IntegrationStage,
) -> Result<Vec<RawTraceTerm>, ExperimentError> {
    if expr_contains_var(kernel_c, "t") {
        return Err(ExperimentError::InvalidConfig(
            "kernel c depends on t".to_string(),
        ));
    }
    let mut terms = Vec::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let expanded =
            integrate_word_simple_trace(word.letters(), *coeff, kernel_c, ctx, alpha, stage)?;
        terms.extend(expanded);
    }
    Ok(terms)
}

fn integrate_word_simple_trace(
    word: &[Expr],
    coeff: Coeff,
    kernel_c: &Expr,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
    stage: IntegrationStage,
) -> Result<Vec<RawTraceTerm>, ExperimentError> {
    if coeff.is_zero() {
        return Ok(Vec::new());
    }
    if word.is_empty() {
        return Ok(endpoint_terms_trace(kernel_c, coeff, ctx, stage));
    }
    let source_last = &word[word.len() - 1];
    let normalized_last = normalize_last_entry(source_last, "t");
    let prefix = &word[..word.len() - 1];
    let expansions = dlog_expand_linear(&normalized_last);
    if expansions.is_empty() {
        return Ok(Vec::new());
    }
    let atom_ctx = AtomicIntegrateCtx {
        kernel_c,
        ctx,
        alpha,
        stage,
        source_last,
        normalized_last: &normalized_last,
    };
    let mut out = Vec::new();
    for (last_letter, last_coeff) in expansions {
        let term_coeff = coeff * last_coeff;
        if term_coeff.is_zero() {
            continue;
        }
        let mut rebuilt = prefix.to_vec();
        rebuilt.push(last_letter);
        let mut terms =
            integrate_word_simple_atomic_trace(&rebuilt, term_coeff, &atom_ctx)?;
        out.append(&mut terms);
    }
    Ok(out)
}

struct AtomicIntegrateCtx<'a> {
    kernel_c: &'a Expr,
    ctx: &'a PentaContext,
    alpha: &'a AlphabetSpec,
    stage: IntegrationStage,
    source_last: &'a Expr,
    normalized_last: &'a Expr,
}

fn integrate_word_simple_atomic_trace(
    word: &[Expr],
    coeff: Coeff,
    args: &AtomicIntegrateCtx<'_>,
) -> Result<Vec<RawTraceTerm>, ExperimentError> {
    if word.is_empty() {
        return Ok(endpoint_terms_trace(
            args.kernel_c,
            coeff,
            args.ctx,
            args.stage,
        ));
    }
    let mut out = Vec::new();
    let last = word[word.len() - 1].clone();
    let prefix = &word[..word.len() - 1];

    out.extend(endpoint_contrib_trace(prefix, &last, coeff, args)?);

    let last_simplified = simplify_expr(&last);
    if let Ok(Some(linear)) = as_linear(&last_simplified, "t") {
        if expr_is_zero(&linear.a) {
            let inner = integrate_word_simple(
                prefix,
                coeff,
                args.kernel_c,
                args.ctx,
                args.alpha,
                args.stage,
            )?;
            for (mut letters, value) in inner {
                letters.push(linear.b.clone());
                out.push(RawTraceTerm {
                    word: letters,
                    coeff: value,
                    origin: OriginKind::LastEntryConst,
                    meta: make_meta(
                        args.stage,
                        args.kernel_c,
                        Some(args.source_last),
                        Some(args.normalized_last),
                        Some(&linear.b),
                        None,
                        None,
                    ),
                });
            }
            return Ok(out);
        }
    }
    if !expr_contains_var(&last_simplified, "t") {
        let inner = integrate_word_simple(
            prefix,
            coeff,
            args.kernel_c,
            args.ctx,
            args.alpha,
            args.stage,
        )?;
        for (mut letters, value) in inner {
            letters.push(last_simplified.clone());
            out.push(RawTraceTerm {
                word: letters,
                coeff: value,
                origin: OriginKind::LastEntryConst,
                meta: make_meta(
                    args.stage,
                    args.kernel_c,
                    Some(args.source_last),
                    Some(args.normalized_last),
                    Some(&last_simplified),
                    None,
                    None,
                ),
            });
        }
        return Ok(out);
    }

    let d = linear_shift_with_fallback(&last_simplified, "t").ok_or_else(|| {
        let word_str = word
            .iter()
            .map(|expr| expr.to_canonical_string())
            .collect::<Vec<_>>()
            .join(" ");
        ExperimentError::InvalidConfig(format!(
            "last entry not linear in t (stage={}, kernel={}, word=[{}], source_last={}, normalized_last={}): {}",
            args.stage.as_str(),
            args.kernel_c.to_canonical_string(),
            word_str,
            args.source_last.to_canonical_string(),
            args.normalized_last.to_canonical_string(),
            last.to_canonical_string()
        ))
    })?;
    let diff =
        shift_difference_expr(args.kernel_c, &d, args.alpha, args.stage).map_err(|err| {
        let word_str = word
            .iter()
            .map(|expr| expr.to_canonical_string())
            .collect::<Vec<_>>()
            .join(" ");
        ExperimentError::InvalidConfig(format!(
            "unexpected shift diff for d={} from last={} (stage={}, kernel={}, word=[{}]): {err}",
            d.to_canonical_string(),
            last.to_canonical_string(),
            args.stage.as_str(),
            args.kernel_c.to_canonical_string(),
            word_str
        ))
    })?;
    let Some(append) = diff else {
        return Ok(out);
    };
    let inner = integrate_word_ratio(
        prefix,
        coeff,
        args.kernel_c,
        &d,
        args.ctx,
        args.alpha,
        args.stage,
    )?;
    for (mut letters, value) in inner {
        letters.push(append.clone());
        out.push(RawTraceTerm {
            word: letters,
            coeff: value,
            origin: OriginKind::LastEntryLinear,
            meta: make_meta(
                args.stage,
                args.kernel_c,
                Some(args.source_last),
                Some(args.normalized_last),
                Some(&last_simplified),
                Some(&d),
                Some(&append),
            ),
        });
    }
    Ok(out)
}

fn endpoint_terms_trace(
    kernel_c: &Expr,
    coeff: Coeff,
    ctx: &PentaContext,
    stage: IntegrationStage,
) -> Vec<RawTraceTerm> {
    let t_plus_c = expr_add(vec![ctx.t.clone(), kernel_c.clone()]);
    let mut out = Vec::new();
    if let Some(upper) = eval_letter_at_endpoint(&t_plus_c, Endpoint::Infinity, ctx).ok().flatten()
    {
        out.push(RawTraceTerm {
            word: vec![upper],
            coeff,
            origin: OriginKind::EndpointUpper,
            meta: make_meta(stage, kernel_c, None, None, None, None, None),
        });
    }
    if let Some(lower) = eval_letter_at_endpoint(&t_plus_c, Endpoint::Zero, ctx).ok().flatten() {
        out.push(RawTraceTerm {
            word: vec![lower],
            coeff: -coeff,
            origin: OriginKind::EndpointLower,
            meta: make_meta(stage, kernel_c, None, None, None, None, None),
        });
    }
    out
}

fn endpoint_contrib_trace(
    prefix: &[Expr],
    last: &Expr,
    coeff: Coeff,
    args: &AtomicIntegrateCtx<'_>,
) -> Result<Vec<RawTraceTerm>, ExperimentError> {
    let t_plus_c = expr_add(vec![args.ctx.t.clone(), args.kernel_c.clone()]);
    let mut out = Vec::new();

    if let (Some(prefix_eval), Some(last_eval), Some(kernel_eval)) = (
        eval_word_at_endpoint(prefix, Endpoint::Infinity, args.ctx)?,
        eval_letter_at_endpoint(last, Endpoint::Infinity, args.ctx)?,
        eval_letter_at_endpoint(&t_plus_c, Endpoint::Infinity, args.ctx)?,
    ) {
        let mut word = prefix_eval;
        word.push(last_eval);
        word.push(kernel_eval);
        out.push(RawTraceTerm {
            word,
            coeff,
            origin: OriginKind::EndpointUpper,
            meta: make_meta(
                args.stage,
                args.kernel_c,
                Some(args.source_last),
                Some(args.normalized_last),
                Some(last),
                None,
                None,
            ),
        });
    }

    if let (Some(prefix_eval), Some(last_eval), Some(kernel_eval)) = (
        eval_word_at_endpoint(prefix, Endpoint::Zero, args.ctx)?,
        eval_letter_at_endpoint(last, Endpoint::Zero, args.ctx)?,
        eval_letter_at_endpoint(&t_plus_c, Endpoint::Zero, args.ctx)?,
    ) {
        let mut word = prefix_eval;
        word.push(last_eval);
        word.push(kernel_eval);
        out.push(RawTraceTerm {
            word,
            coeff: -coeff,
            origin: OriginKind::EndpointLower,
            meta: make_meta(
                args.stage,
                args.kernel_c,
                Some(args.source_last),
                Some(args.normalized_last),
                Some(last),
                None,
                None,
            ),
        });
    }

    Ok(out)
}

fn expand_trace_terms(
    raw_terms: &[RawTraceTerm],
    alpha: &AlphabetSpec,
) -> Result<Vec<OriginTraceTerm>, ExperimentError> {
    let mut out = Vec::new();
    for term in raw_terms {
        let expanded = expand_word_to_letters(&term.word, alpha)?;
        for (letters, coeff) in expanded {
            let combined = term.coeff * coeff;
            if combined.is_zero() {
                continue;
            }
            out.push(OriginTraceTerm {
                word: Word(letters),
                coeff: combined,
                origin: term.origin,
            });
        }
    }
    Ok(out)
}

fn expand_trace_details(
    raw_terms: &[RawTraceTerm],
    alpha: &AlphabetSpec,
    targets: &BTreeSet<Word>,
) -> Result<BTreeMap<Word, Vec<OriginDetail>>, ExperimentError> {
    let mut out: BTreeMap<Word, Vec<OriginDetail>> = BTreeMap::new();
    for term in raw_terms {
        let expanded = expand_word_to_letters(&term.word, alpha)?;
        for (letters, coeff) in expanded {
            let combined = term.coeff * coeff;
            if combined.is_zero() {
                continue;
            }
            let word = Word(letters);
            if !targets.contains(&word) {
                continue;
            }
            let atoms = term
                .meta
                .normalized_last
                .as_ref()
                .map(format_dlog_atoms)
                .unwrap_or_default();
            let detail = OriginDetail {
                word: word.clone(),
                coeff: combined,
                origin: term.origin,
                stage: term.meta.stage.as_str().to_string(),
                kernel: term.meta.kernel.to_canonical_string(),
                source_last: term
                    .meta
                    .source_last
                    .as_ref()
                    .map(|expr| expr.to_canonical_string()),
                normalized_last: term
                    .meta
                    .normalized_last
                    .as_ref()
                    .map(|expr| expr.to_canonical_string()),
                last: term.meta.last.as_ref().map(|expr| expr.to_canonical_string()),
                d: term.meta.d.as_ref().map(|expr| expr.to_canonical_string()),
                diff: term.meta.diff.as_ref().map(|expr| expr.to_canonical_string()),
                atoms,
            };
            out.entry(word).or_default().push(detail);
        }
    }
    Ok(out)
}

pub fn symbol_psi2_blocks() -> Result<Psi2Blocks, ExperimentError> {
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    let qw = q_w_symbol(&ctx);
    let quv = q_uv_symbol(&ctx);
    let qu_over_v = q_u_over_v_symbol(&ctx);

    let last_e1 = expr_div(
        expr_mul(vec![ctx.u.clone(), ctx.v.clone()]),
        expr_mul(vec![ctx.one_minus_u.clone(), ctx.one_minus_v.clone()]),
    );
    let last_e2 = expr_div(
        expr_mul(vec![ctx.u.clone(), ctx.one_minus_v.clone()]),
        expr_mul(vec![ctx.v.clone(), ctx.one_minus_u.clone()]),
    );
    let last_e3 = ctx.one_minus_w.clone();

    let half = Coeff::new(1, 2);
    let term_e1 = symbol_scale(&symbol_append(&quv, last_e1), half);
    let term_e2 = symbol_scale(&symbol_append(&qu_over_v, last_e2), half);
    let term_e3 = symbol_append(&qw, last_e3);

    let e1 = expand_symbol_to_alphabet(&term_e1, &alpha)?;
    let e2 = expand_symbol_to_alphabet(&term_e2, &alpha)?;
    let e3 = expand_symbol_to_alphabet(&term_e3, &alpha)?;

    Ok(Psi2Blocks { e1, e2, e3 })
}

pub fn symbol_q_blocks_expanded() -> Result<QBlocksExpanded, ExperimentError> {
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    let q_uv = expand_symbol_to_alphabet(&q_uv_symbol(&ctx), &alpha)?;
    let q_u_over_v = expand_symbol_to_alphabet(&q_u_over_v_symbol(&ctx), &alpha)?;
    let q_w = expand_symbol_to_alphabet(&q_w_symbol(&ctx), &alpha)?;
    Ok(QBlocksExpanded {
        q_uv,
        q_u_over_v,
        q_w,
    })
}

/// Drummond-style ESymb generator: marginals + optional JSONL for penta-ladder symbols.
pub fn run_pentaladder_gen(cfg: &PentaladderGenConfig) -> Result<PentaladderGenReport, ExperimentError> {
    if cfg.out_dir.as_os_str().is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "missing out_dir".to_string(),
        ));
    }
    if cfg.loops.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "loops list is empty".to_string(),
        ));
    }
    if cfg.emit_jsonl && cfg.max_terms == 0 {
        return Err(ExperimentError::InvalidConfig(
            "max_terms must be >= 1 when emitting jsonl".to_string(),
        ));
    }
    if cfg.matrix_rank && cfg.family != PentaladderFamily::PrefixSuffix {
        return Err(ExperimentError::InvalidConfig(
            "matrix_rank requires prefix-suffix family".to_string(),
        ));
    }

    let mut loops = cfg.loops.clone();
    loops.sort_unstable();
    loops.dedup();
    if loops.contains(&0) {
        return Err(ExperimentError::InvalidConfig(
            "loops must be >= 1".to_string(),
        ));
    }
    let min_loop = loops[0];
    let min_len = min_loop
        .checked_mul(2)
        .ok_or_else(|| ExperimentError::InvalidConfig("loop * 2 overflow".to_string()))?;
    match cfg.family {
        PentaladderFamily::Prefix => {
            if cfg.prefix_len > min_len {
                return Err(ExperimentError::InvalidConfig(format!(
                    "prefix length {} exceeds min word length {min_len}",
                    cfg.prefix_len
                )));
            }
        }
        PentaladderFamily::Suffix => {
            if cfg.suffix_len > min_len {
                return Err(ExperimentError::InvalidConfig(format!(
                    "suffix length {} exceeds min word length {min_len}",
                    cfg.suffix_len
                )));
            }
        }
        PentaladderFamily::PrefixSuffix => {
            if cfg.prefix_len.saturating_add(cfg.suffix_len) > min_len {
                return Err(ExperimentError::InvalidConfig(format!(
                    "prefix-suffix lengths r={},k={} exceed min word length {min_len}",
                    cfg.prefix_len, cfg.suffix_len
                )));
            }
        }
    }

    fs::create_dir_all(&cfg.out_dir)?;

    let data_dir = if cfg.emit_jsonl {
        let dir = cfg
            .data_dir
            .clone()
            .unwrap_or_else(|| cfg.out_dir.join("converted_jsonl"));
        fs::create_dir_all(&dir)?;
        Some(dir)
    } else {
        None
    };
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();

    let mut loop_index = BTreeMap::new();
    let mut loop_set = BTreeSet::new();
    for (idx, loop_value) in loops.iter().copied().enumerate() {
        loop_index.insert(loop_value, idx);
        loop_set.insert(loop_value);
    }

    let collect_prefix = cfg.family == PentaladderFamily::Prefix;
    let collect_suffix = cfg.family == PentaladderFamily::Suffix;
    let collect_pair = cfg.family == PentaladderFamily::PrefixSuffix;
    let prefix_len = if collect_prefix || collect_pair {
        Some(cfg.prefix_len)
    } else {
        None
    };
    let suffix_len = if collect_suffix || collect_pair {
        Some(cfg.suffix_len)
    } else {
        None
    };
    let mut collector = MarginalCollector::new(MarginalCollectorConfig {
        loops: &loops,
        letters: &alpha.names,
        prefix_len,
        suffix_len,
        collect_prefix,
        collect_suffix,
        collect_pair,
        only_observed: false,
        alphabet_project: false,
    });

    let allowed_last: BTreeSet<String> = LAST_ENTRY_NAMES
        .iter()
        .map(|name| name.to_string())
        .collect();
    let max_loop = *loops.last().unwrap_or(&0);

    let mut current = symbol_psi1_raw(&ctx);
    for loop_value in 1..=max_loop {
        let in_requested = loop_set.contains(&loop_value);
        let need_integrability = cfg.validate && loop_value <= cfg.validate_integrability_max_loop;
        let need_jsonl = cfg.emit_jsonl && in_requested;
        let need_expanded = !cfg.stats_only || need_integrability || need_jsonl;
        let should_process = in_requested || cfg.validate;

        if should_process {
            if need_expanded {
                let expanded = expand_symbol_to_alphabet(&current, &alpha)?;

                if cfg.validate {
                    validate_last_entry_symbol(&expanded, &alpha, &allowed_last, loop_value)?;
                }
                if need_integrability {
                    let ok = check_integrable_n(&expanded)?;
                    if !ok {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "pentaladder symbol not integrable at L={loop_value}"
                        )));
                    }
                }
                if in_requested {
                    let idx = *loop_index.get(&loop_value).ok_or_else(|| {
                        ExperimentError::InvalidConfig("loop index missing".to_string())
                    })?;
                    stream_alphabet_terms(&expanded, &alpha, |word, coeff| {
                        collector.observe_term(idx, loop_value, &word, coeff)
                    })?;
                }
                if need_jsonl {
                    if let Some(dir) = data_dir.as_ref() {
                        let path = dir.join(format!("Esymb_L{loop_value}.jsonl"));
                        write_symbol_jsonl(&path, loop_value, &expanded, &alpha, cfg.max_terms)?;
                    }
                }
            } else {
                let idx_opt = loop_index.get(&loop_value).copied();
                stream_expanded_terms(&current, &alpha, |word, coeff| {
                    if cfg.validate {
                        validate_last_entry_names(&word, &allowed_last, loop_value)?;
                    }
                    if in_requested {
                        let idx = idx_opt.ok_or_else(|| {
                            ExperimentError::InvalidConfig("loop index missing".to_string())
                        })?;
                        collector.observe_term(idx, loop_value, &word, coeff)?;
                    }
                    Ok(())
                })?;
            }
        }

        if loop_value < max_loop {
            let half = psi_step_x(&current, &ctx, &alpha)?;
            current = psi_step_y(&half, &ctx, &alpha)?;
        }
    }

    collector.validate()?;
    let (sequences, values) = collector.sequences_and_values(
        collect_prefix,
        collect_suffix,
        collect_pair,
    );
    fs::write(
        cfg.out_dir.join("marginals_observables.csv"),
        render_marginals_observables_csv(&sequences, &values, &loops),
    )?;

    if cfg.matrix_rank {
        let rows = collector.matrix_rank_rows(&DEFAULT_PRIMES)?;
        fs::write(
            cfg.out_dir.join("marginals_matrix_rank.csv"),
            render_marginals_matrix_rank_csv(&rows),
        )?;
    }

    Ok(PentaladderGenReport {
        out_dir: cfg.out_dir.clone(),
        loops,
        data_dir,
        wrote_jsonl: cfg.emit_jsonl,
    })
}

fn symbol_psi_raw_with_source(
    loop_value: usize,
    psi2_source: Psi2Source,
) -> Result<Symbol, ExperimentError> {
    if loop_value == 0 {
        return Ok(Symbol::zero());
    }
    let ctx = PentaContext::new();
    let alpha = ctx.alphabet_spec();
    if loop_value == 1 {
        return Ok(symbol_psi1_raw(&ctx));
    }
    let base2 = match psi2_source {
        // Anchor L=2 to Appendix A (A.1-A.3) for the exact paper symbol.
        Psi2Source::Golden => symbol_psi2_golden_raw(&ctx),
        Psi2Source::Recursive => symbol_psi2_from_recursion_raw(&ctx, &alpha)?,
    };
    if loop_value == 2 {
        return Ok(base2);
    }
    let mut current = base2;
    let trace = should_trace_timing(loop_value);
    for iter in 3..=loop_value {
        let before_terms = if trace { current.terms().count() } else { 0 };
        let start = if trace { Some(Instant::now()) } else { None };
        let half = psi_step_x(&current, &ctx, &alpha)?;
        if trace {
            let after_terms = half.terms().count();
            log_step_timing(
                loop_value,
                iter,
                "psi_step_x",
                start.expect("start").elapsed(),
                before_terms,
                after_terms,
            );
        }
        let before_terms = if trace { half.terms().count() } else { 0 };
        let start = if trace { Some(Instant::now()) } else { None };
        current = psi_step_y(&half, &ctx, &alpha)?;
        if trace {
            let after_terms = current.terms().count();
            log_step_timing(
                loop_value,
                iter,
                "psi_step_y",
                start.expect("start").elapsed(),
                before_terms,
                after_terms,
            );
        }
    }
    Ok(current)
}

fn should_trace_timing(loop_value: usize) -> bool {
    if std::env::var("PENTALADDER_TRACE_TIMING").is_ok() {
        return true;
    }
    if std::env::var("PENTALADDER_TRACE_PROGRESS").is_ok() {
        return true;
    }
    cfg!(debug_assertions) && loop_value == 3
}

fn log_step_timing(
    loop_value: usize,
    iter: usize,
    step: &str,
    elapsed: std::time::Duration,
    before_terms: usize,
    after_terms: usize,
) {
    eprintln!(
        "[pentaladder] L={loop_value} iter={iter} {step}: {} ms terms {} -> {}",
        elapsed.as_millis(),
        before_terms,
        after_terms
    );
}

fn symbol_psi2_from_recursion_raw(
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
) -> Result<Symbol, ExperimentError> {
    let base = symbol_psi1_raw(ctx);
    let half = psi_step_x(&base, ctx, alpha)?;
    psi_step_y(&half, ctx, alpha)
}

fn symbol_psi2_golden_raw(ctx: &PentaContext) -> Symbol {
    let qw = q_w_symbol(ctx);
    let quv = q_uv_symbol(ctx);
    let qu_over_v = q_u_over_v_symbol(ctx);

    let last_e1 = expr_div(
        expr_mul(vec![ctx.u.clone(), ctx.v.clone()]),
        expr_mul(vec![ctx.one_minus_u.clone(), ctx.one_minus_v.clone()]),
    );
    let last_e2 = expr_div(
        expr_mul(vec![ctx.u.clone(), ctx.one_minus_v.clone()]),
        expr_mul(vec![ctx.v.clone(), ctx.one_minus_u.clone()]),
    );
    let last_e3 = ctx.one_minus_w.clone();

    let half = Coeff::new(1, 2);
    let mut total = Symbol::zero();
    let term_e1 = symbol_scale(&symbol_append(&quv, last_e1), half);
    let term_e2 = symbol_scale(&symbol_append(&qu_over_v, last_e2), half);
    let term_e3 = symbol_append(&qw, last_e3);
    total = symbol_add(&total, &term_e1);
    total = symbol_add(&total, &term_e2);
    total = symbol_add(&total, &term_e3);
    total
}

/// He 2020 eq. (2.9): base chiral pentagon symbol (raw).
fn symbol_psi1_raw(ctx: &PentaContext) -> Symbol {
    let terms = vec![
        (Word(vec![ctx.u.clone(), ctx.v.clone()]), Coeff::one()),
        (Word(vec![ctx.v.clone(), ctx.u.clone()]), Coeff::one()),
        (
            Word(vec![ctx.u.clone(), ctx.one_minus_u.clone()]),
            Coeff::from_integer(-1),
        ),
        (
            Word(vec![ctx.v.clone(), ctx.one_minus_v.clone()]),
            Coeff::from_integer(-1),
        ),
        (
            Word(vec![ctx.w.clone(), ctx.one_minus_w.clone()]),
            Coeff::from_integer(-1),
        ),
        (
            Word(vec![ctx.uw.clone(), ctx.one_minus_uw.clone()]),
            Coeff::one(),
        ),
        (
            Word(vec![ctx.vw.clone(), ctx.one_minus_vw.clone()]),
            Coeff::one(),
        ),
    ];
    Symbol::from_terms(terms)
}

/// He 2020 eq. (3.8): Psi_{L+1/2} recursion with dlog((t+1)/t).
fn psi_step_x(
    sym: &Symbol,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
) -> Result<Symbol, ExperimentError> {
    let mut subst = BTreeMap::new();
    subst.insert("u".to_string(), u_x_expr(ctx));
    subst.insert("w".to_string(), w_x_expr(ctx));
    let substituted = substitute_symbol(sym, &subst);
    integrate_symbol_ratio(&substituted, &ctx.one, &ctx.zero, ctx, alpha)
}

/// He 2020 eq. (3.9): Psi_{L+1} recursion with dlog(t+1).
fn psi_step_y(
    sym: &Symbol,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
) -> Result<Symbol, ExperimentError> {
    let mut subst = BTreeMap::new();
    subst.insert("v".to_string(), v_y_expr(ctx));
    subst.insert("w".to_string(), w_y_expr(ctx));
    let substituted = substitute_symbol(sym, &subst);
    integrate_symbol_simple(&substituted, &ctx.one, ctx, alpha, IntegrationStage::Y)
}

fn u_x_expr(ctx: &PentaContext) -> Expr {
    // u * (t + w) / (t + u w)
    expr_div(
        expr_mul(vec![ctx.u.clone(), expr_add(vec![ctx.t.clone(), ctx.w.clone()])]),
        expr_add(vec![ctx.t.clone(), ctx.uw.clone()]),
    )
}

fn w_x_expr(ctx: &PentaContext) -> Expr {
    // w * (t + 1) / (t + w)
    expr_div(
        expr_mul(vec![ctx.w.clone(), expr_add(vec![ctx.t.clone(), ctx.one.clone()])]),
        expr_add(vec![ctx.t.clone(), ctx.w.clone()]),
    )
}

fn v_y_expr(ctx: &PentaContext) -> Expr {
    // v * (t + 1) / (v t + 1)
    let vt_plus_one = expr_add(vec![expr_mul(vec![ctx.v.clone(), ctx.t.clone()]), ctx.one.clone()]);
    expr_div(
        expr_mul(vec![ctx.v.clone(), expr_add(vec![ctx.t.clone(), ctx.one.clone()])]),
        vt_plus_one,
    )
}

fn w_y_expr(ctx: &PentaContext) -> Expr {
    // (t + w) / (t + 1)
    expr_div(
        expr_add(vec![ctx.t.clone(), ctx.w.clone()]),
        expr_add(vec![ctx.t.clone(), ctx.one.clone()]),
    )
}

#[derive(Clone, Copy, Debug)]
enum IntegrationStage {
    XPlus,
    XMinus,
    Y,
}

impl IntegrationStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::XPlus => "X(+)",
            Self::XMinus => "X(-)",
            Self::Y => "Y",
        }
    }
}

fn integrate_symbol_ratio(
    sym: &Symbol,
    c: &Expr,
    d: &Expr,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
) -> Result<Symbol, ExperimentError> {
    let trace = should_trace_timing(sym.terms().count());
    let before_terms = if trace { sym.terms().count() } else { 0 };
    let start = if trace { Some(Instant::now()) } else { None };
    let plus = integrate_symbol_simple(sym, c, ctx, alpha, IntegrationStage::XPlus)?;
    if trace {
        let elapsed = start.expect("start").elapsed();
        let after_terms = plus.terms().count();
        log_step_timing(
            0,
            0,
            "integrate_symbol_simple XPlus",
            elapsed,
            before_terms,
            after_terms,
        );
    }
    let before_terms = if trace { sym.terms().count() } else { 0 };
    let start = if trace { Some(Instant::now()) } else { None };
    let minus = integrate_symbol_simple(sym, d, ctx, alpha, IntegrationStage::XMinus)?;
    if trace {
        let elapsed = start.expect("start").elapsed();
        let after_terms = minus.terms().count();
        log_step_timing(
            0,
            0,
            "integrate_symbol_simple XMinus",
            elapsed,
            before_terms,
            after_terms,
        );
    }
    Ok(symbol_sub(&plus, &minus))
}

fn integrate_symbol_simple(
    sym: &Symbol,
    kernel_c: &Expr,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
    stage: IntegrationStage,
) -> Result<Symbol, ExperimentError> {
    if expr_contains_var(kernel_c, "t") {
        return Err(ExperimentError::InvalidConfig(
            "kernel c depends on t".to_string(),
        ));
    }
    let trace = should_trace_timing(sym.terms().count());
    let total_terms = if trace { sym.terms().count() } else { 0 };
    let log_every = std::env::var("PENTALADDER_TRACE_PROGRESS_EVERY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(200);
    let mut terms = Vec::new();
    for (idx, (word, coeff)) in sym.terms().enumerate() {
        if coeff.is_zero() {
            continue;
        }
        if trace && idx % log_every == 0 {
            eprintln!(
                "[pentaladder] integrate_symbol_simple {stage}: {idx}/{total_terms}",
                stage = stage.as_str()
            );
        }
        let expanded =
            integrate_word_simple(word.letters(), *coeff, kernel_c, ctx, alpha, stage)?;
        for (letters, value) in expanded {
            if value.is_zero() {
                continue;
            }
            terms.push((Word(letters), value));
        }
    }
    Ok(Symbol::from_terms(terms))
}

fn integrate_word_simple(
    word: &[Expr],
    coeff: Coeff,
    kernel_c: &Expr,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
    stage: IntegrationStage,
) -> Result<Vec<(Vec<Expr>, Coeff)>, ExperimentError> {
    if coeff.is_zero() {
        return Ok(Vec::new());
    }
    if word.is_empty() {
        return endpoint_terms(kernel_c, coeff, ctx);
    }
    let source_last = &word[word.len() - 1];
    let normalized_last = normalize_last_entry(source_last, "t");
    let prefix = &word[..word.len() - 1];
    let expansions = dlog_expand_linear(&normalized_last);
    if expansions.is_empty() {
        return Ok(Vec::new());
    }
    let atom_ctx = AtomicIntegrateCtx {
        kernel_c,
        ctx,
        alpha,
        stage,
        source_last,
        normalized_last: &normalized_last,
    };
    let mut out = Vec::new();
    for (last_letter, last_coeff) in expansions {
        let term_coeff = coeff * last_coeff;
        if term_coeff.is_zero() {
            continue;
        }
        let mut rebuilt = prefix.to_vec();
        rebuilt.push(last_letter);
        let mut terms = integrate_word_simple_atomic(&rebuilt, term_coeff, &atom_ctx)?;
        out.append(&mut terms);
    }
    Ok(out)
}

fn integrate_word_simple_atomic(
    word: &[Expr],
    coeff: Coeff,
    args: &AtomicIntegrateCtx<'_>,
) -> Result<Vec<(Vec<Expr>, Coeff)>, ExperimentError> {
    if word.is_empty() {
        return endpoint_terms(args.kernel_c, coeff, args.ctx);
    }
    let mut out = Vec::new();
    let last = word[word.len() - 1].clone();
    let prefix = &word[..word.len() - 1];

    let mut endpoints = endpoint_contrib(prefix, &last, coeff, args.kernel_c, args.ctx)?;
    out.append(&mut endpoints);

    let last_simplified = simplify_expr(&last);
    if let Ok(Some(linear)) = as_linear(&last_simplified, "t") {
        if expr_is_zero(&linear.a) {
            let inner = integrate_word_simple(
                prefix,
                coeff,
                args.kernel_c,
                args.ctx,
                args.alpha,
                args.stage,
            )?;
            for (mut letters, value) in inner {
                letters.push(linear.b.clone());
                out.push((letters, value));
            }
            return Ok(out);
        }
    }
    if !expr_contains_var(&last_simplified, "t") {
        let inner = integrate_word_simple(
            prefix,
            coeff,
            args.kernel_c,
            args.ctx,
            args.alpha,
            args.stage,
        )?;
        for (mut letters, value) in inner {
            letters.push(last_simplified.clone());
            out.push((letters, value));
        }
        return Ok(out);
    }

    let d = linear_shift_with_fallback(&last_simplified, "t").ok_or_else(|| {
        let word_str = word
            .iter()
            .map(|expr| expr.to_canonical_string())
            .collect::<Vec<_>>()
            .join(" ");
        ExperimentError::InvalidConfig(format!(
            "last entry not linear in t (stage={}, kernel={}, word=[{}], source_last={}, normalized_last={}): {}",
            args.stage.as_str(),
            args.kernel_c.to_canonical_string(),
            word_str,
            args.source_last.to_canonical_string(),
            args.normalized_last.to_canonical_string(),
            last.to_canonical_string()
        ))
    })?;
    let diff =
        shift_difference_expr(args.kernel_c, &d, args.alpha, args.stage).map_err(|err| {
            let word_str = word
                .iter()
                .map(|expr| expr.to_canonical_string())
                .collect::<Vec<_>>()
                .join(" ");
            ExperimentError::InvalidConfig(format!(
                "unexpected shift diff for d={} from last={} (stage={}, kernel={}, word=[{}]): {err}",
                d.to_canonical_string(),
                last.to_canonical_string(),
                args.stage.as_str(),
                args.kernel_c.to_canonical_string(),
                word_str
            ))
        })?;
    let Some(append) = diff else {
        return Ok(out);
    };
    let inner = integrate_word_ratio(
        prefix,
        coeff,
        args.kernel_c,
        &d,
        args.ctx,
        args.alpha,
        args.stage,
    )?;
    for (mut letters, value) in inner {
        letters.push(append.clone());
        out.push((letters, value));
    }
    Ok(out)
}

fn integrate_word_ratio(
    prefix: &[Expr],
    coeff: Coeff,
    c: &Expr,
    d: &Expr,
    ctx: &PentaContext,
    alpha: &AlphabetSpec,
    stage: IntegrationStage,
) -> Result<Vec<(Vec<Expr>, Coeff)>, ExperimentError> {
    let mut terms = integrate_word_simple(prefix, coeff, c, ctx, alpha, stage)?;
    let mut sub = integrate_word_simple(prefix, -coeff, d, ctx, alpha, stage)?;
    terms.append(&mut sub);
    Ok(terms)
}

fn endpoint_terms(
    kernel_c: &Expr,
    coeff: Coeff,
    ctx: &PentaContext,
) -> Result<Vec<(Vec<Expr>, Coeff)>, ExperimentError> {
    let t_plus_c = expr_add(vec![ctx.t.clone(), kernel_c.clone()]);
    let mut out = Vec::new();
    if let Some(upper) = eval_letter_at_endpoint(&t_plus_c, Endpoint::Infinity, ctx)? {
        out.push((vec![upper], coeff));
    }
    if let Some(lower) = eval_letter_at_endpoint(&t_plus_c, Endpoint::Zero, ctx)? {
        out.push((vec![lower], -coeff));
    }
    Ok(out)
}

fn endpoint_contrib(
    prefix: &[Expr],
    last: &Expr,
    coeff: Coeff,
    kernel_c: &Expr,
    ctx: &PentaContext,
) -> Result<Vec<(Vec<Expr>, Coeff)>, ExperimentError> {
    let t_plus_c = expr_add(vec![ctx.t.clone(), kernel_c.clone()]);
    let mut out = Vec::new();

    if let (Some(prefix_eval), Some(last_eval), Some(kernel_eval)) = (
        eval_word_at_endpoint(prefix, Endpoint::Infinity, ctx)?,
        eval_letter_at_endpoint(last, Endpoint::Infinity, ctx)?,
        eval_letter_at_endpoint(&t_plus_c, Endpoint::Infinity, ctx)?,
    ) {
        let mut word = prefix_eval;
        word.push(last_eval);
        word.push(kernel_eval);
        out.push((word, coeff));
    }

    if let (Some(prefix_eval), Some(last_eval), Some(kernel_eval)) = (
        eval_word_at_endpoint(prefix, Endpoint::Zero, ctx)?,
        eval_letter_at_endpoint(last, Endpoint::Zero, ctx)?,
        eval_letter_at_endpoint(&t_plus_c, Endpoint::Zero, ctx)?,
    ) {
        let mut word = prefix_eval;
        word.push(last_eval);
        word.push(kernel_eval);
        out.push((word, -coeff));
    }

    Ok(out)
}

#[derive(Clone, Copy)]
enum Endpoint {
    Zero,
    Infinity,
}

fn eval_word_at_endpoint(
    word: &[Expr],
    endpoint: Endpoint,
    ctx: &PentaContext,
) -> Result<Option<Vec<Expr>>, ExperimentError> {
    let mut out = Vec::with_capacity(word.len());
    for letter in word {
        let Some(eval) = eval_letter_at_endpoint(letter, endpoint, ctx)? else {
            return Ok(None);
        };
        out.push(eval);
    }
    Ok(Some(out))
}

fn eval_letter_at_endpoint(
    expr: &Expr,
    endpoint: Endpoint,
    ctx: &PentaContext,
) -> Result<Option<Expr>, ExperimentError> {
    let evaluated = match endpoint {
        Endpoint::Zero => {
            let mut subst = BTreeMap::new();
            subst.insert("t".to_string(), ctx.zero.clone());
            Some(substitute_expr(expr, &subst))
        }
        Endpoint::Infinity => limit_at_infty(expr, "t")?,
    };
    let Some(value) = evaluated else {
        return Ok(None);
    };
    let normalized = value.normalize();
    if !expr_contains_any_var(&normalized, &["u", "v", "w"]) {
        return Ok(None);
    }
    if expr_is_zero(&normalized) {
        return Ok(None);
    }
    Ok(Some(normalized))
}

fn limit_at_infty(expr: &Expr, var: &str) -> Result<Option<Expr>, ExperimentError> {
    let Some((degree, coeff)) = expr_degree_coeff(expr, var)? else {
        return Ok(None);
    };
    if degree != 0 {
        return Ok(None);
    }
    if expr_contains_var(&coeff, var) {
        return Err(ExperimentError::InvalidConfig(
            "limit at infinity still depends on t".to_string(),
        ));
    }
    Ok(Some(coeff.normalize()))
}

fn expr_degree_coeff(expr: &Expr, var: &str) -> Result<Option<(i32, Expr)>, ExperimentError> {
    match expr {
        Expr::Rational(_) => Ok(Some((0, expr.clone()))),
        Expr::Var(name) => {
            if name == var {
                Ok(Some((1, expr_one())))
            } else {
                Ok(Some((0, expr.clone())))
            }
        }
        Expr::Neg(inner) => {
            let Some((deg, coeff)) = expr_degree_coeff(inner, var)? else {
                return Ok(None);
            };
            Ok(Some((deg, expr_neg(coeff))))
        }
        Expr::Add(_) => {
            let Some(linear) = as_linear(expr, var)? else {
                return Ok(None);
            };
            if expr_is_zero(&linear.a) {
                Ok(Some((0, linear.b)))
            } else {
                Ok(Some((1, linear.a)))
            }
        }
        Expr::Mul(children) => {
            let mut degree = 0i32;
            let mut coeff = expr_one();
            for child in children {
                let Some((deg, child_coeff)) = expr_degree_coeff(child, var)? else {
                    return Ok(None);
                };
                degree = degree
                    .checked_add(deg)
                    .ok_or_else(|| {
                        ExperimentError::InvalidConfig("degree overflow".to_string())
                    })?;
                coeff = expr_mul(vec![coeff, child_coeff]);
            }
            Ok(Some((degree, coeff)))
        }
        Expr::Pow(base, exp) => {
            if *exp == 0 {
                return Ok(Some((0, expr_one())));
            }
            let Some((deg, coeff)) = expr_degree_coeff(base, var)? else {
                return Ok(None);
            };
            let degree = deg
                .checked_mul(*exp)
                .ok_or_else(|| ExperimentError::InvalidConfig("degree overflow".to_string()))?;
            Ok(Some((degree, expr_pow(coeff, *exp))))
        }
        Expr::Log(_) | Expr::Li2(_) => Ok(None),
    }
}

fn normalize_last_entry(expr: &Expr, var: &str) -> Expr {
    let mut current = expr.clone();
    for _ in 0..4 {
        let next = rewrite_one_minus_linear_ratio(&current, var);
        let next = factor_common_den_inv(&next);
        let next = distribute_inv_over_mul(&next);
        let next = simplify_mul_factors(&next);
        if expr_key(&next) == expr_key(&current) {
            current = next;
            break;
        }
        current = next;
    }
    let mut simplified = simplify_expr_no_expand(&current);
    if expr_contains_var(&simplified, var) {
        if let Some(linearized) = linearize_expr(&simplified, var) {
            simplified = linearized;
        }
    }
    if !expr_contains_var(&simplified, var) {
        return simplified;
    }
    let candidate = match is_linear_reducible(&simplified, var) {
        Ok(true) => simplified,
        Ok(false) => {
            let Some(frac) = as_fraction_expr(&simplified) else {
                return simplified;
            };
            expr_div(frac.num, frac.denom)
        }
        Err(_) => simplified,
    };
    let rewritten = rewrite_linear_factors(&candidate, var);
    simplify_expr_no_expand(&rewritten)
}

fn rewrite_one_minus_linear_ratio(expr: &Expr, var: &str) -> Expr {
    let rewritten = match expr {
        Expr::Add(children) => {
            let items = children
                .iter()
                .map(|child| rewrite_one_minus_linear_ratio(child, var))
                .collect::<Vec<_>>();
            expr_add(items)
        }
        Expr::Mul(children) => {
            let items = children
                .iter()
                .map(|child| rewrite_one_minus_linear_ratio(child, var))
                .collect::<Vec<_>>();
            expr_mul(items)
        }
        Expr::Neg(inner) => expr_neg(rewrite_one_minus_linear_ratio(inner, var)),
        Expr::Pow(base, exp) => expr_pow(rewrite_one_minus_linear_ratio(base, var), *exp),
        Expr::Log(inner) => Expr::Log(Box::new(rewrite_one_minus_linear_ratio(inner, var))),
        Expr::Li2(inner) => Expr::Li2(Box::new(rewrite_one_minus_linear_ratio(inner, var))),
        Expr::Rational(_) | Expr::Var(_) => expr.clone(),
    };

    try_rewrite_one_minus_linear_ratio(&rewritten, var).unwrap_or(rewritten)
}

fn try_rewrite_one_minus_linear_ratio(expr: &Expr, var: &str) -> Option<Expr> {
    let normalized = expr.normalize();
    let Expr::Add(children) = normalized else {
        return None;
    };
    if children.len() != 2 {
        return None;
    }
    let (one, other) = if expr_is_one(&children[0]) {
        (&children[0], &children[1])
    } else if expr_is_one(&children[1]) {
        (&children[1], &children[0])
    } else {
        return None;
    };
    let _ = one;
    let ratio = extract_negative_ratio(other)?;
    let frac = as_fraction_expr(&ratio)?;
    let num_lf = as_linear(&frac.num, var).ok()??;
    let den_lf = as_linear(&frac.denom, var).ok()??;
    let a = linear_coeff_diff(&den_lf.a, &num_lf.a);
    let b = linear_coeff_diff(&den_lf.b, &num_lf.b);
    let denom = linear_form_expr(&den_lf, var);
    let numer = linear_coeff_expr(&a, &b, var);
    Some(expr_mul(vec![numer, expr_pow(denom, -1)]))
}

fn extract_negative_ratio(expr: &Expr) -> Option<Expr> {
    let (coeff, base) = split_coeff(expr);
    if coeff >= Coeff::zero() {
        return None;
    }
    let abs = -coeff;
    if abs == Coeff::one() {
        Some(base)
    } else {
        Some(expr_mul(vec![Expr::Rational(abs), base]))
    }
}

fn linear_coeff_diff(left: &Expr, right: &Expr) -> Expr {
    if expr_key(left) == expr_key(right) {
        expr_zero()
    } else {
        simplify_expr(&expr_sub(left.clone(), right.clone()))
    }
}

fn linear_form_expr(form: &LinearForm, var: &str) -> Expr {
    linear_coeff_expr(&form.a, &form.b, var)
}

fn linear_coeff_expr(a: &Expr, b: &Expr, var: &str) -> Expr {
    if expr_is_zero(a) {
        return b.clone();
    }
    if expr_is_zero(b) {
        return expr_mul(vec![a.clone(), expr_var(var)]);
    }
    expr_add(vec![expr_mul(vec![a.clone(), expr_var(var)]), b.clone()])
}

fn distribute_inv_over_mul(expr: &Expr) -> Expr {
    match expr {
        Expr::Mul(children) => {
            let items = children
                .iter()
                .map(distribute_inv_over_mul)
                .collect::<Vec<_>>();
            expr_mul(items)
        }
        Expr::Add(children) => {
            let items = children
                .iter()
                .map(distribute_inv_over_mul)
                .collect::<Vec<_>>();
            expr_add(items)
        }
        Expr::Neg(inner) => expr_neg(distribute_inv_over_mul(inner)),
        Expr::Pow(base, exp) => {
            let rewritten = distribute_inv_over_mul(base);
            if *exp == -1 {
                if let Expr::Mul(children) = rewritten {
                    let items = children
                        .into_iter()
                        .map(|child| expr_pow(child, -1))
                        .collect::<Vec<_>>();
                    return expr_mul(items);
                }
            }
            expr_pow(rewritten, *exp)
        }
        Expr::Log(inner) => Expr::Log(Box::new(distribute_inv_over_mul(inner))),
        Expr::Li2(inner) => Expr::Li2(Box::new(distribute_inv_over_mul(inner))),
        Expr::Rational(_) | Expr::Var(_) => expr.clone(),
    }
}

fn factor_common_den_inv(expr: &Expr) -> Expr {
    match expr {
        Expr::Add(children) => {
            let items = children
                .iter()
                .map(factor_common_den_inv)
                .collect::<Vec<_>>();
            if items.len() == 2 {
                if let (Some(left), Some(right)) =
                    (extract_den_inv(&items[0]), extract_den_inv(&items[1]))
                {
                    if expr_key(&left.den) == expr_key(&right.den) {
                        let sum = expr_add(vec![left.rest, right.rest]);
                        return expr_mul(vec![sum, expr_pow(left.den, -1)]);
                    }
                }
            }
            expr_add(items)
        }
        Expr::Mul(children) => {
            let items = children
                .iter()
                .map(factor_common_den_inv)
                .collect::<Vec<_>>();
            expr_mul(items)
        }
        Expr::Neg(inner) => expr_neg(factor_common_den_inv(inner)),
        Expr::Pow(base, exp) => expr_pow(factor_common_den_inv(base), *exp),
        Expr::Log(inner) => Expr::Log(Box::new(factor_common_den_inv(inner))),
        Expr::Li2(inner) => Expr::Li2(Box::new(factor_common_den_inv(inner))),
        Expr::Rational(_) | Expr::Var(_) => expr.clone(),
    }
}

struct DenInvTerm {
    den: Expr,
    rest: Expr,
}

fn extract_den_inv(expr: &Expr) -> Option<DenInvTerm> {
    match expr.normalize() {
        Expr::Neg(inner) => {
            let term = extract_den_inv(&inner)?;
            Some(DenInvTerm {
                den: term.den,
                rest: expr_neg(term.rest),
            })
        }
        Expr::Pow(base, -1) => Some(DenInvTerm {
            den: *base,
            rest: expr_one(),
        }),
        Expr::Mul(children) => {
            let mut den: Option<Expr> = None;
            let mut rest = Vec::new();
            for child in children {
                if let Expr::Pow(base, exp) = &child {
                    if *exp == -1 {
                        if den.is_some() {
                            return None;
                        }
                        den = Some((**base).clone());
                        continue;
                    }
                }
                rest.push(child);
            }
            let den = den?;
            let rest = if rest.is_empty() {
                expr_one()
            } else if rest.len() == 1 {
                rest.remove(0)
            } else {
                expr_mul(rest)
            };
            Some(DenInvTerm { den, rest })
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct FractionExpr {
    num: Expr,
    denom: Expr,
}

fn simplify_expr(expr: &Expr) -> Expr {
    match expr.normalize() {
        Expr::Add(children) => {
            let simplified = children
                .iter()
                .map(simplify_expr)
                .collect::<Vec<_>>();
            let combined = simplify_add_coeffs(&Expr::Add(simplified));
            factor_common_terms(&combined)
        }
        Expr::Mul(children) => {
            let simplified = children
                .iter()
                .map(simplify_expr)
                .collect::<Vec<_>>();
            if let Some(expanded) = expand_binomial_mul(&simplified) {
                return simplify_expr(&expanded);
            }
            expr_mul(simplified)
        }
        Expr::Neg(inner) => expr_neg(simplify_expr(&inner)),
        Expr::Pow(base, exp) => expr_pow(simplify_expr(&base), exp),
        other => other,
    }
}

fn simplify_expr_no_expand(expr: &Expr) -> Expr {
    match expr.normalize() {
        Expr::Add(children) => {
            let simplified = children
                .iter()
                .map(simplify_expr_no_expand)
                .collect::<Vec<_>>();
            let combined = simplify_add_coeffs(&Expr::Add(simplified));
            factor_common_terms_allow_neg(&combined)
        }
        Expr::Mul(children) => {
            let simplified = children
                .iter()
                .map(simplify_expr_no_expand)
                .collect::<Vec<_>>();
            simplify_mul_factors(&expr_mul(simplified))
        }
        Expr::Neg(inner) => expr_neg(simplify_expr_no_expand(&inner)),
        Expr::Pow(base, exp) => expr_pow(simplify_expr_no_expand(&base), exp),
        other => other,
    }
}

fn simplify_add_coeffs(expr: &Expr) -> Expr {
    let normalized = expr.normalize();
    let Expr::Add(children) = normalized else {
        return normalized;
    };
    let mut terms: BTreeMap<String, (Expr, Coeff)> = BTreeMap::new();
    let mut constant = Coeff::zero();
    for child in children {
        let (coeff, base) = split_coeff(&child);
        if coeff.is_zero() {
            continue;
        }
        if expr_is_one(&base) {
            constant += coeff;
            continue;
        }
        let key = expr_key(&base);
        let entry = terms.entry(key).or_insert((base, Coeff::zero()));
        entry.1 += coeff;
    }
    let mut out = Vec::new();
    for (_, (base, coeff)) in terms {
        if coeff.is_zero() {
            continue;
        }
        let term = if coeff == Coeff::one() {
            base
        } else if coeff == Coeff::from_integer(-1) {
            expr_neg(base)
        } else {
            expr_mul(vec![Expr::Rational(coeff), base])
        };
        out.push(term);
    }
    if !constant.is_zero() {
        out.push(Expr::Rational(constant));
    }
    expr_add(out)
}

fn split_coeff(expr: &Expr) -> (Coeff, Expr) {
    match expr.normalize() {
        Expr::Rational(value) => (value, expr_one()),
        Expr::Neg(inner) => {
            let (coeff, base) = split_coeff(&inner);
            (-coeff, base)
        }
        Expr::Mul(children) => {
            let mut coeff = Coeff::one();
            let mut factors = Vec::new();
            for child in children {
                match child {
                    Expr::Rational(value) => coeff *= value,
                    other => factors.push(other),
                }
            }
            let base = if factors.is_empty() {
                expr_one()
            } else {
                expr_mul(factors)
            };
            (coeff, base)
        }
        other => (Coeff::one(), other),
    }
}

fn expand_binomial_mul(children: &[Expr]) -> Option<Expr> {
    if children.len() != 2 {
        return None;
    }
    let (add_terms, other) = match (&children[0], &children[1]) {
        (Expr::Add(terms), other) => (terms, other),
        (other, Expr::Add(terms)) => (terms, other),
        _ => return None,
    };
    if add_terms.len() != 2 || matches!(other, Expr::Add(_)) {
        return None;
    }
    let mut non_one = None;
    for term in add_terms {
        if expr_is_one(term) {
            continue;
        }
        if non_one.is_some() {
            return None;
        }
        non_one = Some(term.clone());
    }
    let non_one = non_one?;
    let (coeff, base) = split_coeff(&non_one);
    if coeff != Coeff::one() && coeff != Coeff::from_integer(-1) {
        return None;
    }
    if matches!(base, Expr::Add(_)) {
        return None;
    }
    let scaled = if coeff == Coeff::one() {
        expr_mul(vec![base, other.clone()])
    } else {
        expr_neg(expr_mul(vec![base, other.clone()]))
    };
    Some(expr_add(vec![other.clone(), scaled]))
}

fn factor_common_terms(expr: &Expr) -> Expr {
    let normalized = expr.normalize();
    let Expr::Add(children) = normalized else {
        return normalized;
    };
    if children.len() < 2 {
        return expr_add(children);
    }
    let mut term_coeffs = Vec::with_capacity(children.len());
    let mut term_factors = Vec::with_capacity(children.len());
    for child in &children {
        let (coeff, factors) = split_term_factors(child);
        term_coeffs.push(coeff);
        term_factors.push(factors);
    }
    let Some(first) = term_factors.first() else {
        return expr_add(children);
    };
    let mut common: BTreeMap<String, (Expr, i32)> = BTreeMap::new();
    for (key, (expr, exp)) in first {
        if *exp > 0 {
            common.insert(key.clone(), (expr.clone(), *exp));
        }
    }
    for factors in term_factors.iter().skip(1) {
        let mut next = BTreeMap::new();
        for (key, (expr, exp)) in &common {
            let Some((_, other_exp)) = factors.get(key) else {
                continue;
            };
            if *other_exp > 0 {
                let min_exp = (*exp).min(*other_exp);
                if min_exp > 0 {
                    next.insert(key.clone(), (expr.clone(), min_exp));
                }
            }
        }
        common = next;
        if common.is_empty() {
            return expr_add(children);
        }
    }
    if common.is_empty() {
        return expr_add(children);
    }
    let mut new_terms = Vec::with_capacity(children.len());
    for (coeff, mut factors) in term_coeffs.into_iter().zip(term_factors.into_iter()) {
        if coeff.is_zero() {
            continue;
        }
        for (key, (_, exp)) in &common {
            if let Some((_, term_exp)) = factors.get_mut(key) {
                *term_exp -= *exp;
                if *term_exp == 0 {
                    factors.remove(key);
                }
            }
        }
        let mut items = Vec::new();
        for (_, (expr, exp)) in factors {
            if exp == 1 {
                items.push(expr);
            } else {
                items.push(expr_pow(expr, exp));
            }
        }
        let base = if items.is_empty() {
            expr_one()
        } else {
            expr_mul(items)
        };
        let term = if coeff == Coeff::one() {
            base
        } else {
            expr_mul(vec![Expr::Rational(coeff), base])
        };
        new_terms.push(term);
    }
    let common_expr = build_factor_expr(&common);
    expr_mul(vec![common_expr, expr_add(new_terms)])
}

fn factor_common_terms_allow_neg(expr: &Expr) -> Expr {
    let normalized = expr.normalize();
    let Expr::Add(children) = normalized else {
        return normalized;
    };
    if children.len() < 2 {
        return expr_add(children);
    }
    let mut term_coeffs = Vec::with_capacity(children.len());
    let mut term_factors = Vec::with_capacity(children.len());
    for child in &children {
        let (coeff, factors) = split_term_factors(child);
        term_coeffs.push(coeff);
        term_factors.push(factors);
    }
    let Some(first) = term_factors.first() else {
        return expr_add(children);
    };
    let mut common: BTreeMap<String, (Expr, i32)> = first
        .iter()
        .map(|(key, (expr, exp))| (key.clone(), (expr.clone(), *exp)))
        .collect();
    for factors in term_factors.iter().skip(1) {
        let mut next = BTreeMap::new();
        for (key, (expr, exp)) in &common {
            let Some((_, other_exp)) = factors.get(key) else {
                continue;
            };
            let min_exp = (*exp).min(*other_exp);
            if min_exp != 0 {
                next.insert(key.clone(), (expr.clone(), min_exp));
            }
        }
        common = next;
        if common.is_empty() {
            return expr_add(children);
        }
    }
    if common.is_empty() {
        return expr_add(children);
    }
    let mut new_terms = Vec::with_capacity(children.len());
    for (coeff, mut factors) in term_coeffs.into_iter().zip(term_factors.into_iter()) {
        if coeff.is_zero() {
            continue;
        }
        for (key, (_, exp)) in &common {
            if let Some((_, term_exp)) = factors.get_mut(key) {
                *term_exp -= *exp;
                if *term_exp == 0 {
                    factors.remove(key);
                }
            }
        }
        let mut items = Vec::new();
        for (_, (expr, exp)) in factors {
            if exp == 1 {
                items.push(expr);
            } else {
                items.push(expr_pow(expr, exp));
            }
        }
        let base = if items.is_empty() {
            expr_one()
        } else {
            expr_mul(items)
        };
        let term = if coeff == Coeff::one() {
            base
        } else {
            expr_mul(vec![Expr::Rational(coeff), base])
        };
        new_terms.push(term);
    }
    let common_expr = build_factor_expr(&common);
    expr_mul(vec![common_expr, expr_add(new_terms)])
}

fn split_term_factors(expr: &Expr) -> (Coeff, BTreeMap<String, (Expr, i32)>) {
    match expr.normalize() {
        Expr::Rational(value) => (value, BTreeMap::new()),
        Expr::Neg(inner) => {
            let (coeff, factors) = split_term_factors(&inner);
            (-coeff, factors)
        }
        Expr::Mul(children) => {
            let mut coeff = Coeff::one();
            let mut factors: BTreeMap<String, (Expr, i32)> = BTreeMap::new();
            for child in children {
                match child {
                    Expr::Rational(value) => coeff *= value,
                    Expr::Pow(base, exp) => {
                        let base_expr = *base;
                        if let Expr::Rational(value) = &base_expr {
                            let pow = expr_pow(Expr::Rational(*value), exp);
                            if let Expr::Rational(rat) = pow {
                                coeff *= rat;
                                continue;
                            }
                        }
                        let key = expr_key(&base_expr);
                        let new_exp = {
                            let entry = factors.entry(key.clone()).or_insert((base_expr, 0));
                            entry.1 += exp;
                            entry.1
                        };
                        if new_exp == 0 {
                            factors.remove(&key);
                        }
                    }
                    other => {
                        let key = expr_key(&other);
                        let new_exp = {
                            let entry = factors.entry(key.clone()).or_insert((other, 0));
                            entry.1 += 1;
                            entry.1
                        };
                        if new_exp == 0 {
                            factors.remove(&key);
                        }
                    }
                }
            }
            (coeff, factors)
        }
        Expr::Pow(base, exp) => {
            let base_expr = *base;
            if let Expr::Rational(value) = &base_expr {
                let pow = expr_pow(Expr::Rational(*value), exp);
                if let Expr::Rational(rat) = pow {
                    return (rat, BTreeMap::new());
                }
            }
            let mut factors = BTreeMap::new();
            let key = expr_key(&base_expr);
            factors.insert(key, (base_expr, exp));
            (Coeff::one(), factors)
        }
        other => {
            let mut factors = BTreeMap::new();
            let key = expr_key(&other);
            factors.insert(key, (other, 1));
            (Coeff::one(), factors)
        }
    }
}

fn build_factor_expr(factors: &BTreeMap<String, (Expr, i32)>) -> Expr {
    let mut items = Vec::new();
    for (expr, exp) in factors.values() {
        if *exp == 1 {
            items.push(expr.clone());
        } else {
            items.push(expr_pow(expr.clone(), *exp));
        }
    }
    expr_mul(items)
}

fn simplify_mul_factors(expr: &Expr) -> Expr {
    match expr.normalize() {
        Expr::Mul(_) => {
            let (coeff, factors) = split_term_factors(expr);
            if coeff.is_zero() {
                return expr_zero();
            }
            let base = build_factor_expr(&factors);
            if coeff == Coeff::one() {
                base
            } else if expr_is_one(&base) {
                Expr::Rational(coeff)
            } else {
                expr_mul(vec![Expr::Rational(coeff), base])
            }
        }
        Expr::Add(children) => {
            let items = children
                .iter()
                .map(simplify_mul_factors)
                .collect::<Vec<_>>();
            expr_add(items)
        }
        Expr::Neg(inner) => expr_neg(simplify_mul_factors(inner.as_ref())),
        Expr::Pow(base, exp) => expr_pow(simplify_mul_factors(base.as_ref()), exp),
        Expr::Log(inner) => Expr::Log(Box::new(simplify_mul_factors(inner.as_ref()))),
        Expr::Li2(inner) => Expr::Li2(Box::new(simplify_mul_factors(inner.as_ref()))),
        Expr::Rational(_) | Expr::Var(_) => expr.clone(),
    }
}

fn linearize_expr(expr: &Expr, var: &str) -> Option<Expr> {
    let mut poly = poly_from_expr(expr, var)?;
    for coeff in &mut poly {
        *coeff = simplify_expr(coeff);
    }
    trim_poly(&mut poly);
    if poly.len() > 2 {
        return None;
    }
    let b = poly.first().cloned().unwrap_or_else(expr_zero);
    let a = poly.get(1).cloned().unwrap_or_else(expr_zero);
    if expr_is_zero(&a) {
        return Some(b);
    }
    if expr_is_zero(&b) {
        return Some(expr_mul(vec![a, expr_var(var)]));
    }
    Some(expr_add(vec![expr_mul(vec![a, expr_var(var)]), b]))
}

fn poly_from_expr(expr: &Expr, var: &str) -> Option<Vec<Expr>> {
    match expr.normalize() {
        Expr::Rational(_) => Some(vec![expr.normalize()]),
        Expr::Var(name) => {
            if name == var {
                Some(vec![expr_zero(), expr_one()])
            } else {
                Some(vec![expr.normalize()])
            }
        }
        Expr::Neg(inner) => {
            let mut poly = poly_from_expr(&inner, var)?;
            for coeff in &mut poly {
                *coeff = expr_neg(coeff.clone());
            }
            Some(poly)
        }
        Expr::Add(children) => {
            let mut poly = vec![expr_zero()];
            for child in children {
                let next = poly_from_expr(&child, var)?;
                poly = poly_add(&poly, &next);
            }
            Some(poly)
        }
        Expr::Mul(children) => {
            let mut poly = vec![expr_one()];
            for child in children {
                let next = poly_from_expr(&child, var)?;
                poly = poly_mul(&poly, &next);
            }
            Some(poly)
        }
        Expr::Pow(base, exp) => {
            if exp < 0 {
                return None;
            }
            if exp == 0 {
                return Some(vec![expr_one()]);
            }
            let base_poly = poly_from_expr(&base, var)?;
            poly_pow(&base_poly, exp as usize)
        }
        Expr::Log(_) | Expr::Li2(_) => None,
    }
}

fn poly_add(left: &[Expr], right: &[Expr]) -> Vec<Expr> {
    let mut out = Vec::new();
    let max_len = left.len().max(right.len());
    for i in 0..max_len {
        let a = left.get(i).cloned().unwrap_or_else(expr_zero);
        let b = right.get(i).cloned().unwrap_or_else(expr_zero);
        out.push(expr_add(vec![a, b]));
    }
    out
}

fn poly_mul(left: &[Expr], right: &[Expr]) -> Vec<Expr> {
    if left.is_empty() || right.is_empty() {
        return Vec::new();
    }
    let mut out = vec![expr_zero(); left.len() + right.len() - 1];
    for (i, a) in left.iter().enumerate() {
        for (j, b) in right.iter().enumerate() {
            let term = expr_mul(vec![a.clone(), b.clone()]);
            let accum = out.get(i + j).cloned().unwrap_or_else(expr_zero);
            out[i + j] = expr_add(vec![accum, term]);
        }
    }
    out
}

fn poly_pow(base: &[Expr], exp: usize) -> Option<Vec<Expr>> {
    if exp == 0 {
        return Some(vec![expr_one()]);
    }
    if exp == 1 {
        return Some(base.to_vec());
    }
    let mut result = vec![expr_one()];
    let mut power = base.to_vec();
    let mut e = exp;
    while e > 0 {
        if e % 2 == 1 {
            result = poly_mul(&result, &power);
        }
        e /= 2;
        if e > 0 {
            power = poly_mul(&power, &power);
        }
    }
    Some(result)
}

fn trim_poly(poly: &mut Vec<Expr>) {
    while let Some(last) = poly.last() {
        if expr_is_zero(last) {
            poly.pop();
        } else {
            break;
        }
    }
    if poly.is_empty() {
        poly.push(expr_zero());
    }
}

fn as_fraction_expr(expr: &Expr) -> Option<FractionExpr> {
    let normalized = expr.normalize();
    match normalized {
        Expr::Rational(_) | Expr::Var(_) => Some(FractionExpr {
            num: normalized,
            denom: expr_one(),
        }),
        Expr::Neg(inner) => {
            let frac = as_fraction_expr(&inner)?;
            Some(FractionExpr {
                num: expr_neg(frac.num),
                denom: frac.denom,
            })
        }
        Expr::Add(children) => {
            let mut iter = children.iter();
            let first = as_fraction_expr(iter.next()?)?;
            let mut num = first.num;
            let mut denom = first.denom;
            for child in iter {
                let next = as_fraction_expr(child)?;
                let left = expr_mul(vec![num.clone(), next.denom.clone()]);
                let right = expr_mul(vec![next.num.clone(), denom.clone()]);
                num = expr_add(vec![left, right]);
                denom = expr_mul(vec![denom, next.denom]);
            }
            Some(FractionExpr { num, denom })
        }
        Expr::Mul(children) => {
            let mut num = expr_one();
            let mut denom = expr_one();
            for child in &children {
                let frac = as_fraction_expr(child)?;
                num = expr_mul(vec![num, frac.num]);
                denom = expr_mul(vec![denom, frac.denom]);
            }
            Some(FractionExpr { num, denom })
        }
        Expr::Pow(base, exp) => {
            if exp == 0 {
                return Some(FractionExpr {
                    num: expr_one(),
                    denom: expr_one(),
                });
            }
            let frac = as_fraction_expr(&base)?;
            let abs = exp.abs();
            if exp > 0 {
                Some(FractionExpr {
                    num: expr_pow(frac.num, abs),
                    denom: expr_pow(frac.denom, abs),
                })
            } else {
                Some(FractionExpr {
                    num: expr_pow(frac.denom, abs),
                    denom: expr_pow(frac.num, abs),
                })
            }
        }
        Expr::Log(_) | Expr::Li2(_) => None,
    }
}

fn is_linear_reducible(expr: &Expr, var: &str) -> Result<bool, ExperimentError> {
    match expr {
        Expr::Rational(_) | Expr::Var(_) => Ok(true),
        Expr::Neg(inner) => is_linear_reducible(inner, var),
        Expr::Add(_) => Ok(as_linear(expr, var)?.is_some()),
        Expr::Mul(children) => {
            for child in children {
                if !is_linear_reducible(child, var)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expr::Pow(base, exp) => {
            if *exp == 0 {
                Ok(true)
            } else {
                is_linear_reducible(base, var)
            }
        }
        Expr::Log(_) | Expr::Li2(_) => Ok(false),
    }
}

fn rewrite_linear_factors(expr: &Expr, var: &str) -> Expr {
    let normalized = expr.normalize();
    match normalized {
        Expr::Add(_) => {
            if let Ok(Some(linear)) = as_linear(&normalized, var) {
                if expr_is_zero(&linear.a) {
                    return simplify_expr(&linear.b);
                }
                let shift = expr_div(linear.b, linear.a.clone());
                let monic = expr_add(vec![expr_var(var), shift]);
                return expr_mul(vec![linear.a, monic]);
            }
            normalized
        }
        Expr::Mul(children) => {
            let items = children
                .iter()
                .map(|child| rewrite_linear_factors(child, var))
                .collect::<Vec<_>>();
            expr_mul(items)
        }
        Expr::Neg(inner) => expr_neg(rewrite_linear_factors(&inner, var)),
        Expr::Pow(base, exp) => expr_pow(rewrite_linear_factors(&base, var), exp),
        Expr::Rational(_) | Expr::Var(_) => normalized,
        Expr::Log(_) | Expr::Li2(_) => normalized,
    }
}

fn dlog_expand_linear(expr: &Expr) -> Vec<(Expr, Coeff)> {
    match expr.normalize() {
        Expr::Rational(_) => Vec::new(),
        Expr::Neg(inner) => dlog_expand_linear(&inner),
        Expr::Mul(children) => {
            let mut out = Vec::new();
            for child in children {
                out.extend(dlog_expand_linear(&child));
            }
            merge_dlog_terms(out)
        }
        Expr::Pow(base, exp) => {
            if exp == 0 {
                return Vec::new();
            }
            let mut out = dlog_expand_linear(&base);
            let factor = Coeff::from_integer(exp as i64);
            for (_, coeff) in out.iter_mut() {
                *coeff *= factor;
            }
            merge_dlog_terms(out)
        }
        other => vec![(other, Coeff::one())],
    }
}

fn merge_dlog_terms(items: Vec<(Expr, Coeff)>) -> Vec<(Expr, Coeff)> {
    let mut map: BTreeMap<String, (Expr, Coeff)> = BTreeMap::new();
    for (expr, coeff) in items {
        if coeff.is_zero() {
            continue;
        }
        let key = expr_key(&expr);
        let entry = map.entry(key).or_insert((expr, Coeff::zero()));
        entry.1 += coeff;
    }
    map.into_values().filter(|(_, coeff)| !coeff.is_zero()).collect()
}

fn linear_shift(expr: &Expr, var: &str) -> Option<Expr> {
    if let Some(mut poly) = poly_from_expr(expr, var) {
        for coeff in &mut poly {
            *coeff = simplify_expr(coeff);
        }
        trim_poly(&mut poly);
        if poly.len() == 2 {
            let b = poly.first().cloned().unwrap_or_else(expr_zero);
            let a = poly.get(1).cloned().unwrap_or_else(expr_zero);
            if !expr_is_zero(&a) {
                return Some(expr_div(b, a));
            }
        }
    }
    let linear = as_linear(expr, var).ok()??;
    if expr_is_zero(&linear.a) {
        return None;
    }
    Some(expr_div(linear.b, linear.a))
}

fn linear_shift_with_fallback(expr: &Expr, var: &str) -> Option<Expr> {
    if let Some(shift) = linear_shift(expr, var) {
        return Some(shift);
    }
    let simplified = simplify_expr(expr);
    if let Ok(Some(linear)) = as_linear(&simplified, var) {
        if !expr_is_zero(&linear.a) {
            return Some(expr_div(linear.b, linear.a));
        }
    }
    let linearized = linearize_expr(&simplified, var)?;
    linear_shift(&linearized, var)
}

#[derive(Clone, Debug)]
struct LinearForm {
    a: Expr,
    b: Expr,
}

fn simplify_linear_form(mut form: LinearForm) -> LinearForm {
    form.a = simplify_expr(&form.a);
    form.b = simplify_expr(&form.b);
    form
}

fn as_linear(expr: &Expr, var: &str) -> Result<Option<LinearForm>, ExperimentError> {
    match expr {
        Expr::Rational(_) => Ok(Some(LinearForm {
            a: expr_zero(),
            b: expr.clone(),
        })),
        Expr::Var(name) => {
            if name == var {
                Ok(Some(LinearForm {
                    a: expr_one(),
                    b: expr_zero(),
                }))
            } else {
                Ok(Some(LinearForm {
                    a: expr_zero(),
                    b: expr.clone(),
                }))
            }
        }
        Expr::Neg(inner) => {
            let Some(mut lf) = as_linear(inner, var)? else {
                return Ok(None);
            };
            lf.a = expr_neg(lf.a);
            lf.b = expr_neg(lf.b);
            Ok(Some(simplify_linear_form(lf)))
        }
        Expr::Add(children) => {
            let mut a = expr_zero();
            let mut b = expr_zero();
            for child in children {
                let Some(lf) = as_linear(child, var)? else {
                    return Ok(None);
                };
                a = expr_add(vec![a, lf.a]);
                b = expr_add(vec![b, lf.b]);
            }
            Ok(Some(simplify_linear_form(LinearForm { a, b })))
        }
        Expr::Mul(children) => {
            let mut constant = expr_one();
            let mut linear: Option<LinearForm> = None;
            for child in children {
                let Some(lf) = as_linear(child, var)? else {
                    return Ok(None);
                };
                let lf = simplify_linear_form(lf);
                if expr_is_zero(&lf.a) {
                    constant = expr_mul(vec![constant, lf.b]);
                } else if linear.is_some() {
                    return Ok(None);
                } else {
                    linear = Some(lf);
                }
            }
            if let Some(lf) = linear {
                Ok(Some(simplify_linear_form(LinearForm {
                    a: expr_mul(vec![lf.a, constant.clone()]),
                    b: expr_mul(vec![lf.b, constant]),
                })))
            } else {
                Ok(Some(simplify_linear_form(LinearForm {
                    a: expr_zero(),
                    b: constant,
                })))
            }
        }
        Expr::Pow(base, exp) => {
            if *exp == 0 {
                return Ok(Some(simplify_linear_form(LinearForm {
                    a: expr_zero(),
                    b: expr_one(),
                })));
            }
            if *exp == 1 {
                return as_linear(base, var);
            }
            if !expr_contains_var(base, var) {
                return Ok(Some(simplify_linear_form(LinearForm {
                    a: expr_zero(),
                    b: expr_pow((**base).clone(), *exp),
                })));
            }
            Ok(None)
        }
        Expr::Log(_) | Expr::Li2(_) => Ok(None),
    }
}

fn shift_difference_expr(
    c: &Expr,
    d: &Expr,
    alpha: &AlphabetSpec,
    stage: IntegrationStage,
) -> Result<Option<Expr>, ExperimentError> {
    let diff = expr_sub(c.clone(), d.clone());
    let normalized = normalize_letter_for_alphabet(&diff, alpha);
    let expanded = dlog_expand_letter(&normalized, alpha).map_err(|err| {
        ExperimentError::InvalidConfig(format!(
            "diff not in alphabet (stage={}, kernel={}, d={}, diff={}): {err}",
            stage.as_str(),
            c.to_canonical_string(),
            d.to_canonical_string(),
            normalized.to_canonical_string()
        ))
    })?;
    if expanded.is_empty() {
        return Ok(None);
    }
    Ok(Some(normalized))
}

fn substitute_symbol(sym: &Symbol, subst: &BTreeMap<String, Expr>) -> Symbol {
    let mut terms = Vec::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let letters = word
            .letters()
            .iter()
            .map(|expr| substitute_expr(expr, subst))
            .collect();
        terms.push((Word(letters), *coeff));
    }
    Symbol::from_terms(terms)
}

fn substitute_expr(expr: &Expr, subst: &BTreeMap<String, Expr>) -> Expr {
    match expr {
        Expr::Rational(_) => expr.clone(),
        Expr::Var(name) => subst.get(name).cloned().unwrap_or_else(|| expr.clone()),
        Expr::Add(children) => {
            let items = children
                .iter()
                .map(|child| substitute_expr(child, subst))
                .collect::<Vec<_>>();
            expr_add(items)
        }
        Expr::Mul(children) => {
            let items = children
                .iter()
                .map(|child| substitute_expr(child, subst))
                .collect::<Vec<_>>();
            expr_mul(items)
        }
        Expr::Neg(inner) => expr_neg(substitute_expr(inner, subst)),
        Expr::Pow(base, exp) => expr_pow(substitute_expr(base, subst), *exp),
        Expr::Log(inner) => Expr::Log(Box::new(substitute_expr(inner, subst))).normalize(),
        Expr::Li2(inner) => Expr::Li2(Box::new(substitute_expr(inner, subst))).normalize(),
    }
}

fn expand_symbol_to_alphabet(sym: &Symbol, alpha: &AlphabetSpec) -> Result<Symbol, ExperimentError> {
    let mut terms = Vec::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let expanded = expand_word_to_letters(word.letters(), alpha)?;
        for (letters, value) in expanded {
            let combined = *coeff * value;
            if combined.is_zero() {
                continue;
            }
            terms.push((Word(letters), combined));
        }
    }
    Ok(Symbol::from_terms(terms))
}

fn expand_word_to_letters(
    word: &[Expr],
    alpha: &AlphabetSpec,
) -> Result<Vec<(Vec<Expr>, Coeff)>, ExperimentError> {
    let mut acc: Vec<(Vec<Expr>, Coeff)> = vec![(Vec::new(), Coeff::one())];
    let word_str = word
        .iter()
        .map(|expr| expr.to_canonical_string())
        .collect::<Vec<_>>()
        .join(" ");
    for (idx, letter) in word.iter().enumerate() {
        let expanded = match dlog_expand_letter(letter, alpha) {
            Ok(expanded) => expanded,
            Err(err) => {
                return Err(ExperimentError::InvalidConfig(format!(
                    "expand_word_to_letters failed at pos {idx} letter {} in word [{word_str}]: {err}",
                    letter.to_canonical_string()
                )));
            }
        };
        if expanded.is_empty() {
            return Ok(Vec::new());
        }
        let mut next = Vec::new();
        for (prefix, coeff) in acc {
            for (expanded_letter, scale) in &expanded {
                let mut letters = prefix.clone();
                letters.push(expanded_letter.clone());
                next.push((letters, coeff * *scale));
            }
        }
        acc = next;
    }
    Ok(acc)
}

fn dlog_expand_letter(
    expr: &Expr,
    alpha: &AlphabetSpec,
) -> Result<Vec<(Expr, Coeff)>, ExperimentError> {
    let normalized = normalize_letter_for_alphabet(expr, alpha);
    let normalized = normalized.normalize();
    match normalized {
        Expr::Rational(_) => Ok(Vec::new()),
        Expr::Neg(inner) => dlog_expand_letter(&inner, alpha),
        Expr::Mul(children) => {
            let mut out = Vec::new();
            for child in children {
                out.extend(dlog_expand_letter(&child, alpha)?);
            }
            Ok(merge_dlog_terms(out))
        }
        Expr::Pow(base, exp) => {
            if exp == 0 {
                return Ok(Vec::new());
            }
            let mut out = dlog_expand_letter(&base, alpha)?;
            let factor = Coeff::from_integer(exp as i64);
            for (_, coeff) in out.iter_mut() {
                *coeff *= factor;
            }
            Ok(merge_dlog_terms(out))
        }
        Expr::Var(_) | Expr::Add(_) => {
            let key = normalized.to_canonical_string();
            if !alpha.map.contains_key(&key) {
                return Err(ExperimentError::InvalidConfig(format!(
                    "letter not in alphabet: {key}"
                )));
            }
            Ok(vec![(normalized, Coeff::one())])
        }
        Expr::Log(_) | Expr::Li2(_) => Err(ExperimentError::InvalidConfig(
            "unexpected log/li2 in letter".to_string(),
        )),
    }
}

fn normalize_letter_for_alphabet(expr: &Expr, alpha: &AlphabetSpec) -> Expr {
    let simplified = expr.normalize();
    let key = simplified.to_canonical_string();
    if alpha.map.contains_key(&key) {
        return simplified;
    }
    if !matches!(simplified, Expr::Add(_)) {
        return simplified;
    }
    let factored = factor_common_terms_allow_neg(&simplified);
    if !matches!(factored, Expr::Add(_)) {
        return factored.normalize();
    }
    if expr_contains_var(&simplified, "t") {
        return simplified;
    }
    if !expr_contains_any_var(&simplified, &["u", "v", "w"]) {
        return simplified;
    }
    let Some(rat) = ratpoly_from_expr(&factored) else {
        return factored;
    };
    let Some(rewritten) = ratpoly_to_expr_for_alphabet(&rat, &alpha.letter_polys) else {
        return factored;
    };
    rewritten.normalize()
}

fn format_dlog_atoms(expr: &Expr) -> Vec<String> {
    let atoms = dlog_expand_linear(expr);
    let merged = merge_dlog_terms(atoms);
    merged
        .into_iter()
        .map(|(atom, coeff)| format!("{}:{coeff}", atom.to_canonical_string()))
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Mono([u32; 3]);

type Poly = BTreeMap<Mono, Coeff>;

#[derive(Clone, Debug)]
struct RatPoly {
    num: Poly,
    den: Poly,
}

fn build_letter_polys(letters: &[Expr]) -> Vec<LetterPoly> {
    letters
        .iter()
        .map(|expr| {
            let poly = ratpoly_from_expr(expr)
                .and_then(|rat| if rat_poly_is_one(&rat.den) { Some(rat.num) } else { None })
                .unwrap_or_default();
            LetterPoly {
                expr: expr.clone(),
                poly,
            }
        })
        .collect()
}

fn ratpoly_from_expr(expr: &Expr) -> Option<RatPoly> {
    match expr.normalize() {
        Expr::Rational(value) => Some(RatPoly {
            num: rat_poly_const(value),
            den: rat_poly_one(),
        }),
        Expr::Var(name) => {
            let idx = var_index(&name)?;
            Some(RatPoly {
                num: rat_poly_var(idx),
                den: rat_poly_one(),
            })
        }
        Expr::Neg(inner) => {
            let mut rat = ratpoly_from_expr(&inner)?;
            rat.num = rat_poly_scale(&rat.num, Coeff::from_integer(-1));
            Some(rat)
        }
        Expr::Add(children) => {
            let mut iter = children.iter();
            let first = ratpoly_from_expr(iter.next()?)?;
            let mut acc = first;
            for child in iter {
                let next = ratpoly_from_expr(child)?;
                acc = rat_add(&acc, &next);
            }
            Some(acc)
        }
        Expr::Mul(children) => {
            let mut iter = children.iter();
            let first = ratpoly_from_expr(iter.next()?)?;
            let mut acc = first;
            for child in iter {
                let next = ratpoly_from_expr(child)?;
                acc = rat_mul(&acc, &next);
            }
            Some(acc)
        }
        Expr::Pow(base, exp) => {
            let base_rat = ratpoly_from_expr(&base)?;
            rat_pow(&base_rat, exp)
        }
        Expr::Log(_) | Expr::Li2(_) => None,
    }
}

fn rat_add(left: &RatPoly, right: &RatPoly) -> RatPoly {
    let left_num = rat_poly_mul(&left.num, &right.den);
    let right_num = rat_poly_mul(&right.num, &left.den);
    RatPoly {
        num: rat_poly_add(&left_num, &right_num),
        den: rat_poly_mul(&left.den, &right.den),
    }
}

fn rat_mul(left: &RatPoly, right: &RatPoly) -> RatPoly {
    RatPoly {
        num: rat_poly_mul(&left.num, &right.num),
        den: rat_poly_mul(&left.den, &right.den),
    }
}

fn rat_pow(base: &RatPoly, exp: i32) -> Option<RatPoly> {
    if exp == 0 {
        return Some(RatPoly {
            num: rat_poly_one(),
            den: rat_poly_one(),
        });
    }
    let exp_abs = exp.unsigned_abs();
    let num = rat_poly_pow(&base.num, exp_abs)?;
    let den = rat_poly_pow(&base.den, exp_abs)?;
    if exp > 0 {
        Some(RatPoly { num, den })
    } else {
        Some(RatPoly { num: den, den: num })
    }
}

fn ratpoly_to_expr_for_alphabet(rat: &RatPoly, letters: &[LetterPoly]) -> Option<Expr> {
    if rat_poly_is_zero(&rat.num) {
        return Some(expr_zero());
    }
    if let (Some(num_factors), Some(den_factors)) = (
        factor_poly_to_letters(&rat.num, letters),
        factor_poly_to_letters(&rat.den, letters),
    ) {
        let mut items = Vec::new();
        items.extend(num_factors);
        for factor in den_factors {
            items.push(expr_pow(factor, -1));
        }
        return Some(expr_mul(items));
    }
    let num_expr = expr_from_poly(&rat.num);
    let den_expr = expr_from_poly(&rat.den);
    Some(expr_mul(vec![num_expr, expr_pow(den_expr, -1)]))
}

fn factor_poly_to_letters(poly: &Poly, letters: &[LetterPoly]) -> Option<Vec<Expr>> {
    if rat_poly_is_zero(poly) {
        return None;
    }
    if rat_poly_is_one(poly) || rat_poly_is_constant(poly) {
        return Some(Vec::new());
    }
    let mut current = poly.clone();
    let mut factors = Vec::new();
    for letter in letters {
        loop {
            let Some(quotient) = rat_poly_div(&current, &letter.poly) else {
                break;
            };
            current = quotient;
            factors.push(letter.expr.clone());
        }
        if rat_poly_is_one(&current) || rat_poly_is_constant(&current) {
            return Some(factors);
        }
    }
    if rat_poly_is_constant(&current) {
        return Some(factors);
    }
    None
}

fn rat_poly_const(value: Coeff) -> Poly {
    let mut poly = Poly::new();
    if !value.is_zero() {
        poly.insert(Mono([0, 0, 0]), value);
    }
    poly
}

fn rat_poly_one() -> Poly {
    rat_poly_const(Coeff::one())
}

fn rat_poly_var(idx: usize) -> Poly {
    let mut poly = Poly::new();
    let mut exps = [0u32; 3];
    if idx < 3 {
        exps[idx] = 1;
    }
    poly.insert(Mono(exps), Coeff::one());
    poly
}

fn rat_poly_is_zero(poly: &Poly) -> bool {
    poly.is_empty()
}

fn rat_poly_is_one(poly: &Poly) -> bool {
    poly.len() == 1
        && poly
            .get(&Mono([0, 0, 0]))
            .map(|coeff| coeff.is_one())
            .unwrap_or(false)
}

fn rat_poly_is_constant(poly: &Poly) -> bool {
    poly.len() == 1 && poly.contains_key(&Mono([0, 0, 0]))
}

fn rat_poly_add(left: &Poly, right: &Poly) -> Poly {
    let mut out = left.clone();
    for (mono, coeff) in right {
        let entry = out.entry(*mono).or_insert_with(Coeff::zero);
        *entry += *coeff;
        if entry.is_zero() {
            out.remove(mono);
        }
    }
    out
}

fn rat_poly_scale(poly: &Poly, factor: Coeff) -> Poly {
    if factor.is_zero() {
        return Poly::new();
    }
    let mut out = Poly::new();
    for (mono, coeff) in poly {
        let value = *coeff * factor;
        if !value.is_zero() {
            out.insert(*mono, value);
        }
    }
    out
}

fn rat_poly_mul(left: &Poly, right: &Poly) -> Poly {
    if rat_poly_is_zero(left) || rat_poly_is_zero(right) {
        return Poly::new();
    }
    let mut out = Poly::new();
    for (mono_left, coeff_left) in left {
        for (mono_right, coeff_right) in right {
            let mono = mono_add(*mono_left, *mono_right);
            let entry = out.entry(mono).or_insert_with(Coeff::zero);
            *entry += *coeff_left * *coeff_right;
            if entry.is_zero() {
                out.remove(&mono);
            }
        }
    }
    out
}

fn rat_poly_pow(base: &Poly, exp: u32) -> Option<Poly> {
    if exp == 0 {
        return Some(rat_poly_one());
    }
    if exp == 1 {
        return Some(base.clone());
    }
    let mut result = rat_poly_one();
    let mut power = base.clone();
    let mut e = exp;
    while e > 0 {
        if e % 2 == 1 {
            result = rat_poly_mul(&result, &power);
        }
        e /= 2;
        if e > 0 {
            power = rat_poly_mul(&power, &power);
        }
    }
    Some(result)
}

fn mono_add(left: Mono, right: Mono) -> Mono {
    Mono([
        left.0[0] + right.0[0],
        left.0[1] + right.0[1],
        left.0[2] + right.0[2],
    ])
}

fn mono_sub(left: Mono, right: Mono) -> Option<Mono> {
    if left.0[0] < right.0[0] || left.0[1] < right.0[1] || left.0[2] < right.0[2] {
        return None;
    }
    Some(Mono([
        left.0[0] - right.0[0],
        left.0[1] - right.0[1],
        left.0[2] - right.0[2],
    ]))
}

fn rat_poly_div(dividend: &Poly, divisor: &Poly) -> Option<Poly> {
    if rat_poly_is_zero(divisor) {
        return None;
    }
    let (div_mono, div_coeff) = rat_poly_leading_term(divisor)?;
    let mut remainder = dividend.clone();
    let mut quotient = Poly::new();
    while !rat_poly_is_zero(&remainder) {
        let (rem_mono, rem_coeff) = rat_poly_leading_term(&remainder)?;
        let diff = mono_sub(rem_mono, div_mono)?;
        let coeff = rem_coeff / div_coeff;
        let term = rat_poly_monomial(diff, coeff);
        quotient = rat_poly_add(&quotient, &term);
        let scaled = rat_poly_mul(&term, divisor);
        remainder = rat_poly_sub(&remainder, &scaled);
    }
    Some(quotient)
}

fn rat_poly_sub(left: &Poly, right: &Poly) -> Poly {
    rat_poly_add(left, &rat_poly_scale(right, Coeff::from_integer(-1)))
}

fn rat_poly_monomial(mono: Mono, coeff: Coeff) -> Poly {
    let mut poly = Poly::new();
    if !coeff.is_zero() {
        poly.insert(mono, coeff);
    }
    poly
}

fn rat_poly_leading_term(poly: &Poly) -> Option<(Mono, Coeff)> {
    let (mono, coeff) = poly.iter().next_back()?;
    Some((*mono, *coeff))
}

fn var_index(name: &str) -> Option<usize> {
    match name {
        "u" => Some(0),
        "v" => Some(1),
        "w" => Some(2),
        _ => None,
    }
}

fn expr_from_poly(poly: &Poly) -> Expr {
    if rat_poly_is_zero(poly) {
        return expr_zero();
    }
    let mut terms = Vec::new();
    for (mono, coeff) in poly {
        if coeff.is_zero() {
            continue;
        }
        let mut factors = Vec::new();
        if !coeff.is_one() {
            factors.push(Expr::Rational(*coeff));
        }
        for (idx, exp) in mono.0.iter().enumerate() {
            if *exp == 0 {
                continue;
            }
            let var = expr_for_index(idx);
            let exp_i32 = (*exp).try_into().unwrap_or(i32::MAX);
            factors.push(expr_pow(var, exp_i32));
        }
        let term = if factors.is_empty() {
            expr_one()
        } else if factors.len() == 1 {
            factors.remove(0)
        } else {
            expr_mul(factors)
        };
        terms.push(term);
    }
    expr_add(terms)
}

fn expr_for_index(idx: usize) -> Expr {
    match idx {
        0 => expr_var("u"),
        1 => expr_var("v"),
        2 => expr_var("w"),
        _ => expr_zero(),
    }
}

#[cfg(test)]
mod ratpoly_tests {
    use super::*;

    #[test]
    fn normalize_letter_ratio_to_alphabet() {
        let ctx = PentaContext::new();
        let expr = expr_add(vec![
            expr_neg(expr_mul(vec![
                expr_pow(ctx.one_minus_u.clone(), -1),
                ctx.u.clone(),
                ctx.w.clone(),
            ])),
            expr_pow(ctx.one_minus_u.clone(), -1),
        ]);
        let rat = ratpoly_from_expr(&expr).expect("ratpoly_from_expr failed");
        let alpha = ctx.alphabet_spec();
        let normalized = normalize_letter_for_alphabet(&expr, &alpha);
        let rebuilt = ratpoly_to_expr_for_alphabet(&rat, &alpha.letter_polys)
            .unwrap_or_else(|| expr.clone());
        let expanded = dlog_expand_letter(&normalized, &alpha)
            .expect("dlog_expand_letter should succeed");
        assert!(
            !expanded.is_empty(),
            "normalized: {}, rebuilt: {}",
            normalized.to_canonical_string(),
            rebuilt.to_canonical_string()
        );
    }

    #[test]
    fn factor_letter_powers() {
        let ctx = PentaContext::new();
        let expr = expr_mul(vec![
            expr_pow(ctx.one_minus_u.clone(), 3),
            expr_pow(ctx.one_minus_uw.clone(), 2),
        ]);
        let rat = ratpoly_from_expr(&expr).expect("ratpoly_from_expr failed");
        assert!(rat_poly_is_one(&rat.den));
        let letters = ctx.alphabet_spec().letter_polys;
        let factors = factor_poly_to_letters(&rat.num, &letters)
            .expect("factor_poly_to_letters failed");
        let rebuilt = expr_mul(factors);
        let expanded = dlog_expand_letter(&rebuilt, &ctx.alphabet_spec())
            .expect("dlog_expand_letter should succeed");
        assert!(!expanded.is_empty());
    }

    #[test]
    fn dlog_expand_letter_q_uv_tail_denominator_signs() {
        let ctx = PentaContext::new();
        let alpha = ctx.alphabet_spec();
        let numerator = expr_mul(vec![
            ctx.u.clone(),
            ctx.v.clone(),
            expr_pow(ctx.one_minus_w.clone(), 2),
        ]);
        let denominator = expr_mul(vec![
            ctx.one_minus_uw.clone(),
            ctx.one_minus_vw.clone(),
            expr_pow(ctx.delta.clone(), 2),
        ]);
        let expr = expr_div(numerator, denominator);
        let expanded = dlog_expand_letter(&expr, &alpha)
            .expect("dlog_expand_letter should succeed");
        let mut map: BTreeMap<String, Coeff> = BTreeMap::new();
        for (letter, coeff) in expanded {
            map.insert(expr_key(&letter), coeff);
        }
        let key_uw = expr_key(&ctx.one_minus_uw);
        let key_vw = expr_key(&ctx.one_minus_vw);
        assert_eq!(
            map.get(&key_uw).cloned().unwrap_or_else(Coeff::zero),
            Coeff::from_integer(-1),
            "expected -1 for dlog(1-uw)"
        );
        assert_eq!(
            map.get(&key_vw).cloned().unwrap_or_else(Coeff::zero),
            Coeff::from_integer(-1),
            "expected -1 for dlog(1-vw)"
        );
    }
}

#[cfg(test)]
mod repro_tests {
    use super::*;

    #[test]
    #[ignore = "debug repro for alphabet normalization failure"]
    fn repro_letter_not_in_alphabet() {
        let ctx = PentaContext::new();
        let alpha = ctx.alphabet_spec();
        let expr_str = r#"(+ (* (+ (- (* (^ (+ 1 (- u)) -1) u w)) (^ (+ 1 (- u)) -1)) (^ (+ 1 (- (* (^ (+ 1 (- u)) -1) u w)) (- (+ (- (* (^ (+ 1 (- u)) -1) u w)) (^ (+ 1 (- u)) -1))) (^ (+ 1 (- u)) -1)) -1)) (- (* (+ 1 (- w)) (^ (+ 1/2 (* -1/2 w) (- (+ 1 (- w)))) -1))))"#;
        let expr = mpl_ir::parse_sexpr(expr_str).expect("parse repro letter");
        let normalized = normalize_letter_for_alphabet(&expr, &alpha);
        let expanded = dlog_expand_letter(&normalized, &alpha);
        assert!(
            expanded.is_ok(),
            "normalized letter not in alphabet: {}",
            normalized.to_canonical_string()
        );
    }
}

#[cfg(test)]
mod shift_tests {
    use super::*;

    #[test]
    fn skip_last_entry_when_c_equals_d() {
        let ctx = PentaContext::new();
        let alpha = ctx.alphabet_spec();
        let last = expr_add(vec![ctx.t.clone(), ctx.one.clone()]);
        let word = vec![last];
        let source_last = &word[0];
        let normalized_last = normalize_last_entry(source_last, "t");
        let atom_ctx = AtomicIntegrateCtx {
            kernel_c: &ctx.one,
            ctx: &ctx,
            alpha: &alpha,
            stage: IntegrationStage::Y,
            source_last,
            normalized_last: &normalized_last,
        };
        let out = integrate_word_simple_atomic(&word, Coeff::one(), &atom_ctx)
        .expect("integrate_word_simple_atomic");
        assert!(out.is_empty(), "expected no last-entry contribution");
    }

    #[test]
    fn skip_last_entry_when_c_zero_d_one() {
        let ctx = PentaContext::new();
        let alpha = ctx.alphabet_spec();
        let last = expr_add(vec![ctx.t.clone(), ctx.one.clone()]);
        let word = vec![last];
        let source_last = &word[0];
        let normalized_last = normalize_last_entry(source_last, "t");
        let atom_ctx = AtomicIntegrateCtx {
            kernel_c: &ctx.zero,
            ctx: &ctx,
            alpha: &alpha,
            stage: IntegrationStage::XMinus,
            source_last,
            normalized_last: &normalized_last,
        };
        let out = integrate_word_simple_atomic(&word, Coeff::one(), &atom_ctx)
        .expect("integrate_word_simple_atomic");
        assert!(out.is_empty(), "expected no last-entry contribution");
    }

    #[test]
    fn skip_last_entry_when_c_one_d_zero() {
        let ctx = PentaContext::new();
        let alpha = ctx.alphabet_spec();
        let word = vec![ctx.t.clone()];
        let source_last = &word[0];
        let normalized_last = normalize_last_entry(source_last, "t");
        let atom_ctx = AtomicIntegrateCtx {
            kernel_c: &ctx.one,
            ctx: &ctx,
            alpha: &alpha,
            stage: IntegrationStage::Y,
            source_last,
            normalized_last: &normalized_last,
        };
        let out = integrate_word_simple_atomic(&word, Coeff::one(), &atom_ctx)
        .expect("integrate_word_simple_atomic");
        assert!(out.is_empty(), "expected no last-entry contribution");
    }

    #[test]
    fn normalize_one_minus_linear_ratio_keeps_mul_form() {
        let ctx = PentaContext::new();
        let expr = expr_add(vec![
            ctx.one.clone(),
            expr_neg(expr_mul(vec![
                expr_add(vec![ctx.t.clone(), ctx.w.clone()]),
                expr_pow(expr_add(vec![ctx.one.clone(), ctx.t.clone()]), -1),
            ])),
        ]);
        let normalized = normalize_last_entry(&expr, "t");
        let factors = match normalized.normalize() {
            Expr::Mul(children) => children,
            other => vec![other],
        };
        let target_one_minus_w =
            expr_add(vec![ctx.one.clone(), expr_neg(ctx.w.clone())]).normalize();
        let target_inv_t_plus_one =
            expr_pow(expr_add(vec![ctx.one.clone(), ctx.t.clone()]), -1).normalize();
        assert!(
            factors
                .iter()
                .any(|factor| expr_key(factor) == expr_key(&target_one_minus_w)),
            "missing (1-w) factor: {}",
            normalized.to_canonical_string()
        );
        assert!(
            factors
                .iter()
                .any(|factor| expr_key(factor) == expr_key(&target_inv_t_plus_one)),
            "missing (t+1)^-1 factor: {}",
            normalized.to_canonical_string()
        );
    }

    #[test]
    fn normalize_one_minus_vy_wy_is_linear_reducible() {
        let ctx = PentaContext::new();
        let vy = v_y_expr(&ctx);
        let wy = w_y_expr(&ctx);
        let expr = expr_sub(expr_one(), expr_mul(vec![vy, wy]));
        let normalized = normalize_last_entry(&expr, "t");
        let expansions = dlog_expand_linear(&normalized);
        assert!(
            !expansions.is_empty(),
            "expected dlog expansion for {}",
            normalized.to_canonical_string()
        );
        for (factor, _) in expansions {
            if expr_contains_var(&factor, "t") {
                assert!(
                    linear_shift(&factor, "t").is_some(),
                    "nonlinear factor: {}",
                    factor.to_canonical_string()
                );
            }
        }
    }

    #[test]
    #[ignore = "debug repro for linear_shift failure"]
    fn repro_last_entry_linear_shift() {
        let expr_str = r#"(+ 1 (* -1 (+ 1 (* -1 (+ 1 t) v) (* t v) (- (+ 1 (- v))))) (* -1 (+ 1 t) v) (* t v) (- (+ 1 (- (* v w)))))"#;
        let expr = mpl_ir::parse_sexpr(expr_str).expect("parse repro expr");
        let poly = poly_from_expr(&expr, "t");
        eprintln!("repro expr: {}", expr.to_canonical_string());
        if let Some(poly) = poly.as_ref() {
            eprintln!("poly_from_expr: len={}", poly.len());
            for (idx, coeff) in poly.iter().enumerate() {
                let simplified = simplify_expr(coeff);
                eprintln!(
                    "  coeff[{idx}] = {} | simplified={}",
                    coeff.to_canonical_string(),
                    simplified.to_canonical_string()
                );
            }
        } else {
            eprintln!("poly_from_expr: None");
        }
        if let Ok(Some(linear)) = as_linear(&expr, "t") {
            eprintln!(
                "as_linear: a={}, b={}",
                linear.a.to_canonical_string(),
                linear.b.to_canonical_string()
            );
        } else {
            eprintln!("as_linear: None");
        }
        let shift = linear_shift_with_fallback(&expr, "t");
        eprintln!(
            "linear_shift_with_fallback: {}",
            shift
                .as_ref()
                .map(|s| s.to_canonical_string())
                .unwrap_or_else(|| "None".to_string())
        );
        assert!(shift.is_some(), "expected linear shift");
    }
}

#[cfg(test)]
mod q_symmetry_tests {
    use super::*;

    #[test]
    fn q_uv_is_symmetric_under_swap() {
        let ctx = PentaContext::new();
        let base = q_uv_symbol(&ctx);
        let swapped = swap_u_v_symbol(&base, &ctx);
        assert_eq!(base, swapped);
    }

    #[test]
    fn q_w_is_symmetric_under_swap() {
        let ctx = PentaContext::new();
        let base = q_w_symbol(&ctx);
        let swapped = swap_u_v_symbol(&base, &ctx);
        assert_eq!(base, swapped);
    }

    #[test]
    fn q_u_over_v_is_antisymmetric_under_swap() {
        let ctx = PentaContext::new();
        let base = q_u_over_v_symbol(&ctx);
        let swapped = swap_u_v_symbol(&base, &ctx);
        let neg = symbol_scale(&base, Coeff::from_integer(-1));
        assert_eq!(swapped, neg);
    }
}

fn stream_expanded_terms<F>(
    sym: &Symbol,
    alpha: &AlphabetSpec,
    mut func: F,
) -> Result<(), ExperimentError>
where
    F: FnMut(Vec<String>, Coeff) -> Result<(), ExperimentError>,
{
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let expanded = expand_word_to_letters(word.letters(), alpha)?;
        for (letters, value) in expanded {
            let combined = *coeff * value;
            if combined.is_zero() {
                continue;
            }
            let names = letters
                .iter()
                .map(|expr| alpha.name_for_expr(expr))
                .collect::<Result<Vec<_>, _>>()?;
            func(names, combined)?;
        }
    }
    Ok(())
}

fn stream_alphabet_terms<F>(
    sym: &Symbol,
    alpha: &AlphabetSpec,
    mut func: F,
) -> Result<(), ExperimentError>
where
    F: FnMut(Vec<String>, Coeff) -> Result<(), ExperimentError>,
{
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let names = word
            .letters()
            .iter()
            .map(|expr| alpha.name_for_expr(expr))
            .collect::<Result<Vec<_>, _>>()?;
        func(names, *coeff)?;
    }
    Ok(())
}

fn validate_last_entry_names(
    word: &[String],
    allowed: &BTreeSet<String>,
    loop_value: usize,
) -> Result<(), ExperimentError> {
    if loop_value < 2 {
        return Ok(());
    }
    let last = word.last().ok_or_else(|| {
        ExperimentError::InvalidConfig("empty word in last-entry check".to_string())
    })?;
    if !allowed.contains(last) {
        return Err(ExperimentError::InvalidConfig(format!(
            "last-entry violation at L={loop_value}: {last}"
        )));
    }
    Ok(())
}

fn validate_last_entry_symbol(
    sym: &Symbol,
    alpha: &AlphabetSpec,
    allowed: &BTreeSet<String>,
    loop_value: usize,
) -> Result<(), ExperimentError> {
    if loop_value < 2 {
        return Ok(());
    }
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let last = word.letters().last().ok_or_else(|| {
            ExperimentError::InvalidConfig("empty word in last-entry check".to_string())
        })?;
        let name = alpha.name_for_expr(last)?;
        if !allowed.contains(&name) {
            return Err(ExperimentError::InvalidConfig(format!(
                "last-entry violation at L={loop_value}: {name}"
            )));
        }
    }
    Ok(())
}

fn write_symbol_jsonl(
    path: &Path,
    loop_value: usize,
    sym: &Symbol,
    alpha: &AlphabetSpec,
    max_terms: u64,
) -> Result<(), ExperimentError> {
    let merged_terms = sym.terms().count();
    if merged_terms as u64 > max_terms {
        return Err(ExperimentError::InvalidConfig(format!(
            "loop {loop_value} merged terms {merged_terms} exceed max_terms={max_terms}"
        )));
    }

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let meta = MetaLine {
        meta: MetaContent {
            name: "He2020PentaLadder",
            loop_index: loop_value,
            merged_terms,
        },
    };
    let meta_line = serde_json::to_string(&meta).map_err(|err| {
        ExperimentError::InvalidConfig(format!("json encode error: {err}"))
    })?;
    writer.write_all(meta_line.as_bytes())?;
    writer.write_all(b"\n")?;

    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let names = word
            .letters()
            .iter()
            .map(|expr| alpha.name_for_expr(expr))
            .collect::<Result<Vec<_>, _>>()?;
        let line = TermLine {
            word: names,
            coeff: format_coeff(coeff),
        };
        let encoded = serde_json::to_string(&line).map_err(|err| {
            ExperimentError::InvalidConfig(format!("json encode error: {err}"))
        })?;
        writer.write_all(encoded.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

#[derive(Serialize)]
struct MetaLine<'a> {
    #[serde(rename = "_meta")]
    meta: MetaContent<'a>,
}

#[derive(Serialize)]
struct MetaContent<'a> {
    name: &'a str,
    #[serde(rename = "loop")]
    loop_index: usize,
    merged_terms: usize,
}

#[derive(Serialize)]
struct TermLine {
    word: Vec<String>,
    coeff: String,
}

fn format_coeff(value: &Coeff) -> String {
    let numer = *value.numer();
    let denom = *value.denom();
    if denom == 1 {
        numer.to_string()
    } else {
        format!("{numer}/{denom}")
    }
}
fn q_w_symbol(ctx: &PentaContext) -> Symbol {
    // He 2020 Appendix A eq. (A.1).
    let base_terms = vec![
        (
            Word(vec![
                ctx.u.clone(),
                ctx.one_minus_u.clone(),
                expr_div(
                    expr_mul(vec![
                        ctx.v.clone(),
                        ctx.one_minus_w.clone(),
                        ctx.one_minus_uw.clone(),
                    ]),
                    expr_mul(vec![ctx.w.clone(), ctx.delta.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.v.clone(),
                expr_div(
                    expr_mul(vec![ctx.w.clone(), ctx.delta.clone()]),
                    expr_mul(vec![ctx.one_minus_uw.clone(), ctx.one_minus_vw.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.u.clone(),
                expr_div(ctx.one_minus_uw.clone(), ctx.one_minus_u.clone()),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.w.clone(),
                expr_div(
                    expr_mul(vec![
                        ctx.v.clone(),
                        ctx.one_minus_w.clone(),
                        ctx.one_minus_uw.clone(),
                    ]),
                    expr_mul(vec![ctx.one_minus_u.clone(), ctx.one_minus_vw.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.w.clone(),
                ctx.u.clone(),
                expr_div(
                    expr_mul(vec![
                        ctx.v.clone(),
                        ctx.one_minus_w.clone(),
                        ctx.one_minus_uw.clone(),
                    ]),
                    expr_mul(vec![ctx.one_minus_u.clone(), ctx.one_minus_vw.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.uw.clone(),
                ctx.one_minus_uw.clone(),
                expr_div(
                    expr_mul(vec![ctx.one_minus_u.clone(), ctx.w.clone(), ctx.delta.clone()]),
                    expr_mul(vec![
                        ctx.v.clone(),
                        ctx.one_minus_w.clone(),
                        expr_pow(ctx.one_minus_uw.clone(), 2),
                    ]),
                ),
            ]),
            Coeff::one(),
        ),
    ];
    let base = Symbol::from_terms(base_terms);
    let swapped = swap_u_v_symbol(&base, ctx);
    let mut sym = symbol_add(&base, &swapped);
    sym = symbol_add(
        &sym,
        &Symbol::from_terms(vec![(
            Word(vec![
                ctx.w.clone(),
                ctx.one_minus_w.clone(),
                expr_div(
                    expr_mul(vec![
                        ctx.u.clone(),
                        ctx.v.clone(),
                        expr_pow(ctx.one_minus_w.clone(), 2),
                    ]),
                    expr_mul(vec![
                        ctx.one_minus_uw.clone(),
                        ctx.one_minus_vw.clone(),
                        ctx.one_minus_u.clone(),
                        ctx.one_minus_v.clone(),
                        ctx.w.clone(),
                        ctx.delta.clone(),
                    ]),
                ),
            ]),
            Coeff::one(),
        )]),
    );
    sym
}

fn q_uv_symbol(ctx: &PentaContext) -> Symbol {
    // He 2020 Appendix A eq. (A.2).
    let base_terms = vec![
        (
            Word(vec![
                ctx.u.clone(),
                ctx.one_minus_u.clone(),
                expr_div(
                    expr_mul(vec![
                        ctx.one_minus_u.clone(),
                        ctx.u.clone(),
                        expr_pow(ctx.v.clone(), 2),
                        ctx.one_minus_w.clone(),
                        ctx.one_minus_uw.clone(),
                    ]),
                    expr_pow(ctx.delta.clone(), 2),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.u.clone(),
                expr_div(
                    ctx.one_minus_uw.clone(),
                    expr_mul(vec![ctx.one_minus_u.clone(), ctx.v.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.v.clone(),
                expr_div(
                    expr_pow(ctx.delta.clone(), 2),
                    expr_mul(vec![
                        ctx.u.clone(),
                        ctx.v.clone(),
                        ctx.one_minus_uw.clone(),
                        ctx.one_minus_vw.clone(),
                    ]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.w.clone(),
                expr_div(
                    expr_mul(vec![ctx.one_minus_w.clone(), ctx.one_minus_uw.clone()]),
                    ctx.one_minus_vw.clone(),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.w.clone(),
                ctx.u.clone(),
                expr_div(
                    expr_mul(vec![ctx.one_minus_w.clone(), ctx.one_minus_uw.clone()]),
                    ctx.one_minus_vw.clone(),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.uw.clone(),
                ctx.one_minus_uw.clone(),
                expr_div(
                    expr_pow(ctx.delta.clone(), 2),
                    expr_mul(vec![
                        ctx.u.clone(),
                        ctx.v.clone(),
                        ctx.one_minus_w.clone(),
                        expr_pow(ctx.one_minus_uw.clone(), 2),
                    ]),
                ),
            ]),
            Coeff::one(),
        ),
    ];
    let base = Symbol::from_terms(base_terms);
    let swapped = swap_u_v_symbol(&base, ctx);
    let mut sym = symbol_add(&base, &swapped);
    sym = symbol_add(
        &sym,
        &Symbol::from_terms(vec![(
            Word(vec![
                ctx.w.clone(),
                ctx.one_minus_w.clone(),
                expr_div(
                    expr_mul(vec![
                        ctx.u.clone(),
                        ctx.v.clone(),
                        expr_pow(ctx.one_minus_w.clone(), 2),
                    ]),
                    expr_mul(vec![
                        ctx.one_minus_uw.clone(),
                        ctx.one_minus_vw.clone(),
                        expr_pow(ctx.delta.clone(), 2),
                    ]),
                ),
            ]),
            Coeff::one(),
        )]),
    );
    sym
}

fn q_u_over_v_symbol(ctx: &PentaContext) -> Symbol {
    // He 2020 Appendix A eq. (A.3), antisymmetrized under u <-> v.
    let base_terms = vec![
        (
            Word(vec![
                ctx.u.clone(),
                ctx.u.clone(),
                expr_div(
                    expr_mul(vec![ctx.v.clone(), ctx.one_minus_uw.clone()]),
                    ctx.one_minus_u.clone(),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.v.clone(),
                expr_div(
                    expr_mul(vec![ctx.u.clone(), ctx.one_minus_vw.clone()]),
                    expr_mul(vec![ctx.v.clone(), ctx.one_minus_uw.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.w.clone(),
                expr_div(
                    expr_mul(vec![ctx.one_minus_uw.clone(), ctx.one_minus_vw.clone()]),
                    ctx.one_minus_w.clone(),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.w.clone(),
                ctx.u.clone(),
                expr_div(
                    expr_mul(vec![ctx.one_minus_uw.clone(), ctx.one_minus_vw.clone()]),
                    ctx.one_minus_w.clone(),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.w.clone(),
                ctx.one_minus_w.clone(),
                expr_mul(vec![ctx.v.clone(), ctx.one_minus_uw.clone()]),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.uw.clone(),
                ctx.one_minus_uw.clone(),
                expr_div(
                    expr_mul(vec![ctx.u.clone(), ctx.one_minus_w.clone()]),
                    expr_mul(vec![ctx.v.clone(), expr_pow(ctx.one_minus_uw.clone(), 2)]),
                ),
            ]),
            Coeff::one(),
        ),
        (
            Word(vec![
                ctx.u.clone(),
                ctx.one_minus_u.clone(),
                expr_div(
                    expr_mul(vec![ctx.one_minus_u.clone(), ctx.one_minus_uw.clone()]),
                    expr_mul(vec![ctx.u.clone(), ctx.one_minus_w.clone()]),
                ),
            ]),
            Coeff::one(),
        ),
    ];
    let base = Symbol::from_terms(base_terms);
    let swapped = swap_u_v_symbol(&base, ctx);
    symbol_sub(&base, &swapped)
}

fn swap_u_v_symbol(sym: &Symbol, ctx: &PentaContext) -> Symbol {
    let mut subst = BTreeMap::new();
    subst.insert("u".to_string(), ctx.v.clone());
    subst.insert("v".to_string(), ctx.u.clone());
    substitute_symbol(sym, &subst)
}

fn symbol_add(left: &Symbol, right: &Symbol) -> Symbol {
    let mut terms = Vec::new();
    for (word, coeff) in left.terms() {
        terms.push((word.clone(), *coeff));
    }
    for (word, coeff) in right.terms() {
        terms.push((word.clone(), *coeff));
    }
    Symbol::from_terms(terms)
}

fn symbol_sub(left: &Symbol, right: &Symbol) -> Symbol {
    symbol_add(left, &symbol_scale(right, Coeff::from_integer(-1)))
}

fn symbol_scale(sym: &Symbol, coeff: Coeff) -> Symbol {
    if coeff.is_zero() {
        return Symbol::zero();
    }
    let mut terms = Vec::new();
    for (word, value) in sym.terms() {
        let scaled = *value * coeff;
        if !scaled.is_zero() {
            terms.push((word.clone(), scaled));
        }
    }
    Symbol::from_terms(terms)
}

fn symbol_append(sym: &Symbol, letter: Expr) -> Symbol {
    let mut terms = Vec::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let mut letters = word.letters().to_vec();
        letters.push(letter.clone());
        terms.push((Word(letters), *coeff));
    }
    Symbol::from_terms(terms)
}

fn expr_var(name: &str) -> Expr {
    Expr::Var(name.to_string()).normalize()
}

fn expr_zero() -> Expr {
    Expr::Rational(Coeff::zero())
}

fn expr_one() -> Expr {
    Expr::Rational(Coeff::one())
}

fn expr_add(items: Vec<Expr>) -> Expr {
    if items.is_empty() {
        expr_zero()
    } else {
        Expr::Add(items).normalize()
    }
}

fn expr_mul(items: Vec<Expr>) -> Expr {
    if items.is_empty() {
        expr_one()
    } else {
        Expr::Mul(items).normalize()
    }
}

fn expr_neg(expr: Expr) -> Expr {
    Expr::Neg(Box::new(expr)).normalize()
}

fn expr_pow(expr: Expr, exp: i32) -> Expr {
    Expr::Pow(Box::new(expr), exp).normalize()
}

fn expr_div(num: Expr, denom: Expr) -> Expr {
    expr_mul(vec![num, expr_pow(denom, -1)])
}

fn expr_sub(left: Expr, right: Expr) -> Expr {
    expr_add(vec![left, expr_neg(right)])
}

fn expr_one_minus(expr: Expr) -> Expr {
    expr_sub(expr_one(), expr)
}

fn expr_contains_var(expr: &Expr, var: &str) -> bool {
    match expr {
        Expr::Var(name) => name == var,
        Expr::Add(children) | Expr::Mul(children) => {
            children.iter().any(|child| expr_contains_var(child, var))
        }
        Expr::Neg(inner) | Expr::Pow(inner, _) | Expr::Log(inner) | Expr::Li2(inner) => {
            expr_contains_var(inner, var)
        }
        Expr::Rational(_) => false,
    }
}

fn expr_contains_any_var(expr: &Expr, vars: &[&str]) -> bool {
    vars.iter().any(|var| expr_contains_var(expr, var))
}

fn expr_is_zero(expr: &Expr) -> bool {
    matches!(expr, Expr::Rational(value) if value.is_zero())
}

fn expr_is_one(expr: &Expr) -> bool {
    matches!(expr, Expr::Rational(value) if value.is_one())
}

fn expr_key(expr: &Expr) -> String {
    expr.normalize().to_canonical_string()
}
