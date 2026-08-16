//! 移动端文件服务 HTTP server 契约测试（L1）
//!
//! 移动端唯一的"服务端"：actix-web 独立端口 + Bearer token 鉴权（src/file_service/）。
//! 与桌面端 L1 的 HTTP 契约测试对称——真实启动服务（bind :0）+ reqwest 真实客户端，
//! 验证：鉴权守卫 → 路由注册 → 沙箱解析 → 下载/Range → 上传 fail-closed → 生命周期。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bedcode_lib::file_service::cipher::PassthroughCipher;
use bedcode_lib::file_service::registry::{FileServiceRegistry, HookTarget, MountEntry};
use bedcode_lib::file_service::server::FileServiceServer;
use bedcode_plugin_api_mobile::FileOperation;
use tempfile::TempDir;

const PLUGIN_ID: &str = "test-plugin";
const MOUNT: &str = "files";

/// 启动服务 + 注入挂载，返回 (server, registry, token, base_url)
async fn start_service(root: PathBuf) -> (Arc<FileServiceServer>, Arc<FileServiceRegistry>, String, String) {
    // insert_entry_for_test 绕过 normalize_roots（挂载校验），这里补 canonicalize
    // 模拟生产 mount 语义：Windows 上 canonicalize 返回 \\?\ 前缀路径，沙箱
    // starts_with 前缀比较依赖两端一致
    let root = root.canonicalize().expect("root must exist");
    let registry = FileServiceRegistry::new();
    registry
        .insert_entry_for_test(MountEntry {
            plugin_id: PLUGIN_ID.into(),
            mount_path: MOUNT.into(),
            roots: vec![root],
            saf_roots: vec![],
            operations: vec![
                FileOperation::List,
                FileOperation::Download,
                FileOperation::Upload,
            ],
            hook: HookTarget::None,
            cipher: Arc::new(PassthroughCipher),
        })
        .await;

    let server = Arc::new(FileServiceServer::new(registry.clone()));
    let port = server.ensure_started().await.expect("server should start");
    let token = server
        .token_guard()
        .current_for_announce()
        .expect("token should be generated on start");
    (
        server,
        registry,
        token,
        format!("http://127.0.0.1:{}", port),
    )
}

/// 构造带 token 的请求客户端
fn authed(token: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .default_headers(
            std::iter::once((
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
            ))
            .collect(),
        )
        .build()
        .unwrap()
}

// ==================== 场景 1：鉴权守卫 ====================

