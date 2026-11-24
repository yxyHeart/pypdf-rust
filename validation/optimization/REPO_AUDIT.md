# REPO_AUDIT.md — P0 仓库审计（OPT-000）

> 对应 `pypdf-rust-optimization-plan.md` §0.3 / §3.1 / §16 第 1–6 项。
> 审计时间：2026-11-24 21:12–22:0x（本地时区 Asia/Shanghai）。
> 审计者：opt-baseline（子 agent）。**本次审计只读库代码；新增文件全部位于
> `validation/optimization/` 下。**

审计结论先写在前面（详细证据在后文）：

1. 本仓库的 Rust 端口**不是**“Rust 原生解析器 + Python 边界层”，而是
   **逐函数直译**：约 5.0 万行 Rust 代码用 `Bound<'py, PyAny>` 完全复刻了
   上游 Python 算法，对象模型、`io.BytesIO` 字节流、矩阵、字符串全部仍是
   Python 对象。因此每次“Python 操作”变成了“Rust → CPython C-API 调用 →
   Python 对象”，**调用次数与原版完全相同，单次成本却是原版的 3–4 倍**。
   这是 2.5–4× 回退的**首要、已被测量**的原因（§BASELINE_DIAGNOSIS 第 1 节）。
2. `read_object` / `_check_limits` 的调用次数可以被精确复现：
   `issue-301.pdf` 上 `read_object` **186,895 次**、`ContentStream._check_limits`
   **317,320 次**，与方案 §0.1 的“约 18.7 万 / 31.7 万”一致，并且两个数字
   与结构计数**完全相等**（见 §4.4）。
3. 内容流解析（`_parse_content_stream`）是 W3 与 W4 的共同主热点；W4 的
   “图像提取”在本语料上**与 CCITT/TIFF 解码无关**（语料内 0 个 CCITT 图像），
   93% 时间花在 `page.images` 触发的**内容流解析 + ImageFile 物化**上。
4. 工作区在本审计期间**不干净**：`crates/pypdf-rs-core/src/lib.rs` 被修改、
   `crates/pypdf-rs-core/src/content_stream.rs` 新增（P1 的 Rust scanner 脚手架，
   由其他 agent 写入）。基线因此以**已构建扩展的 sha256**（而非 git commit）
   作为可复现锚点，见 §1.2。

---

## 1. 版本、工作区与可复现锚点

### 1.1 Git

| 项 | 值 |
|---|---|
| HEAD | `f4d3c45faa878a286ac6c4ee34daf0a8c1cb23b1` |
| 分支 | `main` |
| `git describe` | `f4d3c45-dirty` |
| HEAD 提交时间 / 主题 | `2026-11-24T20:59:38+08:00` / `validation: refresh round-trip comparison output from re-verification run` |
| 工作区 | **不干净**（见下） |

审计开始（21:12）时 `git status --porcelain` 仅有一行
`?? pypdf-rust-optimization-plan.md`；审计过程中出现：

```
 M crates/pypdf-rs-core/src/lib.rs
?? crates/pypdf-rs-core/src/content_stream.rs
?? pypdf-rust-optimization-plan.md
?? validation/optimization/
```

即 **P1（OPT-020）的 Rust scanner 已经开始由其他 agent 落地**。由此产生两个
测量约束，必须在后续所有性能结论中保留：

- 已构建的 `python/pypdf/_pypdf.cpython-314-darwin.so` 对应
  “f4d3c45 + 别人未提交的改动”，git commit **不能**当作二进制身份；
- 性能测量期间扩展确实被重建了（至少 3 次，见下表注），因此基线冻结的是
  **每一次测量的扩展 sha256**；任何跨构建比较都必须显式标注。

### 1.2 可复现锚点（写入 `baseline/git-revision.txt`）

| 侧 | 路径 | 版本 | 扩展 sha256 |
|---|---|---|---|
| port（**审计测量所用构建 A**） | `.venvs/dev` | pypdf 7.1.1（`python/pypdf`） | `08c84cb45894531a57013f5a0e173b8a92165a219ec7e4b873479807a97f961c` |
| reference | `.venvs/oracle` | PyPI pypdf 7.1.1 | 无扩展 |

