# BASELINE_DIAGNOSIS.md — P0 归因诊断（§3.4 / §16）

> 数据来源：`baseline/raw/probe-*.json`（本次测量）、`baseline/raw/W*.csv`（已有
> 10 次重复基线）、`baseline/raw/cprofile-*.txt`（cProfile 证据）。
> 所有“measured”数字都可以用 §9 的命令重跑；标 **inferred** 的是无法用 Python
> 层钩子直接测到、只能由代码结构推断的部分。
>
> **本报告先回答 §16 的核心问题（第 4 节），再逐 W 展开。**

---

## 0. 总结论（10 行）

1. 端口不是“Rust 快内核 + 薄边界”，而是**逐函数直译**：同一算法、**同样的
   Python 对象创建次数**、**同样的函数调用次数**，但每一次调用都要过一次
   CPython C-API。→ 回退的主因是**单次操作的实现成本**，不是算法。
2. 实测对照（`issue-301.pdf`，两侧计数完全相同：`read_object` 186,895、
   `_check_limits` 317,320、`operations` 130,523）：
   解析 **reference 0.417 s → port 1.654 s（3.96×）**；
   而每个 Token 的**逻辑计数一致**，即溢价全在“执行方式”。
   折合 **+3.9 µs/Token 的纯边界/解释开销**。
3. W3 的 706.6 ms 实测拆分为：**内容流解析 ≈ 373 ms（53%）**、
   **文本操作符解释/派发 ≈ 305 ms（43%，其中字形解码+拼接 128 ms）**、
   **字体/CMap 装载 + 页资源 + 其它 ≈ 21 ms（3%）**。
4. W3 回退量（vs 原版 +504 ms）中，**解析路径贡献 ≈ 281 ms（56%）**、
   **文本解释路径贡献 ≈ 224 ms（44%）**、字体/CMap ≈ 0。
5. 每 MB 解码内容流：**32,436 个 Operation、79,500 个 Token、
   72,667 次 `process_operation` 边界调用、43,370 次 `_handle_tj`**；
   解析成本 **1,307 ms/MB**（原版 280 ms/MB）。
6. W4 **不是** CCITT/TIFF 主导：W4 语料里**没有任何 CCITT/TIFFDecode 图像**
   （只有 Flate/LZW/DCT/RunLength/ASCII85）。1,892 ms 中 **~1,767 ms（93%）** 是
   `page.images` 触发的**内容流解析 + ImageFile 物化**；
   纯 filter 解码 9.3 ms（0.5%），Pillow 成像 116 ms（6%）。
7. W5–W7 实测为 **对象图拷贝主导**：copy 占 48–56%、read（重新解析源文件）占
   36–42%、序列化（write）只占 7–18%。端口慢在 **copy + 读取**，不是写盘。
8. 回退归因（W3 实测）：**跨语言边界 + 逐操作解释 ≈ 100%**，
   **算法差异 ≈ 0%**，**Python 对象物化 ≈ 0 增量**（物化次数相同，
   增量成本已包含在“边界/解释”里）。
9. P1/P2 的最小改动点已经定位到 **file:line**（第 8 节），
   其中**最高杠杆**是 `_parse_content_stream` 里的
   `content_stream_check_limits`（每 Token 一次 `getattr`+`__module__`/
   `__qualname__` 比较）与 `call_read_object`（每 Token 一次模块属性查找+身份判定）。
10. **cProfile 能看见 Rust 入口**（因为 Rust 通过 Python 对象协议回调，这些
    调用会经过求值循环），所以本报告的每个结论都有 profiler 与计数器双重证据。
    唯一测不到的是 **Rust→Rust 的内部 `PyObject_Call`**（见第 6 节“不可测边界”）。

---

## 1. 核心解释：同样的调用次数，4 倍的单价

