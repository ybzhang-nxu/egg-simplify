# Experiment Filtration Format (M6)

This document describes the TOML input format accepted by the M6 filtration
runner in `mpl-experiments` and the summary outputs written by the runner.

The filtration spec is additive: it does not replace the M2 single-run spec.
Layer constraints reuse the existing constraints schema documented in
`docs/experiments_format_m2.md`.

## A) Filtration Spec (TOML)

Top-level keys:
- `id` (string)
- `out_dir` (string)
- `alphabet` (same schema as M2)
- `weights` (table with `min`, `max`)
- `engine` (optional table; defaults for budgets and full-run threshold)
- `repeats` (optional integer >= 1; default 1)
- `layers` (array of layer tables; order is preserved)

### `[alphabet]`
Identical to M2. See `docs/experiments_format_m2.md`.

### `[weights]`
Required keys:
- `min` (integer >= 0)
- `max` (integer >= 0)

Validation:
- `min` must be `<= max`.

### `[engine]` (optional)
Optional keys:
- `full_run_max_words` (integer, optional)
- `jobs` (integer >= 1, optional; worker count for per-layer/weight runs)
- `sample_table` (string; default `"default"`)

Optional sub-table:
- `[engine.budget]` (same keys as M2 `[constraints.budget]`)
  - `max_states`, `max_transitions`, `max_words`

The `engine.budget` values are defaults. Layer-level
`[layers.constraints.budget]` entries override them per layer.

CLI note: `mpl-experiments filtration --jobs N` overrides `engine.jobs`.

### `repeats` (optional)
If `repeats > 1`, the runner performs deterministic re-runs and compares
output signatures. Mismatches are reported with `error_code=NonDeterministicOutput`.

### `[[layers]]`
Each layer entry:
- `name` (string, unique)
- `mode` (`"full" | "count_only" | "auto"`, default `"auto"`)
- `constraints` (table; same schema as M2 `[constraints]`)

The `constraints` table must include `adjacency_mode` and should follow the
M2 schema exactly. A layer can be "unconstrained" by setting:

```toml
[layers.constraints]
adjacency_mode = "forbid"
adjacency_pairs = []
```

Any acceptors in `layers.constraints.automaton.acceptors` use the existing
`kgram`, `genealogical`, or `channel_pairs` schemas from M2.

## B) Output Layout

For each layer (in order) and each weight (ascending), the runner creates:

```
<out_dir>/layers/<index>_<sanitized_name>/w<weight>/
```

Depending on the layer mode, the per-weight directory contains either:
- Full run outputs (`basis_stats.txt`, `dim_vs_w.csv`, `pairs.csv`,
  `pairs_by_weight.csv`, `triplets.csv`, `triplets_by_weight.csv`,
  `forbidden_pairs.csv`, `genealogical_rules.json`, `topology_metrics.csv`,
  `skeleton2_metrics.csv`), or
- Count-only output (`counts_only.csv`).

The runner also writes two top-level summary files:
- `filtration_summary.csv`
- `filtration_summary.md`

### `filtration_summary.csv`
Header (exact column order):
```
layer_index,layer_name,weight,mode,status,error_code,error,n_words_allowed,dim,rank,basis_ncols,rows_attempted,rows_inserted,samples_used,envs_total,sample_table,constraints_insufficient_samples
```

Semantics:
- `status/error_code/error` reflect the full run when executed, otherwise the
  count-only status.
- `n_words_allowed` is always filled from count-only.
- Basis stats fields are filled only when a full run succeeded; otherwise empty.
- `error` equals `error_code` (CSV-escaped); empty on `ok`.

### `filtration_summary.md`
Deterministic, human-readable summary:
- Layer list (name + mode).
- Table of `n_words_allowed` and `dim` per layer/weight.
- Explicit failure list (if any).
