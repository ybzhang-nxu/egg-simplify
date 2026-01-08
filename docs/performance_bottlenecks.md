# Performance Bottlenecks (Space Engine)

This note documents known scale limits observed in the current space engine
pipeline (integrability constraint generation + streaming elimination). It is
meant as a reference when experiments approach larger alphabets or weights.

## Known Bottleneck

At around `ncols ~= 5e4` (number of allowed words / columns), integrability
constraint generation and streaming REF elimination become very expensive.
The bottleneck shows up as long runtimes even when counting with acceptors is
fast and deterministic.

Typical symptom:
- `count` completes quickly, but `run` stalls in basis construction.

This is expected given the current algorithm:
- We enumerate all allowed words.
- We generate constraint rows for each context and variable pair.
- We stream rows into a REF/dictionary elimination structure (no global RREF).

## What to Record When It Hits

Capture these fields from `dim_vs_w.csv` and `basis_stats.txt`:
- `n_words_allowed` (ncols)
- `rows_attempted`, `rows_inserted`
- `samples_used`, `envs_total`
- `max_row_nnz`, `avg_row_nnz`
- `constraints_insufficient_samples`

These provide a deterministic profile of the workload.

## Recommended Mitigations

Short-term (no code changes):
- Use `mpl-experiments count --spec ...` to capture `n_words_allowed` without
  running the basis build.
- Lower `w_max` for full runs and keep a separate count-only spec for larger
  weights.
- Apply `[constraints.budget]` to force a fast `ConstraintBudgetExceeded` when
  a run is expected to be too large.
- Keep stress specs `#[ignore]` in tests; prefer explicit CLI runs.

Longer-term (design constraints apply):
- Maintain streaming REF + back-substitution (do not switch to global RREF).
- Consider more aggressive pruning of contexts or variable pairs only if it
  preserves determinism and documented invariants.

## Example Threshold (Observed)

For `ncols ~= 50,000`, basis construction becomes the dominant cost even when
word counting is fast. Use count-only or budgeted runs for higher weights.
