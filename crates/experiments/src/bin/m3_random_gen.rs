use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const M: usize = 64;
const W: usize = 5;
const P_LIST: &[(u32, u32)] = &[(6, 100), (12, 100)];
const SEEDS: &[u64] = &[20250301, 20250302];

#[derive(Clone, Debug)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn next_range(&mut self, low: i32, high: i32) -> i32 {
        let span = (high - low + 1) as u32;
        low + (self.next_u32() % span) as i32
    }
}

fn threshold(p_num: u32, p_den: u32) -> u64 {
    let denom = p_den as u64;
    let scale = u32::MAX as u64 + 1;
    (p_num as u64) * scale / denom
}

fn name_width(m: usize) -> usize {
    let max_idx = m.saturating_sub(1);
    max_idx.to_string().len().max(1)
}

fn format_name(idx: usize, width: usize) -> String {
    format!("l{:0width$}", idx, width = width)
}

fn format_p(p_num: u32, p_den: u32) -> String {
    let p = p_num as f64 / p_den as f64;
    format!("{p:.2}")
}

fn build_spec(m: usize, w: usize, p_num: u32, p_den: u32, seed: u64, id: &str) -> String {
    let mut rng = Lcg::new(seed);
    let width = name_width(m);
    let mut names = Vec::with_capacity(m);
    for i in 0..m {
        names.push(format_name(i, width));
    }

    let mut out = String::new();
    out.push_str("[experiment]\n");
    out.push_str(&format!("id = \"{id}\"\n"));
    out.push_str(&format!("out_dir = \"reports/m3/random/{id}\"\n"));
    out.push_str(&format!("w_min = {w}\n"));
    out.push_str(&format!("w_max = {w}\n\n"));

    out.push_str("[alphabet]\n");
    out.push_str("vars = [\"x\", \"y\"]\n\n");

    for name in &names {
        let a = rng.next_range(1, 9);
        let b = rng.next_range(1, 9);
        out.push_str("[[alphabet.letters]]\n");
        out.push_str(&format!("name = \"{name}\"\n"));
        out.push_str(&format!(
            "expr = \"(+ 1 (* {a} (^ x 2)) (* {b} (^ y 2)))\"\n\n"
        ));
    }

    let mut pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
    for i in 0..m {
        pairs.insert((i, (i + 1) % m));
    }
    let thresh = threshold(p_num, p_den);
    for a in 0..m {
        for b in 0..m {
            if a == b || pairs.contains(&(a, b)) {
                continue;
            }
            if (rng.next_u32() as u64) < thresh {
                pairs.insert((a, b));
            }
        }
    }

    out.push_str("[constraints]\n");
    out.push_str("adjacency_mode = \"allow\"\n");
    out.push_str("adjacency_pairs = [\n");
    let pairs_vec: Vec<(usize, usize)> = pairs.into_iter().collect();
    for (idx, (a, b)) in pairs_vec.iter().enumerate() {
        out.push_str(&format!("  [\"{}\", \"{}\"]", names[*a], names[*b]));
        if idx + 1 != pairs_vec.len() {
            out.push(',');
        }
        if (idx + 1) % 4 == 0 || idx + 1 == pairs_vec.len() {
            out.push('\n');
        } else {
            out.push(' ');
        }
    }
    out.push_str("]\n\n");

    out.push_str("[constraints.budget]\n");
    out.push_str(&format!("max_states = {m}\n\n"));

    out.push_str("[pairs]\n");
    out.push_str("count_mode = \"active_word_positions\"\n");
    out
}

fn run() -> Result<(), String> {
    let out_dir = PathBuf::from("experiments").join("m3").join("random");
    fs::create_dir_all(&out_dir).map_err(|err| err.to_string())?;

    let mut catalog = String::new();
    catalog.push_str("spec,m,w,p,seed\n");
    let mut count = 0usize;

    for &seed in SEEDS {
        for &(p_num, p_den) in P_LIST {
            let p_label = format_p(p_num, p_den);
            let id = format!("RND_m{M}_w{W}_p{p_label}_seed{seed}");
            let spec_name = format!("{id}.toml");
            let spec = build_spec(M, W, p_num, p_den, seed, &id);
            fs::write(out_dir.join(&spec_name), spec).map_err(|err| err.to_string())?;
            catalog.push_str(&format!("{spec_name},{M},{W},{p_label},{seed}\n"));
            count += 1;
        }
    }

    fs::write(out_dir.join("catalog.csv"), catalog).map_err(|err| err.to_string())?;
    println!("wrote {count} specs to {}", out_dir.display());
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
