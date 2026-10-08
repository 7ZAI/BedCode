//! 票 08 · 断点续传（redial）场景链 + 终态历史持久化往返
//!
//! 本组是**插件侧集成口径**：把「引擎事件 → store 归约 → 重启恢复 → 重试回放」
//! 串成一条真实时序，断言任务真源（store）在整条链路上始终自洽——尤其是
//! 节点下线后的三段语义：① 在飞批如实标 `interrupted`（不假装还在跑）
//! ② 陈旧 session 句柄摘除但endpoint memo 保留（重拨寻址依据）③ 回放
//! 换批不复制历史行（同一条逻辑传输一条历史）。
//!
//! 引擎交互面（`peer-send-files` / `peer-pull-files`）需真WASM 宿主才能驱动，
//! 留在票 20 真机互连验收；此处覆盖的是「判据 + 归约 + 持久化」纯函数链。

use super::MockHost;
use crate::device_bridge;
use crate::peer::{
    history_view, load_entries, next_local_failure_id, persist_entries, restore_entries,
    running_send_count, send_row, TransferEntry,
};
use crate::settings_store;
use crate::transfer_store::{self, RetryMeta};

/// 造一条本端发起的 send 行（与 `peer::send_row` 同形状，便于链路易读）
fn initiated_send(id: &str, node_id: &str) -> TransferEntry {
    send_row(
        node_id,
        id,
        "running",
        RetryMeta::Send {
            paths: vec!["/sdcard/report.pdf".into()],
        },
        None,
    )
}

fn engine_progress(batch: &str, transferred: u64, total: u64, ts: u64) -> serde_json::Value {
    serde_json::json!({
        "kind": "progress", "batchId": batch,
        "transferred": transferred, "total": total, "rateBps": 1024.0, "tsMs": ts,
    })
}

fn engine_terminal(batch: &str, state: serde_json::Value, ts: u64) -> serde_json::Value {
    serde_json::json!({ "kind": "terminal", "batchId": batch, "state": state, "tsMs": ts })
}

// ==================== redial 场景链 ====================

/// 完整时序：发起 → 进度 → 节点下线（标中断 + 槽位空出）→ 重试回放（换批）
/// → 新一轮进度与完成。全程**一条历史行**，batchId 由旧换新。
#[test]
fn redial_scenario_keeps_single_history_row_across_replay() {
    let mut store = vec![initiated_send("batch-1", "aa")];

    // 首轮数据面推进
    assert!(transfer_store::reduce_event(
        &mut store,
        "send",
        &engine_progress("batch-1", 400, 1000, 10),
        ""
    ));
    assert_eq!(store[0].transferred_bytes, 400);
    assert_eq!(store[0].total_bytes, 1000);
    // 发起方向占着一个发送槽位（并发上限 1 时占满）
    assert_eq!(running_send_count(&store), 1);
    assert!(!transfer_store::send_slot_open(1, 1));
    assert!(transfer_store::send_slot_open(
        1,
        settings_store::DEFAULT_CONCURRENCY
    ));

    // 节点下线：引擎通道关闭 → 在飞批如实标 interrupted，槽位立即空出
    assert_eq!(transfer_store::mark_active_interrupted(&mut store), 1);
    assert_eq!(store[0].status, "interrupted");
    assert_eq!(store[0].rate_bps, 0.0);
    assert_eq!(running_send_count(&store), 0, "中断批不再占槽");
    assert!(transfer_store::send_slot_open(
        0,
        settings_store::DEFAULT_CONCURRENCY
    ));

    // 重试回放：判据命中（终态 + 有回放凭证）→ 顶替 batchId
    let (node_id, meta) = transfer_store::retry_source(&store, "batch-1").expect("replayable");
    assert_eq!(node_id, "aa");
    assert_eq!(
        meta,
        RetryMeta::Send {
            paths: vec!["/sdcard/report.pdf".into()]
        }
    );
    assert!(transfer_store::apply_retry(
        &mut store, "batch-1", "batch-2", 0
    ));
    assert_eq!(store.len(), 1, "回放不复制历史行");
    assert_eq!(store[0].batch_id, "batch-2");
    assert_eq!(store[0].status, "running");
    assert_eq!(store[0].transferred_bytes, 0, "回放从头计数");
    assert_eq!(
        store[0].retry_meta,
        Some(meta.clone()),
        "凭证保留，可再次回放"
    );
    assert_eq!(running_send_count(&store), 1, "回放重新占槽");

    // 新一轮：进度 → 完成
    assert!(transfer_store::reduce_event(
        &mut store,
        "send",
        &engine_progress("batch-2", 900, 1000, 20),
        ""
    ));
    assert!(transfer_store::reduce_event(
        &mut store,
        "send",
        &engine_terminal("batch-2", serde_json::json!({ "type": "completed" }), 30),
        ""
    ));
    assert_eq!(store[0].status, "completed");
    assert_eq!(store[0].transferred_bytes, 1000);
    assert!(store[0].is_terminal());
    let history: Vec<&str> = history_view(&store)
        .iter()
        .map(|e| e.batch_id.as_str())
        .collect();
    assert_eq!(history, vec!["batch-2"], "历史视图只见最终一批");
}

