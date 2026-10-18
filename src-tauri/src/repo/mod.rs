//! 纯 SQL 仓储层（契约 2 章）：不感知 Tauri，只依赖 rusqlite 与 DTO。
//! 边界：transactions.rs 强制 append-only（仅 INSERT / SELECT，改写旧行的语句不出现）；
//! 改写类语句仅允许出现在 items / partners / operators / settings 上（契约 4.4）。

#![allow(dead_code)]

pub mod inventory;
pub mod items;
pub mod merge;
pub mod operators;
pub mod partners;
pub mod settings;
pub mod stats;
pub mod transactions;

use crate::error::{CommandError, ErrorCode};

/// rusqlite 错误 → 契约 7.5 错误码映射：
/// UNIQUE → E_DUPLICATE、外键 → E_NOT_FOUND、NOT NULL/CHECK → E_VALIDATION、其余 → E_DB；
/// detail 保留 SQLite 约束上下文（含冲突字段清单）。
pub(crate) fn map_db_err(err: &rusqlite::Error, context: &str) -> CommandError {
    match err {
        rusqlite::Error::SqliteFailure(e, msg) => {
            let msg = msg.as_deref().unwrap_or("").to_string();
            if e.code == rusqlite::ffi::ErrorCode::ConstraintViolation {
                // 形如 "UNIQUE constraint failed: operators.name" → 提取字段清单
                let fields = msg.split_once(": ").map(|(_, f)| f.to_string());
                let mk = |kind: &str, code: ErrorCode, message: String| {
                    let mut detail = serde_json::json!({ "constraint": kind });
                    if let Some(f) = &fields {
                        detail["fields"] = serde_json::Value::String(f.clone());
                    }
                    CommandError::new(code, message).with_detail(detail)
                };
                return match e.extended_code {
                    rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                    | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY => {
                        mk("UNIQUE", ErrorCode::Duplicate, format!("{context}：数据重复（违反唯一约束）"))
                    }
                    rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY => {
                        mk("FOREIGN_KEY", ErrorCode::NotFound, format!("{context}：引用的目标不存在"))
                    }
                    rusqlite::ffi::SQLITE_CONSTRAINT_NOTNULL => {
                        mk("NOT_NULL", ErrorCode::Validation, format!("{context}：必填字段缺失（{msg}）"))
                    }
                    rusqlite::ffi::SQLITE_CONSTRAINT_CHECK => {
                        mk("CHECK", ErrorCode::Validation, format!("{context}：取值不符合检查约束（{msg}）"))
                    }
                    _ => CommandError::new(ErrorCode::Db, format!("{context}：{msg}")),
                };
            }
            CommandError::new(ErrorCode::Db, format!("{context}：{msg}"))
        }
        rusqlite::Error::QueryReturnedNoRows => {
            CommandError::new(ErrorCode::NotFound, format!("{context}：目标不存在"))
        }
        other => CommandError::new(ErrorCode::Db, format!("{context}：{other}")),
    }
}

/// 可空列入参规整：空白 → None（NULL），非空去首尾空白。
pub(crate) fn norm_opt(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 校验并归一化 RFC3339 时间为 UTC 毫秒格式（契约 7.3：存储统一 UTC，字符串序 = 时间序）。
pub(crate) fn normalize_rfc3339(s: &str, field: &str) -> Result<String, CommandError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .map_err(|_| CommandError::new(ErrorCode::Validation, format!("{field} 不是合法的 RFC3339 时间")))
}
