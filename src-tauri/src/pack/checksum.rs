//! SHA-256 规范化校验和（契约 5.2）。
//! 算法：`checksum` 置 `None` → `serde_json::to_string`（结构体字段序由声明固定，规范串确定）
//! → SHA-256 → hex 小写，前缀 `sha256:`。导入时同法重算比对，不一致 → `E_CHECKSUM` 拒绝。

use sha2::{Digest, Sha256};

/// 校验和前缀（契约 5.2：`sha256:<hex小写>`）。
pub const CHECKSUM_PREFIX: &str = "sha256:";

/// 对规范 JSON 串计算 SHA-256，返回 `sha256:<hex小写>`。
pub fn sha256_hex(canonical: &str) -> String {
    let digest = Sha256::digest(canonical.as_bytes());
    format!("{CHECKSUM_PREFIX}{}", hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector_lowercase_hex_with_prefix() {
        // sha256("abc") 标准向量
        let sum = sha256_hex("abc");
        assert_eq!(
            sum,
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let hex_part = &sum[CHECKSUM_PREFIX.len()..];
        assert_eq!(hex_part.len(), 64);
        assert!(hex_part.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        assert_ne!(sha256_hex("abc"), sha256_hex("abd"), "不同输入必须不同");
    }
}
