use std::fs;
use std::path::Path;

use crate::output::csv::CsvWriter;
use crate::run::filtration::FiltrationReport;
use crate::{ExperimentError, Status};

pub fn write_filtration_summary(
    report: &FiltrationReport,
    out_dir: &Path,
) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;
    fs::write(
        out_dir.join("filtration_summary.csv"),
        render_filtration_summary_csv(report),
    )?;
    fs::write(
        out_dir.join("filtration_summary.md"),
        render_filtration_summary_md(report),
    )?;
    Ok(())
}

pub fn render_filtration_summary_csv(report: &FiltrationReport) -> String {
    let mut writer = CsvWriter::new();
    writer.push_raw("layer_index,layer_name,weight,mode,status,error_code,error,n_words_allowed,dim,rank,basis_ncols,rows_attempted,rows_inserted,samples_used,envs_total,sample_table,constraints_insufficient_samples\n");
    for row in &report.rows {
        let error_code = row.error_code.map(|code| code.as_str()).unwrap_or("");
        let dim = row.dim.map(|value| value.to_string()).unwrap_or_default();
        let rank = row.rank.map(|value| value.to_string()).unwrap_or_default();
        let basis_ncols = row
            .basis_ncols
            .map(|value| value.to_string())
            .unwrap_or_default();
        let rows_attempted = row
            .rows_attempted
            .map(|value| value.to_string())
            .unwrap_or_default();
        let rows_inserted = row
            .rows_inserted
            .map(|value| value.to_string())
            .unwrap_or_default();
        let samples_used = row
            .samples_used
            .map(|value| value.to_string())
            .unwrap_or_default();
        let envs_total = row
            .envs_total
            .map(|value| value.to_string())
            .unwrap_or_default();
        let constraints_insufficient_samples = row
            .constraints_insufficient_samples
            .map(|value| value.to_string())
            .unwrap_or_default();

        writer.push_record([
            row.layer_index.to_string(),
            row.layer_name.clone(),
            row.weight.to_string(),
            row.mode.as_str().to_string(),
            row.status.as_str().to_string(),
            error_code.to_string(),
            error_code.to_string(),
            row.n_words_allowed.to_string(),
            dim,
            rank,
            basis_ncols,
            rows_attempted,
            rows_inserted,
            samples_used,
            envs_total,
            row.sample_table.as_str().to_string(),
            constraints_insufficient_samples,
        ]);
    }
    writer.into_string()
}

pub fn render_filtration_summary_md(report: &FiltrationReport) -> String {
    let mut out = String::new();
    out.push_str("# Filtration Summary\n\n");
    out.push_str(&format!("id: {}\n\n", report.name));
    out.push_str("## Layers\n");
    for layer in &report.layers {
        out.push_str(&format!(
            "- {}: {} (mode: {})\n",
            layer.index,
            layer.name,
            layer.mode.as_str()
        ));
    }
    out.push('\n');
    out.push_str("## Summary\n");
    out.push_str("| layer | weight | n_words_allowed | dim | status | error_code |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for row in &report.rows {
        let dim = row.dim.map(|value| value.to_string()).unwrap_or_default();
        let error_code = row.error_code.map(|code| code.as_str()).unwrap_or("");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            row.layer_name,
            row.weight,
            row.n_words_allowed,
            dim,
            row.status.as_str(),
            error_code
        ));
    }
    out.push('\n');
    out.push_str("## Failures\n");
    let mut failures = 0;
    for row in &report.rows {
        if row.status == Status::Err {
            let error_code = row.error_code.map(|code| code.as_str()).unwrap_or("");
            out.push_str(&format!(
                "- layer={} weight={} error={}\n",
                row.layer_name, row.weight, error_code
            ));
            failures += 1;
        }
    }
    if failures == 0 {
        out.push_str("- none\n");
    }
    out
}