| 指标（`resources/issue-301.pdf`，1 页） | reference | port | 倍率 |
|---|---:|---:|---:|
| 解码内容流字节 | 1,202,333 | 1,202,333 | 1.00× |
| Operations | 130,523 | 130,523 | 1.00× |
| 操作数 Token | 186,797 | 186,797 | 1.00× |
| `read_object` 调用 | 186,895 | 186,895 | **1.00×** |
| `ContentStream._check_limits` | 317,320 | 317,320 | **1.00×** |
| 物化对象数（Float+Number+Array…） | 186,895 | 186,895 | 1.00× |
| **`operations`（解析）耗时** | **0.417 s** | **1.654 s** | **3.96×** |
| **`extract_text()` 耗时** | **0.494 s** | **1.810 s** | **3.66×** |
| 单 Token 解析成本 | 1.31 µs | 5.21 µs | 3.96× |
| 单操作符解释成本（`process_operation`） | 84 ns | 180 ns | 2.14× |

**这是本轮最重要的证据**：算法、数据、对象数量完全相同，唯一差别是
“CPython 字节码执行一次操作” vs “Rust 通过 C-API 执行一次同样的操作”。
后者每次都要：字符串哈希查属性 → 建立 `PyDict`/`PyTuple` 参数 → 类型转换 →
错误检查。端口热路径上每 Token 有 **20–40 次**这样的调用（§2.2 的调用点清单），
这就是 3.96× 的来源。

> 因此方案 §0.1 的“逐 Token 往返”判断方向正确，但**表述需要修正**：
> 问题不只是“有回调”，而是**整个算法都在用 Python 对象协议执行**。
> 仅仅“消灭回调”而不改变“用 Python 对象表示 Token”的做法（方案 §2.2 已警告）
> 不会带来数量级收益。

---

## 2. W1–W8 主耗时阶段

### 2.1 基线（已有 10 次重复，`baseline/raw/W*.csv`）

| W | ref 中位数 | port 中位数 | speedup | 主耗时阶段（实测依据） |
|---|---:|---:|---:|---|
| W1 | 12.62 ms | 34.94 ms | 0.361× | **xref/trailer + 间接对象解析**（`baseline/raw/cprofile-w1-{rust,reference}.txt`）：port 里 `datastructures_read_object` 1,639 次调用、`dictionaryobject_read_from_stream` 238 次、`reader_read_object_header` 367 次，合计 ≈ 65% 的 52 ms cProfile 时间；20 个小文件 → 每次 open 约 1.7 ms vs 原版 0.19 ms |
| W2 | 221.72 ms | 561.07 ms | 0.395× | **页树 + 间接对象解析**：`GeoTopo.pdf` 单文件 port 0.312 s vs ref 0.247 s；`reader_get_object_from_stream` 24 次调用占 0.294 s（94%），其内部 `read_object` 递归 35,643 次（ref 侧同名函数 35,643 次也占 0.208 s）→ 同一对象解析路径的单价问题 |
| W3 | 178.54 ms | 706.64 ms | 0.253× | 见第 4 节：解析 53% + 文本解释 43% + 其余 3% |
| W4 | 608.22 ms | 1865.03 ms | 0.326× | 见第 5 节：`page.images` → 内容流解析 93% |
| W5 | 11.75 ms | 40.80 ms | 0.288× | read 29.7 ms(42%) + copy 36.1 ms(51%) + write 5.2 ms(7%) |
| W6 | 15.21 ms | 54.06 ms | 0.281× | read 14.5(25%) + copy 32.3(56%) + write 10.4(18%) |
| W7 | 20.86 ms | 70.82 ms | 0.295× | read 25.9(35%) + copy 35.4(48%) + write 13.2(18%) |
| W8 | 445.79 ms | 1360.92 ms | 0.328× | 端到端叠加：7 文件中 2 页文本提取（解析+解释）+ 全量 append + write + 重开；实测 `open_total` 85.1 ms（ref 32.6 ms），其余按 W3/W5 比例叠加 |

> 说明：W1 的 `open_total` 在 `probe-stages` 里只有 7.8 ms（ref 3.7 ms），因为
> 那里测的是“同一进程内第 2 次打开的最快值”；基线 34.94 ms 是**冷启动**
> （首进程首批 import + xref 首次解析）。两者都指向同一结论：小文件场景下
> 固定开销与对象解析单价都被放大。

### 2.2 冷/热路径的固定开销（measured）

- `import pypdf`（port）会触发 `_api_annotations.apply_module` 等 Python 层
  元数据装配（cProfile 中 0.003 s，`_imp.create_dynamic` 0.032 s——扩展加载）。
