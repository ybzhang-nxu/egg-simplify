use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use mpl_symbol::Coeff;

use crate::analysis::esymb_rank_scan::{
    EsymbRankScanReport, NormalizeMode, Recurrence, ScreenStatus, SequenceAnalysis,
};
use crate::output::csv::CsvWriter;
use crate::ExperimentError;

pub fn write_esymb_rank_scan_report(
    report: &EsymbRankScanReport,
    out_dir: &Path,
) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;
    fs::write(
        out_dir.join("rank_scan.csv"),
        render_esymb_rank_scan_csv(report),
    )?;
    fs::write(
        out_dir.join("summary.md"),
        render_esymb_rank_scan_md(report),
    )?;
    Ok(())
}

pub fn render_esymb_rank_scan_csv(report: &EsymbRankScanReport) -> String {
    let mut writer = CsvWriter::new();
    let mut header = vec![
        "alphabet_mode".to_string(),
        "pairs_mode".to_string(),
        "family".to_string(),
        "params".to_string(),
        "normalize_mode_selected".to_string(),
        "normalize_candidates_tried".to_string(),
        "normalize_skipped".to_string(),
        "screen_status".to_string(),
        "nmax".to_string(),
        "recovered".to_string(),
    ];
    for loop_index in &report.loops {
        header.push(format!("cL{loop_index}"));
    }
    header.push("rank_mod_p_by_N".to_string());
    header.push("rank_float_by_N".to_string());
    header.push("rank_subsample_by_N".to_string());
    header.push("plateau_rank".to_string());
    header.push("recurrence_order".to_string());
    header.push("recurrence_coeffs".to_string());
    header.push("predict_next_d".to_string());
    header.push("predict_next_c".to_string());
    header.push("candidate_solve_attempted".to_string());
    header.push("candidate_recurrence".to_string());
    header.push("candidate_predict_next_d".to_string());
    header.push("candidate_predict_next_c".to_string());
    writer.push_record(header);

    for seq in &report.sequences {
        let mut row = Vec::new();
        row.push(report.alphabet_mode.as_str().to_string());
        row.push(report.pairs_mode.as_str().to_string());
        row.push(seq.spec.family.as_str().to_string());
        row.push(seq.spec.param_string());
        row.push(seq.normalize_mode.as_str().to_string());
        row.push(format_mode_list(&seq.normalize_candidates_tried));
        row.push(format_string_list(&seq.normalize_skipped));
        row.push(seq.screen_status.as_str().to_string());
        row.push(seq.nmax.to_string());
        row.push(if seq.recovered { "true" } else { "false" }.to_string());
        for value in &seq.values {
            row.push(format_coeff(value));
        }
        row.push(format_usize_list(&seq.rank_mod_p));
        row.push(format_usize_list(&seq.rank_float));
        row.push(format_usize_list(&seq.rank_subsample));
        row.push(opt_usize(seq.plateau_rank));
        row.push(
            seq.recurrence
                .as_ref()
                .map(|r| r.order.to_string())
                .unwrap_or_default(),
        );
        row.push(
            seq.recurrence
                .as_ref()
                .map(format_recurrence)
                .unwrap_or_default(),
        );
        row.push(
            seq.predict_next_d
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default(),
        );
        row.push(
            seq.predict_next_c
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default(),
        );
        row.push(if seq.candidate_solve_attempted {
            "true".to_string()
        } else {
            "false".to_string()
        });
        row.push(
            seq.candidate_recurrence
                .as_ref()
                .map(render_recurrence_d)
                .unwrap_or_default(),
        );
        row.push(
            seq.candidate_predict_next_d
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default(),
        );
        row.push(
            seq.candidate_predict_next_c
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default(),
        );
        writer.push_record(row);
    }

    writer.into_string()
}

