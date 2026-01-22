# Cross-loop Recurrence (Design & Repo Scan)

## Repository Scan Summary
- Language: Rust workspace (`Cargo.toml` at repo root).
- Entry points:
  - `mpl-simplify` CLI (`crates/cli/src/main.rs`)
  - `mpl-experiments` CLI (`crates/experiments/src/bin/mpl_experiments.rs`)
- Core crates:
  - `mpl-ir`: AST, s-expr parser, normalization, canonical printing.
  - `mpl-symbol`: Symbol tensor (`Symbol`, `Word`), integrability, space engine.
  - `mpl-experiments`: alphabet/constraints specs, basis construction runners.

## Symbol Data Model
- `Symbol` is a sparse tensor: `BTreeMap<Word, Coeff>`, deterministic ordering.
- `Word` is `Vec<Expr>` (letters), ordered by canonical strings.
- `Coeff` is `num_rational::Rational64`.
- Iteration is streaming via `Symbol::terms()`; no hash-based ordering.

## Basis / V_L Construction
- `build_integrable_basis` in `mpl_symbol::space`:
  - Enumerates allowed words lexicographically by letter id.
  - Builds sparse integrability rows using wedge(dlog) constraints.
  - Performs streaming REF/dictionary elimination (smallest pivot column).
  - Produces `Basis { words, vectors, free_cols, stats }`.
- `Basis::vectors` is a nullspace basis with free-variable convention.

## Existing MPL / Hopf / Calculus Hooks
- `Symbol::deconcat()` yields `Coproduct` (deconcatenation coproduct).
- `calculus::deriv()` supports algebraic letters (log/li2 not implemented).
- No full MPL/GPL or coaction identities yet (per roadmap).

## Cross-loop Module (New)
### Goal
Analyze suffix-projection maps between spaces:
```
T_s(Σ c_w [l1..lw]) = Σ_{w ends with s} c_w [l1..l_{w-k}]
```
with emphasis on strike-two (suffix length k=2).

### Core APIs (mpl-experiments)
- `SuffixSpec`: resolved suffix (letter ids + Expr + names).
- `apply_suffix_projection` (mpl-symbol): symbol-level operator.
- `image_rank`: build sparse row matrix and compute rank + diagnostics.
- `express_images_in_lower_space`: solve in `V_{L-1}` using `reduce_to_basis`.
- `run_cross_loop` / `run_cross_loop_scan`: full pipeline from spec.

### Outputs
Single weight:
- `cross_loop_report.txt`: ranks, dims, pivot info, residual stats.
- `mapping_matrix.csv`: sparse `row,col,value` for `R_L`.
- `mapping_shape.txt`: matrix shape.
- `residuals.txt`: failed columns with sample residual words.
- `constraints_coupled.csv` (optional): coupled constraints
  `R_L * alpha_L - alpha_{L-1} = 0`.

Scan mode:
- `cross_loop_scan.csv`: per-weight rank summary + prefactor hints, plus
  `suffix_len`, `n_suffixes_total`, and `suffix_index` metadata columns.
- `cross_loop_scan_fits.txt`: simple fits for rank-1 prefactors.
- `cross_loop_scan_index.csv`: (multi-suffix) index of suffix to subdirectory.

Load shedding:
- `--row-prefix` filters truncated words by prefix.
- `--row-limit` caps unique rows for rank estimation.

### CLI Usage
Single weight:
```
cargo run -p mpl-experiments -- cross-loop \
  --spec experiments/m2/M2_toy_xy_unconstrained.toml \
  --weight 4 \
  --suffix f f \
  --out reports/cross_loop_w4
```

Scan range:
```
cargo run -p mpl-experiments -- cross-loop \
  --spec experiments/m2/M2_toy_xy_unconstrained.toml \
  --weight-min 2 --weight-max 6 \
  --suffix f f \
  --out reports/cross_loop_scan
```

Multi-suffix scan (repeat `--suffix`):
```
cargo run -p mpl-experiments -- cross-loop \
  --spec experiments/m2/M2_toy_xy_unconstrained.toml \
  --weight-min 2 --weight-max 6 \
  --suffix f f \
  --suffix a21 a21 \
  --out reports/cross_loop_scan
```

Multi-suffix scan from TOML:
```
suffixes = [["f", "f"], ["a21", "a21"]]
```
```
cargo run -p mpl-experiments -- cross-loop \
  --spec experiments/m2/M2_toy_xy_unconstrained.toml \
  --weight-min 2 --weight-max 6 \
  --suffixes-toml experiments/m2/suffixes.toml \
  --out reports/cross_loop_scan
```

Notes:
- Duplicate suffixes are deduped by resolved letter ids; the CLI logs
  `deduped suffix list: N -> M` when this happens.

### Limitations / Notes
- Rank estimates are exact over `Rational64`; no finite-field sampling yet.
- Scan-mode fits are limited to simple geometric, polynomial (deg <= 2),
  and small integer sequence candidates.
- `--loop` is a convenience label with `weight-per-loop` (default 2),
  but core logic never hardcodes `weight = 2L`.
