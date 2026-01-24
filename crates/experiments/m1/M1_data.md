# M1 Data Pack (v0.2.3)

This folder contains 4 reproducible dataset specs for the file-driven M1 runner.
Each spec is a single TOML file that can be passed to:

```bash
cargo run -p mpl-experiments -- run --spec crates/experiments/m1/L1_A2_cluster.toml
```

The runner reads the spec and produces (per dataset):

- `basis_stats.txt`
- `dim_vs_w.csv`
- `pairs.csv` (aggregated across weights)
- `pairs_by_weight.csv` (per weight)
- `triplets.csv` / `triplets_by_weight.csv`
- `forbidden_pairs.csv`
- `genealogical_rules.json`
- `topology_metrics.csv`
- `skeleton2_metrics.csv`

Scope: M1 uses only `Alphabet` + `WordConstraints` (first-entry + adjacency)
and builds integrable bases via `build_integrable_basis`. Triplets / genealogical
/ automata constraints belong to M2+, and the additional fields are optional.

---

## Recommended directory layout

```
crates/experiments/m1/
  M1_data.md
  L1_A2_cluster.toml
  L2_Dixon_Sprime_Steinmann.toml
  L3_Hexagon_Su_rational_subset.toml
  R1_rand_affine_seed20260107.toml
reports/m1/
  <experiment.id>/
    basis_stats.txt
    dim_vs_w.csv
    pairs.csv
    pairs_by_weight.csv
    triplets.csv
    triplets_by_weight.csv
    forbidden_pairs.csv
    genealogical_rules.json
    topology_metrics.csv
    skeleton2_metrics.csv
```

---

## TOML spec schema (M1)

Each `*.toml` contains the following sections.

### `[experiment]`
```toml
[experiment]
id = "L1_A2_cluster"
title = "..."
out_dir = "reports/m1/L1_A2_cluster"
w_min = 1
w_max = 12
```

### `[alphabet]`
```toml
[alphabet]
vars = ["x", "y", "z"]

[[alphabet.letters]]
name = "a"
expr = "(/ x (+ 1 y))"   # mpl-ir s-expression string; parsed + normalized
channel = "A"            # optional; required when genealogical seen="channel"
```

### `[constraints]`
```toml
[constraints]
first_entry = ["a", "b"]
adjacency_mode = "allow" # "allow" | "forbid"
adjacency_pairs = [["a","b"], ["b","a"]]
```

Semantics:
- `adjacency_mode = "allow"`: only listed `(a,b)` pairs are allowed; all others are forbidden.
- `adjacency_mode = "forbid"`: listed `(a,b)` pairs are forbidden; all others are allowed.

Optional additions (M2, additive; M1 still valid):
```toml
[constraints.budget]
max_states = 1000
max_transitions = 5000
max_words = 100000

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "kgram"
k = 3
mode = "allowed" # "allowed" | "forbidden"
triplets = [["a","b","c"], ["b","c","a"]]

[[constraints.automaton.acceptors]]
kind = "genealogical"
seen = "channel" # "channel" | "letter"
rules = [
  { if_seen = "A", forbid = ["B", "C"] },
]
```

### `[pairs]`
```toml
[pairs]
count_mode = "active_word_positions"
```

---

## Pair counting definition (used by the M1 runner)

The default space-invariant definition is `count_mode = active_word_positions`:

1. Determine active words at weight `w`: column indices that appear with a nonzero
   coefficient in at least one nullspace basis vector.
2. For each active word, count its adjacent positions: for each position `i` in
   `0..w-1`, increment `count(letter[i], letter[i+1]) += 1`.

This is independent of the particular choice of basis vectors (as long as the
basis spans the subspace).

---

## Runner outputs (required)

### `basis_stats.txt`
- One line per weight.
- Must include `BasisStats::one_line()` verbatim.
- Runner appends stable fields after the one-line payload:

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

Interpretation: sum of `active_word_positions` counts across all successful weights
in `[w_min, w_max]`.

### `pairs_by_weight.csv` (per weight)
- Columns: `weight,a,b,count`
- Directed edges.
- Rows sorted lexicographically by `(weight,a,b)`.

Weights that fail basis construction emit no rows; the per-weight error state is
recorded in `dim_vs_w.csv` / `topology_metrics.csv`.

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
- Graph is directed.
- Loops are allowed in the density denominator; density uses `V^2` as the maximum
  number of directed edges (including loops). If `V=0`, denominator is set to `1`.
- Out-degrees are computed from deduplicated outgoing neighbors.

---

## Included datasets

| file | experiment.id | m | vars | suggested weights | purpose |
| --- | --- | ---: | --- | --- | --- |
| `L1_A2_cluster.toml` | `L1_A2_cluster` | 5 | x1,x2 | 1..12 | sparse cluster-adjacency baseline |
| `L2_Dixon_Sprime_Steinmann.toml` | `L2_Dixon_Sprime_Steinmann` | 6 | u,v,w | 1..8 | literature Steinmann/ES forbidden pairs |
| `L3_Hexagon_Su_rational_subset.toml` | `L3_Hexagon_Su_rational_subset` | 6 | u,v,w | 1..6 | hexagon baseline (first-entry only) |
| `R1_rand_affine_seed20260107.toml` | `R1_rand_affine_seed20260107` | 6 | x,y | 1..12 | deterministic synthetic control |

