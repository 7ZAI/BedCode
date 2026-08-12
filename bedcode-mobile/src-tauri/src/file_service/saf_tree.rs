//! SAF 树根解析与中转复制（file_service 三端点 SAF 化，M2）
//!
//! 挂载根分两类（spec「Implementation Decisions」file_service 三端点）：
//! - 真实路径根（`PathBuf`）：app 私有下载目录特殊条目等，走 std::fs（现有逻辑）
//! - SAF 树根（`content://tree/...`）：经 [`SafIo`]::list_tree（DocumentsContract
//!   遍历）替代 std::fs::read_dir；下载源经 [`SafIo`]::read_to_cache 中转复制
//!   （Relay Copy，不可续）到私有中转目录后，由现有 Range 响应从中转文件服务
//!
//! 本模块提供：
//! - [`is_saf_tree_uri`] / [`tree_document_id`] / [`tree_alias`]：树 URI 判别与
//!   根 document id / 顶层别名派生（list 顶层条目名 + 请求路径首段映射）
//! - [`match_saf_root`]：相对路径首段命中 SAF 根别名 → 返回 (tree_uri, 剩余分量)
//! - [`walk_to_entry`]：沿分量逐层 list_tree 下降到目标条目（文件或目录）
//! - [`ensure_relay_copy`]：SAF 源 → 私有中转目录顺序流复制（不可续），完成后
//!   `.part` → 最终名原子落位；副本命名含源 URI 哈希（FNV-1a，跨重启确定），
//!   续传命中（同 URI 请求）复用同一副本
//! - [`arm_relay_cleanup`] / [`sweep_relay_dir`]：副本 TTL 清理（滑动续期）与
//!   启动扫描清理（服务启动时删除全部副本，重启残留可重新生成）
//!
//! 中转副本生命周期（spec）：服务完成/超时后清理（TTL 内续传命中需文件仍在）；
//! 启动扫描清理兜底进程崩溃残留。本模块不依赖 tauri / actix，可独立单测。

use crate::plugin::saf_io::{SafEntry, SafIo};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// 中转缓存子目录名（app cache 下；与 Kotlin 上传 staging `bedcode_uploads`
/// 分离——两者生命周期不同：上传 staging 由 Kotlin cleanupStaleCopies 清扫，
/// 下载中转副本由本模块 TTL/启动扫描管理）
const RELAY_CACHE_SUBDIR: &str = "bedcode_downloads";

/// 副本 TTL：最后一次访问后 1 小时删除（桌面端断点续传窗口内文件仍命中；
/// 副本可随时从 SAF 重新生成，无需更长保留）
const RELAY_CACHE_TTL: Duration = Duration::from_secs(3600);

/// 中转复制完成轮询间隔（Kotlin 复制在后台线程执行，轮询取其终态）
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// 中转副本共享状态（进程内；服务重启时由 sweep_relay_dir 重置）
struct RelayState {
    /// 副本哈希 → 最后访问时间（TTL 清理滑动续期依据）
    last_access: HashMap<String, Instant>,
    /// 副本哈希 → 已有清理任务在跑（防同哈希重复 spawn）
    pending_cleanup: HashSet<String>,
}

static RELAY_STATE: LazyLock<Mutex<RelayState>> = LazyLock::new(|| {
    Mutex::new(RelayState {
        last_access: HashMap::new(),
        pending_cleanup: HashSet::new(),
    })
});

// ==================== 树 URI 解析 ====================

/// 挂载根是否为 SAF 树 URI
///
/// 识别两种形态：
/// - 紧凑内部形态 `content://tree/<treeId>`（mock/单测/历史条目）
/// - 系统目录树选择器返回的完整形态 `content://<authority>/tree/<treeId>`
///   （如 content://com.android.externalstorage.documents/tree/primary%3A…）
/// 两种形态的 treeId 均为 URI 末段，`tree_document_id` / `tree_alias` 通用；
/// 完整形态必须识别，否则挂载时 SAF 根被误当作真实路径根（canonicalize
/// 失败 → 挂载失败；或作为伪真实根被 list 静默跳过 → 对端永远看不到该目录）
pub fn is_saf_tree_uri(root: &str) -> bool {
    if root.starts_with("content://tree/") {
        return true;
    }
    if let Some(rest) = root.strip_prefix("content://") {
        // content://<authority>/tree/<treeId>：路径段数 >= 3 且第 2 段为 "tree"
        let segments: Vec<&str> = rest.split('/').collect();
        segments.len() >= 3 && segments[1] == "tree"
    } else {
        false
    }
}

