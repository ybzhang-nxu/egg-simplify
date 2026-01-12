use crate::output::single::{
    render_basis_stats, render_count_only, render_dim_vs_w, render_forbidden_pairs,
    render_genealogical_rules, render_pairs, render_pairs_by_weight, render_skeleton2_metrics,
    render_topology_metrics, render_triplets, render_triplets_by_weight,
};
use crate::run::count::CountReport;
use crate::run::single::ExperimentReport;

pub(crate) fn signature_from_count_report(report: &CountReport) -> String {
    let entries = vec![("counts_only.csv", render_count_only(report))];
    build_signature_from_entries(entries)
}

pub(crate) fn signature_from_full_report(report: &ExperimentReport) -> String {
    let entries = vec![
        ("basis_stats.txt", render_basis_stats(report)),
        ("dim_vs_w.csv", render_dim_vs_w(report)),
        ("pairs.csv", render_pairs(report)),
        ("pairs_by_weight.csv", render_pairs_by_weight(report)),
        ("triplets.csv", render_triplets(report)),
        ("triplets_by_weight.csv", render_triplets_by_weight(report)),
        ("forbidden_pairs.csv", render_forbidden_pairs(report)),
        ("genealogical_rules.json", render_genealogical_rules(report)),
        ("topology_metrics.csv", render_topology_metrics(report)),
        ("skeleton2_metrics.csv", render_skeleton2_metrics(report)),
    ];
    build_signature_from_entries(entries)
}

fn build_signature_from_entries(mut entries: Vec<(&'static str, String)>) -> String {
    // Signature uses the same render_* outputs as on-disk files and excludes paths/timestamps.
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut out = String::new();
    for (file, content) in entries {
        out.push_str(file);
        out.push('\n');
        out.push_str(&normalize_newlines(&content));
        out.push('\n');
    }
    out
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}