> **二进制在审计期间被并发重建（观测到 4 个 sha256：`08c84cb4` → `8b40f70d` →
> `6fe0ab95` → `4cac8756`）。** `REPO_AUDIT` §2–§6 与
> `BASELINE_DIAGNOSIS` §0–§8 的全部数据来自**构建 A**（probe 采集于 21:14–21:16，
> 早于所有重建）；`environment.txt` / `git-revision.txt` 记录的是生成它们那一刻
> 磁盘上的构建（`8b40f70d`）。重建后的构建已包含 P1 的在制品，
> 复测见 `BASELINE_DIAGNOSIS.md` §10（**非冻结基线**）。

两侧 Python 均为 **CPython 3.14.7 (main, 2026-08-05, Clang 21.0.0)**。

### 1.3 工具链

| 项 | 值 |
|---|---|
| rustc | 1.100.0 (0ef1bc386 2026-11-09) |
| cargo | 1.100.0 (3acc79c0c 2026-11-05) |
| 默认 toolchain / 已装 target | `stable-aarch64-apple-darwin` / `aarch64-apple-darwin`（**无 linux target**） |
| workspace `rust-version` | 1.83（`Cargo.toml`） |
| pyo3 | `0.30.1`，`features = ["extension-module"]`（`Cargo.toml:15`） |
| maturin | 1.16.0；`manifest-path = crates/pypdf-py/Cargo.toml`、`python-source = python`、`profile = release`（`pyproject.toml`） |
| release profile | `lto = "thin"`, `codegen-units = 16` |
| 离线构建 | 有 `vendor/` 目录；本次未执行 `cargo build`/`maturin`（按分工由 lead 重建） |

### 1.4 机器

| 项 | 值 |
|---|---|
| 机型 / 架构 | Apple M1 Pro，arm64（`Darwin 25.6.0`，macOS 26.6.2） |
| 核数 | 10 logical / 10 physical，无 NUMA |
| 内存 | 17,179,869,184 B（16 GiB） |
| glibc 等价 | 不适用（Darwin / libSystem）；部署到 Linux aarch64 需另行验证（目标环境 ABI 未在本机可得） |
| 负载 | 审计期间 `loadavg` **16.4 / 9.5 / 8.5**，属高负载共享机；`uptime` 12 天 |

> 结论：本机只适合判断**大比例差异**（≥ 20%），不能用于 5% 级的验收
> （方案 §3.3 / §12.3 已要求 ABBA 顺序与更多重复）。

---

## 2. Rust 扩展的暴露方式（谁在真正执行）

`crates/pypdf-py/src/lib.rs` 只有 27 行：`include!(OUT_DIR/modules_generated.rs)`
把 `src/modules/*` 的 `register()` 注册进 `_pypdf`；`build.rs` 负责扫描模块。

- 代码规模：`crates/` 下 58 个 `.rs`，其中 `crates/pypdf-py/src/modules/`
  约 5.0 万行；对照上游 `python/pypdf/` 的每个模块都有一个同名 `*_mod.rs`。
- 每个上游函数被拆成 **Python 类骨架 + Rust `#[pyfunction]`**：
  例如 `python/pypdf/generic/_data_structures.py:51-135` 从扩展 import 了
  **93 个** `datastructures_*` 函数，方法体只有一行 `return _xxx(...)`。
- 因此 `import pypdf` 之后，**几乎所有算法都在 Rust 里执行**，但执行方式是
  “Rust 代码通过 `Bound<PyAny>` 逐步调用 Python 对象协议”，而不是 Rust 原生
  数据结构。`crates/pypdf-py/src/modules/` 共 49,982 行；其中含
  `call_method`/`getattr`/`setattr` 的代码行有 **3,768 行**，仅
  `data_structures_mod.rs`(4,041 行) 就有 **600 行**。

### 2.1 内容流相关入口（file:line）