/// 树 URI → 根 document id（content://tree/<treeId> 取末段）
///
/// DocumentsContract.getTreeDocumentId(uri) 的等价解析；Kotlin listTreeChildren
/// 以 treeUri + documentId 定位目录，根 document id 是遍历起点
/// 树 URI → 根 document id（percent-decode 后）
///
/// DocumentsContract.getTreeDocumentId(uri) 的等价解析；Kotlin listTreeChildren
/// 以 treeUri + documentId 定位目录，根 document id 是遍历起点。URI 路径段是
/// 百分号编码形态（primary%3ADownload），而 Kotlin 返回的 SafEntry.document_id
/// 与 buildChildDocumentsUriUsingTree 均为解码形态（primary:Download），
/// 根 document id 必须同样解码，否则根级 list_tree 查不到任何条目。
pub fn tree_document_id(tree_uri: &str) -> Option<String> {
    let doc_id = tree_uri.rsplit('/').next().filter(|s| !s.is_empty())?;
    Some(percent_decode(doc_id))
}

/// 树 URI → 顶层别名（根 document id 剥 provider 前缀）
///
/// 别名用于：list 顶层条目名（桌面端看到的名字）+ 请求路径首段映射。
/// primary:Download → Download、0123-4567:DCIM → DCIM（主存储/SD 卡
/// 常见形态）；无冒号前缀的 provider（如自定义 document id）按原样。
pub fn tree_alias(tree_uri: &str) -> Option<String> {
    let decoded = tree_document_id(tree_uri)?;
    // 剥 "provider:" 前缀（primary:Download → Download）；冒号后为空时保原样
    Some(
        match decoded.split_once(':') {
            Some((_, rest)) if !rest.is_empty() => rest.to_string(),
            _ => decoded,
        },
    )
}

