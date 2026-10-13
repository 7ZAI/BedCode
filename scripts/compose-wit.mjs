#!/usr/bin/env node
/**
 * 票 03 · WIT 分片拼装脚本（生产版，基于票 01 POC compose.py 升级）
 *
 * 端 `wit/` 目录是生成物：core.wit（wasm-core 共享真源）+ cap-*.wit（能力域 /
 * 端 cap 真源）+ bedcode.wit（package 声明 + world plugin { include … }）。
 * 「组合了什么」的唯一答案在双端 `compose.json` 端清单。
 *
 * **包版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`**（ADR 0051 D3）：Rust
 * 常量而非 JSON——能力域 crate 的导出名清单走 `inventory` 静态自报（要求编译期
 * 常量），JSON 只能运行期解析，与该约束结构性冲突。端清单的 `package` 字段只声明
 * **包名**（`bedcode:plugin`），`bedcode.wit` 的 `package bedcode:plugin@<version>;`
 * 行由本脚本按真源拼装——版本只有一处手写，双端清单不持版本字面量（否则 bump 要改
 * 三处，且清单会退化成第二真源）。
 * bump 政策（任何 WIT 变更含函数级追加 → patch；interface 增删 / 签名变 → minor）
 * 见 ADR 0051 D4；源文件 package 行、生成物 package 行与真源的一致性由
 * `wit_package_version_lock.rs` 断言（本脚本只负责注入，不重复做一致性判定）。
 *
 * 用法（仓库根执行）：
 *   node scripts/compose-wit.mjs <end>           # 拼装写入端 wit/ 目录
 *   node scripts/compose-wit.mjs <end> --check   # 只读：比对现有生成物与真源（漂移锁）
 *   node scripts/compose-wit.mjs --all           # 双端拼装
 *
 * 幂等：连续两次输出 `diff -r` 为空；`--check` 两次的比对结果一致。
 */
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const ENDS = {
  desktop: "bedcode-desktop/packages/plugin-sdk-desktop/compose.json",
  mobile: "bedcode-mobile/packages/plugin-sdk-mobile/compose.json",
};

/** 仓根 WIT 包版本真源（ADR 0051 D3）——Rust 常量，全仓唯一手写处 */
const PACKAGE_SOURCE = join(ROOT, "contracts/bedcode-plugin-package.rs");

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function readManifest(end) {
  const manifestPath = join(ROOT, ENDS[end]);
  let raw;
  try {
    raw = readFileSync(manifestPath, "utf-8");
  } catch {
    throw new Error(`端清单不存在: ${manifestPath}`);
  }
  let m;
  try {
    m = JSON.parse(raw);
  } catch (e) {
    throw new Error(`端清单 JSON 解析失败 ${manifestPath}: ${e.message}`);
  }
  m.core = resolve(ROOT, m.core);
  m.out = resolve(ROOT, m.out);
  for (const k of Object.keys(m.caps)) m.caps[k] = resolve(ROOT, m.caps[k]);
  return m;
}

/// 读仓根包版本真源并做最小形状校验（fail-visible：真源缺字段/非 semver 即抛，不猜默认值）
///
/// 真源是 Rust 常量文件，用正则取 `pub const <NAME>: &str = "<值>";` —— 只认三个常量
/// （PACKAGE_NAME / PACKAGE_VERSION / PACKAGE），少一个即抛（半迁移状态当场红）。
function readPackage() {
  let raw;
  try {
    raw = readFileSync(PACKAGE_SOURCE, "utf-8");
  } catch (e) {
    throw new Error(`包版本真源不存在或不可读 ${PACKAGE_SOURCE}: ${e.message}`);
  }
  const read = (name) => {
    const m = raw.match(new RegExp(`pub\\s+const\\s+${name}\\s*:\\s*&str\\s*=\\s*"([^"]+)"\\s*;`));
    if (!m) throw new Error(`包版本真源缺常量 ${name}: ${PACKAGE_SOURCE}`);
    return m[1];
  };
  const name = read("PACKAGE_NAME");
  const version = read("PACKAGE_VERSION");
  const full = read("PACKAGE");
  if (!name.includes(":")) {
    throw new Error(`包名应为「bedcode:<ns>」形态，实际 ${name}`);
  }
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error(`包版本应为三段 semver，实际 ${version}`);
  }
  if (full !== `${name}@${version}`) {
    throw new Error(
      `真源自相矛盾：PACKAGE="${full}" ≠ PACKAGE_NAME@PACKAGE_VERSION="${name}@${version}"`,
    );
  }
  return { name, version };
}

