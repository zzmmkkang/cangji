//! 数据包（.cjpack）实现（契约 5 章）。【A4 实现】：
//! 导出（四表全量 + 规范化 JSON + SHA-256 校验和）与导入前校验（5.3 规则）。

#![allow(dead_code)]

pub mod checksum;
pub mod model;