/// percent-decode（最小实现：%XX 与 + 保持字面；document id 仅含 %XX 转义）
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]);
            let lo = hex_val(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 相对路径 → 命中的 SAF 根（首段 == 树别名）
///
/// 返回 (tree_uri, 剥离别名后的剩余分量)；未命中返回 None（走真实路径解析）。
/// 多根同别名时取第一个（与真实路径根别名语义一致：优先保证导航可达）。
/// 别名可能含 '/'（如 primary%3ADCIM%2FCamera → DCIM/Camera），按整串前缀
/// 匹配（rel == alias 或 rel 以 "alias/" 开头），不以首段切分。
pub fn match_saf_root(saf_roots: &[String], rel: &str) -> Option<(String, Vec<String>)> {
    if rel.is_empty() {
        return None;
    }
    for root in saf_roots {
        let alias = tree_alias(root)?;
        let prefix = format!("{}/", alias);
        let rest = if rel == alias {
            ""
        } else if let Some(rest) = rel.strip_prefix(&prefix) {
            rest
        } else {
            continue;
        };
        let parts: Vec<String> = rest
            .split('/')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect();
        return Some((root.clone(), parts));
    }
    None
}

/// 沿分量列表逐层 list_tree 下降到目标条目（文件或目录）
///
/// parts 为空返回根目录条目（列表根时用）。每层在子条目中按名称精确匹配；
/// 中间分量必须是目录（文件无法继续下降），缺失/非目录返回 NotFound。
pub async fn walk_to_entry(
    saf: &dyn SafIo,
    tree_uri: &str,
    root_doc_id: &str,
    parts: &[String],
) -> crate::Result<SafEntry> {
    if parts.is_empty() {
        return Ok(SafEntry {
            name: tree_alias(tree_uri).unwrap_or_else(|| tree_uri.to_string()),
            is_dir: true,
            size: 0,
            mime: String::new(),
            uri: tree_uri.to_string(),
            document_id: root_doc_id.to_string(),
        });
    }

    let mut doc_id = root_doc_id.to_string();
    for (i, part) in parts.iter().enumerate() {
        let children = saf.list_tree(tree_uri, &doc_id).map_err(|e| {
            crate::AppError::Plugin(format!(
                "saf walk: list_tree({}, {}) failed (permission may be revoked): {}",
                tree_uri, doc_id, e
            ))
        })?;
        let child = children.into_iter().find(|c| &c.name == part).ok_or_else(|| {
            crate::AppError::NotFound(format!(
                "'{}' not found in SAF tree {}",
                part, tree_uri
            ))
        })?;
        if i == parts.len() - 1 {
            return Ok(child);
        }
        if !child.is_dir {
            return Err(crate::AppError::NotFound(format!(
                "'{}' is not a directory in SAF tree {}",
                child.name, tree_uri
            )));
        }
        doc_id = child.document_id;
    }
    unreachable!("loop covers all parts")
}

// ==================== 中转复制（Relay Copy） ====================

/// FNV-1a 64 位哈希（副本命名：源 URI 哈希，跨进程/重启确定）
///
/// 不用 DefaultHasher（实现细节可能随 Rust 版本变化，跨重启不一致会破坏
/// 续传命中的命名约定）；FNV-1a 手写稳定且无依赖
fn fnv1a64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 源 URI → 中转副本最终路径（relay_dir/{hash:016x}）
pub fn relay_cache_path(relay_dir: &Path, uri: &str) -> PathBuf {
    relay_dir.join(format!("{:016x}", fnv1a64(uri)))
}

/// 确保源 URI 的中转副本存在且完整，返回副本路径
///
/// - 最终名已存在（上次复制完成）：直接复用（续传命中，文件仍在）
/// - 否则启动新复制：`{hash}.part` 经 SafIo::read_to_cache（Kotlin staging）
///   完成后 rename 到中转目录最终名（原子落位标记完整性——最终名存在即完整，
///   崩溃残留的 `.part` 不会伪装成完整副本）
///
/// 副本生命周期（TTL 清理）由调用方在服务完成后经 [`arm_relay_cleanup`] 管理。
pub async fn ensure_relay_copy(
    saf: &dyn SafIo,
    relay_dir: &Path,
    uri: &str,
) -> crate::Result<PathBuf> {
    if let Err(e) = std::fs::create_dir_all(relay_dir) {
        return Err(crate::AppError::Internal(format!(
            "ensure_relay_copy: failed to create relay dir '{}': {}",
            relay_dir.display(),
            e
        )));
    }

    let final_path = relay_cache_path(relay_dir, uri);
    if final_path.is_file() {
        touch(&final_path);
        return Ok(final_path);
    }

    // 复制到 Kotlin staging（bedcode_uploads），完成后 rename 到中转目录；
    // dest_name 传 `{hash}.part`——并发同名复制在 Kotlin 侧自动加 -N 后缀，
    // 两个完整副本内容一致，rename 到同一最终名无碍
    let part_name = format!(
        "{}.part",
        final_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| final_path.display().to_string())
    );
    let handle = saf.read_to_cache(uri, &part_name).map_err(|e| {
        crate::AppError::Plugin(format!("ensure_relay_copy: read_to_cache({}) failed: {}", uri, e))
    })?;

    // 轮询复制终态（顺序流不可续，等待完成；取消/失败即整体失败）
    loop {
        let status = saf.copy_status(&handle.copy_id).map_err(|e| {
            crate::AppError::Plugin(format!(
                "ensure_relay_copy: copy_status({}) failed: {}",
                handle.copy_id, e
            ))
        })?;
        if status.finished {
            if status.cancelled {
                return Err(crate::AppError::Plugin(format!(
                    "ensure_relay_copy: relay copy cancelled by user ({})",
                    uri
                )));
            }
            if let Some(err) = status.error {
                return Err(crate::AppError::Plugin(format!(
                    "ensure_relay_copy: relay copy failed for {}: {}",
                    uri, err
                )));
            }
            break;
        }
        tokio::time::sleep(COPY_POLL_INTERVAL).await;
    }

    // 原子落位：staging 完成副本 → 中转目录最终名（同 cache 卷，rename 跨
    // 子目录原子；失败说明副本被 Kotlin 清扫竞态删除，整体报错由调用方重试）
    std::fs::rename(&handle.dest_path, &final_path).map_err(|e| {
        crate::AppError::Internal(format!(
            "ensure_relay_copy: rename '{}' -> '{}' failed (stale relay copy?): {}",
            handle.dest_path,
            final_path.display(),
            e
        ))
    })?;
    touch(&final_path);
    Ok(final_path)
}

