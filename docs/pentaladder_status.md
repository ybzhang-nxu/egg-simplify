# Penta-Ladder Status (He 2020)

Status: **experimental / unstable**.

This generator lives in `mpl-experiments` as `gen-pentaladder` and targets the
He 2020 penta-ladder recursion. It is useful for stats-only workflows, but the
recursion still has open issues at higher loop order.

## Current Behavior
- `Psi_2` is **anchored to Appendix A** (A.1–A.3) as the source of truth.
  This is enforced in `symbol_psi` via `Psi2Source::Golden` by default.
- A switch exists for debugging the recursive path:
  `symbol_psi_with_psi2_source(2, Psi2Source::Recursive)`.
- The recursion for `L >= 3` is still under investigation; `Psi_3` currently
  takes too long to compute in the test harness.

## Known Issues / Risks
- `Psi_3` generation is slow and can exceed 10 minutes in tests.
- The recursive `Psi_2` path is not yet consistent with the Appendix A golden.

## Tests (Separated by Stability)
Stable checks live in:
- `crates/experiments/tests/pentaladder_gen.rs`

Experimental checks (ignored by default) live in:
- `crates/experiments/tests/pentaladder_unstable.rs`

Run unstable tests explicitly:
```bash
cargo test -p mpl-experiments --test pentaladder_unstable -- --ignored --nocapture
```

## Debug Flags for Timing/Progress
Use these environment variables to trace recursion cost:
- `PENTALADDER_TRACE_TIMING=1`: timing for `psi_step_x` / `psi_step_y` and
  X+/X- integration steps.
- `PENTALADDER_TRACE_PROGRESS=1`: progress logs inside integration loops.
- `PENTALADDER_TRACE_PROGRESS_EVERY=1000`: override progress interval.

Example:
```powershell
$env:PENTALADDER_TRACE_TIMING=1
$env:PENTALADDER_TRACE_PROGRESS=1
$env:PENTALADDER_TRACE_PROGRESS_EVERY=1000
cargo test -p mpl-experiments --test pentaladder_unstable psi3_last_entry_span_checks -- --nocapture
```

## Box Ladder vs Penta-Ladder
- Box ladder (Drummond 2010 / `gen-ladder`) is stable and suitable for
  production stats pipelines.
- Penta-ladder (He 2020 / `gen-pentaladder`) is experimental and should be
  treated as unstable until the recursion path is fixed and `Psi_3` is fast.
