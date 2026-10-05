//! 出站原语的闸门用例组：声明门 → 出站授权门 → 执行（迁移前同属一个内联 `mod tests`）

use serde_json::json;

use super::scaffold::*;
use crate::plugin_binding::egress::{execute_http_request, http_fetch};

/// 正常小响应体：完整返回，不受上限影响
#[tokio::test]
async fn http_fetch_small_response_ok() {
    disable_proxy_for_loopback();
    let addr = spawn_mock_server(b"{\"ok\":true}".to_vec()).await;
    let resp = execute_http_request(&json!({
        "method": "GET",
        "url": format!("http://{}/small", addr),
    }))
    .await
    .expect("small response must succeed");
    assert_eq!(resp["status"], 200);
    assert_eq!(resp["body"], "{\"ok\":true}");
}

/// 未声明 network:http 的插件调用 fetch：宿主侧直接拒绝（Rust 端最终仲裁）。
/// 错误消息含明确原因，与请求类错误可区分。
#[test]
fn http_fetch_permission_denied_rejected() {
    let ports = denying_permission();
    let err = http_fetch(&as_ports(&ports), "p1", r#"{"url":"http://127.0.0.1:1/x"}"#, true)
        .expect_err("unpermissioned fetch must be rejected");
    assert!(
        err.contains("permission denied") && err.contains("network:http"),
        "error should state permission reason, got: {}",
        err
    );
}

/// 声明 network:http 后放行：请求进入执行阶段（此处以缺 url 的请求 JSON 验证
/// 错误从「权限拒绝」变为「请求错误」，证明权限检查通过且未碰网络）。
///
/// **变异判据**：把声明门挪到授权门之后 ⇒ 未授权插件的拒绝原因会变成授权拒绝，
/// 本条转红。
#[test]
fn http_fetch_permission_granted_passes() {
    let ports = granting();
    let err =
        http_fetch(&as_ports(&ports), "p1", r#"{"method":"GET"}"#, true).expect_err("missing url is a request error");
    assert!(
        err.contains("Missing 'url'") || err.contains("http error"),
        "after permission, error should be request-level, got: {}",
        err
    );
    assert!(
        !err.contains("permission denied"),
        "permissioned plugin should not hit permission denial, got: {}",
        err
    );
}

/// 非法请求 JSON：权限放行后仍是解析错误（错误分类保持：权限拒绝 ≠ 请求错误）
#[test]
fn http_fetch_invalid_json_is_request_error_not_permission() {
    let ports = granting();
    let err = http_fetch(&as_ports(&ports), "p1", "not-json", true).expect_err("invalid JSON is a request error");
    assert!(
        err.contains("invalid request JSON"),
        "error should be parse-level, got: {}",
        err
    );
}

/// 超限响应体：立即拒绝并报错引导 stream:true，绝不把大载荷交给 guest
/// （保证 guest 侧 serde 解析工作量有界 → 不可能耗尽 fuel 预算被 trap）
#[tokio::test]
async fn http_fetch_oversized_response_rejected() {
    disable_proxy_for_loopback();
    let addr = spawn_mock_server(vec![
        0u8;
        bedcode_server_base::constants::PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES
            + 1
    ])
    .await;
    let err = execute_http_request(&json!({
        "method": "GET",
        "url": format!("http://{}/big", addr),
    }))
    .await
    .expect_err("oversized response must be rejected");
    assert!(
        err.to_string().contains("exceeds"),
        "error should mention size limit, got: {}",
        err
    );
    assert!(
        err.to_string().contains("stream:true"),
        "error should guide to streaming mode, got: {}",
        err
    );
}

/// 未记录的目标在**触达网络之前**被拒（变异判据：把授权检查挪到执行之后、或只门
/// 非流式分支 ⇒ 夹具服务器会收到连接，命中数从 0 变 1）。
#[tokio::test]
async fn unrecorded_origin_is_denied_before_any_network_io() {
    let ports = granting();
    let (addr, hits) = spawn_counting_server(b"{\"ok\":true}".to_vec()).await;

    let err = http_fetch(
        &as_ports(&ports),
        "p1",
        &json!({"method": "GET", "url": format!("http://{}/x", addr)}).to_string(),
        true,
    )
    .expect_err("未记录的目标必须被拒");

    assert!(
        err.contains("network authorization denied"),
        "错误须是授权拒绝而非请求错误: {err}"
    );
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "被拒的请求不得触达网络（授权检查必须在执行之前）"
    );
}

/// 正例：授权命中的 origin 真正放行（请求到达对端并拿到响应）
///
/// **必须 multi_thread**：出站原语是同步函数，内部经端口的桥阻塞调用线程；
/// 单线程测试运行时下夹具服务的 accept 任务被一同卡死（表现为连接超时）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn allow_record_releases_the_request() {
    let (addr, hits) = spawn_counting_server(b"{\"ok\":true}".to_vec()).await;
    let ports = granting().with_auth(AuthScript::AllowOrigins {
        allowed: vec![normalize_origin(&format!("http://{addr}"))],
        reason: "no-record",
    });

    let response = http_fetch(
        &as_ports(&ports),
        "p1",
        &json!({"method": "GET", "url": format!("http://{}/x", addr)}).to_string(),
        true,
    )
    .expect("记录命中的 origin 必须放行")
    .expect("fetch returns payload");
    // 返回值是响应对象的 JSON 文本（body 字段仍是被转义的原文）
    let parsed: serde_json::Value = serde_json::from_str(&response).expect("响应是合法 JSON");
    assert_eq!(parsed["status"], 200);
    assert_eq!(parsed["body"], "{\"ok\":true}", "响应体应原样返回");
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "放行的请求应真实到达对端"
    );
}