| 入口 | 位置 | 说明 |
|---|---|---|
| `ContentStream.__init__` | `crates/pypdf-py/src/modules/data_structures_mod.rs:2597` `datastructures_contentstream_new` | 调 `stream.get_data()` 完成 Filter 解码，把**解码后**字节 `set_data` 到新对象；数组形式 `/Contents` 在此拼接（`:2632-2677`） |
| `PageObject.get_contents` | `page_mod/page.rs:1155` `page_get_contents` | `ContentStream(resolved_object, pdf)` |
| `PageObject._parse_content_stream` | Python shim `python/pypdf/generic/_data_structures.py:754` → `datastructures_contentstream_parse_content_stream` **`data_structures_mod.rs:2848`** | **内容流解析主体** |
| `ContentStream.operations` (getter) | shim `_data_structures.py:761` → `datastructures_contentstream_operations` **`data_structures_mod.rs:2996`** | 惰性触发：`_operations` 空且 `_data` 非空 → `BytesIO(_data)` + `_parse_content_stream`，然后 `_data = b""` |
| `operations` setter / `set_data` / `write_to_stream` | `:3013` / `:2982` / `:3052` | 公开可变性语义 |
| `read_object` | shim `_data_structures.py:783/803` → **`data_structures_mod.rs:3089`** | 逐 Token 对象解析 |
| `ContentStream._check_limits` | shim `_data_structures.py:740` → **`:2789`** | 逐 Token 限额检查 |
| `ArrayObject._check_limits` | shim `:283` → **`:574`** | 数组元素限额 |

### 2.2 回调/边界点（Rust 主动回头调 Python 的位置）

| 位置 | 作用 | 频率 |
|---|---|---|
| `data_structures_mod.rs:2848` 循环体 | `utils.read_non_whitespace` / `seek(-1,1)` / `peek.isalpha()` / `read_until_regex` | 每 Token/操作符 3–5 次 |
| `:2823` `content_stream_check_limits` | 每次先 `content_stream_class.getattr("_check_limits")` + `is_default_shim_function()`（读 `__module__`/`__qualname__`，两次字符串比较）再决定直接调 Rust 还是调 Python | **每 Token 1 次（317,320 次/issue-301）** |
| `:817` `call_read_object` | 每次先 `shim_attr(py,"read_object")`（模块字典查找）+ `is_default_shim_function` 身份判定 | **每操作数 1 次（186,895 次/issue-301）** |
| `:2888 / :2904 / :2927` | BI / 操作符 / 操作数三种情形的 limit 站点 | 同上 |
| `:2930` | `call_read_object(...)` → `datastructures_read_object` | 同上 |
| `datastructures_read_object:3089` 内 | `read(1)`、`seek(-1,1)`、`eq_bytes`（每次新建 `bytes` 对象）、`IndirectPattern.match`、`NumberPattern.search`、切片、`int()`、`FloatObject()/NumberObject()` | 每 Token 约 20–40 次 C-API 往返 |

### 2.3 对象转换/物化点

- `data_structures_mod.rs:3116-3146`：`NameObject.read_from_stream`、
  `DictionaryObject.read_from_stream`、`ArrayObject.read_from_stream`、
  `BooleanObject...` 等全部通过 **Python 类对象构造**。
- `:3219-3237`：数字分支。`peek = read(20)` → `NumberPattern.search` →
  切片 → 含 `.` 走 `FloatObject(num)`，否则 `NumberObject(num)`。
- `:1446 / :1488`：字典键/值都用 `call_read_object`。
- 反向（Python→Rust 传参）同样昂贵：`py_int/py_bytes/py_str` 每次包装一个
  Python 对象；`kwargs = PyDict::new(py); kwargs.set_item(...)` 在热路径上
  每 Token 建 1–2 个 dict。

---

## 3. 内容流解码字节从哪来

链路（实测确认，见 `probe-issue301-rust.json`）：

1. `PageObject.get_contents()`（`page_mod/page.rs:1155`）取 `/Contents` 原始
   `EncodedStreamObject`，构造 `ContentStream`。
