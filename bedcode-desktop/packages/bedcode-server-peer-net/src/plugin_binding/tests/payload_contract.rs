//! 载荷契约与 fail-visible（票 05 新增组：随实现迁入而把「不触引擎即可判定」的
//! 契约钉在本域内）
//!
//! 这一组的用例都刻意停在**引擎之前**：句柄寻址 / 属主判定 / JSON 载荷校验 / 退役
//! 字段检测都在域内完成，既不依赖 `peer_ctx` 也不依赖真实节点，是「机制自持」的
//! 直接证据。

use super::scaffold::{drop_handle, mint, FakePorts};
use super::*;

fn granted() -> Arc<dyn PeerPorts> {
    FakePorts::with(&[("com.bedcode.owner-a", PERMISSION_PEER)])
}

/// v31 fail-visible：`concurrency` 并发脉冲字段已退役——serde 默认忽略未知字段，
/// 必须显性检测并点名重建，否则旧产物静默超并发
#[test]
fn send_files_rejects_retired_concurrency_field() {
    let ports = granted();
    let h = mint("payload-session");
    let err = super::peer_send_files(
        &ports,
        "com.bedcode.owner-a",
        &h,
        r#"[{"path":"/tmp/a.bin","concurrency":4}]"#,
    )
    .expect_err("退役字段必须显性报错");
    assert!(err.contains("'concurrency' retired in ABI v31"), "got: {err}");
    assert!(err.contains("rebuild plugin artifact with current SDK"), "got: {err}");
    drop_handle(&h);
}

/// 双形态载荷兼容：纯 string 与 `{ path, encrypt? }` 都过 JSON 校验这一关
/// （真正发文件要触引擎，这里只锁「校验通过后进入自动重拨/引擎分支」）
#[test]
fn send_files_accepts_both_path_shapes_past_validation() {
    let ports = granted();
    let h = mint("shapes-session");
    for payload in [r#"["/tmp/a.bin"]"#, r#"[{"path":"/tmp/a.bin","encrypt":true}]"#] {
        let err = super::peer_send_files(&ports, "com.bedcode.owner-a", &h, payload).expect_err("无头上下文取不到引擎");
        assert!(
            !err.contains("invalid paths json"),
            "载荷形态 {payload} 应过校验: {err}"
        );
        assert_eq!(err, ports::HEADLESS_UNAVAILABLE);
    }
    drop_handle(&h);
}

/// 非 JSON 载荷的错误串逐字保留（插件据此区分「载荷错」与「引擎错」）
#[test]
fn send_files_reports_invalid_json_verbatim() {
    let ports = granted();
    let h = mint("badjson-session");
    let err = super::peer_send_files(&ports, "com.bedcode.owner-a", &h, "not-json").expect_err("非法 JSON 必须报错");
    assert!(err.starts_with("send files: invalid paths json:"), "got: {err}");
    drop_handle(&h);
}

/// 句柄寻址失败：不透明入参不得透传为 node-id（Phase 4 收紧）
#[test]
fn data_plane_rejects_unknown_handle_verbatim() {
    let ports = granted();
    let err = super::peer_browse_directory(&ports, "com.bedcode.owner-a", "not-a-handle", "dir-1", "")
        .expect_err("未知句柄必须报错");
    assert_eq!(err, "invalid session handle: not-a-handle");
}

/// 属主判定先于载荷解析：非属主拿到的必须是属主拒绝（不是载荷错、也不是 headless）
#[test]
fn data_plane_checks_owner_before_payload() {
    let ports = FakePorts::with(&[("com.bedcode.intruder-b", PERMISSION_PEER)]);
    let h = mint("owner-scoped-session");
    let err =
        super::peer_pull_files(&ports, "com.bedcode.intruder-b", &h, "dir-1", "not-json").expect_err("非属主必须被拒");
    assert_eq!(err, format!("{NOT_OWNER}: {h}"), "got: {err}");
    drop_handle(&h);
}

/// 属主路径上载荷解析错误仍逐字保留（在句柄解析之后、引擎之前）
#[test]
fn pull_files_reports_invalid_files_json_verbatim() {
    let ports = granted();
    let h = mint("pull-session");
    let err = super::peer_pull_files(&ports, "com.bedcode.owner-a", &h, "dir-1", "not-json")
        .expect_err("非法 files json 必须报错");
    assert!(err.starts_with("pull files: invalid files json:"), "got: {err}");
    drop_handle(&h);
}

/// `collect-outgoing` 是唯一**不取引擎上下文**的原语（纯文件系统枚举，无头亦可用）：
/// 该差异是 wire 契约的一部分（见 WIT 注释），用例会证明它不因无头而失败
#[test]
fn collect_outgoing_works_without_engine_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = dir.path().join("docs");
    std::fs::create_dir_all(&folder).expect("mkdir");
    std::fs::write(folder.join("a.txt"), vec![0u8; 4]).expect("write a");
    std::fs::write(folder.join("b.txt"), vec![0u8; 2]).expect("write b");

    let ports = granted();
    let json = super::peer_collect_outgoing(
        &ports,
        "com.bedcode.owner-a",
        &format!(r#"["{}"]"#, folder.to_string_lossy()),
    )
    .expect("collect outgoing 不依赖引擎上下文");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid json array");
    assert_eq!(rows.len(), 2, "目录递归后两行: {json}");
    assert_eq!(rows[0]["path"], "docs/a.txt");
    assert_eq!(rows[0]["size"], 4);
    assert_eq!(rows[1]["path"], "docs/b.txt");
    assert_eq!(rows[1]["size"], 2);
}

/// 同上，权限门仍然生效（无头可用 ≠ 免权限）
#[test]
fn collect_outgoing_still_requires_permission() {
    let ports = FakePorts::with(&[]);
    assert_eq!(
        super::peer_collect_outgoing(&ports, "com.bedcode.no-peer", "[]").unwrap_err(),
        denied()
    );
}
