/**
 * 共享 wasm 产物声明（`package.json` 的 `bedcode.sharedRustManifest`）读取
 *
 * 单产物插件（file-transfer，ADR 0050）在移动端没有自己的 `rust/`：wasm 由共享
 * crate（桌面 wasm-app，按桌面 world 构建）编出，声明值相对插件目录指向该 crate
 * 的 `Cargo.toml`。本声明是两处消费的共用事实源：
 *   - `scripts/plugin-build.js`：构建共享 crate 并把 wasm 复制到宿主资源目录；
 *   - `scripts/dev-run.js`：把共享 crate 并入产物新鲜度判据，并在补建时改走
 *     `plugin-build.js`（SDK CLI 的 `--rust-only` 硬编码 `<插件>/rust/Cargo.toml`，
 *     对单产物插件必然失败）。
 * 落点 package.json 而非 plugin.json：后者是**宿主解析的清单**，新增字段要过两端
 * manifest 结构校验；前者只被构建链消费，零宿主影响。
 */

import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

/** 读取插件目录 package.json 的共享 rust 声明；未声明 / 解析失败时返回 null */
export function readSharedRustManifest(pluginRoot) {
  const pkgPath = join(pluginRoot, 'package.json')
  if (!existsSync(pkgPath)) return null
  try {
    const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'))
    return pkg.bedcode?.sharedRustManifest ?? null
  } catch {
    return null
  }
}