pub fn render_esymb_rank_scan_md(report: &EsymbRankScanReport) -> String {
    let mut out = String::new();
    out.push_str("# esymb_rank_scan\n\n");
    out.push_str(&format!("loops = {}\n\n", format_usize_list(&report.loops)));
    out.push_str(&format!("primes = {}\n\n", format_i64_list(&report.primes)));
    out.push_str(&format!("seed = {}\n\n", report.seed));
    out.push_str(&format!(
        "alphabet_mode = {}\n\n",
        report.alphabet_mode.as_str()
    ));
    if report.alphabet_mode == crate::analysis::esymb_rank_scan::AlphabetMode::Auto {
        out.push_str(&format!(
            "auto_discovered_letters = {}\n\n",
            format_string_list(&report.auto_discovered_letters)
        ));
    }
    out.push_str(&format!("pairs_mode = {}\n\n", report.pairs_mode.as_str()));
    if report.pairs_mode == crate::analysis::esymb_rank_scan::PairsMode::Auto {
        out.push_str(&format!(
            "auto_discovered_pairs_count = {}\n\n",
            report.auto_discovered_pairs_count
        ));
    }
    out.push_str(&format!(
        "attempt_solve_inconclusive = {}\n\n",
        if report.attempt_solve_inconclusive {
            "true"
        } else {
            "false"
        }
    ));

    out.push_str("## loop_meta\n\n");
    for meta in &report.loop_meta {
        out.push_str(&format!(
            "- L{} merged_terms={} source={}\n",
            meta.loop_index,
            meta.merged_terms,
            meta.source.display()
        ));
    }
    out.push('\n');

    out.push_str("## screen_summary\n\n");
    let counts = count_statuses(&report.sequences);
    out.push_str(&format!("- pass(nontrivial)={}\n", counts.pass_nontrivial));
    out.push_str(&format!("- inconclusive={}\n", counts.inconclusive));
    out.push_str(&format!("- fail={}\n", counts.fail));
    out.push_str(&format!("- trivial={}\n\n", counts.trivial));

    out.push_str("## recurrences_recovered\n\n");
    render_recovered_table(&mut out, &report.sequences);
    out.push('\n');

    if report.attempt_solve_inconclusive {
        out.push_str("## candidate_recurs\n\n");
        render_candidate_table(&mut out, &report.sequences);
        out.push('\n');
    }

    out.push_str("## sequences\n\n");
    render_sequence_group(&mut out, "pass", ScreenStatus::Pass, &report.sequences);
    render_sequence_group(
        &mut out,
        "inconclusive",
        ScreenStatus::Inconclusive,
        &report.sequences,
    );
    render_sequence_group(&mut out, "fail", ScreenStatus::Fail, &report.sequences);
    render_sequence_group(
        &mut out,
        "trivial",
        ScreenStatus::Trivial,
        &report.sequences,
    );
    out
}

fn render_sequence_md(out: &mut String, seq: &SequenceAnalysis) {
    out.push_str(&format!(
        "- family={} params={}\n",
        seq.spec.family.as_str(),
        seq.spec.param_string()
    ));
    out.push_str(&format!(
        "  normalize_mode={}\n",
        seq.normalize_mode.as_str()
    ));
    out.push_str(&format!(
        "  normalize_candidates_tried={}\n",
        format_mode_list(&seq.normalize_candidates_tried)
    ));
    if !seq.normalize_skipped.is_empty() {
        out.push_str(&format!(
            "  normalize_skipped={}\n",
            format_string_list(&seq.normalize_skipped)
        ));
    }
    out.push_str(&format!("  screen_status={}\n", seq.screen_status.as_str()));
    out.push_str(&format!(
        "  recovered={}\n",
        if seq.recovered { "true" } else { "false" }
    ));
    out.push_str(&format!("  nmax={}\n", seq.nmax));
    out.push_str(&format!("  values={}\n", format_coeff_list(&seq.values)));
    if seq.normalize_mode != NormalizeMode::None {
        out.push_str(&format!(
            "  normalized_values={}\n",
            format_coeff_list(&seq.normalized_values)
        ));
        out.push_str(&format!(
            "  normalize_formula={}\n",
            normalize_formula(seq.normalize_mode)
        ));
    }
    out.push_str(&format!(
        "  rank_mod_p={}\n",
        format_usize_list(&seq.rank_mod_p)
    ));
    if !seq.rank_float.is_empty() {
        out.push_str(&format!(
            "  rank_float={}\n",
            format_usize_list(&seq.rank_float)
        ));
    }
    if !seq.rank_subsample.is_empty() {
        out.push_str(&format!(
            "  rank_subsample={}\n",
            format_usize_list(&seq.rank_subsample)
        ));
    }
    out.push_str(&format!("  plateau_rank={}\n", opt_usize(seq.plateau_rank)));
    if let Some(rec) = seq.recurrence.as_ref() {
        out.push_str(&format!(
            "  recurrence_order={} coeffs={}\n",
            rec.order,
            format_coeff_list(&rec.coeffs)
        ));
        out.push_str(&format!("  recurrence_d={}\n", render_recurrence_d(rec)));
        if seq.normalize_mode != NormalizeMode::None {
            out.push_str(&format!(
                "  mapped_recurrence={}\n",
                render_mapped_recurrence(rec)
            ));
        }
    }
    if let Some(next) = seq.predict_next_d.as_ref() {
        out.push_str(&format!("  predict_next_d={}\n", format_coeff(next)));
    }
    if let Some(next) = seq.predict_next_c.as_ref() {
        out.push_str(&format!("  predict_next_c={}\n", format_coeff(next)));
    }
    if seq.candidate_solve_attempted {
        out.push_str("  candidate_solve_attempted=true\n");
        if let Some(rec) = seq.candidate_recurrence.as_ref() {
            out.push_str(&format!(
                "  candidate_recurrence_d={}\n",
                render_recurrence_d(rec)
            ));
            out.push_str(&format!(
                "  candidate_mapped_recurrence={}\n",
                render_mapped_recurrence(rec)
            ));
        }
        if let Some(next) = seq.candidate_predict_next_d.as_ref() {
            out.push_str(&format!(
                "  candidate_predict_next_d={}\n",
                format_coeff(next)
            ));
        }
        if let Some(next) = seq.candidate_predict_next_c.as_ref() {
            out.push_str(&format!(
                "  candidate_predict_next_c={}\n",
                format_coeff(next)
            ));
        }
    }
}