- 这些是**一次性的**；W1 的 12.6 → 34.9 ms 主要不是 import，而是 xref+对象解析。

---

## 3. W3：706.6 ms 的五段拆分（核心答案）

### 3.1 测量方法

`probe.py w3ops`（每个 W3 文件、12 个文件）：

1. `content_stream_info()`：对每页 `get_contents()` → `get_data()`（计时=**解码**）
   → `operations`（计时=**解析**，含 Token→Python 对象物化）；
2. `NamedTimers` 在类上包装 `TextExtraction.process_operation` /
   `_handle_tj` / `_handle_tj_operation` / `_post_process_text_operation`
   （Rust 通过 `slf.call_method1(name)` 调用它们，所以类级 patch 对 port 生效），
   计时=**操作符解释/派发**；
3. 于是 **剩余 = extract_text − 解析 − 解码 − 派发** = 字体/CMap 装载、
   页资源解析、`extract_text` 外层胶水；
4. `issue301` 模式另外用 `process_operation` 计时验证“解释成本”占比。

### 3.2 实测结果（12 文件合计，port）

| 阶段 | 耗时 | 占比 | 依据 |
|---|---:|---:|---|
| (a) 内容流扫描/语法解析 + Token 物化 | **363.7 ms** | **52.9%** | `probe-w3ops-rust.json` `totals.parse_s`（另一次 `probe-w3-rust` 测得 405.0 ms，两次差异来自 loadavg，区间 364–405 ms） |
| (b) 文本操作符解释/派发（`process_operation` 整体） | **303.8 ms** | **44.2%** | `totals.dispatch_s`（22,524 次调用） |
| ├ 其中字形解码 + 文本拼接（`_handle_tj`） | 125.1 ms | 18.2% | `totals.tj_s`（13,443 次） |
| ├ 其中 `_handle_tj_operation`（含上面） | 155.9 ms | 22.7% | `totals.tj_op_s` |
| └ 其它操作符（Td/Tm/T*/Tf/cm/q/Q/TJ 偏移…） | 178.7 ms | 26.0% | dispatch − 其它 handler 计时的残差（**部分 inferred**：未逐个 handler 计时） |
| (c) 字体/CMap 装载 + 页资源 + extract_text 胶水 | **20.5 ms** | **3.0%** | `totals.remaining_s` |
| (d) 内容流 Filter 解码 | **0.016 ms** | **0.002%** | `decode_s` |
| 合计（不含 reader open） | **688.1 ms** | 100% | 对应基线 706.6 ms（差值为 open 12.7 ms + 噪声） |

### 3.3 被测 vs 推断

| 结论 | 状态 |
|---|---|
| 解析占 53%（364–405 ms） | **measured**（`operations` 属性计时 + 计数器） |
| 解释/派发占 44%（304 ms） | **measured**（类级钩子计时，22,524 次） |
| 字形解码+拼接占 18%（125 ms） | **measured**（`_handle_tj` 钩子） |
| 字体/CMap 一次性装载只占 3%（21 ms） | **measured** 的残差；内部构成（`Font`/`CMap`/`ToUnicode` 各占多少）**inferred** |
| 逐 handler 细分（Td/Tm 各占多少） | **inferred**（未逐函数计时） |
| 边界调用次数（Rust→Rust 的 C-API 次数） | **not measurable**（见 §6.2），只能给单 Token 调用点计数 |

### 3.4 逐文件画像（port，ms）

| 文件 | 总 | 解析 | 派发 | `_handle_tj` | 其余 |
|---|---:|---:|---:|---:|---:|
| `test Orient.pdf`（189 KB 内容流） | 144.1 | 128.5 | 7.1 | 2.2 | 8.5 |
| `pdflatex-4-pages.pdf` | 178.2 | 63.8 | 120.7 | 51.2 | 0.0 |
| `GeoBase_…UML_EN.pdf` | 137.3 | 87.1 | 40.3 | 14.6 | 9.9 |
| `pdflatex-outline.pdf` | 105.4 | 38.1 | 67.6 | 28.5 | 0.0 |
| `multicolumn.pdf` | 81.9 | 32.9 | 53.2 | 22.7 | 0.0 |
| 其余 7 个小文件 | 41.2 | 12.1 | 14.2 | 3.3 | 2.1 |

