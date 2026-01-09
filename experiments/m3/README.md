# M3 experiment datasets

This folder contains deterministic random specs and literature-inspired
alphabets for M3 runs.

## Random suite

Generate or refresh the random specs:

```
cargo run -p mpl-experiments --bin m3_random_gen
```

Run a random spec:

```
cargo run -p mpl-experiments -- run --spec experiments/m3/random/RND_m64_w5_p0.06_seed20250301.toml
```

The generated catalog lives at `experiments/m3/random/catalog.csv`.

## Literature suite

Run a literature spec:

```
cargo run -p mpl-experiments -- run --spec experiments/m3/literature/lit_henn_4pt_onshell.toml
```

## Ignored stress tests

```
cargo test -p mpl-experiments --release -- --ignored --nocapture --test-threads=1
```

## Skeleton2 summary helper

```
python scripts/skeleton2_summary.py reports
```
