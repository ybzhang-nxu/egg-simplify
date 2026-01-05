use criterion::{criterion_group, criterion_main, Criterion};
use mpl_ir::parse_sexpr;
use mpl_rewrite::{simplify_algebra, RewriteConfig, RewriteMode};

fn bench_rewrite_simplify(c: &mut Criterion) {
    let expr = parse_sexpr("(+ (* x y) (* x z) (* x w))")
        .expect("parse")
        .normalize();
    let cfg = RewriteConfig {
        iters: 15,
        node_limit: 20_000,
        time_limit_ms: 200,
        mode: RewriteMode::Aggressive,
    };
    c.bench_function("rewrite_simplify_aggressive", |b| {
        b.iter(|| {
            let _ = simplify_algebra(&expr, &cfg).expect("simplify");
        })
    });
}

criterion_group!(benches, bench_rewrite_simplify);
criterion_main!(benches);