→ **两类瓶颈并存**：向量图重的文件（`test Orient.pdf`）90% 在解析；
文本重的文件（`pdflatex-4-pages.pdf`）68% 在“每操作符 4 次边界往返”。

---

## 4. 每 MB 内容流的计数（§3.2 要求）

W3 全语料：解码内容流 **325,019 B = 0.310 MB**，24 页。

| 指标 | 每 MB |
|---|---:|
| Operation | **32,436** |
| Token（操作数） | **79,500** |
| 物化 Python PDF 对象 | **79,500**（Token 与对象 1:1，其中空数组 18,634/1.2MB） |
| `process_operation` 边界调用（Python 可见） | **72,667** |
| `_handle_tj` 调用 | **43,370** |
| `_post_process_text_operation` 调用 | **46,506** |
| `read_object` 调用（按 issue-301 比例外推） | ≈ **155,800** |
| `_check_limits` 调用（按同一比例外推） | ≈ **264,600** |
| 解析耗时 | port **1,307 ms/MB** / ref **280 ms/MB** |
| 解释+派发耗时 | port **980 ms/MB** / ref **258 ms/MB** |

> issue-301 是最密集的样本：**155,395 Token/MB、108,568 Operation/MB**
> （1,202,333 B → 186,797 Token / 130,523 Operation），
> 解析 **1,376 ms/MB**。方案 §16 关注的就是这个量级。

---

## 5. W4：图像提取的真实构成

### 5.1 语料事实（measured）

W4 的 10 个文件、16 个图像（`probe.py w4`）/ 15 个（`probe.py w4b`，inline-image 文件没有 `/XObject` 路径，单独报错），`/Filter` 分布：
`/FlateDecode` 9、`/LZWDecode` 3、`/DCTDecode` 2、`/RunLengthDecode` 1、
`/ASCII85Decode` 1、无 Filter 1（inline image）。
**没有任何 `/CCITTFaxDecode`**。→ 方案 §1.2/§8 关于“W4 被 CCITT/TIFF 解码主导”
的假设在本语料上**不成立**。

### 5.2 三段拆分（port，合计 1,892 ms）

| 阶段 | port | ref | 占比 |
|---|---:|---:|---:|
| `list(page.images)`（= W4 实际执行的动作） | **1,892.4 ms** | 693.8 ms | 100% |
| └ 纯 filter 解码（`xobject.get_data()`） | 9.3 ms | 33.7 ms | **0.5%**（port 反而更快） |
| └ Pillow 成像（`decode_as_image() − get_data()`） | 115.6 ms | 114.1 ms | 6.1%（两侧相同 → 不是回退来源） |
| └ ImageFile/列表/内容流解析开销 | **1,767.4 ms** | 546.0 ms | **93.4%** |

### 5.3 这 93% 是什么（cProfile 证据，measured）

`issue-301.pdf` 的 `list(page.images)`（port 1.72 s）：

```
1.532 s tottime  {built-in method ...datastructures_contentstream_parse_content_stream}
0.060 s         {basemod_float_new}            93,182 次
0.046 s         {basemod_number_new}           74,942 次
0.043 s         {datastructures_arrayobject_read_from_stream} 18,637 次
0.033 s / 1.756 s cum  {page_parse_images_from_content_stream}
```

即：`page.images` → `_parse_images_from_content_stream`（`page_mod/page.rs:733`）
→ **完整解析内容流**找 `/Do` 图像 XObject → 再为每个图像建 `ImageFile`。
**W4 的主热点与 W3 是同一个函数**，因此 P1 的 tokenizer 改造会同时改善 W4；
“CCITT 专项（OPT-090）”在本语料上**没有收益空间**，应改为
“`page.images` 枚举路径复用已解析 Operation”。

> 分文件：`issue-301.pdf` 1,703 ms（其中解析 1.53 s）、`jpeg.pdf` 62 ms
> （DCT 直通 + Pillow）、`cmyk-image.pdf` 44 ms（Flate 1 MB + Pillow）、
> `imagemagick-images.pdf` 62 ms（6 个小图，LZW/Flate/RunLength/DCT）。
> 即除 issue-301 外，其余文件 port/ref 差异 < 15%。