#[tokio::test]
async fn auth_guard_401_without_or_wrong_token() {
    let tmp = TempDir::new().unwrap();
    let (server, _registry, token, base) = start_service(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();
    let list_url = format!("{}/{}/{}/list", base, PLUGIN_ID, MOUNT);

    // 无 token → 401 JSON
    let resp = client.get(&list_url).send().await.unwrap();
    assert_eq!(resp.status(), 401, "无 token 应 401");
    let body = resp.text().await.unwrap();
    assert!(body.contains("unauthorized"), "401 应说明原因: {}", body);

    // 错误 token → 401
    let resp = client
        .get(&list_url)
        .header(reqwest::header::AUTHORIZATION, "Bearer wrong-token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "错误 token 应 401");

    // 正确 token → 放行（200，挂载根存在）
    let resp = authed(&token).get(&list_url).send().await.unwrap();
    assert_eq!(resp.status(), 200, "正确 token 应放行");

    server.stop().await;
}

// ==================== 场景 2：目录列举 ====================

#[tokio::test]
async fn list_root_and_subdir() {
    let tmp = TempDir::new().unwrap();
    // 挂载根别名语义：list path="" 返回每个 root 的 file_name 作为顶层条目，
    // 文件需放在 root 子目录内，请求路径带别名段（如 path=root/sub）
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.txt"), "aaa").unwrap();
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/b.txt"), "bbb").unwrap();

    let (_server, _registry, token, base) = start_service(root.clone()).await;
    let client = authed(&token);
    let list_url = format!("{}/{}/{}/list", base, PLUGIN_ID, MOUNT);

    // 根列表：root 别名作为顶层条目
    let resp = client.get(&list_url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value =
        serde_json::from_str(&resp.text().await.unwrap()).expect("valid JSON");
    assert_eq!(json["path"], "");
    let entries = json["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 1, "根列表应只含 root 别名条目");
    assert_eq!(entries[0]["name"], "root");
    assert_eq!(entries[0]["isDir"], true);

    // 别名段解析 → root 内容
    let resp = client
        .get(&list_url)
        .query(&[("path", "root")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value =
        serde_json::from_str(&resp.text().await.unwrap()).expect("valid JSON");
    let entries = json["entries"].as_array().unwrap();
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["sub", "a.txt"], "条目目录优先、按名称排序");
    let sub = entries.iter().find(|e| e["name"] == "sub").unwrap();
    assert_eq!(sub["isDir"], true, "目录 isDir=true");
    assert_eq!(sub["size"], 0);

    // 子目录列表
    let resp = client
        .get(&list_url)
        .query(&[("path", "root/sub")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value =
        serde_json::from_str(&resp.text().await.unwrap()).expect("valid JSON");
    let entries = json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "b.txt");
    assert_eq!(entries[0]["size"], 3);

    // 不存在的路径 → 404
    let resp = client
        .get(&list_url)
        .query(&[("path", "root/nope")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "不存在的目录应 404");
}

// ==================== 场景 3：路径穿越拒绝 ====================

#[tokio::test]
async fn list_path_traversal_rejected() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
    let (_server, _registry, token, base) = start_service(tmp.path().to_path_buf()).await;
    let client = authed(&token);
    let list_url = format!("{}/{}/{}/list", base, PLUGIN_ID, MOUNT);

    // 相对路径越界 → 404（沙箱解析拒绝）
    for bad in ["../", "..\\", "../../etc", "sub/../.."] {
        let resp = client
            .get(&list_url)
            .query(&[("path", bad)])
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            404,
            "路径 '{}' 应被沙箱拒绝（404）",
            bad
        );
    }
}

// ==================== 场景 4：下载 + Range 续传 ====================

#[tokio::test]
async fn download_full_and_range() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let content = b"0123456789abcdef";
    std::fs::write(root.join("data.bin"), content).unwrap();

    let (_server, registry, token, base) = start_service(root.clone()).await;
    let client = authed(&token);
    let file_url = format!("{}/{}/{}/file", base, PLUGIN_ID, MOUNT);

    // 全量下载 → 200 + 内容一致
    let resp = client
        .get(&file_url)
        .query(&[("path", "root/data.bin")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), content);

    // Range 续传 → 206 + Content-Range + 段内容
    let resp = client
        .get(&file_url)
        .query(&[("path", "root/data.bin")])
        .header(reqwest::header::RANGE, "bytes=4-7")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206, "Range 命中应 206");
    let cr = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .expect("Content-Range header");
    assert_eq!(cr, format!("bytes 4-7/{}", content.len()), "Content-Range 形状");
    assert_eq!(
        resp.bytes().await.unwrap().as_ref(),
        &content[4..8],
        "段内容应一致"
    );

    // 尾部开放 Range → 206
    let resp = client
        .get(&file_url)
        .query(&[("path", "root/data.bin")])
        .header(reqwest::header::RANGE, "bytes=10-")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &content[10..]);

    // 不存在的文件 → 404
    let resp = client
        .get(&file_url)
        .query(&[("path", "root/missing.bin")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ==================== 场景 5：HEAD 元数据 ====================

#[tokio::test]
async fn head_returns_metadata() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello").unwrap();
    let (_server, _registry, token, base) = start_service(root).await;
    let client = authed(&token);
    let file_url = format!("{}/{}/{}/file", base, PLUGIN_ID, MOUNT);

    let resp = client
        .head(&file_url)
        .query(&[("path", "root/a.txt")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    // size/mtime 指纹经自定义头承载（actix 对 HEAD 自动将 Content-Length 置 0）
    let size = resp
        .headers()
        .get("X-File-Size")
        .and_then(|v| v.to_str().ok())
        .expect("X-File-Size");
    assert_eq!(size, "5", "X-File-Size 应返回文件大小");
    assert!(
        resp.headers().contains_key("X-File-Mtime"),
        "X-File-Mtime 应存在"
    );
}

// ==================== 场景 6：上传 fail-closed ====================

#[tokio::test]
async fn upload_fail_closed_without_hook() {
    let tmp = TempDir::new().unwrap();
    let (server, registry, token, base) = start_service(tmp.path().to_path_buf()).await;
    // downloads_dir 必须真实存在（沙箱解析校验父目录），且挂载无钩子
    // （HookTarget::None）→ fail-closed 拒绝
    let downloads = tmp.path().join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    // 与 start_service 同理：downloads_dir 也需 canonicalize（resolve 内部前缀比较）
    registry
        .set_downloads_dir(downloads.canonicalize().unwrap())
        .await;

    let client = authed(&token);
    let upload_url = format!("{}/{}/{}/upload", base, PLUGIN_ID, MOUNT);
    let resp = client
        .post(&upload_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(r#"{"relativePath":"incoming.bin","size":10}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        403,
        "无上传钩子的挂载应 fail-closed 拒绝（403）"
    );
    let body = resp.text().await.unwrap();
    assert!(body.contains("403") || body.contains("hook"), "拒绝应说明原因: {}", body);

    server.stop().await;
}

#[tokio::test]
async fn upload_without_downloads_dir_500() {
    let tmp = TempDir::new().unwrap();
    let (server, _registry, token, base) = start_service(tmp.path().to_path_buf()).await;

    let client = authed(&token);
    let upload_url = format!("{}/{}/{}/upload", base, PLUGIN_ID, MOUNT);
    let resp = client
        .post(&upload_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(r#"{"relativePath":"incoming.bin","size":10}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        500,
        "downloads_dir 未配置时应 500（沙箱目标不可用）"
    );

    server.stop().await;
}

// ==================== 场景 7：生命周期 ====================

#[tokio::test]
async fn stop_revokes_token_and_frees_port() {
    let tmp = TempDir::new().unwrap();
    let (server, _registry, _token, base) = start_service(tmp.path().to_path_buf()).await;
    let port: u16 = base.rsplit(':').next().unwrap().parse().unwrap();

    // 停止：服务关闭 + token 吊销（幂等）
    server.stop().await;
    server.stop().await;
    assert!(!server.is_running().await, "stop 后 is_running=false");
    assert!(
        !server.token_guard().is_active(),
        "stop 后 token 应吊销"
    );

    // 端口释放：连接应失败（Windows 上 SYN 重传可能延迟，轮询宽容）
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let mut refused = false;
    while std::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            refused = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(refused, "stop 后端口应释放（连接拒绝）");

    // 幂等重启：再次 ensure_started 生成新 token 并可服务
    let new_port = server.ensure_started().await.expect("restart should work");
    assert!(new_port != port || new_port > 0);
    assert!(server.token_guard().is_active(), "重启后应生成新 token");
    server.stop().await;
}