/// 记录副本访问（TTL 滑动续期；幂等，不 spawn 清理任务）
fn touch(path: &Path) {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    if let Ok(mut st) = RELAY_STATE.lock() {
        st.last_access.insert(name, Instant::now());
    }
}
/// 副本 TTL 清理（服务完成/超时后清理；续传窗口内文件仍在）
///
/// 同哈希已有清理任务时不重复 spawn（滑动窗口由任务自身检查 last_access
/// 决定是否续期）；任务到期后删除该哈希的全部副本（最终名 + `.part` 变体）。
pub fn arm_relay_cleanup(relay_dir: &Path, path: &Path) {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    {
        let Ok(mut st) = RELAY_STATE.lock() else {
            return;
        };
        st.last_access.insert(name.clone(), Instant::now());
        if st.pending_cleanup.contains(&name) {
            return;
        }
        st.pending_cleanup.insert(name.clone());
    }

    let relay_dir = relay_dir.to_path_buf();
    crate::system::error_boundary::spawn_with_error_boundary("saf_relay_cleanup", async move {
        loop {
            tokio::time::sleep(RELAY_CACHE_TTL).await;
            let fresh = RELAY_STATE
                .lock()
                .map(|st| {
                    st.last_access
                        .get(&name)
                        .map(|t| t.elapsed() < RELAY_CACHE_TTL)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if fresh {
                continue;
            }
            let mut removed = 0;
            if let Ok(rd) = std::fs::read_dir(&relay_dir) {
                for entry in rd.flatten() {
                    if entry.file_name().to_string_lossy().starts_with(&name) {
                        match std::fs::remove_file(entry.path()) {
                            Ok(()) => removed += 1,
                            Err(e) => {
                                tracing::warn!(
                                    hash = %name,
                                    file = %entry.path().display(),
                                    "saf relay cleanup: delete failed: {}",
                                    e
                                );
                            }
                        }
                    }
                }
            }
            if let Ok(mut st) = RELAY_STATE.lock() {
                st.last_access.remove(&name);
                st.pending_cleanup.remove(&name);
            }
            tracing::info!(hash = %name, removed, "saf relay cache cleaned after TTL");
            break;
        }
    });
}

/// 启动扫描清理：删除中转目录全部副本（服务启动时调用）
///
/// 副本均为可重新生成的临时数据；进程崩溃残留（含未落位 `.part`）在此
/// 统一清除。同时重置共享状态（上个服务周期的清理任务已随进程结束，
/// pending 标记必须清除，否则同哈希新副本将无法 spawn 清理任务）。
/// 返回删除数量。
pub fn sweep_relay_dir(relay_dir: &Path) -> usize {
    if let Ok(mut st) = RELAY_STATE.lock() {
        st.last_access.clear();
        st.pending_cleanup.clear();
    }
    let Ok(rd) = std::fs::read_dir(relay_dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_file() {
            match std::fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(e) => {
                    tracing::warn!(
                        file = %path.display(),
                        "saf relay sweep: delete failed: {}",
                        e
                    );
                }
            }
        }
    }
    if removed > 0 {
        tracing::info!(removed, dir = %relay_dir.display(), "saf relay dir swept at server start");
    }
    removed
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_uri_parsing() {
        assert!(is_saf_tree_uri("content://tree/primary%3ADownload"));
        assert!(!is_saf_tree_uri("/storage/emulated/0/Download"));
        assert!(!is_saf_tree_uri("content://com.android.externalstorage.documents"));
        // 完整形态（系统目录树选择器返回）：同样识别，treeId 为末段
        assert!(is_saf_tree_uri(
            "content://com.android.externalstorage.documents/tree/primary%3ADownload"
        ));
        assert!(is_saf_tree_uri(
            "content://com.android.externalstorage.documents/tree/primary%3A%E4%B8%8B%E8%BD%BD"
        ));
        assert!(!is_saf_tree_uri(
            "content://com.android.externalstorage.documents/document/primary%3ADownload"
        ));
        assert!(!is_saf_tree_uri(
            "content://com.android.externalstorage.documents/tree"
        ));
        // 根 document id / 别名解析对完整形态同样生效
        assert_eq!(
            tree_document_id(
                "content://com.android.externalstorage.documents/tree/primary%3A%E4%B8%8B%E8%BD%BD"
            )
            .as_deref(),
            Some("primary:下载")
        );
        assert_eq!(
            tree_alias(
                "content://com.android.externalstorage.documents/tree/primary%3A%E4%B8%8B%E8%BD%BD"
            )
            .as_deref(),
            Some("下载")
        );
        // 根 document id = 解码形态（与 Kotlin getTreeDocumentId / SafEntry.document_id 一致）
        assert_eq!(
            tree_document_id("content://tree/primary%3ADownload").as_deref(),
            Some("primary:Download")
        );
        assert_eq!(
            tree_document_id("content://tree/0123-4567%3ADCIM%2FCamera").as_deref(),
            Some("0123-4567:DCIM/Camera")
        );
        // 别名 = 根 document id 剥 provider 前缀（primary:Download → Download）
        assert_eq!(tree_alias("content://tree/primary%3ADownload").as_deref(), Some("Download"));
        assert_eq!(
            tree_alias("content://tree/primary%3ADCIM%2FCamera").as_deref(),
            Some("DCIM/Camera")
        );
        assert_eq!(tree_alias("content://tree/0123-4567%3ADownload").as_deref(), Some("Download"));
        // 无冒号前缀的 provider：解码原样
        assert_eq!(tree_alias("content://tree/root").as_deref(), Some("root"));
    }

    #[test]
    fn match_saf_root_hits_alias_and_strips_prefix() {
        let roots = vec!["content://tree/primary%3ADownload".to_string()];
        let hit = match_saf_root(&roots, "Download/sub/file.txt");
        assert_eq!(hit.as_ref().unwrap().0, "content://tree/primary%3ADownload");
        assert_eq!(hit.unwrap().1, vec!["sub".to_string(), "file.txt".to_string()]);

        // 根目录本身
        let hit = match_saf_root(&roots, "Download");
        assert!(hit.unwrap().1.is_empty());

        // 未命中
        assert!(match_saf_root(&roots, "Photo/a.jpg").is_none());
        assert!(match_saf_root(&roots, "").is_none());
    }

    #[test]
    fn match_saf_root_handles_alias_with_slash() {
        // 别名含 '/'（%2F 转义）时按整串前缀匹配（DCIM/Camera/xxx）
        let roots = vec!["content://tree/primary%3ADCIM%2FCamera".to_string()];
        let hit = match_saf_root(&roots, "DCIM/Camera/2026/IMG_1.jpg");
        assert_eq!(
            hit.unwrap().1,
            vec!["2026".to_string(), "IMG_1.jpg".to_string()]
        );
        assert!(match_saf_root(&roots, "DCIM/Other").is_none());
    }

    #[test]
    fn relay_cache_path_is_stable_and_unique() {
        let dir = Path::new("/cache/bedcode_downloads");
        let p1 = relay_cache_path(dir, "content://tree/a/document/f1");
        let p2 = relay_cache_path(dir, "content://tree/a/document/f1");
        let p3 = relay_cache_path(dir, "content://tree/a/document/f2");
        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
        assert!(p1.starts_with(dir));
        // 命名含源 URI 哈希：16 位 hex
        let name = p1.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name.len(), 16);
        assert!(name.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("primary%3ADownload"), "primary:Download");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("a%2Fb%zz"), "a/b%zz");
    }
}