---

## 6. W5–W7：对象图拷贝 vs 序列化

### 6.1 实测拆分（port / ref，ms）

| W | read（重新解析输入） | copy（add_page/append 的对象图拷贝） | write（序列化） | 结论 |
|---|---:|---:|---:|---|
| W5 append | 29.7 / 11.1 | **36.1 / 9.1** | 5.2 / 2.3 | copy 占 51%，read 占 42% |
| W6 page_ops | 14.5 / 4.3 | **32.3 / 8.0** | 10.4 / 3.8 | copy 占 56% |
| W7 clone | 25.9 / 7.8 | **35.4 / 7.7** | 13.2 / 4.7 | copy 占 48% |

→ **不是序列化主导**：write 只占 7–18%，且绝对增量仅 3–8 ms。
回退集中在 **(1) 对象图拷贝（clone）** 和 **(2) `reader` 侧重新解析**
（`append(path)` 会重开文件）。

cProfile（port，W5，0.054 s）证实 copy 的内部仍是解析+克隆：

```
0.018 s cum  datastructures_dictionaryobject_read_from_stream
0.025 s cum  datastructures_read_object            1,446 次
0.004 s      basemod_name_read_from_stream         1,047 次
0.003 s      basemod_name_renumber                   965 次
0.015 s cum  datastructures_dictionaryobject_clone_inner
0.007 s cum  datastructures_arrayobject_clone
```

即 `clone_inner`（`data_structures_mod.rs:921`）逐键 `call_read_object` 式递归，
每个键一次 `NameObject` 物化 + `setitem`；`write_to_stream` 段几乎不可见
（< 1 ms）。

### 6.2 “不可测边界”说明

Rust 内部 `Bound::call_method*` / `PyDict::new` / `PyBytes::new` 这些
**C→C 的 `PyObject_Call`** 不会产生 profiler 事件，也无法从 Python 计数。
因此“W3 解析路径每 Token 到底发生多少次 C-API 调用”**只能从源码数**：
按 `data_structures_mod.rs:2848-2938` + `:3089-3247` 的调用点，**每个数字 Token
≈ 35–45 次 C-API 操作**（`read(1)` ×2、`seek` ×2、`eq_bytes` ×8–12、`bytes`
构造 ×8–12、`NumberPattern.search`、切片、`int()`、`FloatObject()`、
`kwargs` 建 1–2 个 dict、`content_stream_class.getattr` + 2 次字符串比较、
`shim_attr` 模块查找 + 2 次字符串比较）。**这是 OPT-010 必须补齐的观测能力**。

---

## 7. 2.5–4× 回退的归因（measured vs inferred）

以 W3（+528 ms，ref 178.5 → port 706.6）为例，用**同计数、同语料**的两侧
测量做差：

| 归因项 | 增量 (ms) | 占回退 | 状态 |
|---|---:|---:|---|
| 内容流解析（扫描+Token→Python 对象） | **+281.5** | **55.8%** | **measured**（364 vs 82 ms；issue-301 上 1.237 s / 3.96×） |
| 文本操作符解释/派发（含字形解码+拼接） | **+223.8** | **44.4%** | **measured**（304 vs 80 ms） |
| 字体/CMap/页资源 | −0.9 | −0.2% | **measured 残差**（21.4 vs 20.5 ms） |
| Filter 解码 | +0.01 | 0.0% | measured |
| reader open（xref/页树） | +2.9 | 0.6% | measured（12.7 vs 4.0 ms） |
| **算法差异** | **0** | **0%** | measured：调用次数/对象数/字节数完全一致 |
| **Python 对象物化“额外份数”** | **0** | **0%** | measured：物化次数相同（186,895 vs 186,895） |
| 内存分配/GC 差异 | 未单独测量 | — | **inferred**：物化次数相同但每对象多了 `kwargs`/切片临时对象，估计 < 10% |

**结论（对应方案 §1.1 的告诫）**：
本轮回退**不是**“跨语言回调 vs 物化”二选一，而是
**“用 Python 对象模型 + Python 对象协议执行 Rust 代码”**这一实现的**单价问题**。
所以：

