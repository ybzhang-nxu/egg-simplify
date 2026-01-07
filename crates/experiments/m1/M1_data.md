# M1 Data Pack (v0.1.5)

This folder contains **4 reproducible dataset specs** (3 literature-derived + 1 fixed-seed synthetic control)
for the **file-driven M1 runner**. Each spec is a single TOML file that can be passed to:

```bash
cargo run -p mpl-experiments -- run --spec <PATH/TO/SPEC.toml> --out <OUT_DIR>
```

The runner should read the spec and produce (per dataset):

- `basis_stats.txt`
- `dim_vs_w.csv`
- `pairs.csv` (aggregated across weights)
- `pairs_by_weight.csv` (per weight)
- `topology_metrics.csv`

> Scope: M1 uses only **Alphabet + WordConstraints(first-entry + adjacency)** and builds integrable bases
> via `build_integrable_basis`. Triplets / genealogical / automata constraints belong to M2+.

---

## Recommended directory layout

```
experiments/
  m1_data/
    M1_data.md
    L1_A2_cluster.toml
    L2_Dixon_Sprime_Steinmann.toml
    L3_Hexagon_Su_rational_subset.toml
    R1_rand_affine_seed20260107.toml
  m1_out/                   # runner output (do not hand-edit)
    <meta.id>/
      basis_stats.txt
      dim_vs_w.csv
      pairs.csv
      pairs_by_weight.csv
      topology_metrics.csv
```

---

## TOML spec schema (M1)

Each `*.toml` contains the following sections.

### `[meta]`
```toml
[meta]
id = "L1_A2_cluster"
title = "..."
kind = "literature"   # or "random"
source = "..."
```

Only `id` is required for output directory naming; the other fields are informational.

### `[weights]`  (Step C)
```toml
[weights]
min = 1
max = 8
```

### `[alphabet]`  (Step A)

```toml
[alphabet]
vars = ["x", "y", "z"]   # variables used for wedge constraints

[[alphabet.letters]]
name = "a"               # stable letter name (used in constraints and CSV outputs)
expr = "(/ x (+ 1 y))"   # mpl-ir s-expression string; will be parsed + normalized
```

### `[constraints.first_entry]`  (Step B)
```toml
[constraints.first_entry]
allowed = ["a", "b"]     # names allowed as the first letter
```

### `[constraints.adjacency]`  (Step B)
```toml
[constraints.adjacency]
mode = "allowed"         # "allowed" | "forbidden" | "none"
pairs = [["a","b"], ["b","a"]]   # directed adjacent pairs
```

Semantics:
- `mode="allowed"`: only listed `(a,b)` pairs are allowed; all others are forbidden.
- `mode="forbidden"`: listed `(a,b)` pairs are forbidden; all others are allowed.
- `mode="none"` (or missing): no adjacency restriction (equivalent to “all allowed”).

---

## Pair counting definition (used by the M1 runner)

The default **space-invariant** definition is `count_mode = active_word_positions`:

1. Determine **active words** at weight `w`: column indices that appear with a nonzero coefficient
   in at least one nullspace basis vector (i.e. the coordinate projection of the integrable subspace
   onto that column is nonzero).
2. For each active word, count its adjacent positions: for each position `i` in `0..w-1`,
   increment `count(letter[i], letter[i+1]) += 1`.

This is independent of the particular choice of basis vectors (as long as the basis spans the subspace).

---

## Runner outputs (required)

### `basis_stats.txt`
- One line per weight.
- Must include `BasisStats::one_line()` verbatim.
- Runner appends stable fields **after** the one-line payload:

Example:
```
w=5 ncols=... dim=... rank=... ... avg_row_nnz=... status=ok error_code=
```

Fields:
- `status`: `ok` or `err`
- `error_code`: stable enum-style code (e.g. `InsufficientSamples`, `NotImplemented`, ...)

### `dim_vs_w.csv`
- One row per weight (sorted by `weight`).
- Must include at least:
  - `weight`
  - `n_words_allowed` (deterministically counted even if basis construction fails)
  - all `BasisStats::one_line()` fields as columns (where available)
  - `status`, `error_code` (and optional `error`, which should equal `error_code` if present)

### `pairs.csv` (aggregated across weights)
- Columns: `a,b,count`
- Directed edges.
- Rows sorted lexicographically by `(a,b)`.

Interpretation: sum of `active_word_positions` counts across all successful weights in `[w_min, w_max]`.

### `pairs_by_weight.csv` (per weight)
- Columns: `weight,a,b,count`
- Directed edges.
- Rows sorted lexicographically by `(weight,a,b)`.

Weights that fail basis construction may emit no rows (or only rows for weights that succeeded);
the per-weight error state is recorded in `dim_vs_w.csv` / `topology_metrics.csv`.

### `topology_metrics.csv`
- One row per weight (sorted by `weight`).
- Recommended columns (integers unless noted):
  - `weight`
  - `n_vertices`, `n_edges`, `n_active_words`
  - `weakly_connected_components`
  - `strongly_connected_components`
  - `max_out_degree`
  - `avg_out_degree_num`, `avg_out_degree_den` (exact rational)
  - `density_num`, `density_den` (exact rational)
  - `status`, `error_code` (and optional `error`)

Topology semantics (M1 default):
- Graph is **directed**.
- Loops are **allowed** in the density denominator; density uses `V^2` as the maximum number of
  directed edges (including loops). If `V=0`, denominator is set to `1` to avoid division by zero.
- Out-degrees are computed from **deduplicated** outgoing neighbors.

---

## Included datasets

| file | meta.id | m | vars | suggested weights | purpose |
| --- | --- | ---: | --- | --- | --- |
| `L1_A2_cluster.toml` | `L1_A2_cluster` | 5 | x1,x2 | 1..12 | sparse cluster-adjacency baseline |
| `L2_Dixon_Sprime_Steinmann.toml` | `L2_Dixon_Sprime_Steinmann` | 6 | u,v,w | 1..8 | literature Steinmann/ES forbidden pairs |
| `L3_Hexagon_Su_rational_subset.toml` | `L3_Hexagon_Su_rational_subset` | 6 | u,v,w | 1..6 | hexagon baseline (first-entry only) |
| `R1_rand_affine_seed20260107.toml` | `R1_rand_affine_seed20260107` | 6 | x,y | 1..12 | deterministic synthetic control |

