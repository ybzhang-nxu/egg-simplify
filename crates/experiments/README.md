# mpl-experiments

Deterministic experiment runners for MPL symbol and Hankel workflows.

## kze2-hankel-mvp

Deterministic Hankel/spectral learning reconstruction over GF(p) for the
five-letter KZE2 alphabet.

Example:

```bash
cargo run -p mpl-experiments -- kze2-hankel-mvp --r 20 --prime 1000003 --prefix-len 2 --holdout-len 6 --out-dir reports/kze2_hankel_mvp
```

Outputs (when `--out-dir` is supplied):

- `params.json`
- `stats.txt`

Note: if `stats.txt` reports `hankel_rank < r`, increase `--prefix-len`
(for example, use `--prefix-len 3`) to capture the full rank.