- 只删 `_check_limits`/`read_object` 的 Python 往返（P1/OPT-040）**预计只消除
  解析路径的一部分**（每 Token 2 次边界判定，源码估计占解析成本 ~15–25%，**inferred**）；
- 真正的数量级收益必须来自 **OPT-050（Rust 紧凑 Token/Operation 表示）**：
  让 Token 不再以 `FloatObject`/`NumberObject`/`PyDict`/`io.BytesIO` 存在；
- 文本侧（44%）必须 **OPT-070**：把 `process_operation` 的 4 次嵌套
  `call_method1` + dict 派发改成 Rust 内部 `match operator_id`，
  并把 `self.text` 的 Python `str` 拼接改成 Rust `String`。

---

## 8. P1/P2 的具体改动点与风险（file:line）

### 8.1 P1（OPT-020/030/040）：内容流解析去 Python 对象化

| 优先级 | 位置 | 现状 | 改法 | 风险 |
|---|---|---|---|---|
| ★★★ | `data_structures_mod.rs:2848-2938` `datastructures_contentstream_parse_content_stream` | 用 `io.BytesIO` + `read_non_whitespace`/`read_until_regex`/`read(1)`/`seek` 逐 Token 驱动；每 Token 1 次 `_check_limits`、1 次 `read_object` | 新增 `rust_parse_content_stream(decoded_bytes, limits) -> Vec<Operation>`：在 `&[u8]` 上扫描（对应新文件 `crates/pypdf-rs-core/src/content_stream.rs`，已由他人起稿） | 语义等价：注释/空白/`\r\n`、`read_until_regex` 的 delimiter、`_OPERATOR_LENGTH_LIMIT`、非法操作数时的 `AssertionError`、EOF 行为 |
| ★★★ | `:2823-2845` `content_stream_check_limits` | 每 Token：`getattr("_check_limits")` + `is_default_shim_function()`（2 次 `__module__`/`__qualname__` 读 + 字符串比较）+ `__sub__` + `getattr/setattr remaining_budget` | 预算用 Rust `i64`，在 Rust 内递减；**保留** `StreamParsingBudget` 的对外可见状态（在流结束时同步一次） | 异常时机与消息必须与 `LimitReachedError("Reached maximum number of tokens per stream.")` 一致；`_configuration.stream_parsing_work_maximum_count` 语义 |
| ★★ | `:817-845` `call_read_object` | 每 Token：`shim_attr(py,"read_object")` 模块字典查找 + `is_default_shim_function` | 直接调 Rust `datastructures_read_object`；monkeypatch 兼容改为**一次**探测（缓存判定结果，按 module `__dict__` 版本失效） | 第三方/测试 monkeypatch `read_object` 的兼容性（当前靠每次身份判定兜底）；缓存判定需保证 patch 后仍走 Python |
| ★★ | `:3089-3247` `datastructures_read_object` | 数字扫描用 `read(20)` + `IndirectPattern.match` + `NumberPattern.search` + 切片 + `int()` + `FloatObject/NumberObject` 构造 | Rust 直接解析数字字面量（含整数/浮点/间接引用 `n g R`），只在**需要公开对象时**物化 | 数字精度与 `FloatObject` 的 `%.5f` 截断/`NumberObject` 整数语义；`IndirectPattern` 前瞻 20 字节的边界；`forced_encoding` |
| ★★ | `:2927-2931`（操作数分支）+ `:1446/:1488/:2930` | 每个字典键/值都 `call_read_object` | 同上的 Rust 内部递归（`ContentValue` arena） | 递归深度限额、`DictionaryObject` 重复键、`__setitem__` 的 `NameObject` 键规范化 |
| ★ | `:2996-3011` `operations` getter | `BytesIO(_data)` → `_parse_content_stream` → `_data=b""` | 新增批量入口：一次物化 `list[tuple[list[PdfObject], bytes]]`（仅在公开 API 需要时） | `_data`/`_operations` 的互斥与可变性；`isolate_graphics_state`、`set_operations` 仍需可用 |
| ★ | inline image（`pypdf.generic._image_inline.read_inline_image`，`:2900` 调用） | 逐字节 Python 循环；`BI/ID/EI` 判定 | 保留 Python 实现做 oracle，Rust 侧先用“扫描到 `ID` 后按长度/`EI` 规则” | 二进制数据里的伪 `EI`、截断、`/F`/`/DP` 参数（方案 §4.3 要求先做差分） |