2. `ContentStream.__init__`（`data_structures_mod.rs:2597`）调
   `stream.get_data()` → `StreamObject.get_data` 跑 Filter 链
   （Flate/LZW/ASCII85/RunLength/DCT 等，实现在
   `crates/pypdf-py/src/modules/filters_mod.rs`，1,622 行），**解码后**字节由
   `StreamObject.set_data` 写入新对象的 `_data`。
3. 若 `/Contents` 是数组：`:2632-2677` 逐个解码、用 `\n` 拼接、并执行
   `array_based_stream_maximum_output_length` 限额。
4. `ContentStream.operations`（`:2996`）在首次访问时把 `_data` 包成
   `io.BytesIO` 交给 `_parse_content_stream`，解析完成后 `_data = b""`。
   也就是说 **`_data` 在解析前是“已解码、未解析”的字节，之后被清空，改由
   `_operations` 承载内容**；`get_data()` 在 `_data` 为空时会从 `_operations`
   重新序列化（`:2939`，逐 operand `write_to_stream`）。

实测（issue-301.pdf，原始文件 122,438 B）：`_data` 长度 **1,202,333 B**，
`/Filter` 为 `None`，`get_data() is _data` → 解码发生在 `get_contents()`，
**不是**在 parser 内部。`decode_seconds` 在 W3 全语料仅 **0.016 ms**（Flate
解压很快，且 pypdf 只在需要时解码）。

---

## 4. `issue-301.pdf` 调用链与计数复现

### 4.1 调用链

```
PdfReader("resources/issue-301.pdf")            # 22 次 read_object（xref/trailer/页树）
└─ pages[0]
   ├─ get_contents()                            # ContentStream(...) → get_data() 解码 1,202,333 B
   └─ extract_text()                            # _page.py:1074 → page_extract_text (Rust)
      └─ page_extract_text_inner (page_mod/text.rs:179)
         ├─ :212 字体资源装载（/Resources → /Font → Font 对象、CMap/ToUnicode）
         ├─ :266 content.getattr("operations")  # ← 触发解析
         │   └─ datastructures_contentstream_operations (:2996)
         │       └─ _parse_content_stream (:2848)
         │           for each token:
         │             content_stream_check_limits (:2823) → _check_limits (:2789)
         │             call_read_object (:817) → datastructures_read_object (:3089)
         │               └─ FloatObject/NumberObject/NameObject/ArrayObject 构造
         └─ :267 for (operands, operator) → :449 extractor.process_operation(operator, operands)
             └─ text_extraction_mod/extractor.rs:188 → handler dict 派发（call1）
                 └─ _handle_tj_operation (:825) → _handle_tj（字形解码 + 文本拼接）
```

### 4.2 计数方法（可复现）

`validation/optimization/scripts/probe.py issue301`：在 Python 层替换
`pypdf.generic._data_structures.read_object` 与
`ContentStream._check_limits` 为计数包装器，用**深度计数**区分顶层/递归调用，
并按 `type(value).__name__` 统计物化类型。

复现命令：

```bash
PYTHONHASHSEED=0 .venvs/dev/bin/python      validation/optimization/scripts/probe.py issue301
PYTHONHASHSEED=0 .venvs/oracle/bin/python   validation/optimization/scripts/probe.py issue301
```

### 4.3 结果（两侧完全一致）

| 计数器 | port (`.venvs/dev`) | reference (`.venvs/oracle`) |
|---|---:|---:|
| `read_object` 总调用 | **186,895** | **186,895** |
| ├ 顶层（depth 0） | 186,797 | 186,797 |
| └ 递归（depth ≥ 1） | 98 | 98 |
| 最大递归深度 | 1 | 1 |
| `ContentStream._check_limits` | **317,320** | **317,320** |
| `ArrayObject._check_limits` | 98 | 98 |
| `operations` 数 | 130,523 | 130,523 |
| 操作数 Token 数 | 186,797 | 186,797 |
| 解码内容流字节 | 1,202,333 | 1,202,333 |

返回类型分布（两侧一致）：`FloatObject` 93,182、`NumberObject` 74,936、
`ArrayObject` 18,634（全是空数组，如 `[] 0 d` 的 dash 数组）、
`TextStringObject` 90、`NameObject` 53。

