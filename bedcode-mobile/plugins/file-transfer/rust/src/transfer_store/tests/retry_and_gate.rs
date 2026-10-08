//! 票 08 · 重试判据/ 发送闸门 / 拉取意图队列（纯函数组，cargo 直测）

use super::*;

/// 造一条带retryMeta 的终态 send 条目
fn send_terminal(id: &str, status: &str, meta: Option<RetryMeta>) -> TransferEntry {
    let mut e: TransferEntry = serde_json::from_value(serde_json::json!({
        "batchId": id, "nodeId": "aa", "direction": "send", "status": status,
    }))
    .unwrap();
    e.retry_meta = meta;
    e
}

fn send_meta() -> RetryMeta {
    RetryMeta::Send {
        paths: vec!["/sdcard/a.bin".into()],
    }
}

// ==================== 重试判据单点 ====================

/// 四条判据各自的判定：不存在 / 仍在跑 / 无回放凭证 / 命中
#[test]
fn retry_source_classifies_every_refusal_and_hit() {
    let mut running = send_terminal("live", "running", Some(send_meta()));
    running.node_id = "aa".into();
    let store = vec![
        running,
        send_terminal("done", "completed", Some(send_meta())),
        // 供流记账行（send 方向但非本端发起，无回放凭证）
        send_terminal("serve", "failed", None),
        send_terminal("paused", "paused", Some(send_meta())),
    ];

    // 终态 + 有凭证 → 命中，回放源带 node_id 与元数据
    let (node_id, meta) = retry_source(&store, "done").expect("terminal entry replayable");
    assert_eq!(node_id, "aa");
    assert_eq!(meta, send_meta());

    assert_eq!(retry_source(&store, "ghost"), Err(RetryRefusal::NotFound));
    assert_eq!(retry_source(&store, "live"), Err(RetryRefusal::NotTerminal));
    assert_eq!(
        retry_source(&store, "paused"),
        Err(RetryRefusal::NotTerminal)
    );
    assert_eq!(
        retry_source(&store, "serve"),
        Err(RetryRefusal::MissingMeta)
    );
}

/// 三类拒绝各有可区分文案（真机「点了没反应」要能分辨原因）
#[test]
fn retry_refusal_messages_name_the_cause() {
    assert_eq!(RetryRefusal::NotFound.message("x"), "task not found: x");
    assert_eq!(
        RetryRefusal::NotTerminal.message("x"),
        "task still in flight, not retryable: x"
    );
    assert_eq!(
        RetryRefusal::MissingMeta.message("x"),
        "task not retryable (initiator metadata missing): x"
    );
}

/// 排他性：completed / failed / rejected / cancelled / interrupted 全部可重试
/// （interrupted = 引擎会话已死，本就是最典型的重试来源）
#[test]
fn retry_source_accepts_every_terminal_variant() {
    for status in [
        "completed",
        "failed",
        "rejected",
        "cancelled",
        "interrupted",
    ] {
        let store = [send_terminal("t", status, Some(send_meta()))];
        assert!(
            retry_source(&store, "t").is_ok(),
            "{status} 应视为可重试终态"
        );
    }
}

/// 回放后新批仍带凭证（再次终态后可再回放）；进行中不可重试
#[test]
fn apply_retry_keeps_meta_and_never_revives_active_entries() {
    let mut store = vec![send_terminal("old", "failed", Some(send_meta()))];
    assert!(apply_retry(&mut store, "old", "new", 42));
    assert_eq!(store[0].batch_id, "new");
    assert_eq!(store[0].status, "running");
    assert_eq!(
        store[0].retry_meta,
        Some(send_meta()),
        "新批仍由本端发起，须保留回放凭证"
    );
    // 进行中不可重试（apply_retry 与 retry_source 判据一致，不复活活跃条目）
    assert_eq!(retry_source(&store, "new"), Err(RetryRefusal::NotTerminal));
    assert!(!apply_retry(&mut store, "new", "newer", 43));
    // 新批再次失败后仍可回放（凭证没丢）
    assert!(reduce_event(
        &mut store,
        "send",
        &serde_json::json!({
            "kind": "terminal", "batchId": "new",
            "state": { "type": "failed", "detail": "io" }, "tsMs": 50u64,
        }),
        ""
    ));
    assert_eq!(store[0].detail.as_deref(), Some("io"));
    assert!(retry_source(&store, "new").is_ok(), "再次终态后可重试");
}

// ==================== 发送闸门判据 ====================

/// 槽位判据：running < limit 放行；等于/超出即满；limit ≤ 0 时下限 1
/// （配置缺失不得把发送全锁死）
#[test]
fn send_slot_open_uses_clamped_limit() {
    assert!(send_slot_open(0, 3));
    assert!(send_slot_open(2, 3));
    assert!(!send_slot_open(3, 3));
    assert!(!send_slot_open(4, 3));
    // 下限 1：并发配置为 0 时仍允许一个槽位
    assert!(send_slot_open(0, 0));
    assert!(!send_slot_open(1, 0));
}

// ==================== 拉取意图队列 ====================

/// 意图入队 → `pull-started` 事件按 node × rel_path 取回（收窄到单文件）
#[test]
fn pull_intent_round_trips_by_node_and_rel_path() {
    let mut list = vec![];
    push_pull_intent(
        &mut list,
        "aa",
        RetryMeta::Pull {
            dir_id: "docs".into(),
            files: vec![
                PullFileSpec {
                    rel_path: "docs/x.bin".into(),
                    size: 1,
                },
                PullFileSpec {
                    rel_path: "docs/y.bin".into(),
                    size: 2,
                },
            ],
        },
    );
    // 节点不符 → 不消费（别把别的主机的批挂上本插件凭证）
    assert!(take_pull_intent(&mut list, "bb", "docs/x.bin").is_none());
    assert_eq!(list.len(), 1, "未命中不消费意图");
    // 路径不符 → 不消费
    assert!(take_pull_intent(&mut list, "aa", "docs/z.bin").is_none());
    // 命中：收窄为单文件规格 + 保留共享根
    let meta = take_pull_intent(&mut list, "aa", "docs/y.bin").expect("intent consumed");
    assert_eq!(
        meta,
        RetryMeta::Pull {
            dir_id: "docs".into(),
            files: vec![PullFileSpec {
                rel_path: "docs/y.bin".into(),
                size: 2
            }],
        }
    );
    assert!(list.is_empty(), "取出即消费（幂等挂载）");
    assert!(take_pull_intent(&mut list, "aa", "docs/y.bin").is_none());
}

/// 封顶在入队点裁剪（票 08 修：原先只在成功挂载后裁剪，全部挂不上时无界增长）
#[test]
fn pull_intent_queue_is_capped_on_push() {
    let mut list = vec![];
    for i in 0..(PULL_INTENT_CAP + 4) {
        push_pull_intent(
            &mut list,
            "aa",
            RetryMeta::Pull {
                dir_id: "docs".into(),
                files: vec![PullFileSpec {
                    rel_path: format!("f{i}.bin"),
                    size: 0,
                }],
            },
        );
    }
    assert_eq!(list.len(), PULL_INTENT_CAP);
    // 最旧 4 条已丢（连不上 retry_meta 的行如实不可重试），最新一条仍在
    assert!(take_pull_intent(&mut list, "aa", "f0.bin").is_none());
    let last = PULL_INTENT_CAP + 3;
    assert!(take_pull_intent(&mut list, "aa", &format!("f{last}.bin")).is_some());
}