### 8.2 P2/P3（OPT-060/070）：文本消费去 Python 对象

| 优先级 | 位置 | 现状 | 改法 | 风险 |
|---|---|---|---|---|
| ★★★ | `page_mod/text.rs:266-449` | `content.getattr("operations")` 得到 Python list，然后每操作符 `extractor.call_method1("process_operation", ...)` | Rust 内部直接遍历 `Vec<Operation>`，`match op_id` 分派；一次 `extract_text` 调用内不下 Python | visitor/`orientation`/layout 语义；异常顺序 |
| ★★★ | `text_extraction_mod/extractor.rs:188-215` `process_operation` | 每操作符：`handlers.contains()` + `get_item()`（Python dict）+ `call1(handler)` + 4 次 `operator.eq(bytes)` + 可能 `_post_process_text_operation` | Rust `enum OperatorId` + `match`，handler 改成 Rust `fn(&mut TextState, &[ValueId])` | `operation_handlers` 是被 Python 子类/测试观察的属性；需保留兼容外壳 |
| ★★ | `extractor.rs:825-853` `_handle_tj_operation` + `:115` `_handle_tj` | 4 层嵌套 `call_method1`（`process_operation`→handler→`_handle_tj`→`process_tj_offset`） | 合并成一次 Rust 调用；字形解码走 Rust `char_map` 视图 | `char_map`/`Encoding`/`ToUnicode` 的 malformed CMap 行为；`visitor_text` 回调点 |
| ★★ | `extractor.rs` 内 `set_attr(slf,"text",…)`、`common::concat`、矩阵为 Python list（`get_item(0..5)`/`set_item`） | 每操作符若干次 Python `str` 拼接与 list 读写 | `text`/`output` 用 Rust `String`，矩阵用 `[f64;6]`，**结束时**一次性写回 Python 属性 | 若下游读取 `self.text` 中途状态（visitor/子类），需在事件点同步 |
| ★ | `text_extraction_mod/layout_page.rs`(886) / `layout_state.rs`(744) | 同样逐操作符 Python 对象 | layout 模式可后置（W3 基准不走 layout） | `extract_text(extraction_mode="layout")` 差分 |

### 8.3 不建议改动（本轮证据支持“先别动”）

- **W4 的 CCITT/TIFF**：语料中不存在 CCITT 图像；`filter decode` 端口已比原版快
  （9.3 vs 33.7 ms）。OPT-090 应改为“复用已解析 Operation 枚举图像”。
- **Pillow 成像**：两侧 115.6 vs 114.1 ms，几乎相同，不是回退来源。
- **序列化**：W5–W7 write 只占 7–18%，先改 copy 与 read。
- **微优化（类引用缓存等）**：单次 `getattr` 的省法相对 3.9 µs/Token 的边界
  成本收益极低，除非先完成 Rust 原生表示。

---

## 9. 复现命令

```bash
cd /Users/yxy/Desktop/pypdf-rust
export PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1

# 0) 基线资产（environment/git-revision/inputs.sha256/CSV/report）
.venvs/dev/bin/python validation/optimization/scripts/make_baseline.py

# 1) issue-301 调用计数 + 解释成本（两侧）
.venvs/dev/bin/python    validation/optimization/scripts/probe.py issue301
.venvs/oracle/bin/python validation/optimization/scripts/probe.py issue301

# 2) W3 拆分（解析 / 解释 / 字体-残差），两侧
.venvs/dev/bin/python    validation/optimization/scripts/probe.py w3
.venvs/dev/bin/python    validation/optimization/scripts/probe.py w3ops
.venvs/oracle/bin/python validation/optimization/scripts/probe.py w3ops

# 3) W4 三段拆分
.venvs/dev/bin/python    validation/optimization/scripts/probe.py w4b
.venvs/oracle/bin/python validation/optimization/scripts/probe.py w4b

# 4) W5–W7 read/copy/write 拆分
.venvs/dev/bin/python    validation/optimization/scripts/probe.py w57

# 5) cProfile 证据（W1/W2/W5/W6/W7）
.venvs/dev/bin/python validation/optimization/scripts/probe_cprofile_workloads.py

# 6) harness 复核（--repeat 2，写入 baseline/raw/quick/）
.venvs/oracle/bin/python validation/perf/run.py --backend reference --repeat 2 \
    --out validation/optimization/baseline/raw/quick/perf-reference-repeat2.json
.venvs/dev/bin/python    validation/perf/run.py --backend rust      --repeat 2 \
    --out validation/optimization/baseline/raw/quick/perf-rust-repeat2.json
```

