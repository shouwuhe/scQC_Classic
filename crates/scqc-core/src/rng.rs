//! 种子派生（DESIGN §12）。
//!
//! 单一用户可见 `seed` → 每个步骤 / 每个并行分组各自派生一个子种子，
//! 因此结果与线程数、调度顺序无关（SPEC §14.5 可复现性）。

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// 默认种子（SPEC §14.5：默认固定种子）。
pub const DEFAULT_SEED: u64 = 20260911;

/// 由 (seed, 步骤名, 分组编号) 确定性地派生一个 RNG。
///
/// 使用 SHA-256 混合，避免相邻分组的种子相关。
pub fn derive_rng(seed: u64, step: &str, group: u64) -> ChaCha8Rng {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(seed.to_le_bytes());
    h.update(step.as_bytes());
    h.update(group.to_le_bytes());
    let digest = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest[..32]);
    ChaCha8Rng::from_seed(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn same_inputs_same_stream() {
        let mut a = derive_rng(1, "emptydrops", 7);
        let mut b = derive_rng(1, "emptydrops", 7);
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn different_group_different_stream() {
        let mut a = derive_rng(1, "emptydrops", 7);
        let mut b = derive_rng(1, "emptydrops", 8);
        assert_ne!(a.next_u64(), b.next_u64());
    }
}
