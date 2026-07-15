async function activate(context) {
  context.events.on("task:statusChanged", (data) => {
    const { taskStatus, taskReason } = data;
    const statusMessages = {
      idle: "空闲",
      in_progress: "执行中",
      asking: "等待输入",
      completed: "已完成",
      interrupted: "已中断"
    };
    const label = statusMessages[taskStatus] || taskStatus;
    console.log(`[Auto Task] 状态变更: ${label}${taskReason ? ` - ${taskReason}` : ""}`);
  });
  context.events.on("session:modeChanged", (data) => {
    const { autoApprove } = data;
    console.log(`[Auto Task] 模式变更: ${autoApprove ? "自动授权" : "手动模式"}`);
  });
  console.log("[Auto Task] Plugin activated");
}
async function deactivate() {
  console.log("[Auto Task] Plugin deactivated");
}
export {
  activate,
  deactivate
};