注意事项：

- 测量期间**不要**同时 rebuild 扩展或跑其它重负载；本机 loadavg 16+，
  单次数字抖动可达 20%。本报告所有比例结论都取自**同一次连续测量**的两侧对照。
- 仪器化会让 port 走 Python 兜底路径（见 `REPO_AUDIT.md` §4.4），
  所以“计数”可信、“带仪器的耗时”只能当上界；计时拆分使用**无仪器化**的
  `operations`/`extract_text` 计时 + 类级钩子（钩子本身有 ~0.1 µs/次开销，
  已在表中标注为 measured-inclusive）。

---

## 10. 附：测量窗口内二进制漂移 + P1 落地后的复测（**非冻结基线**）

### 10.1 二进制漂移（必须读）

审计期间 `python/pypdf/_pypdf.cpython-314-darwin.so` 被并发重建，观测到 **4 个
不同 sha256**：

| 观测顺序 | sha256（前 12） | 说明 |
|---|---|---|
| A（**本报告 §0–§8 全部数据**） | `08c84cb45894` | 冻结基线的构建；所有 probe 在 21:14–21:16 采集，早于任何重建 |
| B | `8b40f70d2f69` | `make_baseline.py` 二次运行时记录在 `environment.txt`/`git-revision.txt` |
| C | `6fe0ab95f3f3` | 21:20:35 重建；此时 `crates/pypdf-py/src/modules/data_structures_mod.rs` 已 +325 行 |
| D | `4cac8756bae9` | 本文 §10.2 复测时的临时构建 |

→ **§1–§8 的所有结论只对构建 A 成立**。构建 B/C/D 已包含 P1（Rust 原生内容流
扫描）的在制品，见下。

### 10.2 复测（构建 D，`probe-*-afterrebuild.json`）

| 指标（W3，12 文件） | 构建 A（冻结基线） | 构建 D（P1 在制品） | reference |
|---|---:|---:|---:|
| 解析耗时 | 363.7–405.0 ms | **107.2 ms** | 82.2 ms |
| 解析 ms/MB | 1,307 | **346** | 280 |
| `extract_text` | 663.8–688.1 ms | **409.0 ms**（无钩子）/ 436.8 ms（带钩子） | 156.7–183.6 ms |
| 操作符派发（`process_operation`） | 303.8 ms | **308.4 ms（不变）** | 80.0 ms |
| `read_object` / `_check_limits` Python 调用 | 186,895 / 317,320（issue-301） | **计数器为 0**（不再走 Python 入口） | 同 A |
| `issue-301` 解析 | 1.654 s | **0.249 s** | 0.417 s |

结论：

1. P1 的“一次调用解析整条内容流”**已经在制品中生效**：`issue-301` 解析
   1.654 s → 0.249 s（6.6×，且已快于原版 1.67×），W3 解析阶段 3.4–3.8×。
2. 因此 **W3 现在的瓶颈已经转移到文本侧**：派发 308 ms 占
   `extract_text`（409 ms）的 **75%**，其中 `_handle_tj` 127 ms。
   → **OPT-070（文本提取 Rust 原生消费）现在是 W3 的第一优先级**，
   这与 §7 的预测一致，但现在有实测支撑。
3. 该复测不进入冻结基线（构建在变、且 `data_structures_mod.rs` 仍有未提交改动）。
   一旦 lead 声明“P1 构建稳定”，应重新采集一份 `W1–W8` 并把这里的数字替换为
   正式 run。