/// 凭据红线（AGENTS §8）：拒绝错误串带 origin，**不带 path / query 里的 token**
#[tokio::test]
async fn denial_error_carries_origin_but_never_query() {
    let ports = granting();
    let err = http_fetch(
        &as_ports(&ports),
        "p1",
        r#"{"method":"GET","url":"https://api.example.com/v1?access_token=SECRET_TOKEN"}"#,
        true,
    )
    .expect_err("未记录 origin 必须被拒");

    assert!(
        err.contains("https://api.example.com:443"),
        "错误须点明被拒的 origin: {err}"
    );
    assert!(!err.contains("SECRET_TOKEN"), "token 不得进错误串: {err}");
    assert!(!err.contains("/v1"), "path 不得进错误串: {err}");
}

/// 任务单元（池线程）路径：may_prompt=false ⇒ 只能靠记录，未记录即拒
///
/// 变异判据：把两个路径写反（或池线程也去弹窗）时 reason 会变，本条转红。
#[tokio::test]
async fn task_unit_path_denies_unrecorded_origin_with_no_record_reason() {
    let ports = granting();
    let (addr, hits) = spawn_counting_server(b"{}".to_vec()).await;

    let err = http_fetch(
        &as_ports(&ports),
        "p1",
        &json!({"method": "GET", "url": format!("http://{}/x", addr)}).to_string(),
        false,
    )
    .expect_err("池线程不得弹窗，未记录目标必须被拒");

    assert!(
        err.contains("no-record"),
        "池线程拒绝原因应为 no-record（而非询问侧的 user-denied）: {err}"
    );
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "被拒的请求不得触达网络"
    );
}

/// 流式分支同样在授权之后：未记录目标不得拿到 streamId（变异：只门非流式）
#[tokio::test]
async fn streaming_mode_is_gated_too() {
    let ports = granting();
    let err = http_fetch(
        &as_ports(&ports),
        "p1",
        r#"{"method":"POST","url":"https://api.example.com/v1/chat","stream":true,"streamEvent":"x:y"}"#,
        true,
    )
    .expect_err("流式请求同样要过授权门");
    assert!(
        err.contains("network authorization denied"),
        "流式分支必须被同一道门拦住: {err}"
    );
}

/// 授权检查自身失败：错误分类与「拒绝」不同（不得被调用方当成策略拒绝吞掉）
#[tokio::test]
async fn authorization_check_failure_is_its_own_error_class() {
    let ports = granting().with_auth(AuthScript::CheckFailed("store unavailable".to_string()));
    let err = http_fetch(
        &as_ports(&ports),
        "p1",
        r#"{"method":"GET","url":"https://api.example.com/v1"}"#,
        true,
    )
    .expect_err("检查失败必须报错");
    assert!(
        err.contains("network authorization check failed") && err.contains("store unavailable"),
        "错误须点明检查失败而非策略拒绝: {err}"
    );
}

/// 缺 `url` 的请求不在授权门拦（归执行层的错误分类，保持不变）：授权脚本即使
/// 一律放行，错误仍应是 `Missing 'url'` 而不是授权相关文案。
#[tokio::test]
async fn missing_url_is_execution_error_not_authorization_error() {
    let ports = granting().with_auth(AuthScript::Deny("no-record"));
    let err = http_fetch(&as_ports(&ports), "p1", r#"{"method":"GET"}"#, true).expect_err("缺 url 是请求错误");
    assert!(err.contains("Missing 'url'"), "错误须归执行层: {err}");
    assert!(!err.contains("authorization"), "缺 url 不该被归类成授权错误: {err}");
}