struct StatusCounts {
    pass_nontrivial: usize,
    inconclusive: usize,
    fail: usize,
    trivial: usize,
}

fn count_statuses(sequences: &[SequenceAnalysis]) -> StatusCounts {
    let mut counts = StatusCounts {
        pass_nontrivial: 0,
        inconclusive: 0,
        fail: 0,
        trivial: 0,
    };
    for seq in sequences {
        match seq.screen_status {
            ScreenStatus::Pass => {
                if seq.recovered {
                    counts.pass_nontrivial += 1;
                }
            }
            ScreenStatus::Inconclusive => counts.inconclusive += 1,
            ScreenStatus::Fail => counts.fail += 1,
            ScreenStatus::Trivial => counts.trivial += 1,
        }
    }
    counts
}

fn render_recovered_table(out: &mut String, sequences: &[SequenceAnalysis]) {
    let mut groups: BTreeMap<String, usize> = BTreeMap::new();
    let mut group_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut rows: BTreeMap<usize, Vec<String>> = BTreeMap::new();

    for seq in sequences {
        if !seq.recovered {
            continue;
        }
        let rec = match seq.recurrence.as_ref() {
            Some(value) => value,
            None => continue,
        };
        let key = format!(
            "{}|{}|{}",
            seq.normalize_mode.as_str(),
            format_coeff_list(&seq.normalized_values),
            format_coeff_list(&rec.coeffs)
        );
        let next_id = groups.len() + 1;
        let group_id = *groups.entry(key).or_insert(next_id);
        *group_counts.entry(group_id).or_insert(0) += 1;
        if !rows.contains_key(&group_id) {
            let row = vec![
                group_id.to_string(),
                "1".to_string(),
                seq.spec.family.as_str().to_string(),
                seq.spec.param_string(),
                seq.normalize_mode.as_str().to_string(),
                rec.order.to_string(),
                render_recurrence_d(rec),
                render_mapped_recurrence(rec),
                seq.predict_next_d
                    .as_ref()
                    .map(format_coeff)
                    .unwrap_or_default(),
                seq.predict_next_c
                    .as_ref()
                    .map(format_coeff)
                    .unwrap_or_default(),
            ];
            rows.insert(group_id, row);
        }
    }

    if rows.is_empty() {
        out.push_str("_none_\n");
        return;
    }

    out.push_str("| group_id | count | family | params | normalize | order | recurrence_d | mapped_recurrence | predict_next_d | predict_next_c |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");

    for (group_id, mut row) in rows {
        if let Some(count) = group_counts.get(&group_id) {
            row[1] = count.to_string();
        }
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8], row[9]
        ));
    }
}

