#!/usr/bin/env node
/**
 * 票 03 · WIT 分片拼装脚本（生产版，基于票 01 POC compose.py 升级）
 *
 * 端 `wit/` 目录是生成物：core.wit（wasm-core 共享真源）+ cap-*.wit（能力域 /
 * 端 cap 真源）+ bedcode.wit（package 声明 + world plugin { include … }）。
 * 「组合了什么」的唯一答案在双端 `compose.json` 端清单。
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

/** 计算生成物清单：name -> { kind: "copy", src } | { kind: "gen", content }（不落盘） */
function computeFiles(m) {
  const files = {};
  files["core.wit"] = { kind: "copy", src: m.core };
  for (const name of Object.keys(m.caps)) {
    files[`cap-${name}.wit`] = { kind: "copy", src: m.caps[name] };
  }
  const body = ["core", ...Object.keys(m.caps).map((n) => `cap-${n}`)]
    .map((w) => `    include ${w};`)
    .join("\n");
  files["bedcode.wit"] = { kind: "gen", content: `${m.package}\n\nworld ${m.world} {\n${body}\n}\n` };
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
  for (const end of all ? Object.keys(ENDS) : ends) {
    const m = readManifest(end);
    const files = computeFiles(m);
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
