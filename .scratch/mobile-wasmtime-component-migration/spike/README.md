# Spike — wit-bindgen 0.60.0 × wasmtime 47 兼容性实证（ticket 01）

一次性探路 spike，结论已写回 spec（§S0 / §5 R1 / §8）。代码**有意不进入真实 SDK**：
版本未定前不污染 `plugin-sdk-mobile`；S1/S2 落地时按 ticket 01 的「版本输入表」执行。

## 结构

```
spike/
  wit/spike.wit    # 最小契约：2 import（host-log / host-storage）+ 1 export（command，含标量 probe）
  guest/           # wit-bindgen 0.60.0（macros）生成绑定，wasm32-unknown-unknown，cdylib
  host/            # wasmtime 47.0.3 + wit-component 0.256.0；bindgen! 来自 wasmtime 自带宏
```

## 运行

```bash
cargo run --release -p spike-host
```

host 会自动：嵌套 cargo build guest（`--target-dir` 指向 `guest/target`）→
`ComponentEncoder().validate(true).module().encode()` → 字节形态断言 →
`Component::from_binary` + `linker.instantiate`（fuel + ResourceLimiter 与生产同配置）→
`invoke`（import 往返）与 `probe`（bool/u64/u32 标量）断言。

期望输出（全 PASS 时最后一行为）：

```
[spike] PASS — wit-bindgen 0.60.0 × wasmtime 47 兼容：组件实例化 + 命令调用 + import 往返全部成功
```

## 关键发现（S2/S3 落地照此执行）

1. **0.60 import 函数 string 参数为 `&str`**（0.41 是 `String`）；export Guest trait 参数仍为 `String`
2. **宿主侧不加独立 wit-bindgen 依赖**：用 `wasmtime::component::bindgen!`（47 自带，
   wasmtime-internal-wit-bindgen 47.0.3 / wit-parser 0.252），与桌面端 `component.rs` 模式一致
3. **组件编码用 wit-component 0.256**（复制桌面 `tools/componentize` 时按此升级；产物字节
   形态 `00 61 73 6d 0d 00 01 00`，模块段在组件头之前）
4. **`add_to_linker` 显式标注**：`iface::<State, wasmtime::component::HasSelf<State>>(&mut linker, |s| s)`

## 未覆盖（诚实边界）

- fuel trap / ResourceLimiter 拒绝 / AOT `Component::serialize`：S1 单测范畴
- Android aarch64 运行差异：两端 wasmtime 同版本锚定（ADR 0019），不在 spike 范围
- WIT record/variant/resource：§3.1 契约不含，未引入