/// redial 的前提：节点下线后陈旧句柄必须摘除（否则重拨路径不可达，数据面
/// 命令全打在死会话上），而 endpoint memo 保留（重启后按 memo 重拨）
#[test]
fn node_stop_drops_stale_handles_but_keeps_endpoint_for_redial() {
    let _guard = device_bridge::statics_lock().lock().expect("statics lock");
    let node = "redial-node";
    device_bridge::forget_session(node);
    let ep = device_bridge::DialEndpoint {
        node_id: node.to_string(),
        addr: "192.168.1.42".to_string(),
        port: 47821,
    };
    device_bridge::remember_session(&ep, "sess-redial".to_string());
    assert_eq!(
        device_bridge::session_of(node).as_deref(),
        Some("sess-redial")
    );

    // 节点下线：peer.rs::on_receive_event 的 node-stopped 分支动作（drain）
    let drained = device_bridge::drain_sessions();
    assert!(drained.contains(&"sess-redial".to_string()));
    assert_eq!(
        device_bridge::session_of(node),
        None,
        "陈旧句柄已摘（重拨可达）"
    );
    assert_eq!(
        device_bridge::resolve_endpoint(None, node),
        Some(ep),
        "endpoint memo 保留 = 重启后可直接重拨"
    );
    device_bridge::forget_session(node);
}

/// 不可重试的三类条目在**调引擎之前**就被拒（判据前置的收益：send 方向无
/// 引擎建行事件，先发后校验会铸出永不入店的孤儿会话并占死槽位）
#[test]
fn replay_precheck_blocks_every_unreplayable_shape() {
    let mut store = vec![
        initiated_send("live", "aa"),
        send_row(
            "serve",
            "serve",
            "failed",
            RetryMeta::Send { paths: vec![] },
            None,
        ),
        initiated_send("queued", "aa"),
    ];
    // 供流记账行（无凭证）由纯函数直接构造不可行（retry_meta 是Option），
    // 这里用 None 表达同一形状
    store[1].retry_meta = None;
    store[2].status = "paused".to_string();

    assert_eq!(
        transfer_store::retry_source(&store, "live"),
        Err(transfer_store::RetryRefusal::NotTerminal)
    );
    assert_eq!(
        transfer_store::retry_source(&store, "queued"),
        Err(transfer_store::RetryRefusal::NotTerminal)
    );
    assert_eq!(
        transfer_store::retry_source(&store, "serve"),
        Err(transfer_store::RetryRefusal::MissingMeta)
    );
    assert_eq!(
        transfer_store::retry_source(&store, "absent"),
        Err(transfer_store::RetryRefusal::NotFound)
    );
}

// ==================== 终态历史持久化往返 ====================

/// 落盘 → 重启载入：终态原样保留（历史不丢），在飞批如实标 interrupted 并
/// **回写**（下次启动不再重复标注）
#[test]
fn terminal_history_survives_restart_and_active_rows_are_marked() {
    let h = MockHost::new();
    let mut store = vec![
        initiated_send("done", "aa"),
        initiated_send("live", "bb"),
        initiated_send("paused", "cc"),
    ];
    store[0].status = "failed".to_string();
    store[0].detail = Some("io".to_string());
    store[0].updated_at_ms = 90;
    store[2].status = "paused".to_string();
    persist_entries(&h, &store);

    // 重启：载入
    let mut loaded = load_entries(&h).expect("entries load");
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].status, "failed", "终态原样保留");
    assert_eq!(loaded[0].detail.as_deref(), Some("io"));
    assert_eq!(loaded[0].updated_at_ms, 90, "时间戳随持久层保真");

    // 恢复标注 + 回写
    assert_eq!(
        restore_entries(&h, &mut loaded),
        2,
        "running + paused 被标注"
    );
    let reloaded = load_entries(&h).expect("entries reload");
    assert_eq!(reloaded[0].status, "failed");
    assert_eq!(reloaded[1].status, "interrupted");
    assert_eq!(reloaded[2].status, "interrupted");
    // 二次启动不再重复标注（已落盘为终态）
    let mut again = reloaded;
    assert_eq!(restore_entries(&h, &mut again), 0);
}

