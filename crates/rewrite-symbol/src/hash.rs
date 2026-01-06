const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Returns a deterministic 64-bit hash for a byte slice.
pub fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Returns a deterministic 64-bit hash for a string.
pub fn stable_hash_str(value: &str) -> u64 {
    stable_hash_bytes(value.as_bytes())
}

pub(crate) struct StableHasher {
    hash: u64,
}

impl StableHasher {
    pub(crate) fn new() -> Self {
        Self {
            hash: FNV_OFFSET_BASIS,
        }
    }

    pub(crate) fn update_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }

    pub(crate) fn update_str(&mut self, value: &str) {
        self.update_bytes(value.as_bytes());
    }

    pub(crate) fn update_u64(&mut self, value: u64) {
        self.update_bytes(&value.to_le_bytes());
    }

    pub(crate) fn update_i64(&mut self, value: i64) {
        self.update_bytes(&value.to_le_bytes());
    }

    pub(crate) fn finish(&self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::{stable_hash_bytes, stable_hash_str};

    #[test]
    fn stable_hash_matches_bytes() {
        let text = "stable";
        assert_eq!(stable_hash_str(text), stable_hash_bytes(text.as_bytes()));
    }

    #[test]
    fn stable_hash_is_deterministic() {
        let first = stable_hash_str("abc");
        let second = stable_hash_str("abc");
        assert_eq!(first, second);
    }
}
