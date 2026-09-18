# 03: 构建链全量 wasip3 + wasi2 清理（A0-4 + A0-5）

**What to build:** 插件统一构建脚本与宿主 dev-build 路径从 `wasm32-unknown-unknown` 切到 `wasm32-wasip3`；**全部存量桌面插件**（file-transfer / ai-chatbox / agent-hub / auto-task）与测试 fixture 重建为 wasip3 产物并零回归（spike 已验证零代码改动、17s/插件）；wasm32-unknown-unknown 构建路径与 p2 专用残留清理（A0-5）。

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] 4 个存量桌面插件以 wasip3 产物重新构建，宿主加载/激活/功能测试全绿（零代码改动或改动最小化）
- [ ] 测试 fixture 构建路径切 wasip3（含宿主测试内联构建路径）
- [ ] 宿主代码与脚本中 unknown-unknown 构建路径移除或标注废弃；无 p2 专用死代码残留
- [ ] 全量验证：桌面 cargo test + pnpm run test:run + 根目录 eslint 0 error
