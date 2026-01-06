#[test]
fn simplify_is_deterministic_across_processes() {
    let exprs = ["(+ (* x y) (* x z))", "(+ (+ x y) z)", "(* (* x y) z)"];
    let bin = assert_cmd::cargo::cargo_bin!("symbol_simplify");

    for expr in exprs {
        let mut outputs = Vec::new();
        for _ in 0..20 {
            let output = std::process::Command::new(bin)
                .arg("--aggressive")
                .arg(expr)
                .output()
                .expect("run symbol_simplify");
            assert!(output.status.success());
            outputs.push(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }

        for out in outputs.iter().skip(1) {
            assert_eq!(out, &outputs[0], "expr={expr}");
        }
    }
}
