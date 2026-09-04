//! 模型资源：数据目录存在性、占用大小、删除/从 APK 恢复（spec §4.5）
//!
//! 05 阶段：目录状态机 + 删除可用；assets 打包与惰性解压在票据 06。

use crate::Result;
use std::path::{Path, PathBuf};

/// 模型目录名（app 数据目录下）
pub const MODELS_DIR_NAME: &str = "ocr_models";

/// 模型目录路径（app 数据目录下，与插件资源解压同风格）
pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MODELS_DIR_NAME)
}

/// 模型是否已就位（目录存在且非空；空目录视为未解压完成）
pub fn models_present(data_dir: &Path) -> bool {
    match std::fs::read_dir(models_dir(data_dir)) {
        Ok(mut entries) => entries.next().is_some(),
        Err(_) => false,
    }
}

/// 模型占用字节数（递归统计；目录不存在 → 0）
pub fn models_bytes(data_dir: &Path) -> u64 {
    fn dir_size(path: &Path) -> u64 {
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let p = e.path();
                        if p.is_dir() {
                            dir_size(&p)
                        } else {
                            e.metadata().map(|m| m.len()).unwrap_or(0)
                        }
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
    let dir = models_dir(data_dir);
    if dir.is_dir() {
        dir_size(&dir)
    } else {
        0
    }
}

/// 删除已解压模型（不存在 → 幂等返回 deleted=false）；返回释放字节数。
/// 删除成功即 engineLoaded 复位（引擎加载在 07，届时先释放 session 再删）。
pub fn delete_models(data_dir: &Path) -> Result<(bool, u64)> {
    let dir = models_dir(data_dir);
    if !dir.exists() {
        return Ok((false, 0));
    }
    let freed = models_bytes(data_dir);
    std::fs::remove_dir_all(&dir).map_err(|e| {
        crate::AppError::Io(std::io::Error::new(
            e.kind(),
            format!("plugin_ocr_delete_models: failed to remove {}: {}", dir.display(), e),
        ))
    })?;
    Ok((true, freed))
}

/// 从 APK assets 恢复模型（惰性解压 + 版本标记；幂等：已就位则跳过）。
/// Android：经 OcrModelExtractorPlugin 解压；其他平台（桌面 dev）无 APK assets，
/// 已就位则跳过，否则明确报错（真机/模拟器之外无法恢复）。
pub async fn restore_models(data_dir: &Path, _app_version: &str) -> Result<bool> {
    if models_present(data_dir) {
        return Ok(true);
    }
    #[cfg(target_os = "android")]
    {
        let count = crate::plugin::android_plugins::extract_ocr_models(_app_version).await?;
        if count == 0 {
            return Err(crate::AppError::Plugin(
                "plugin_ocr_restore_models: no model assets in APK (resources/ocr_models)".into(),
            ));
        }
        if !models_present(data_dir) {
            return Err(crate::AppError::Internal(format!(
                "plugin_ocr_restore_models: extraction reported {} model(s) but ocr_models/ is still absent in {}",
                count,
                data_dir.display()
            )));
        }
        Ok(true)
    }
    #[cfg(not(target_os = "android"))]
    {
        Err(crate::AppError::Internal(
            "plugin_ocr_restore_models: no APK assets on this platform (models extract on Android)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 独立临时目录（带进程 id 防并行测试互踩），用后清理
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bedcode-ocr-models-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 未解压：present=false、bytes=0
    #[test]
    fn absent_dir_reports_missing_and_zero_bytes() {
        let dir = temp_dir("absent");
        assert!(!models_present(&dir));
        assert_eq!(models_bytes(&dir), 0);
    }

    /// 目录含文件即视为已就位，占用字节数正确
    #[test]
    fn present_when_dir_has_files() {
        let dir = temp_dir("present");
        std::fs::create_dir_all(models_dir(&dir)).unwrap();
        std::fs::write(models_dir(&dir).join("ch_PP-OCRv4_det_infer.onnx"), vec![0u8; 1024]).unwrap();
        assert!(models_present(&dir));
        assert_eq!(models_bytes(&dir), 1024);
    }

    /// 空目录视为未解压完成（解压中断残留）
    #[test]
    fn empty_dir_not_present() {
        let dir = temp_dir("empty");
        std::fs::create_dir_all(models_dir(&dir)).unwrap();
        assert!(!models_present(&dir));
    }

    /// 删除：目录移除、freedBytes 为实际占用
    #[test]
    fn delete_removes_dir_and_reports_freed_bytes() {
        let dir = temp_dir("delete");
        std::fs::create_dir_all(models_dir(&dir)).unwrap();
        std::fs::write(models_dir(&dir).join("a.onnx"), vec![0u8; 512]).unwrap();
        std::fs::write(models_dir(&dir).join("b.onnx"), vec![0u8; 256]).unwrap();
        let (deleted, freed) = delete_models(&dir).unwrap();
        assert!(deleted);
        assert_eq!(freed, 768);
        assert!(!models_dir(&dir).exists());
    }

    /// 未解压时删除幂等成功（deleted=false），不报错
    #[test]
    fn delete_is_idempotent_when_absent() {
        let dir = temp_dir("delete-absent");
        let (deleted, freed) = delete_models(&dir).unwrap();
        assert!(!deleted);
        assert_eq!(freed, 0);
    }

    /// 恢复：已就位幂等跳过；缺失 → 非 Android 平台明确报错（真机才有 APK assets）
    #[tokio::test]
    async fn restore_skips_when_present_and_rejects_when_absent() {
        let dir = temp_dir("restore");
        std::fs::create_dir_all(models_dir(&dir)).unwrap();
        std::fs::write(models_dir(&dir).join("a.onnx"), vec![0u8; 8]).unwrap();
        assert!(restore_models(&dir, "2.0.0").await.unwrap());

        std::fs::remove_dir_all(models_dir(&dir)).unwrap();
        let err = restore_models(&dir, "2.0.0").await.unwrap_err().to_string();
        assert!(err.contains("no APK assets"), "got: {}", err);
    }
}