### 4.4 一致性验证（回答“是否递归重复计数”）

- **无重复计数**：`top_level = 186,797` **恰好等于**
  `sum(len(operands) for operands, _ in operations)`，即“每个操作数 Token
  恰好一次顶层 `read_object`”。
- **317,320 = 130,523（操作符）+ 186,797（操作数）**，恰好等于
  `operations + tokens`。即 `_check_limits` 对**每个 Token 和每个操作符各调用
  一次**（`BI` 内联图像例外为 2 次）。
- 递归只来自 98 个**非空数组**（README 侧读取页树/字体字典时）；
  内容流里的 18,634 个数组全部为空，因此不产生递归。
  结论：方案 §0.1/§16 第 4 项担心的“递归重复计数”**不存在**，
  18.7 万/31.7 万是**真实逻辑调用次数**。
- 仪器化代价（同一进程内）：`operations` 1.654 s（无包装）→ 1.973 s（包装），
  `extract_text` 1.810 s → 2.088 s。包装只是把 port 的“内部快路径”强制改走
  Python 入口，所以**计数准确、时间只能当上界**。

---

## 5. 文本提取 / 图像 / Writer / Clone 入口

| 能力 | 公开入口（shim） | Rust 实现 | 关键下游 |
|---|---|---|---|
| 文本提取 | `_page.py:1074 extract_text` → `:958 _extract_text` | `page_mod/text.rs:56 page_extract_text`、`:179 page_extract_text_inner`、`:479 page_extract_text_xform` | `page_mod/text.rs:266` 取 `operations`；`:449` 每操作符 `process_operation`；`text_extraction_mod/extractor.rs` 全部状态机 |
| 布局模式文本 | `_page.py` layout_mode 分支 | `text_extraction_mod/layout_page.rs`（886 行）、`layout_state.rs`（744 行） | `page_mod/text.rs:662-668` |
| 图像枚举/提取 | `_page.py:images` property（`VirtualListImages`）、`:571 _get_image` | `page_mod/page.rs:443-760`（`_get_ids_image`、`_parse_images_from_content_stream`、`page_get_image`）、`page_mod/imagefile.rs` | `_parse_images_from_content_stream`（`page.rs:733`）会**再解析一次内容流**找 `/Do`；`stream.decode_as_image`（`data_structures_mod.rs`，`StreamObject.decode_as_image`）→ Pillow |
| 过滤器解码 | `filters.py` | `filters_mod.rs`（1,622 行，含 LZW/CCITT/ASCII85/RunLength/Flate） | `utils_mod/streams.rs` |
| Writer | `_writer.py:137` `import pypdf._pypdf as _writermod`；`PdfWriter.write` → `_writer.py:395` | `writer_mod.rs` | 序列化走 `datastructures_*_write_to_stream` |
| Clone | `_writer.py` `add_page`/`append` → `PageObject` clone | `data_structures_mod.rs:369 arrayobject_clone`、`:892 dictionaryobject_clone`、`:2737 contentstream_clone`、`page_mod/transformation.rs` | clone 后写盘 |

---

## 6. A1 / 差分 / Round-trip 门禁：真实命令与覆盖

