use criterion::{criterion_group, criterion_main, Criterion};
use mpl_ir::parse_sexpr;

fn bench_parse_normalize(c: &mut Criterion) {
    let input = "(+ (* x 2) (+ y 3) (* (+ x 1) (+ y 2)) (* 4 x y))";
    c.bench_function("parse_normalize_1000", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let expr = parse_sexpr(input).expect("parse");
                let _normalized = expr.normalize();
            }
        })
    });
}

criterion_group!(benches, bench_parse_normalize);
criterion_main!(benches);
