//! 壳 UI 资产导入（用户选图 → 落 `$APPDATA`）
//!
//! 机制面，不是业务面（AGENTS §5.1 三问）：
//!   ① 离宿主能不能实现？不能——前端零资源访问红线（§6）下壳不能自己读盘选图；
//!   ② 携带产品语义吗？不携带——宿主只认识「命名空间 + 一张图片文件」，
//!      不知道它会被当成壁纸还是别的；
//!   ③ 纯增量、无产品默认值。
//!
//! 只做三件事：弹系统图片选择器 → 把选中文件拷进
//! `app_data_dir/shell-assets/<namespace>/`（单槽位，旧图删除）→ 返回落盘绝对路径。
//! 前端拿路径经 asset protocol 取图（见 `src/plugin/assets.ts`），文件内容不过前端。
//!
//! 双平台差异只在「选中的是什么」：桌面返回真实路径，Android 的
//! `ACTION_GET_CONTENT` 返回 `content://` URI（分区存储下不可按路径直读），
//! 后者先经 SafIo 中转复制到应用 cache 再读——与对等传输的上传选源同一条链路。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::{AppError, Result};

/// 资产根目录（`app_data_dir` 下）
const ASSETS_ROOT: &str = "shell-assets";
/// 允许的图片扩展名：对话框过滤器只是 UX，真正的准入判据在这里
const ALLOWED_IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
/// 命名空间长度上限（防超长目录名）
const NAMESPACE_MAX_LEN: usize = 32;
/// 魔数嗅探所需前缀字节数
const SNIFF_LEN: usize = 12;
/// SAF 中转副本的临时名（cache 下，拷完即删）
const SAF_TEMP_NAME: &str = "shell-asset-source";
/// 同毫秒内连续导入的序号（保证文件名不撞：撞名会让 asset URL 不变、WebView 缓存旧的图）
static ASSET_SEQ: AtomicU64 = AtomicU64::new(0);

/// 让用户挑一张图片并落盘，返回落盘后的绝对路径
///
/// - `namespace`：调用方给的目录分区（如壳的 `wallpaper`）；宿主不解释其含义，
///   只把它当合法目录名做隔离（不同用途的资产互不覆盖）。
/// - 用户取消返回 `Ok(None)`（不是错误，调用方不提示错误）。
#[tauri::command]
pub async fn shell_import_image(app_handle: AppHandle, namespace: String) -> Result<Option<String>> {
    validate_namespace(&namespace)?;

    // 系统选择器是阻塞调用：丢到阻塞线程池，别占住 async 运行时线程等用户操作
    let dialog_handle = app_handle.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_handle
            .dialog()
            .file()
            .add_filter("Image", ALLOWED_IMAGE_EXTS)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| AppError::Internal(format!("shell asset: picker task failed: {e}")))?;

    let Some(picked) = picked else { return Ok(None) };

    // 选中项 → 本机可读路径（+ 需要清理的中转副本）
    let (source, temp_copy) = match picked {
        tauri_plugin_dialog::FilePath::Path(path) => (path, None),
        tauri_plugin_dialog::FilePath::Url(url) => {
            if url.scheme() == "file" {
                let path = url.to_file_path().map_err(|_| {
                    AppError::InvalidInput(format!("shell asset: file url not convertible: {url}"))
                })?;
                (path, None)
            } else {
                let cache_path = copy_saf_to_cache(&app_handle, url.as_str()).await?;
                (cache_path.clone(), Some(cache_path))
            }
        }
    };

    // 扩展名：文件名优先，SAF 的 content URI 常无后缀 → 用魔数嗅探兜底
    let ext = match image_ext_from_name(&source.to_string_lossy()) {
        Some(ext) => ext,
        None => read_head(&source)
            .and_then(|head| sniff_image_ext(&head).map(str::to_string))
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "shell asset: unsupported image '{}' (allowed: {})",
                    source.display(),
                    ALLOWED_IMAGE_EXTS.join("/")
                ))
            })?,
    };

    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("shell asset: app data dir unavailable: {e}")))?
        .join(ASSETS_ROOT)
        .join(&namespace);

    let stored = store_asset(&dir, &namespace, &ext, &source);

    // 中转副本用完即删：失败不影响本次结果（资产已落盘），但要留痕
    if let Some(temp) = temp_copy {
        if let Err(e) = fs::remove_file(&temp) {
            tracing::warn!("[ShellUi] SAF relay copy cleanup failed: path={} err={}", temp.display(), e);
        }
    }

    stored.map(Some)
}

