//! Tauri 命令薄封装层（契约第 4 章）：参数校验 → 调 repo。

pub mod app;
pub mod backup;
pub mod inventory;
pub mod items;
pub mod operators;
pub mod pack;
pub mod partners;
pub mod stats;
pub mod transactions;

use std::path::{Path, PathBuf};

use crate::error::{CommandError, ErrorCode};

/// 导出类命令共用的路径解析（契约 4.9 / 7.5）：
/// - `target_path` 给定：空路径或扩展名不符 → `E_PATH_INVALID`；
/// - `target_path` 缺省（Android 或调用方未选）：写入库文件同级 `exports/` 目录，
///   文件名 `cangji-<kind>-YYYYMMDD-HHmmss.<ext>`（设备本地时间，契约 5.2 命名口径）。
pub(crate) fn resolve_export_path(
    target_path: Option<&str>,
    db_path: &str,
    extension: &str,
    kind: &str,
) -> Result<PathBuf, CommandError> {
    if let Some(raw) = target_path {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CommandError::new(ErrorCode::PathInvalid, "导出路径不能为空"));
        }
        let path = Path::new(trimmed);
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case(extension) => {}
            _ => {
                return Err(CommandError::new(
                    ErrorCode::PathInvalid,
                    format!("导出文件扩展名应为 .{extension}"),
                ));
            }
        }
        return Ok(path.to_path_buf());
    }

    // 缺省目录：库文件所在目录/exports（内存库或无父目录时退到系统临时目录）
    let base = Path::new(db_path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("exports");
    std::fs::create_dir_all(&dir)
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("创建导出目录失败：{e}")))?;
    let name = format!(
        "cangji-{kind}-{}.{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        extension
    );
    Ok(dir.join(name))
}

/// 从导出结果路径取文件名（显示与 ExportedFile.file_name 用）。
pub(crate) fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
