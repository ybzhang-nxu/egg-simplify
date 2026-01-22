use std::fs;
use std::path::Path;

use mpl_symbol::Coeff;
use num_traits::Zero;

use crate::analysis::cross_loop::{CrossLoopReport, CrossLoopScanReport, MappingReport};
use crate::output::csv::escape_csv_field;
use crate::ExperimentError;

pub fn write_cross_loop_outputs(
    report: &CrossLoopReport,
    out_dir: &Path,
    export_constraints: bool,
) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;
    fs::write(
        out_dir.join("cross_loop_report.txt"),
        render_cross_loop_report(report),
    )?;

    if let Some(mapping) = report.mapping.as_ref() {
        fs::write(
            out_dir.join("mapping_shape.txt"),
            render_shape(
                mapping.matrix.len(),
                mapping.matrix.first().map(|r| r.len()).unwrap_or(0),
            ),
        )?;
        fs::write(
            out_dir.join("mapping_matrix.csv"),
            render_sparse_matrix_csv(&mapping.matrix),
        )?;
        fs::write(out_dir.join("residuals.txt"), render_residuals(mapping))?;

        if export_constraints {
            if !mapping.failed_cols.is_empty() {
                return Err(ExperimentError::InvalidConfig(
                    "cannot export coupled constraints: mapping has failed columns".to_string(),
                ));
            }
            let rows = mapping.matrix.len();
            let cols = mapping
                .matrix
                .first()
                .map(|row| row.len())
                .unwrap_or(0)
                .saturating_add(rows);
            fs::write(
                out_dir.join("constraints_shape.txt"),
                render_shape(rows, cols),
            )?;
            fs::write(
                out_dir.join("constraints_coupled.csv"),
                render_coupled_constraints_csv(&mapping.matrix),
            )?;
        }
    }

    Ok(())
}

pub fn write_cross_loop_scan_outputs(
    report: &CrossLoopScanReport,
    out_dir: &Path,
) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;
    fs::write(
        out_dir.join("cross_loop_scan.csv"),
        render_cross_loop_scan_csv(report),
    )?;
    fs::write(
        out_dir.join("cross_loop_scan_fits.txt"),
        render_scan_fits(report),
    )?;
    Ok(())
}

pub fn render_cross_loop_report(report: &CrossLoopReport) -> String {
    let mut out = String::new();
    out.push_str("cross_loop_report\n");
    out.push_str(&format!("weight={}\n", report.weight));
    out.push_str(&format!("lower_weight={}\n", report.lower_weight));
    out.push_str(&format!("suffix={}\n", format_list(&report.suffix.names)));
    out.push_str(&format!("upper_dim={}\n", report.upper_dim));
    out.push_str(&format!("lower_dim={}\n", report.lower_dim));
    out.push_str(&format!(
        "image_rank={} rows={} zero_cols={} row_limit_hit={}\n",
        report.image_rank.rank,
        report.image_rank.row_count,
        report.image_rank.zero_columns.len(),
        report.image_rank.row_limit_hit
    ));
    out.push_str(&format!(
        "pivot_cols={}\n",
        format_usize_list(&report.image_rank.pivot_columns)
    ));

    match report.mapping.as_ref() {
        Some(mapping) => {
            out.push_str(&format!(
                "mapping_rank={} success_cols={} failed_cols={}\n",
                mapping.rank,
                mapping.success_cols.len(),
                mapping.failed_cols.len()
            ));
            out.push_str(&format!(
                "residual_rank={} residual_rows={}\n",
                mapping.residual_rank, mapping.residual_row_count
            ));
            out.push_str(&format!(
                "rank_one={}\n",
                if mapping.rank_one.is_some() {
                    "true"
                } else {
                    "false"
                }
            ));
        }
        None => {
            out.push_str("mapping_rank=\n");
        }
    }

    out
}

pub fn render_cross_loop_scan_csv(report: &CrossLoopScanReport) -> String {
    let mut out = String::new();
    out.push_str("weight,suffix_len,n_suffixes_total,suffix_index,image_rank,row_count,zero_columns,mapping_rank,mapping_failed,rank_one,prefactor_col,prefactor_value\n");
    let suffix_len = report.suffix.ids.len();
    for row in &report.rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            row.weight,
            suffix_len,
            report.suffix_total,
            report.suffix_index,
            row.image_rank,
            row.row_count,
            row.zero_columns,
            opt_usize(row.mapping_rank),
            opt_usize(row.mapping_failed),
            if row.rank_one { "true" } else { "false" },
            opt_usize(row.prefactor_col),
            row.prefactor_value
                .as_ref()
                .map(format_coeff)
                .unwrap_or_default()
        ));
    }
    out
}

pub fn render_cross_loop_scan_index(rows: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str("suffix,dir\n");
    for (suffix, dir) in rows {
        out.push_str(&format!(
            "{},{}\n",
            escape_csv_field(suffix),
            escape_csv_field(dir)
        ));
    }
    out
}

fn render_scan_fits(report: &CrossLoopScanReport) -> String {
    let mut out = String::new();
    out.push_str("suffix=");
    out.push_str(&format_list(&report.suffix.names));
    out.push('\n');
    out.push_str(&format!("suffix_len={}\n", report.suffix.ids.len()));
    out.push_str(&format!("suffix_index={}\n", report.suffix_index));
    out.push_str(&format!("n_suffixes_total={}\n", report.suffix_total));
    for fit in &report.fits {
        out.push_str(&format!(
            "model={} scale={}\n",
            fit.model,
            format_coeff(&fit.scale)
        ));
    }
    out
}

fn render_sparse_matrix_csv(matrix: &[Vec<Coeff>]) -> String {
    let mut out = String::new();
    out.push_str("row,col,value\n");
    for (row_idx, row) in matrix.iter().enumerate() {
        for (col_idx, value) in row.iter().enumerate() {
            if value.is_zero() {
                continue;
            }
            out.push_str(&format!(
                "{},{},{}\n",
                row_idx,
                col_idx,
                format_coeff(value)
            ));
        }
    }
    out
}

fn render_coupled_constraints_csv(matrix: &[Vec<Coeff>]) -> String {
    let mut out = String::new();
    out.push_str("row,col,value\n");
    let cols_upper = matrix.first().map(|row| row.len()).unwrap_or(0);
    for (row_idx, row) in matrix.iter().enumerate() {
        for (col_idx, value) in row.iter().enumerate() {
            if value.is_zero() {
                continue;
            }
            out.push_str(&format!(
                "{},{},{}\n",
                row_idx,
                col_idx,
                format_coeff(value)
            ));
        }
        let col = cols_upper + row_idx;
        out.push_str(&format!("{},{},-1\n", row_idx, col));
    }
    out
}

fn render_residuals(mapping: &MappingReport) -> String {
    let mut out = String::new();
    out.push_str("column,word_count,sample_words\n");
    for residual in &mapping.residuals {
        out.push_str(&format!(
            "{},{},{}\n",
            residual.column,
            residual.word_count,
            escape_csv_field(&format_list(&residual.sample_words))
        ));
    }
    out
}

fn render_shape(rows: usize, cols: usize) -> String {
    format!("rows={rows}\ncols={cols}\n")
}

fn format_list(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    format!("[{}]", items.join(", "))
}

fn format_usize_list(items: &[usize]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let rendered = items
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
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

fn opt_usize(value: Option<usize>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}