| 门禁 | 命令（仓库现有） | 计数 | 本次是否快速验证 |
|---|---|---|---|
| **A1 官方测试** | `./scripts/gate.sh`（内部：`.venvs/dev/bin/python -m pytest tests -n auto -q -p no:benchmark --junitxml=.audit/rust-junit.xml` 然后用 `.venvs/oracle/bin/python tools/baseline/snapshot.py .audit/rust-junit.xml --compare .audit/baseline-outcomes.json` 逐用例比对） | `pytest tests --collect-only` → **2,724 用例**；文档记录 `2691 passed / 25 skipped / 8 xfailed` = 2,724 | ✅ `pytest tests/test_generic.py -q -p no:benchmark` → **206 passed in 7.90s** |
| **103 场景差分** | `PYTHONHASHSEED=0 .venvs/oracle/bin/python tools/oracle/audit.py --out .audit/oracle --seed 42`（port 同理 → `.audit/rust`）再 `tools/oracle/compare.py .audit/oracle .audit/rust` | `audit.json["scenarios"]` = **103**（读/加密读/文本/写 13 类） | ✅ dev 侧 `audit.py --seed 42` 完整跑完，`scenarios=103`，输出在 `baseline/raw/smoke/audit-dev/` |
| **150 Round-trip** | `.venvs/dev/bin/python validation/roundtrip.py --backend dev`（默认 `--limit 25`，6 个 workflow：RW/MERGE/TRANSFORM/EDIT/ENCRYPT/INCREMENTAL）→ 25×6=**150**；`.venvs/oracle/...` 同理；`validation/compare_roundtrip.py` 比对 | `validation/results/compare-roundtrip.json`: `common_cases=150`, `diff_count=0`, `status=MATCH` | ✅ `--limit 1 --workflows RW` 两侧各 1 例 + `compare_roundtrip.py` → `RESULT: MATCH`（`raw/smoke/roundtrip-compare.json`，exit 0） |
| 图像字节一致性 | `./scripts/probe_image_parity.sh`（A5） | — | ❌ 未跑（按时间预算跳过） |
| 符号覆盖 | `tools/manifest/check.py --require 100`（A6） | — | ❌ 未跑 |

> 注意：`scripts/gate.sh` 第一步会 `./scripts/build.sh` → **会 rebuild 扩展**，
> 与本次分工（lead 负责重建）冲突，因此本审计**没有**执行 `gate.sh`，只跑了
> 上表“本次是否快速验证”列中的子命令。

---

## 7. 目标环境与离线构建约束

- **当前构建/运行环境**：macOS 26.6.2 / arm64 / CPython 3.14.7 /
  `aarch64-apple-darwin` / pyo3 0.30.1 / maturin 1.16.0。
- **部署目标（方案 §17 第 10 项）**：Linux aarch64。当前 rustup **只装了
  `aarch64-apple-darwin`**，没有 `aarch64-unknown-linux-gnu`，也没有 Linux
  容器/交叉链接器；`glibc` 版本无法在本机确定。→ **目标环境验证目前是
  BLOCKED**，需要在有 Linux/aarch64 机器时补做（构建 + `pytest tests -q` +
  W1–W8 子集）。
- **离线**：`vendor/` 存在，`~/.cargo` 已有 registry 缓存痕迹，但本次未验证
  `cargo build --offline`；网络不可用（按环境约束）。任何新依赖都必须先
  vendor，否则构建会失败。
- Python 侧依赖：`.venvs/{dev,oracle}` 已就绪；runner 需 `Pillow`（W4）与
  `pytest-xdist`（`-n auto`）。

---

## 8. 审计产出的可复现资产

```
validation/optimization/
├── REPO_AUDIT.md                 # 本文件
├── BASELINE_DIAGNOSIS.md         # 归因报告（§3.4/§16）
├── PROGRESS.md                   # OPT-000..OPT-110 跟踪表
├── scripts/probe.py              # stages / w3 / w3ops / w4 / w4b / w57 / issue301 / cprofile
├── scripts/probe_cprofile_workloads.py
├── scripts/make_baseline.py      # 生成 environment/git-revision/inputs.sha256/CSV/report
└── baseline/
    ├── environment.txt  git-revision.txt  inputs.sha256  commands.md
    ├── raw/W1.csv … W8.csv                  # 现有 10 次重复 run 的原始值（逐 repetition）
    ├── raw/quick/perf-{reference,rust}-repeat2.json   # 本次 harness 复核（repeat=2）
    ├── raw/probe-*.json                     # 本次所有测量原始数据（两侧）
    ├── raw/cprofile-*.txt                   # cProfile 证据（W1/W2/W5/W6/W7，两侧）
    ├── raw/smoke/                           # 差分/round-trip 冒烟输出
    └── reports/baseline-report.md           # 基线中位数与 speedup 表
```

`inputs.sha256` 覆盖 W1–W8 全部 **52** 个输入文件（去重后）。
