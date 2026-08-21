# -*- coding: utf-8 -*-
"""生成 dict.rs + synth_pmap.bin（票据 07 测试资源）"""
import json
import numpy as np

ROOT = r"D:\tauriProject\BedCode\bedcode-mobile\src-tauri"
REF = r"D:\tauriProject\BedCode\.scratch\ocr-plugin\ref"


def esc(c):
    if c == "\\":
        return "\\\\"
    if c == "'":
        return "\\'"
    if c == "\n":
        return "\\n"
    return c


lines = open(REF + r"\ppocr_keys_v1.txt", encoding="utf-8").read().splitlines()
assert len(lines) == 6623, len(lines)
body = "\n".join("    '%s'," % esc(c) for c in lines)
head = """//! PP-OCRv4 ch 识别字典（ppocr_keys_v1.txt，RapidOCR 官方源，Apache-2.0）
//!
//! 自动生成：RapidOCR 官方 `ppocr_keys_v1.txt`（6623 行，无重复、无空行）。
//! 与 rec 模型输出维度对应：index 0 = CTC blank，1..=6623 = 本字典，6624 = 空格（总计 6625）。
//! 变更字典须同步模型（ch_PP-OCRv4_rec_infer.onnx 输出 softmax_11.tmp_0 末维 6625）。

/// 6623 个识别字符（按模型导出时的字典序；若含重复则按位索引，勿去重）
pub static CH_DICT: [char; 6623] = [
"""
with open(ROOT + r"\src\ocr\ppocr\dict.rs", "w", encoding="utf-8") as f:
    f.write(head + body + "\n];\n")
print("dict.rs written")

rng = np.random.default_rng(42)
pmap = np.zeros((64, 96), dtype=np.float32)
pmap[10:24, 12:60] = 0.9
pmap[40:52, 30:80] = 0.85
pmap += rng.normal(0, 0.02, pmap.shape).astype(np.float32)
pmap[pmap < 0] = 0
pmap[pmap > 1] = 1
pmap[60:64, 0:10] = 0.2
pmap.astype("<f4").tofile(ROOT + r"\testdata\synth_pmap.bin")
g = json.load(open(REF + r"\golden_synth.json", encoding="utf-8"))
print("golden boxes:", g["boxes"])
print("golden scores:", g["scores"])