/// 校验命名空间：仅 `[a-z0-9_-]`，1..=32 字符
///
/// 目录名直接来自调用方，必须挡住路径穿越（`../`）、绝对路径与大小写歧义
/// （APFS / ext4 上 `Wallpaper` 与 `wallpaper` 是两个目录，会各自留一份资产）。
fn validate_namespace(namespace: &str) -> Result<()> {
    if namespace.is_empty() || namespace.len() > NAMESPACE_MAX_LEN {
        return Err(AppError::InvalidInput(format!(
            "shell asset: namespace length must be 1..={NAMESPACE_MAX_LEN}, got {}",
            namespace.len()
        )));
    }
    let ok = namespace
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !ok {
        return Err(AppError::InvalidInput(format!(
            "shell asset: namespace must match [a-z0-9_-]+, got '{namespace}'"
        )));
    }
    Ok(())
}

/// 取文件名里的小写扩展名；不在图片白名单内（或没有扩展名）返回 None
fn image_ext_from_name(name: &str) -> Option<String> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    ALLOWED_IMAGE_EXTS.contains(&ext.as_str()).then_some(ext)
}

/// 按魔数嗅探图片格式（选择器给不出扩展名时的兜底）
fn sniff_image_ext(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("png");
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        return Some("webp");
    }
    if head.starts_with(b"BM") {
        return Some("bmp");
    }
    None
}

/// 读文件头若干字节（够嗅探即可；读不到返回 None，由调用方报不支持）
fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut file = fs::File::open(path).ok()?;
    let mut buf = vec![0u8; SNIFF_LEN];
    let read = file.read(&mut buf).ok()?;
    buf.truncate(read);
    Some(buf)
}

/// 把源文件拷进资产目录（单槽位），返回落盘绝对路径
///
/// 文件名带毫秒时间戳 + 进程内序号：asset protocol 的 URL 由路径决定，换图后路径
/// 必须变化，否则 WebView 会命中上一张的缓存（表现为「换了图但没生效」）。
/// 只用毫秒时间戳不够——同一毫秒内的两次导入会算出同一个名字（单测实测报红），
/// 序号兼做存在性回退：撞名就继续推进。
/// 先拷新、再删旧——新图落盘失败时至少还留着上一张可用，不会两头都空。
fn store_asset(dir: &Path, namespace: &str, ext: &str, source: &Path) -> Result<String> {
    fs::create_dir_all(dir).map_err(|e| {
        AppError::Internal(format!(
            "shell asset: create dir '{}' failed: {e}",
            dir.display()
        ))
    })?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let (target, file_name) = loop {
        let seq = ASSET_SEQ.fetch_add(1, Ordering::Relaxed);
        let file_name = format!("{namespace}-{stamp}-{seq}.{ext}");
        let candidate = dir.join(&file_name);
        if !candidate.exists() {
            break (candidate, file_name);
        }
    };
    fs::copy(source, &target).map_err(|e| {
        AppError::Internal(format!(
            "shell asset: copy '{}' -> '{}' failed: {e}",
            source.display(),
            target.display()
        ))
    })?;

    remove_stale_assets(dir, namespace, &file_name);
    Ok(target.to_string_lossy().to_string())
}

/// 删除同命名空间的其它资产（单槽位语义）
///
/// 清理失败只记日志：本次导入已经成功，不能因为清不掉旧文件就把新图也报成失败。
fn remove_stale_assets(dir: &Path, namespace: &str, keep: &str) {
    let prefix = format!("{namespace}-");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("[ShellUi] asset dir unreadable for cleanup: dir={} err={}", dir.display(), e);
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!("[ShellUi] asset dir entry unreadable: dir={} err={}", dir.display(), e);
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if name == keep || !name.starts_with(&prefix) {
            continue;
        }
        if let Err(e) = fs::remove_file(entry.path()) {
            tracing::warn!(
                "[ShellUi] stale asset cleanup failed: path={} err={}",
                entry.path().display(),
                e
            );
        }
    }
}

