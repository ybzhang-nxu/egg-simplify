use std::fs;
use std::path::Path;

use crate::build::alphabet::letter_display_names;
use crate::output::csv::{escape_csv_field, vars_csv};
use crate::run::count::CountReport;
use crate::run::single::{ExperimentReport, GenealogicalKind};
use crate::ExperimentError;

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
        out_dir.join("forbidden_pairs.csv"),
        render_forbidden_pairs(report),
    )?;
    fs::write(
        out_dir.join("genealogical_rules.json"),
        render_genealogical_rules(report),
    )?;
    fs::write(
        out_dir.join("topology_metrics.csv"),
        render_topology_metrics(report),
    )?;
    fs::write(
        out_dir.join("skeleton2_metrics.csv"),
        render_skeleton2_metrics(report),
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
    out.push_str("weight,n_words_allowed,dim,rank,rows_attempted,rows_inserted,samples_used,envs_total,sample_table,rows_skipped_singular,constraints_insufficient_samples,vars,max_row_nnz,avg_row_nnz,status,error_code,error\n");
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
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            summary.weight,
            summary.n_words_allowed,
            stats.dim,
            stats.rank,
            stats.rows_attempted,
            stats.rows_inserted,
            stats.samples_used,
            stats.envs_total,
            escape_csv_field(stats.sample_table.as_str()),
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

pub fn render_forbidden_pairs(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("lhs,rhs\n");

    let keys = &report.genealogical.keys;
    for &(lhs_idx, rhs_idx) in &report.genealogical.forbidden_pairs {
        let lhs = match keys.get(lhs_idx) {
            Some(value) => value,
            None => continue,
        };
        let rhs = match keys.get(rhs_idx) {
            Some(value) => value,
            None => continue,
        };
        out.push_str(&format!(
            "{},{}\n",
            escape_csv_field(lhs),
            escape_csv_field(rhs)
        ));
    }
    out
}

pub fn render_genealogical_rules(report: &ExperimentReport) -> String {
    const NOTES: &str =
        "A->B forbidden means: no support word contains A at position i and B at position j>i";
    let rules = &report.genealogical;
    let key_label = match rules.kind {
        GenealogicalKind::Channel => "channels",
        GenealogicalKind::Letter => "letters",
    };

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"kind\": \"{}\",\n", rules.kind.as_str()));
    out.push_str(&format!("  \"method\": \"{}\",\n", rules.method));
    out.push_str(&format!("  \"weight_min\": {},\n", rules.weight_min));
    out.push_str(&format!("  \"weight_max\": {},\n", rules.weight_max));
    out.push_str(&format!(
        "  \"n_support_words\": {},\n",
        rules.n_support_words
    ));
    out.push_str(&format!("  \"{key_label}\": ["));
    let mut first = true;
    for key in &rules.keys {
        if !first {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(&escape_json_string(key));
        out.push('"');
        first = false;
    }
    out.push_str("],\n");
    out.push_str("  \"forbidden_pairs\": [");
    let mut first_pair = true;
    for (lhs_idx, rhs_idx) in &rules.forbidden_pairs {
        let lhs = match rules.keys.get(*lhs_idx) {
            Some(value) => value,
            None => continue,
        };
        let rhs = match rules.keys.get(*rhs_idx) {
            Some(value) => value,
            None => continue,
        };
        if !first_pair {
            out.push_str(", ");
        }
        out.push('[');
        out.push('"');
        out.push_str(&escape_json_string(lhs));
        out.push_str("\", \"");
        out.push_str(&escape_json_string(rhs));
        out.push_str("\"]");
        first_pair = false;
    }
    out.push_str("],\n");
    out.push_str("  \"notes\": \"");
    out.push_str(&escape_json_string(NOTES));
    out.push_str("\"\n");
    out.push_str("}\n");
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

pub fn render_skeleton2_metrics(report: &ExperimentReport) -> String {
    let mut out = String::new();
    out.push_str("weight,status,error_code,error,n_vertices,n_edges_undirected,triangles,clustering_num,clustering_den,beta1_est,triplets_supported_by_triangles_num,triplets_supported_by_triangles_den\n");
    for summary in &report.summaries {
        let metrics = &summary.skeleton2;
        let status_field = summary.status.as_str();
        let error_code = summary.error_code.map(|code| code.as_str()).unwrap_or("");
        let error_code_field = escape_csv_field(error_code);
        let error_field = error_code_field.clone();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            summary.weight,
            escape_csv_field(status_field),
            error_code_field,
            error_field,
            metrics.n_vertices,
            metrics.n_edges_undirected,
            metrics.triangles,
            metrics.clustering_num,
            metrics.clustering_den,
            metrics.beta1_est,
            metrics.triplets_supported_by_triangles_num,
            metrics.triplets_supported_by_triangles_den
        ));
    }
    out
}

fn escape_json_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
