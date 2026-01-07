# M1 Data Pack: File-driven space-engine experiments (v0.1.5)

This directory provides **4 experiment inputs** (TOML) that can be fed directly into an **M1 runner**. The goal is that, once you implement **M1 (minimal experimental loop)**, you can:

* Read `*.toml` → construct **Alphabet + WordConstraints (first-entry + adjacency)** (Step A/B)
* For each weight `w_min..=w_max`, build the integrable basis (Step D)
* Automatically produce:

  * `basis_stats.txt`
  * `dim_vs_w.csv`
  * `pairs.csv`
  * `pairs_by_weight.csv`
  * `topology_metrics.csv`

> Note: M1 includes only **first-entry + adjacency**. Steinmann/ES, triplets, genealogical, etc. are **M2+**.

---

## Directory Layout

```
experiments/m1/
  M1_data.md
  L1_A2_cluster.toml
  L2_Dixon_Sprime_Steinmann.toml
  L3_Hexagon_Su_rational_subset.toml
  R1_rand_affine_seed20260107.toml
```

---

## TOML Spec (aligned with the Experiment Spec Book: Step A/B/C/D)

Each `*.toml` follows the same minimal schema:

### `[experiment]` (general metadata / Step 0)

* `id`: experiment/data-pack id (string)
* `title`: description
* `out_dir`: output directory (relative to repo root)
* `w_min`, `w_max`: weight range (Step C/D)

### `[alphabet]` (Step A)

* `vars`: list of variable names (the space engine uses these for wedge constraints)
* `[[alphabet.letters]]`: one entry per letter:

  * `name`: stable name (used in output CSV `a,b`)
  * `expr`: mpl-ir s-expression string (will be parsed + normalized)

### `[constraints]` (Step B)

* `first_entry`: list of letter names allowed as the first letter
* `adjacency_mode`: `"allow"` or `"forbid"`
* `adjacency_pairs`: `[[a,b], ...]` (directed)

  * mode=`allow`: only these pairs are allowed; all others are forbidden
  * mode=`forbid`: these pairs are forbidden; all others are allowed

### `[pairs]` (Step E definition pinned in config; M1 default)

* `count_mode = "active_word_positions"`

  * Count adjacent positions over **active words** (space-invariant definition, not dependent on a particular basis choice)
  * Active words = columns with nonzero coeff in any nullspace basis vector

---

## How to Run (recommended M1 CLI shape)

After implementing the M1 runner, it should support something like:

```bash
cargo run -p mpl-experiments -- run --spec experiments/m1/L1_A2_cluster.toml
```

This writes the four output files under `out_dir`.

---

## Data Pack List

1. `L1_A2_cluster.toml`

   * 5-letter A2 cluster (pentagon) alphabet
   * adjacency = “cluster adjacency” (only cyclic neighbors allowed)

2. `L2_Dixon_Sprime_Steinmann.toml`

   * Dixon lecture 6-letter ??′ = {a,b,c,d,e,f}
   * first-entry = {a,b,c}
   * adjacency = forbidden pairs listed in the lecture (usable as an engineering input for Steinmann/ES)

3. `L3_Hexagon_Su_rational_subset.toml`

   * Rational subset of hexagon alphabet: {u,v,w,1-u,1-v,1-w}
   * first-entry = {u,v,w}
   * no adjacency constraint (baseline)

4. `R1_rand_affine_seed20260107.toml`

   * Fixed-seed random “affine-linear” alphabet (6 letters, 2 vars)
   * sparse allow-adjacency (random ensemble control)

---

## Required Outputs (the M1 runner must guarantee these)

* `basis_stats.txt`

  * one line per weight; **must include `BasisStats::one_line()` verbatim**
* `dim_vs_w.csv`

  * one row per weight; must include at least:
    `weight,n_words_allowed,dim,rank,rows_attempted,rows_inserted,samples_used,envs_total,rows_skipped_singular,constraints_insufficient_samples,vars,max_row_nnz,avg_row_nnz,status,error_code`
  * if an `error` column exists, it must equal `error_code` (stable)
* `pairs.csv`

  * `a,b,count` (directed; stable sorting)
  * aggregated across all weights
* `pairs_by_weight.csv`

  * `weight,a,b,count` (directed; stable sorting)
* `topology_metrics.csv`

  * one row per weight (stable sorting)
  * recommended columns:
    `weight,n_vertices,n_edges,n_active_words,weakly_connected_components,strongly_connected_components,density_num,density_den,max_out_degree,avg_out_degree_num,avg_out_degree_den,status,error_code,error`
