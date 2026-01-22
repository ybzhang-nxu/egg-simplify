# M2 Experiment Specs

This directory contains curated M2 TOML specs that follow
`docs/experiments_format_m2.md`.

## Normal (day-to-day)

- `M2_toy_xy_unconstrained.toml`:
  baseline XY alphabet with no adjacency constraints.
  Run: `mpl-experiments run --spec experiments/m2/M2_toy_xy_unconstrained.toml`

- `M2_toy_xy_forbid_xy.toml`:
  forbids the adjacent pair (x,y) to exercise adjacency filtering.
  Run: `mpl-experiments run --spec experiments/m2/M2_toy_xy_forbid_xy.toml`

- `M2_toy_xy_alternating_allow.toml`:
  only allows alternating adjacency x->y and y->x (xx/yy forbidden).
  Run: `mpl-experiments run --spec experiments/m2/M2_toy_xy_alternating_allow.toml`

- `M2_lang_ring12_allow.toml`:
  12-letter ring with allow-adjacency, predictable topology/pairs.
  Run: `mpl-experiments run --spec experiments/m2/M2_lang_ring12_allow.toml`

- `M2_lang_sparse16_allow_seedC0FFEE.toml`:
  sparse, deterministic allow-adjacency on 16 letters.
  Run: `mpl-experiments run --spec experiments/m2/M2_lang_sparse16_allow_seedC0FFEE.toml`

- `M2_kgram3_cycle_allowed.toml`:
  k-gram (k=3) acceptor with a 3-cycle of allowed triplets.
  Run: `mpl-experiments run --spec experiments/m2/M2_kgram3_cycle_allowed.toml`

- `M2_gene_channel_no_interleave_w9.toml`:
  genealogical channel rule: once channel A appears, channel B is forbidden.
  Full run capped at w=9 for stable outputs.
  Run: `mpl-experiments run --spec experiments/m2/M2_gene_channel_no_interleave_w9.toml`

- `M2_gene_channel_no_interleave_count_w12.toml`:
  same genealogical rule but count-only at w=12.
  Run: `mpl-experiments count --spec experiments/m2/M2_gene_channel_no_interleave_count_w12.toml`

## Cross-loop suffix probe packs

These are convenience inputs for `mpl-experiments cross-loop --suffixes-toml`.
They are written for specific alphabets; update letters to match other specs.

- `suffixes_len2_diag.toml`:
  repeated-letter suffixes (strike-two probe).
- `suffixes_len2_smallgrid.toml`:
  2x2 ordered pair grid over {x, y}.
- `suffixes_len2_fullpairs.toml`:
  full ordered pairs over {x, y} (same as smallgrid for XY).

- `suffixes_len2_diag_abc.toml`:
  repeated-letter suffixes for the k-gram cycle spec (letters a,b,c).
- `suffixes_len2_smallgrid_abc.toml`:
  ordered pairs over {a,b,c} for the k-gram cycle spec.
- `suffixes_len2_diag_a1a2b1b2.toml`:
  repeated-letter suffixes for the genealogical channel spec (a1,a2,b1,b2).

## Stress (expected ConstraintBudgetExceeded)

These are intended to validate budget handling and error_code mapping.
Each should emit per-weight `status=err` and `error_code=ConstraintBudgetExceeded`,
while still producing output files.

- `STRESS_budget_words_exceeded.toml`:
  large alphabet + weight triggers `max_words`.
  Run: `mpl-experiments run --spec experiments/m2/STRESS_budget_words_exceeded.toml`

- `STRESS_budget_states_exceeded.toml`:
  tiny `max_states` (1) triggers state budget.
  Run: `mpl-experiments run --spec experiments/m2/STRESS_budget_states_exceeded.toml`

- `STRESS_budget_transitions_exceeded.toml`:
  dense allow-adjacency with tiny `max_transitions`.
  Run: `mpl-experiments run --spec experiments/m2/STRESS_budget_transitions_exceeded.toml`

- `STRESS_gene_no_interleave_w12_budget_words50k.toml`:
  genealogical w=12 run that intentionally exceeds `max_words` to validate
  `ConstraintBudgetExceeded` behavior.
  Run: `mpl-experiments run --spec experiments/m2/STRESS_gene_no_interleave_w12_budget_words50k.toml`

## Deprecated

- `M2_gene_channel_no_interleave.toml`:
  w=12 full run can stall during basis construction; use the w=9 full run
  and w=12 count-only specs instead.