/// SAF content URI → 应用 cache 中转副本（Android：分区存储下 content URI 不可按路径直读）
///
/// 与对等传输的上传选源同一套轮询语义：read_to_cache 起拷贝 → copy_status 轮询到
/// finished。非 Android 平台没有 content URI（选择器返回真实路径），走不到这里。
async fn copy_saf_to_cache(app: &AppHandle, uri: &str) -> Result<PathBuf> {
    let saf = app.state::<crate::plugin::saf_io::SafIoState>();
    let handle = saf
        .read_to_cache(uri, SAF_TEMP_NAME)
        .map_err(|e| AppError::Internal(format!("shell asset: SAF copy start failed: {e}")))?;
    loop {
        let status = saf
            .copy_status(&handle.copy_id)
            .map_err(|e| AppError::Internal(format!("shell asset: SAF copy poll failed: {e}")))?;
        if status.finished {
            if status.cancelled {
                return Err(AppError::Internal("shell asset: SAF copy cancelled".to_string()));
            }
            if let Some(err) = status.error {
                return Err(AppError::Internal(format!("shell asset: SAF copy failed: {err}")));
            }
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    Ok(PathBuf::from(handle.dest_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== validate_namespace ====================

    #[test]
    fn accepts_lowercase_slug_namespace() {
        for ns in ["wallpaper", "a", "shell-ui_9", "x".repeat(NAMESPACE_MAX_LEN).as_str()] {
            assert!(validate_namespace(ns).is_ok(), "应接受合法命名空间: {ns}");
        }
    }

    #[test]
    fn rejects_namespace_with_path_injection_case_or_length() {
        // 路径穿越 / 绝对路径 / 大写 / 空 / 超长：全部拒绝（目录名直接进路径）
        for ns in ["", "..", "../etc", "a/b", "/abs", "Wallpaper", "wall paper", "壁纸"] {
            assert!(validate_namespace(ns).is_err(), "应拒绝非法命名空间: {ns}");
        }
        assert!(validate_namespace(&"a".repeat(NAMESPACE_MAX_LEN + 1)).is_err());
    }

    // ==================== 扩展名判定 ====================

    #[test]
    fn reads_lowercase_ext_from_image_name() {
        assert_eq!(image_ext_from_name("a.PNG"), Some("png".to_string()));
        assert_eq!(image_ext_from_name("a.JPEG"), Some("jpeg".to_string()));
        assert_eq!(image_ext_from_name("dir/sub/photo.WebP"), Some("webp".to_string()));
    }

    #[test]
    fn returns_none_for_non_image_name() {
        for name in ["a.txt", "noext", "a.png.txt", ".hidden", ""] {
            assert_eq!(image_ext_from_name(name), None, "不应认作图片: {name}");
        }
    }

    // ==================== 魔数嗅探 ====================

    #[test]
    fn sniffs_known_formats_from_head_bytes() {
        let cases: [(&[u8], &str); 5] = [
            (&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], "png"),
            (&[0xFF, 0xD8, 0xFF, 0xE0], "jpg"),
            (b"GIF89a\x00\x00", "gif"),
            (b"RIFF\x00\x00\x00\x00WEBPVP8 ", "webp"),
            (b"BM\x00\x00", "bmp"),
        ];
        for (head, expected) in cases {
            assert_eq!(sniff_image_ext(head), Some(expected));
        }
    }

    #[test]
    fn returns_none_for_non_image_head() {
        assert_eq!(sniff_image_ext(b"plain text"), None);
        assert_eq!(sniff_image_ext(&[]), None);
        // RIFF 但不是 WEBP（如 wav）不能误判
        assert_eq!(sniff_image_ext(b"RIFF\x00\x00\x00\x00WAVEfmt "), None);
    }

    // ==================== 落盘（单槽位） ====================

    #[test]
    fn stores_and_replaces_asset_for_same_namespace() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("wallpaper");
        let first_src = root.path().join("first.png");
        let second_src = root.path().join("second.png");
        fs::write(&first_src, b"first-image").expect("write first");
        fs::write(&second_src, b"second-image").expect("write second");

        let first = store_asset(&dir, "wallpaper", "png", &first_src).expect("store first");
        assert!(Path::new(&first).exists(), "首次落盘的文件应存在");
        assert!(first.contains("wallpaper-"), "文件名应带命名空间前缀: {first}");

        let second = store_asset(&dir, "wallpaper", "png", &second_src).expect("store second");
        assert_ne!(first, second, "换图后路径必须变化（asset URL 缓存击穿）");
        assert!(Path::new(&second).exists());
        assert!(!Path::new(&first).exists(), "旧图应被单槽位清理掉");

        let remaining: Vec<String> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(remaining.len(), 1, "同命名空间只应保留一份: {remaining:?}");
        assert_eq!(fs::read(&second).expect("read back"), b"second-image");
    }

    #[test]
    fn keeps_other_namespaces_when_cleaning() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("assets");
        fs::create_dir_all(&dir).expect("mkdir");
        // 另一命名空间的资产（前缀不同）不得被本次清理误删
        let other = dir.join("avatar-1.png");
        fs::write(&other, b"avatar").expect("write other");
        let src = root.path().join("src.png");
        fs::write(&src, b"wall").expect("write src");

        let stored = store_asset(&dir, "wallpaper", "png", &src).expect("store");

        assert!(Path::new(&stored).exists());
        assert!(other.exists(), "其它命名空间的资产必须保留");
    }

    #[test]
    fn reports_error_when_source_missing() {
        let root = tempfile::tempdir().expect("tempdir");
        let err = store_asset(
            &root.path().join("ns"),
            "ns",
            "png",
            &root.path().join("missing.png"),
        )
        .expect_err("源文件不存在应报错");
        // 错误必须带操作上下文（AGENTS：禁止无上下文透传）
        assert!(format!("{err}").contains("copy"), "错误应含操作上下文: {err}");
    }
}