/// 封顶在重启后依然成立：202 条终态 → 落盘 → 载入 → 逐出最旧 → 落盘 → 载入
#[test]
fn history_cap_holds_across_persistence_round_trip() {
    let h = MockHost::new();
    let mut store: Vec<TransferEntry> = (0..202u64)
        .map(|i| {
            let mut e = initiated_send(&format!("t{i}"), "aa");
            e.status = "completed".to_string();
            e.updated_at_ms = i;
            e
        })
        .collect();
    store.push(initiated_send("live", "aa"));
    persist_entries(&h, &store);

    let mut loaded = load_entries(&h).expect("entries load");
    assert_eq!(loaded.len(), 203);
    assert_eq!(
        transfer_store::evict_overflow(&mut loaded),
        2,
        "逐出最旧两条终态"
    );
    persist_entries(&h, &loaded);

    let after = load_entries(&h).expect("entries reload");
    assert_eq!(after.len(), 201);
    assert!(after
        .iter()
        .all(|e| e.batch_id != "t0" && e.batch_id != "t1"));
    assert!(
        after.iter().any(|e| e.batch_id == "live"),
        "在飞批不参与封顶淘汰"
    );
    assert!(after.iter().any(|e| e.batch_id == "t201"), "最新终态保留");
}

/// 排队批派发失败落终态行（票 08：排队批在 store 里没有行，失败静默丢弃
/// = 丢用户意图）——行形状可重试、batchId 本地唯一
#[test]
fn locally_failed_send_row_is_visible_and_replayable() {
    let first = next_local_failure_id();
    let second = next_local_failure_id();
    assert_ne!(first, second, "本地终态行 batchId 必须唯一（无引擎铸号）");

    let row = send_row(
        "aa",
        &first,
        "failed",
        RetryMeta::Send {
            paths: vec!["/sdcard/a.bin".into()],
        },
        Some("send dispatch failed: dial refused".to_string()),
    );
    assert!(row.is_terminal());
    assert_eq!(row.direction, "send");
    assert!(row.detail.as_deref().unwrap().contains("dial refused"));
    let (node_id, meta) = transfer_store::retry_source(&[row], &first).expect("可从历史重试");
    assert_eq!(node_id, "aa");
    assert_eq!(
        meta,
        RetryMeta::Send {
            paths: vec!["/sdcard/a.bin".into()]
        }
    );
}

/// 拉取意图先于引擎调用入队（票 08 修的竞态）：入队 → `pull-started` 事件
/// 挂载 → 行可重试；未入队的引擎批（对端发起的）如实不挂
#[test]
fn pull_started_attaches_retry_meta_only_after_intent_pushed() {
    let mut intents = vec![];
    transfer_store::push_pull_intent(
        &mut intents,
        "aa",
        RetryMeta::Pull {
            dir_id: "docs".into(),
            files: vec![transfer_store::PullFileSpec {
                rel_path: "docs/a.bin".into(),
                size: 7,
            }],
        },
    );
    let mut store = vec![];
    let own = serde_json::json!({
        "kind": "pull-started", "batchId": "pull-aa-0", "nodeId": "aa",
        "files": [{ "path": "docs/a.bin", "size": 7 }], "totalSize": 7u64, "tsMs": 5u64,
    });
    let foreign = serde_json::json!({
        "kind": "pull-started", "batchId": "pull-bb-0", "nodeId": "bb",
        "files": [{ "path": "docs/a.bin", "size": 7 }], "totalSize": 7u64, "tsMs": 6u64,
    });

    // 本端发起的批挂上凭证
    assert!(transfer_store::reduce_event(
        &mut store, "receive", &own, ""
    ));
    let meta = transfer_store::take_pull_intent(&mut intents, "aa", "docs/a.bin");
    store[0].retry_meta = meta;
    assert!(store[0].retry_meta.is_some());
    // 进行中不可重试；一旦失败落终态即可回放（凭证已在行上）
    assert_eq!(
        transfer_store::retry_source(&store, "pull-aa-0"),
        Err(transfer_store::RetryRefusal::NotTerminal)
    );
    assert!(transfer_store::reduce_event(
        &mut store,
        "receive",
        &engine_terminal(
            "pull-aa-0",
            serde_json::json!({ "type": "failed", "detail": "io" }),
            9
        ),
        ""
    ));
    assert!(
        transfer_store::retry_source(&store, "pull-aa-0").is_ok(),
        "失败后可回放"
    );

    // 别的主机的批不挂凭证（意图队列已空，且 node 不匹配）
    assert!(transfer_store::reduce_event(
        &mut store, "receive", &foreign, ""
    ));
    let no_meta = transfer_store::take_pull_intent(&mut intents, "bb", "docs/a.bin");
    store[1].retry_meta = no_meta;
    assert!(store[1].retry_meta.is_none(), "非本端发起的拉取批不可重试");
    // 终态后仍不可回放：凭证永挂不上（对照本端发起的批失败即可回放）
    assert!(transfer_store::reduce_event(
        &mut store,
        "receive",
        &engine_terminal(
            "pull-bb-0",
            serde_json::json!({ "type": "failed", "detail": "io" }),
            11
        ),
        ""
    ));
    assert_eq!(
        transfer_store::retry_source(&store, "pull-bb-0"),
        Err(transfer_store::RetryRefusal::MissingMeta)
    );
}
