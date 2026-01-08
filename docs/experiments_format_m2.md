# Experiment Data Format (M2)

This document describes the TOML input format accepted by `mpl-experiments` and
the output files written by `write_outputs`. It is implementation-defined by the
current code in `crates/experiments/src/lib.rs` and `crates/symbol/src/space/*`.

## A) Input Spec (TOML)

### Required sections

#### `[experiment]`
Required keys:
- `id` (string)
- `out_dir` (string)
- `w_min` (integer >= 0)
- `w_max` (integer >= 0)

Optional keys:
- `title` (string)

Validation:
- `w_min` must be `<= w_max` or parsing fails.

#### `[alphabet]`
Required keys:
- `vars` (array of strings). May be empty; see "Vars handling" below.
- `[[alphabet.letters]]` (array of tables), at least one entry.

Each `[[alphabet.letters]]` entry:
- `name` (string, unique across letters)
- `expr` (string; mpl-ir s-expression)
- `channel` (string, optional; required when genealogical `seen = "channel"`)

Parsing/normalization:
- `expr` is parsed with `mpl_ir::parse_sexpr` and normalized.
- Duplicate `name` entries are rejected.
- The output CSVs use the provided `name` values.

Vars handling:
- `vars` is carried into output reporting.
- If `vars` is empty, `run_experiment` derives `vars` from letter expressions.
  The collector visits `Var`, `Add`, `Mul`, `Neg`, and `Pow` nodes; it ignores
  `Log` and `Li2` nodes.

#### `[constraints]`
Required keys:
- `adjacency_mode` (string, `"allow"` or `"forbid"`)

Optional keys:
- `first_entry` (array of letter names)
- `adjacency_pairs` (array of `[name, name]`, default empty)
- `[constraints.budget]` (optional table)
- `[constraints.automaton]` (optional table)

`first_entry`:
- If provided, only listed letters may appear at position 0.
- Unknown letter names cause a parse error.

`adjacency_mode` + `adjacency_pairs`:
- `adjacency_mode = "allow"`: only listed pairs are allowed; all others forbidden.
- `adjacency_mode = "forbid"`: listed pairs are forbidden; all others allowed.
- If `adjacency_mode = "allow"` and `adjacency_pairs` is empty, parsing fails.
- Unknown letter names in `adjacency_pairs` cause a parse error.

#### `[constraints.budget]` (optional)
All keys are optional; omitted keys mean "no limit":
- `max_states` (integer)
- `max_transitions` (integer)
- `max_words` (integer, u64)

Budgets are applied during acceptor graph construction and word counting. When
exceeded, the run reports `ConstraintBudgetExceeded` per-weight.

#### `[constraints.automaton]` (optional)
Optional keys:
- `acceptors` (array of tagged tables). If missing or empty, no extra acceptors
  are used beyond `WordConstraints`.

The order of entries in `acceptors` is preserved and determines acceptor order.

##### `[[constraints.automaton.acceptors]]` entries

Tagged union via `kind`:

1) `kind = "kgram"`
- `k` (integer, must be `3`)
- `mode` (`"allowed"` or `"forbidden"`)
- `triplets` (array of `[name, name, name]`)

Validation:
- `k != 3` is an error.
- `mode = "allowed"` with empty `triplets` is an error
  (`InvalidSpecEmptyAllowList`).
- Unknown letter names in `triplets` are errors.
- Duplicate triplets are rejected by the acceptor constructor.

2) `kind = "genealogical"`
- `seen` (string, optional; `"channel"` or `"letter"`, default `"channel"`)
- `rules` (array of tables; may be empty)

Each `rules` entry:
- `if_seen` (string)
- `forbid` (array of strings)

Validation:
- `seen = "channel"`: every letter must define a non-empty `channel`. `if_seen`
  and `forbid` values refer to channel names.
- `seen = "letter"`: `if_seen` and `forbid` values refer to letter names.
- Unknown channel/letter names are errors (`InvalidSpecUnknownChannel` or
  `InvalidSpecUnknownLetter`).
- Duplicate entries inside a single `forbid` list are errors
  (`InvalidSpecDuplicateForbid`).
- Duplicate rules with identical `(if_seen, forbid)` are errors
  (`InvalidSpecDuplicateRule`).
- If `rules` is empty, the acceptor is skipped (no extra constraints), but
  `seen = "channel"` still requires all letters to have channels.

#### `[pairs]` (optional)
Optional keys:
- `count_mode` (string). Only `"active_word_positions"` is supported.
  Any other value is a parse error.

When omitted, the runner still computes pairs using the
`active_word_positions` definition.

## B) Output Directory Layout

`write_outputs(report, out_dir)` creates the directory and writes:
- `basis_stats.txt`
- `dim_vs_w.csv`
- `pairs.csv`
- `pairs_by_weight.csv`
- `triplets.csv`
- `triplets_by_weight.csv`
- `topology_metrics.csv`

Count-only runs (`mpl-experiments count --spec ...`) write:
- `counts_only.csv`

Rows for per-weight files are emitted in ascending weight order
(`w_min..=w_max`). Pair/triplet outputs use lexicographic ordering over
letter-id keys via `BTreeMap`.

### `basis_stats.txt`
One line per weight:
```
w=<weight> <BasisStats::one_line()> status=<ok|err> [error_code=<Code>]
```

`BasisStats::one_line()` fields (in order):
```
ncols, dim, rank, rows_attempted, rows_inserted, samples_used, envs_total,
rows_skipped_singular, constraints_insufficient_samples, vars, max_row_nnz,
avg_row_nnz
```

