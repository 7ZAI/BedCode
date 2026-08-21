# 03 — WebGL custom glyphs 光栅化

**What to build:** WebGL 渲染器启用 `customGlyphs`，unicode/box-drawing 符号直接 GPU 光栅化，减少 canvas 光栅化开销、改善渲染一致。实测内置 glyph 覆盖差异决定是否保留；WebGL context loss 恢复/回退 DOM 路径不回归。

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [x] WebGL 渲染器启用 `customGlyphs`
  - 0.19 addon-webgl 无构造参数（仅 preserveDrawingBuffer），开关走 xterm Terminal 选项（`customGlyphs: true`，TextureAtlas 读 config）
- [ ] 实测内置 glyph 覆盖差异，记录是否保留（含 box-drawing / emoji 渲染一致性结论）
- [x] WebGL context loss 后 1s 恢复 + 回退 DOM 渲染路径不回归（未改动 initWebGL 重建/回退逻辑，仅开 glyph 开关）
- [ ] 真机回归：TUI 应用（opencode / vim 边框）渲染一致、无残影 + 实测内置 glyph 覆盖差异决定是否保留（需真机）
