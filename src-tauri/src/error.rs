//! 统一错误结构（契约 4.0 通用约定 + 7.5 错误码表）。
//!
//! 所有 Tauri 命令返回 `Result<T, CommandError>`；
//! 序列化后前端 catch 到 `{ code, message, detail }`（camelCase）。

use serde::{Deserialize, Serialize};

/// 错误码字符串枚举（契约 7.5），序列化为 `E_*`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    #[serde(rename = "E_DB")]
    Db,
    #[serde(rename = "E_VALIDATION")]
    Validation,
    #[serde(rename = "E_NOT_FOUND")]
    NotFound,
    #[serde(rename = "E_DUPLICATE")]
    Duplicate,
    #[serde(rename = "E_PACK_FORMAT")]
    PackFormat,
    #[serde(rename = "E_PACK_VERSION")]
    PackVersion,
    #[serde(rename = "E_CHECKSUM")]
    Checksum,
    #[serde(rename = "E_BACKUP")]
    Backup,
    #[serde(rename = "E_IO")]
    Io,
    #[serde(rename = "E_PATH_INVALID")]
    PathInvalid,
    #[serde(rename = "E_INTERNAL")]
    Internal,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            ErrorCode::Db => "E_DB",
            ErrorCode::Validation => "E_VALIDATION",
            ErrorCode::NotFound => "E_NOT_FOUND",
            ErrorCode::Duplicate => "E_DUPLICATE",
            ErrorCode::PackFormat => "E_PACK_FORMAT",
            ErrorCode::PackVersion => "E_PACK_VERSION",
            ErrorCode::Checksum => "E_CHECKSUM",
            ErrorCode::Backup => "E_BACKUP",
            ErrorCode::Io => "E_IO",
            ErrorCode::PathInvalid => "E_PATH_INVALID",
            ErrorCode::Internal => "E_INTERNAL",
        };
        f.write_str(code)
    }
}

/// 命令统一错误：code + 中文可读 message（可直接 toast）+ 可选 detail。
#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<serde_json::Value>,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    /// 附带结构化上下文（如重名字段、冲突明细）。
    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }
    // A4 已交付全部数据包/备份/统计 Excel 命令，原「unimplemented 占位」helper 随之移除。
}

/// 作为 Tauri 命令 Err 序列化给 WebView：`{ code, message, detail }`。
impl Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Repr<'a> {
            code: ErrorCode,
            message: &'a str,
            detail: &'a Option<serde_json::Value>,
        }

        Repr {
            code: self.code,
            message: self.message.as_str(),
            detail: &self.detail,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_camel_case_object() {
        let err = CommandError::new(ErrorCode::Db, "数据库读写失败");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "E_DB");
        assert_eq!(json["message"], "数据库读写失败");
        assert!(json["detail"].is_null());
    }
}