`error_code` is only present when status is `err`.

### `dim_vs_w.csv`
Header (exact column order):
```
weight,n_words_allowed,dim,rank,rows_attempted,rows_inserted,samples_used,envs_total,rows_skipped_singular,constraints_insufficient_samples,vars,max_row_nnz,avg_row_nnz,status,error_code,error
```

Row semantics:
- `n_words_allowed` comes from acceptor-based counting. If counting fails
  (budget exceeded or other error), it is set to `0` and status is `err`.
- `avg_row_nnz` is `sum_row_nnz / rows_inserted` (integer division), or `0` when
  `rows_inserted == 0`.
- `vars` is the comma-joined list from the report; it may be empty.
- `error` equals `error_code` (CSV-escaped); empty on `ok`.

### `pairs.csv`
Header:
```
a,b,count
```

Definition:
- Counts adjacent ordered pairs across **active words** only.
- Active words are columns with nonzero coefficients in any basis vector.
- Aggregated across successful weights only.

Ordering:
- Sorted by `(a,b)` in letter-id order.
- `a` and `b` are letter display names from the alphabet.

### `pairs_by_weight.csv`
Header:
```
weight,a,b,count
```

Definition:
- Same pair counting as `pairs.csv`, but per weight.
- Weights with failed basis construction emit no rows.

Ordering:
- Sorted by `weight`, then `(a,b)`.

### `triplets.csv`
Header:
```
a,b,c,count
```

Definition:
- Counts consecutive ordered triplets across **active words** only.
- Aggregated across successful weights only.

Ordering:
- Sorted by `(a,b,c)` in letter-id order.

### `triplets_by_weight.csv`
Header:
```
weight,a,b,c,count
```

Definition:
- Same triplet counting as `triplets.csv`, but per weight.
- Weights with failed basis construction emit no rows.

Ordering:
- Sorted by `weight`, then `(a,b,c)`.

### `topology_metrics.csv`
Header (exact column order):
```
weight,n_vertices,n_edges,n_active_words,weakly_connected_components,strongly_connected_components,density_num,density_den,max_out_degree,avg_out_degree_num,avg_out_degree_den,status,error_code,error
```

Graph definition:
- Vertices are letters (`n_vertices = alphabet size`).
- Directed edges are unique `(a,b)` pairs with nonzero pair counts.
- `n_edges` counts unique directed edges (one per pair key).
- `weakly_connected_components`: computed on the undirected version.
- `strongly_connected_components`: computed on the directed graph.
- `max_out_degree`: maximum number of distinct outgoing neighbors.
- `avg_out_degree_num / avg_out_degree_den`: `n_edges / n_vertices` (denominator
  is `1` when `n_vertices = 0`).
- `density_num / density_den`: `n_edges / (n_vertices^2)` (denominator is `1`
  when `n_vertices = 0`; loops are allowed).

`error` equals `error_code` (CSV-escaped); empty on `ok`.

### `counts_only.csv` (count-only runs)
Header:
```
weight,n_words_allowed,status,error_code,error
```

Definition:
- Reports acceptor-based word counts only.
- No basis construction is performed.

Ordering:
- Sorted by `weight` (ascending).

### CSV escaping
Fields are quoted if they contain a comma, quote, or newline; quotes are doubled.

## C) Error/Status Contract

Per-weight status values:
- `ok`
- `err`

Error codes (from `SymbolError`):
- `NotImplemented`
- `Eval`
- `InsufficientSamples`
- `FuelExhausted`
- `ConstraintBudgetExceeded`

Mapping:
- `SymbolError::NotImplemented` -> `NotImplemented`
- `SymbolError::Eval` -> `Eval`
- `SymbolError::InsufficientSamples` -> `InsufficientSamples`
- `SymbolError::FuelExhausted` -> `FuelExhausted`
- `SymbolError::ConstraintBudgetExceeded(_)` -> `ConstraintBudgetExceeded`

Behavior:
- A failure at a given weight does **not** abort the experiment; the runner
  continues to the next weight.
- If `n_words_allowed` counting fails, the weight summary uses `BasisStats::default()`,
  `n_words_allowed = 0`, and `status = err`.
- If basis construction fails, `n_words_allowed` is preserved (counted earlier),
  and `status = err` with the error code from the basis error.

Spec validation errors (TOML parsing or config validation) are returned as
`ExperimentError::InvalidConfig` and no output files are written.

## D) Minimal Examples

### 1) Toy xy unconstrained
```toml
[experiment]
id = "toy_xy"
out_dir = "reports/m2/toy_xy"
w_min = 1
w_max = 3

[alphabet]
vars = ["x", "y"]

[[alphabet.letters]]
name = "x"
expr = "x"

[[alphabet.letters]]
name = "y"
expr = "y"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []
```

### 2) Budget exceeded (max_states too small)
```toml
[experiment]
id = "budget_exceeded"
out_dir = "reports/m2/budget_exceeded"
w_min = 1
w_max = 2

[alphabet]
vars = ["x", "y"]

[[alphabet.letters]]
name = "x"
expr = "x"

[[alphabet.letters]]
name = "y"
expr = "y"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.budget]
max_states = 2
```

## E) Compatibility Notes

- M1 specs remain valid; M2 additions are optional.
- Budgets and automaton acceptors are additive only.
- Triplet CSV outputs are additive; existing M1 outputs and column orders are
  unchanged.
