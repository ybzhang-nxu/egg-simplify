use crate::ExperimentError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fp {
    value: u64,
}

impl Fp {
    pub fn is_zero(self) -> bool {
        self.value == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    prime: u64,
}

impl Field {
    pub fn new(prime: u64) -> Result<Self, ExperimentError> {
        if prime < 2 {
            return Err(ExperimentError::InvalidConfig(
                "prime must be >= 2".to_string(),
            ));
        }
        Ok(Self { prime })
    }

    pub fn prime(self) -> u64 {
        self.prime
    }

    pub fn zero(self) -> Fp {
        Fp { value: 0 }
    }

    pub fn one(self) -> Fp {
        self.reduce_u64(1)
    }

    pub fn reduce_u64(self, value: u64) -> Fp {
        Fp {
            value: value % self.prime,
        }
    }

    pub fn reduce_i64(self, value: i64) -> Fp {
        let prime = self.prime as i128;
        let reduced = (value as i128).rem_euclid(prime) as u64;
        Fp { value: reduced }
    }

    pub fn add(self, left: Fp, right: Fp) -> Fp {
        let sum = (left.value as u128 + right.value as u128) % self.prime as u128;
        Fp { value: sum as u64 }
    }

    pub fn sub(self, left: Fp, right: Fp) -> Fp {
        if left.value >= right.value {
            Fp {
                value: left.value - right.value,
            }
        } else {
            Fp {
                value: self.prime - (right.value - left.value),
            }
        }
    }

    pub fn mul(self, left: Fp, right: Fp) -> Fp {
        let prod = (left.value as u128 * right.value as u128) % self.prime as u128;
        Fp { value: prod as u64 }
    }

    pub fn inv(self, value: Fp) -> Option<Fp> {
        if value.is_zero() {
            return None;
        }
        let mut t: i128 = 0;
        let mut new_t: i128 = 1;
        let mut r: i128 = self.prime as i128;
        let mut new_r: i128 = value.value as i128;
        while new_r != 0 {
            let q = r / new_r;
            let next_t = t - q * new_t;
            t = new_t;
            new_t = next_t;
            let next_r = r - q * new_r;
            r = new_r;
            new_r = next_r;
        }
        if r != 1 {
            return None;
        }
        if t < 0 {
            t += self.prime as i128;
        }
        Some(self.reduce_u64(t as u64))
    }
}
