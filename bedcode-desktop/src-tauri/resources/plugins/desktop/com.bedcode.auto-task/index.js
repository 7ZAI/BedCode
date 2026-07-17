const { defineComponent, inject, ref, computed, onMounted, onUnmounted, openBlock, createElementBlock, createElementVNode, normalizeClass, toDisplayString, createCommentVNode, Fragment, renderList } = window.__BEDCODE_SHARED__["vue"];
const _hoisted_1 = { class: "h-full flex flex-col bg-[var(--bg-primary)]" };
const _hoisted_2 = {
  key: 0,
  class: "flex-1 flex items-center justify-center"
};
const _hoisted_3 = {
  key: 1,
  class: "flex-1 overflow-y-auto px-4 py-3 space-y-4"
};
const _hoisted_4 = {
  key: 0,
  class: "rounded-lg border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 p-3"
};
const _hoisted_5 = { class: "flex items-center gap-2 mb-1" };
const _hoisted_6 = { class: "text-sm text-[var(--text-primary)] truncate" };
const _hoisted_7 = { class: "text-xs text-[var(--text-tertiary)] mt-1" };
const _hoisted_8 = { key: 1 };
const _hoisted_9 = { class: "text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2" };
const _hoisted_10 = { class: "space-y-1" };
const _hoisted_11 = { class: "text-xs text-[var(--text-tertiary)] w-5 text-right flex-shrink-0" };
const _hoisted_12 = { class: "text-[var(--text-primary)] truncate flex-1" };
const _hoisted_13 = { key: 2 };
const _hoisted_14 = { class: "space-y-1" };
const _hoisted_15 = ["onClick"];
const _hoisted_16 = { class: "flex-1 min-w-0" };
const _hoisted_17 = { class: "text-sm text-[var(--text-primary)] truncate" };
const _hoisted_18 = { class: "text-xs text-[var(--text-tertiary)]" };
const _hoisted_19 = {
  key: 3,
  class: "flex-1 flex flex-col items-center justify-center py-12"
};
const _sfc_main = /* @__PURE__ */ defineComponent({
  __name: "TaskHistoryView",
  setup(__props) {
    const context = inject("pluginContext");
    const tasks = ref([]);
    const queue = ref([]);
    const loading = ref(true);
    const selectedSessionId = ref("");
    const statusLabel = {
      idle: "空闲",
      in_progress: "执行中",
      asking: "等待输入",
      completed: "已完成",
      interrupted: "已中断",
      pending: "待执行"
    };
    const statusColor = {
      idle: "text-[var(--text-tertiary)]",
      in_progress: "text-blue-500",
      asking: "text-amber-500",
      completed: "text-green-500",
      interrupted: "text-red-500",
      pending: "text-[var(--text-tertiary)]"
    };
    const statusDot = {
      idle: "bg-[var(--text-tertiary)]",
      in_progress: "bg-blue-500",
      asking: "bg-amber-500",
      completed: "bg-green-500",
      interrupted: "bg-red-500",
      pending: "bg-[var(--text-tertiary)]"
    };
    const currentTask = computed(
      () => tasks.value.find((t) => t.status === "in_progress" || t.status === "asking")
    );
    const historyTasks = computed(
      () => tasks.value.filter((t) => t.status === "completed" || t.status === "interrupted")
    );
    async function loadHistory() {
      try {
        const result = await context.commands.execute("auto-task.list-task-history");
        if (result == null ? void 0 : result.tasks) {
          tasks.value = result.tasks;
        }
      } catch (e) {
        console.error("[Auto Task] Failed to load history:", e);
      }
    }
    async function loadQueue(sessionId) {
      if (!sessionId) {
        queue.value = [];
        return;
      }
      try {
        const result = await context.commands.execute("auto-task.list-task-queue", { session_id: sessionId });
        if (result == null ? void 0 : result.tasks) {
          queue.value = result.tasks;
        }
      } catch (e) {
        console.error("[Auto Task] Failed to load queue:", e);
      }
    }
    async function refresh() {
      loading.value = true;
      await loadHistory();
      if (!selectedSessionId.value && tasks.value.length > 0) {
        const firstSession = tasks.value[0].session_id;
        if (firstSession) {
          selectedSessionId.value = firstSession;
          await loadQueue(firstSession);
        }
      } else if (selectedSessionId.value) {
        await loadQueue(selectedSessionId.value);
      }
      loading.value = false;
    }
    function onStatusChanged() {
      loadHistory();
    }
    function onQueueChanged(data) {
      if ((data == null ? void 0 : data.session_id) === selectedSessionId.value || !selectedSessionId.value) {
        loadQueue((data == null ? void 0 : data.session_id) || selectedSessionId.value);
      }
      loadHistory();
    }
    let statusDisposable = null;
    let queueDisposable = null;
    onMounted(async () => {
      await refresh();
      statusDisposable = context.events.on("task:status-changed", onStatusChanged);
      queueDisposable = context.events.on("task:queue-changed", onQueueChanged);
    });
    onUnmounted(() => {
      statusDisposable == null ? void 0 : statusDisposable.dispose();
      queueDisposable == null ? void 0 : queueDisposable.dispose();
    });
    function formatTime(isoStr) {
      if (!isoStr) return "-";
      try {
        const d = new Date(isoStr.replace(" ", "T"));
        return d.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
      } catch {
        return isoStr;
      }
    }
    function selectSession(sessionId) {
      selectedSessionId.value = sessionId;
      loadQueue(sessionId);
    }
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1, [
        _cache[3] || (_cache[3] = createElementVNode("div", { class: "px-4 py-3 border-b border-[var(--border)] flex-shrink-0" }, [
          createElementVNode("h2", { class: "text-sm font-semibold text-[var(--text-primary)]" }, "任务历史")
        ], -1)),
        loading.value ? (openBlock(), createElementBlock("div", _hoisted_2, [..._cache[0] || (_cache[0] = [
          createElementVNode("span", { class: "text-sm text-[var(--text-tertiary)]" }, "加载中...", -1)
        ])])) : (openBlock(), createElementBlock("div", _hoisted_3, [
          currentTask.value ? (openBlock(), createElementBlock("div", _hoisted_4, [
            createElementVNode("div", _hoisted_5, [
              createElementVNode("div", {
                class: normalizeClass(["w-2 h-2 rounded-full", statusDot[currentTask.value.status] || "bg-blue-500"])
              }, null, 2),
              createElementVNode("span", {
                class: normalizeClass(["text-xs font-medium", statusColor[currentTask.value.status] || "text-blue-500"])
              }, toDisplayString(statusLabel[currentTask.value.status] || currentTask.value.status), 3)
            ]),
            createElementVNode("p", _hoisted_6, toDisplayString(currentTask.value.name || currentTask.value.session_id), 1),
            createElementVNode("p", _hoisted_7, toDisplayString(formatTime(currentTask.value.started_at || currentTask.value.created_at)), 1)
          ])) : createCommentVNode("", true),
          queue.value.length > 0 ? (openBlock(), createElementBlock("div", _hoisted_8, [
            createElementVNode("h3", _hoisted_9, " 待执行队列 (" + toDisplayString(queue.value.length) + ") ", 1),
            createElementVNode("div", _hoisted_10, [
              (openBlock(true), createElementBlock(Fragment, null, renderList(queue.value, (item) => {
                return openBlock(), createElementBlock("div", {
                  key: item.id,
                  class: "flex items-center gap-2 px-3 py-2 rounded-md bg-[var(--bg-hover)] text-sm"
                }, [
                  createElementVNode("span", _hoisted_11, "#" + toDisplayString(item.position), 1),
                  createElementVNode("span", _hoisted_12, toDisplayString(item.prompt), 1)
                ]);
              }), 128))
            ])
          ])) : createCommentVNode("", true),
          historyTasks.value.length > 0 ? (openBlock(), createElementBlock("div", _hoisted_13, [
            _cache[1] || (_cache[1] = createElementVNode("h3", { class: "text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2" }, " 历史记录 ", -1)),
            createElementVNode("div", _hoisted_14, [
              (openBlock(true), createElementBlock(Fragment, null, renderList(historyTasks.value, (task) => {
                return openBlock(), createElementBlock("div", {
                  key: task.id,
                  class: "flex items-center gap-2 px-3 py-2 rounded-md hover:bg-[var(--bg-hover)] cursor-pointer transition-colors",
                  onClick: ($event) => selectSession(task.session_id)
                }, [
                  createElementVNode("div", {
                    class: normalizeClass(["w-2 h-2 rounded-full flex-shrink-0", statusDot[task.status] || "bg-[var(--text-tertiary)]"])
                  }, null, 2),
                  createElementVNode("div", _hoisted_16, [
                    createElementVNode("p", _hoisted_17, toDisplayString(task.name || task.session_id), 1),
                    createElementVNode("p", _hoisted_18, toDisplayString(formatTime(task.completed_at || task.created_at)), 1)
                  ]),
                  createElementVNode("span", {
                    class: normalizeClass(["text-xs flex-shrink-0", statusColor[task.status]])
                  }, toDisplayString(statusLabel[task.status] || task.status), 3)
                ], 8, _hoisted_15);
              }), 128))
            ])
          ])) : createCommentVNode("", true),
          !currentTask.value && queue.value.length === 0 && historyTasks.value.length === 0 ? (openBlock(), createElementBlock("div", _hoisted_19, [..._cache[2] || (_cache[2] = [
            createElementVNode("svg", {
              class: "w-12 h-12 text-[var(--text-tertiary)] mb-3",
              fill: "none",
              stroke: "currentColor",
              viewBox: "0 0 24 24"
            }, [
              createElementVNode("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "1.5",
                d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
              })
            ], -1),
            createElementVNode("p", { class: "text-sm text-[var(--text-tertiary)]" }, "暂无任务记录", -1),
            createElementVNode("p", { class: "text-xs text-[var(--text-tertiary)] mt-1" }, "启动会话后任务将自动记录", -1)
          ])])) : createCommentVNode("", true)
        ]))
      ]);
    };
  }
});
async function activate(context) {
  context.ui.registerSidebarPanel({
    id: "auto-task.history",
    title: "任务历史",
    icon: "📋",
    component: _sfc_main
  });
  context.events.on("task:status-changed", (data) => {
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
  context.events.on("session:mode-changed", (data) => {
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
