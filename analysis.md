# ESymb Analysis Pipelines (A/B/C)

This note summarizes the ESymb analysis pipeline in `mpl-experiments`:

- A: `esymb-rank-scan` (marginals scan + rank screening)
- B: `esymb-span-deps` (span rank + readable dependencies)
- C: `esymb-hankel-subblock` (prefix-suffix Hankel subblocks)

All outputs are deterministic: ordering is stable (lexicographic by key), and
results do not depend on hash iteration order.

---

## Input format: Esymb JSONL (A)
Each loop is a JSONL file named `Esymb_L{L}.jsonl`, with:

1. A meta line:
   `{"_meta":{"name":"Esymb","loop":L,"merged_terms":N}}`
2. Term lines:
   `{"word":["x","y",...],"coeff":"1/2"}`

`coeff` is an exact rational (integer or `n/d`).

---

## A) `esymb-rank-scan` (marginals + screening)
Purpose:
- Scan ESymb terms per loop.
- Build prefix/suffix/prefix-suffix marginals.
- Screen sequences by mod-p rank; optionally export observables and matrix ranks.

Input:
- `--data-dir <dir>` with `Esymb_L*.jsonl`, or `--glob <pattern>`.
- `--loops <list or range>` (e.g. `1..6,8`).
- Alphabet: `--letters <names...>` (manual) or `--alphabet auto`.

Outputs (under `--out-dir`, default `reports/esymb_rank_scan`):
- `rank_scan.csv` and `summary.md`
- `marginals_observables.csv` (when `--export-observables`)
- `marginals_matrix_rank.csv` (when `--matrix-rank`)

Key options:
- `--family prefix|suffix|prefix-suffix` (repeatable)
- `--prefix-len <r>` / `--suffix-len <k>`
- `--letters <names...>` or `--alphabet auto`
- `--only-observed` (emit only observed buckets)
- `--validate-marginals` (prefix/suffix conservation checks)
- `--export-observables` (writes `marginals_observables.csv`)
- `--matrix-rank` (writes `marginals_matrix_rank.csv`)

Notes:
- For prefix-suffix, require `r + k <= 2 * min_loop`.
- If `r=2,k=2`, loops must start at `L>=2`.

Example:
```bash
cargo run -p mpl-experiments -- esymb-rank-scan \
  --data-dir reports/esymb_jsonl --loops 2..6 \
  --family prefix-suffix --prefix-len 2 --suffix-len 2 \
  --letters x,y --export-observables --matrix-rank
```

---

## B) `esymb-span-deps` (span deps)
Purpose:
- Compute span rank and nullity for observable families.
- Extract sparse relations (pm1/pm2), equivalence classes, basis expansions.
- Report zero-pattern masks and allowed-graph edges (prefix-suffix).

Input:
- `--observables <path>` (or `--in`) pointing to `marginals_observables.csv`.

Outputs (under `--out-dir`, default `reports/esymb_span_deps`):
- `span_stats.csv`, `span_deps.csv`, `span_deps.md`
- `basis_keys.csv`, `basis_expansions_modp.csv`
- `support_mask.csv`, `mask_histogram.csv`
- `allowed_graph.csv` (prefix-suffix only)
- `equiv_classes.csv` (with `--export-equiv-classes`)
- `forbidden_keys.csv`, `nonzero_keys.csv` (with `--export-forbidden`)

Key options:
- `--family prefix|suffix|prefix-suffix|all`
- `--support-max <n>` (default 3)
- `--coef-set pm1|pm2`
- `--top-k <n>` (limit reported relations)
- `--export-forbidden`
- `--export-equiv-classes`

Example:
```bash
cargo run -p mpl-experiments -- esymb-span-deps \
  --in reports/esymb_rank_scan/marginals_observables.csv \
  --family prefix-suffix --coef-set pm2 --support-max 3
```

---

## C) `esymb-hankel-subblock` (Hankel subblocks)
Purpose:
- Rebuild the prefix-suffix marginal matrix per loop.
- Compute mod-p rank; optionally extract row/col dependencies.

Input:
- `--in <path>` pointing to `marginals_observables.csv`.
- `--r <n>` and `--k <n>` to select prefix/suffix length.
- Optional `--loops <list or range>` (defaults to all loops in CSV).

Outputs (under `--out-dir`, default `reports/esymb_hankel_subblock`):
- `hankel_subblock_stats.csv`
- `hankel_subblock.md`
- `hankel_row_deps.csv` / `hankel_col_deps.csv` (with `--exact`)

Key options:
- `--r <n>` / `--k <n>`
- `--loops <list or range>`
- `--primes <list>` (mod-p rank)
- `--exact` (emit row/col dependencies over the max-rank prime)

Note:
- `--exact` currently reports dependencies over a finite field (prime that
  attains the max rank). Treat them as candidate merges.

Example:
```bash
cargo run -p mpl-experiments -- esymb-hankel-subblock \
  --in reports/esymb_rank_scan/marginals_observables.csv \
  --r 2 --k 2 --loops 2..6 --exact
```

---

## Pipeline order (A -> B -> C)
1) Run A to generate `marginals_observables.csv`.
2) Run B and C using that CSV.

Minimal flow:
```bash
cargo run -p mpl-experiments -- esymb-rank-scan ... --export-observables
cargo run -p mpl-experiments -- esymb-span-deps --in reports/esymb_rank_scan/marginals_observables.csv
cargo run -p mpl-experiments -- esymb-hankel-subblock --in reports/esymb_rank_scan/marginals_observables.csv --r 2 --k 2
```