/** 计算生成物清单：name -> { kind: "copy", src } | { kind: "gen", content }（不落盘） */
function computeFiles(m, pkg) {
  const files = {};
  files["core.wit"] = { kind: "copy", src: m.core };
  for (const name of Object.keys(m.caps)) {
    files[`cap-${name}.wit`] = { kind: "copy", src: m.caps[name] };
  }
  const body = ["core", ...Object.keys(m.caps).map((n) => `cap-${n}`)]
    .map((w) => `    include ${w};`)
    .join("\n");
  // package 行是本脚本唯一的版本注入点：源文件（core / cap 分片）自带版本，
  // 逐字拷贝即随之进入生成物；此处不重写源文件版本，避免第二处版本真源。
  files["bedcode.wit"] = {
    kind: "gen",
    content: `package ${pkg.name}@${pkg.version};\n\nworld ${m.world} {\n${body}\n}\n`,
  };
  return files;
}

function materialize(m, files) {
  if (!existsSync(m.out)) mkdirSync(m.out, { recursive: true });
  for (const [name, spec] of Object.entries(files)) {
    const dst = join(m.out, name);
    if (spec.kind === "copy") copyFileSync(spec.src, dst);
    else writeFileSync(dst, spec.content, "utf-8");
  }
}

function fileSha(spec) {
  if (spec.kind === "copy") return sha256(spec.src);
  return createHash("sha256").update(spec.content).digest("hex");
}

function main() {
  const args = process.argv.slice(2);
  const check = args.includes("--check");
  const ends = check ? args.filter((a) => a !== "--check") : args;
  const all = ends.includes("--all");

  let exitCode = 0;
  const pkg = readPackage();
  for (const end of all ? Object.keys(ENDS) : ends) {
    const m = readManifest(end);
    // 端清单只声明包名：与真源 name 不一致即抛（版本真源单点化的前提，见 ADR 0051 D3）
    if (m.package !== pkg.name) {
      throw new Error(
        `[${end}] 端清单 package "${m.package}" 与仓根真源 name "${pkg.name}" 漂移——` +
          `包名/版本真源单点化在 ${PACKAGE_SOURCE}，清单只写包名`,
      );
    }
    const files = computeFiles(m, pkg);
    if (check) {
      // 只读漂移锁：现有生成物 sha256 == 真源现算 sha256（真源已改则提示重跑拼装）
      for (const [name, spec] of Object.entries(files)) {
        const want = fileSha(spec);
        const path = join(m.out, name);
        if (!existsSync(path)) {
          console.error(`[${end}] ${name} 缺失（生成物不存在，需重跑拼装）`);
          exitCode = 1;
          continue;
        }
        const got = sha256(path);
        if (got !== want) {
          console.error(`[${end}] ${name} 漂移: 期望 ${want} 实际 ${got}（真源已改或生成物被手改）`);
          exitCode = 1;
        } else {
          console.log(`[${end}] ${name} ok`);
        }
      }
    } else {
      materialize(m, files);
      for (const [name, spec] of Object.entries(files)) {
        console.log(`[${end}] ${name} ${fileSha(spec)}`);
      }
    }
  }
  process.exit(exitCode);
}

main();
