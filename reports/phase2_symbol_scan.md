# Phase 2 symbol-aware scan (deterministic)

Grid:

- iters: 3, 10
- node_limit: 2000, 10000
- symbol_fuel: 0, 20, 100, 1000

Full results: `reports/phase2_symbol_scan.csv`

## Expr: `(+ (* x y) (* x z))`

| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3 | 2000 | 0 | 5 | Unknown(BudgetExhausted) | `(* (+ y z) x)` |
| 2 | 3 | 2000 | 20 | 5 | ByWeight([]) | `(* (+ y z) x)` |
| 3 | 3 | 2000 | 100 | 5 | ByWeight([]) | `(* (+ y z) x)` |

## Expr: `(+ (+ x y) z)`

| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3 | 2000 | 0 | 4 | Unknown(BudgetExhausted) | `(+ x y z)` |
| 2 | 3 | 2000 | 20 | 4 | ByWeight([]) | `(+ x y z)` |
| 3 | 3 | 2000 | 100 | 4 | ByWeight([]) | `(+ x y z)` |

## Expr: `(* (* x y) z)`

| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3 | 2000 | 0 | 4 | Unknown(BudgetExhausted) | `(* x y z)` |
| 2 | 3 | 2000 | 20 | 4 | ByWeight([]) | `(* x y z)` |
| 3 | 3 | 2000 | 100 | 4 | ByWeight([]) | `(* x y z)` |

## Expr: `(li2 x)`

| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3 | 2000 | 0 | 2 | Unknown(BudgetExhausted) | `(li2 x)` |
| 2 | 3 | 2000 | 20 | 2 | ByWeight([2]) | `(li2 x)` |
| 3 | 3 | 2000 | 100 | 2 | ByWeight([2]) | `(li2 x)` |

## Expr: `(* (log x) (log y))`

| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3 | 2000 | 0 | 5 | Unknown(BudgetExhausted) | `(* (log x) (log y))` |
| 2 | 3 | 2000 | 20 | 5 | ByWeight([2]) | `(* (log x) (log y))` |
| 3 | 3 | 2000 | 100 | 5 | ByWeight([2]) | `(* (log x) (log y))` |

## Expr: `(+ 7 (* (log x) (log y) (log z)))`

| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3 | 2000 | 0 | 9 | Unknown(BudgetExhausted) | `(+ 7 (* (log x) (log y) (log z)))` |
| 2 | 3 | 2000 | 20 | 9 | Unknown(SymbolNotImplemented) | `(+ 7 (* (log x) (log y) (log z)))` |
| 3 | 3 | 2000 | 100 | 9 | Unknown(SymbolNotImplemented) | `(+ 7 (* (log x) (log y) (log z)))` |

