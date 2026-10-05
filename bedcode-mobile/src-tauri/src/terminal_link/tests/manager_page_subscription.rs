//! 段2 订阅态（管理器，链路独立） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// 段2 订阅态与推送通道槽挂在管理器上（独立于链路）：链路未建立也记录订阅
/// 意愿，同一会话始终返回同一份开关/通道（链路重建后沿用）；取消订阅与会话
/// 删除必须一并清空通道，否则推送会持续往已卸载的页面发
#[test]
fn manager_page_subscription_is_session_scoped_and_persistent() {
    let manager = TerminalLinkManager {
        links: Mutex::new(HashMap::new()),
        consumers: Mutex::new(HashMap::new()),
        page_channels: Mutex::new(HashMap::new()),
    };
    let channel: Channel<InvokeResponseBody> = Channel::new(|_| Ok(()));

    assert!(!manager.is_page_subscribed("s1"));

    manager.page_subscribe("s1", channel.clone());
    assert!(manager.is_page_subscribed("s1"));
    let flag = manager.consumer_flag("s1");
    assert!(flag.load(Ordering::SeqCst));

    // 同一会话多次 get-or-create 返回同一 Arc（链路重建后沿用订阅态 + 通道槽）
    assert!(Arc::ptr_eq(&flag, &manager.consumer_flag("s1")));
    let slot = manager.consumer_channel("s1");
    assert!(slot.lock().unwrap().is_some(), "段2 订阅必须登记推送通道");
    assert!(Arc::ptr_eq(&slot, &manager.consumer_channel("s1")));

    // 会话隔离：另一会话既不订阅、也没有通道
    assert!(!manager.is_page_subscribed("s2"));
    assert!(manager.consumer_channel("s2").lock().unwrap().is_none());

    manager.page_unsubscribe("s1");
    assert!(!manager.is_page_subscribed("s1"));
    assert!(
        !flag.load(Ordering::SeqCst),
        "取消订阅必须作用在同一份开关上（链路持有的 Arc 同步可见）"
    );
    assert!(
        manager.consumer_channel("s1").lock().unwrap().is_none(),
        "退出页面必须清空推送通道"
    );

    // 会话删除：订阅态与通道一并作废
    manager.page_subscribe("s1", channel);
    manager.remove("s1");
    assert!(!manager.is_page_subscribed("s1"));
    assert!(manager.consumer_channel("s1").lock().unwrap().is_none());
}