fn render_candidate_table(out: &mut String, sequences: &[SequenceAnalysis]) {
    let mut rows = Vec::new();
    for seq in sequences {
        if !seq.candidate_solve_attempted {
            continue;
        }
        if seq.screen_status == ScreenStatus::Pass {
            continue;
        }
        let rec = match seq.candidate_recurrence.as_ref() {
            Some(value) => value,
            None => continue,
        };
        let row = vec![
            seq.spec.family.as_str().to_string(),
            seq.spec.param_string(),
            seq.normalize_mode.as_str().to_string(),
            rec.order.to_string(),
            render_recurrence_d(rec),
            render_mapped_recurrence(rec),
            seq.candidate_predict_next_d
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default(),
            seq.candidate_predict_next_c
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default(),
            seq.screen_status.as_str().to_string(),
        ];
        rows.push(row);
    }

    if rows.is_empty() {
        out.push_str("_none_\n");
        return;
    }

    out.push_str("| family | params | normalize | order | candidate_recurrence_d | mapped_recurrence | candidate_predict_next_d | candidate_predict_next_c | screen_status |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for row in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8]
        ));
    }
}

fn render_sequence_group(
    out: &mut String,
    label: &str,
    status: ScreenStatus,
    sequences: &[SequenceAnalysis],
) {
    out.push_str(&format!("### {label}\n\n"));
    let mut any = false;
    for seq in sequences {
        if seq.screen_status == status {
            render_sequence_md(out, seq);
            any = true;
        }
    }
    if !any {
        out.push_str("_none_\n");
    }
    out.push('\n');
}

fn format_recurrence(rec: &Recurrence) -> String {
    format_coeff_list(&rec.coeffs)
}

fn normalize_formula(mode: NormalizeMode) -> String {
    match mode {
        NormalizeMode::None => "d_L = c_L".to_string(),
        NormalizeMode::OddDoubleFactorial => "d_L = c_L / (2L-3)!!".to_string(),
        NormalizeMode::EvenDoubleFactorial => "d_L = c_L / (2L-2)!!".to_string(),
        NormalizeMode::FactorialLm1 => "d_L = c_L / (L-1)!".to_string(),
        NormalizeMode::CentralBinomialLm1 => "d_L = c_L / C(2L-2, L-1)".to_string(),
    }
}

fn render_recurrence_d(rec: &Recurrence) -> String {
    if rec.order == 1 {
        let rho = -rec.coeffs[0];
        return format!("d_L = {} d_{{L-1}}", format_coeff(&rho));
    }
    let mut parts = Vec::new();
    for (idx, coeff) in rec.coeffs.iter().take(rec.order).enumerate() {
        let term = format!("{} * d_{{L-{}}}", format_coeff(coeff), rec.order - idx);
        parts.push(term);
    }
    format!("d_L = -({})", parts.join(" + "))
}

fn render_mapped_recurrence(rec: &Recurrence) -> String {
    if rec.order == 1 {
        let rho = -rec.coeffs[0];
        return format!("c_L = {} * (2L-3) * c_{{L-1}}", format_coeff(&rho));
    }
    let mut terms = Vec::new();
    let r = rec.order;
    for k in 0..r {
        let alpha = -rec.coeffs[k];
        let lower = format!("L-{}", r - k);
        let upper = "L-1".to_string();
        let product = format!("prod_{{j={}}}^{{{}}}(2j-1)", lower, upper);
        let term = format!("{} * {} * c_{{L-{}}}", format_coeff(&alpha), product, r - k);
        terms.push(term);
    }
    format!("c_L = {}", terms.join(" + "))
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

fn format_coeff_list(values: &[Coeff]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let rendered = values
        .iter()
        .map(format_coeff)
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}

fn format_usize_list(values: &[usize]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let rendered = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}

fn format_i64_list(values: &[i64]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let rendered = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}

fn format_mode_list(values: &[NormalizeMode]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let rendered = values
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}

fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let rendered = values.join(", ");
    format!("[{rendered}]")
}

fn opt_usize(value: Option<usize>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "none".to_string())
}
