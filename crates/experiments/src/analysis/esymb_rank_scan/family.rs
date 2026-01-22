#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyType {
    PowLast,
    Block2,
}

impl FamilyType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PowLast => "pow-last",
            Self::Block2 => "block2",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SequenceSpec {
    pub family: FamilyType,
    pub params: Vec<String>,
    pub words: Vec<Vec<String>>,
}

impl SequenceSpec {
    pub fn param_string(&self) -> String {
        self.params.join(",")
    }

    pub fn sort_key(&self) -> (String, String) {
        (self.family.as_str().to_string(), self.param_string())
    }
}

pub fn generate_pow_last(x_set: &[String], y_set: &[String], loops: &[usize]) -> Vec<SequenceSpec> {
    let mut out = Vec::new();
    for x in x_set {
        for y in y_set {
            let mut words = Vec::with_capacity(loops.len());
            for &loop_index in loops {
                let len = 2usize.saturating_mul(loop_index);
                if len == 0 {
                    words.push(Vec::new());
                    continue;
                }
                let mut word = Vec::with_capacity(len);
                let repeat = len.saturating_sub(1);
                for _ in 0..repeat {
                    word.push(x.clone());
                }
                word.push(y.clone());
                words.push(word);
            }
            out.push(SequenceSpec {
                family: FamilyType::PowLast,
                params: vec![format!("x={x}"), format!("y={y}")],
                words,
            });
        }
    }
    out
}

pub fn generate_block2(pairs: &[String], loops: &[usize]) -> Vec<SequenceSpec> {
    let mut out = Vec::new();
    for u in pairs {
        for v in pairs {
            let mut words = Vec::with_capacity(loops.len());
            for &loop_index in loops {
                let len = 2usize.saturating_mul(loop_index);
                let mut word = Vec::with_capacity(len);
                for _ in 0..loop_index {
                    word.push(u.clone());
                    word.push(v.clone());
                }
                words.push(word);
            }
            out.push(SequenceSpec {
                family: FamilyType::Block2,
                params: vec![format!("u={u}"), format!("v={v}")],
                words,
            });
        }
    }
    out
}

pub fn generate_block2_pairs(pairs: &[(String, String)], loops: &[usize]) -> Vec<SequenceSpec> {
    let mut out = Vec::new();
    for (u, v) in pairs {
        let mut words = Vec::with_capacity(loops.len());
        for &loop_index in loops {
            let len = 2usize.saturating_mul(loop_index);
            let mut word = Vec::with_capacity(len);
            for _ in 0..loop_index {
                word.push(u.clone());
                word.push(v.clone());
            }
            words.push(word);
        }
        out.push(SequenceSpec {
            family: FamilyType::Block2,
            params: vec![format!("u={u}"), format!("v={v}")],
            words,
        });
    }
    out
}
