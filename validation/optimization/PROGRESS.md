# PROGRESS.md — OPT 任务跟踪表（方案 §13）

> 状态说明：`DONE` 已完成并有可复现证据；`PARTIAL` 部分完成（缺项写明）；
> `TODO` 未开始；`BLOCKED` 有外部阻塞。
> 每个 `DONE/PARTIAL` 都指向 `validation/optimization/` 内的证据文件。
> 更新时间：2026-11-24（P0 阶段审计结束）。

## 0. 总览

| 任务 | 名称 | 状态 | 负责人 | 证据 / 备注 |
|---|---|---|---|---|
| OPT-000 | 审计和冻结基线 | **DONE** | opt-baseline | `REPO_AUDIT.md`、`BASELINE_DIAGNOSIS.md`、`baseline/`（environment/git-revision/inputs.sha256/commands.md/raw W1–W8/report） |
| OPT-010 | 增加性能计数器和阶段计时 | **PARTIAL**（Python 可见部分完成；扩展内计数器缺失） | opt-baseline | `scripts/probe.py`、`baseline/raw/probe-*.json`、`cprofile-*.txt`；缺 PyO3 入口级计数器（需改库代码，P0 不允许） |
| OPT-020 | Rust Scanner 基础语法 | **IN PROGRESS（他人）** | 其他 agent | 工作区已出现 `crates/pypdf-rs-core/src/content_stream.rs` + `data_structures_mod.rs` +325 行；重建后的构建已把 `issue-301` 解析从 1.654 s 降到 0.249 s（`BASELINE_DIAGNOSIS.md` §10） |
| OPT-030 | Inline Image 和资源限制 | TODO | — | 约束：必须先做 `BI/ID/EI` 差分（方案 §4.3） |
| OPT-040 | 批量 PyO3 接口 | **IN PROGRESS（他人，已见效）** | 其他 agent | 重建后 `read_object`/`_check_limits` 的 Python 入口计数变为 **0**，W3 解析 107 ms（原 364–405 ms）；改动点见 `BASELINE_DIAGNOSIS.md` §8.1 |
| OPT-050 | 紧凑 Rust Token/Operation 表示 | TODO | — | 依据 `BASELINE_DIAGNOSIS.md` §1：这是数量级收益的关键 |
| OPT-060 | Python 兼容物化 | TODO | — | — |
| OPT-070 | 文本提取 Rust 原生消费 | TODO | — | 改动点：`page_mod/text.rs:266-449`、`text_extraction_mod/extractor.rs:188-215`、`:825` |
| OPT-080 | Reader / 页树优化 | TODO | — | 已给出 W1/W2 归因（`BASELINE_DIAGNOSIS.md` §2.1） |
| OPT-090 | 图像专项 | TODO（**范围需要修正**） | — | 实测：W4 语料无 CCITT/TIFF 图像；主热点是内容流解析 → 应改为“复用已解析 Operation 枚举图像”，见 §5.1 |
| OPT-100 | Writer / Clone 优化 | TODO | — | 已给出 W5–W7 归因：copy 48–56%、read 36–42%、write 7–18% |
| OPT-110 | 最终验收与发布 | TODO（**目标环境 BLOCKED**） | — | 本机只有 `aarch64-apple-darwin`，无 Linux/aarch64 target 与 glibc 信息 |

## 1. OPT-000：审计和冻结基线（DONE）

方案要求的 5 个 checkbox：

- [x] 核查当前分支、commit、工作区和版本 → `REPO_AUDIT.md` §1
      （HEAD `f4d3c45`，**工作区 dirty**：P1 脚手架 + 本目录）
- [x] 找到 W1–W8 的真实入口和报告 → `REPO_AUDIT.md` §2（`validation/perf/run.py`
      的 `OPS` 表 + `workloads.json`；`compare.py` 做输出字节/SHA-256 比对）
- [x] 固化输入样本哈希和环境信息 → `baseline/inputs.sha256`（52 个文件）、
      `baseline/environment.txt`、`baseline/git-revision.txt`
- [x] 复跑当前基准 → **PARTIAL**：`--repeat 2` 复核（`baseline/raw/quick/`）+
      复用已有 10 次重复记录；**未**重跑完整 10 次（本机 loadavg 16+，
      且需与 lead 的重建排期错开）
- [x] 输出 `REPO_AUDIT.md` 和 `BASELINE_DIAGNOSIS.md` → 本目录

**完成标准评估**：代码路径、测试命令、环境均可复现（已实测 A1 子集 / 103 场景
audit / round-trip 冒烟）；性能报告采用**冻结的已有 run** + 显式标注的复核 run。
→ **PASS（带一条限制：完整 10 次重跑待低负载窗口）**

## 2. OPT-010：计数器与阶段计时（PARTIAL）

- [x] 每个内容流长度、Token 数、Operation 数 → `probe.py w3`/`w3ops`
      （W3：325,019 B / 24,642 Token / 10,054 Op；issue-301：1,202,333 B /
      186,797 Token / 130,523 Op）
- [x] Python callback 数 → `NamedTimers`：W3 上 22,524 次 `process_operation`、
      13,443 次 `_handle_tj`、14,415 次 `_post_process_text_operation`；
      `read_object` 186,895 / `_check_limits` 317,320（issue-301）
- [ ] PyO3 入口级边界计数（`#[pyfunction]` 进入次数）→ **未做**：
      需要在 `pyutil`/`register` 层加计数器，属于库代码改动，P0 分工禁止。
      当前只给出“源码级每 Token 35–45 次 C-API”的**估计**（见诊断 §6.2）
- [x] Python PDF 对象物化数量与类型 → issue-301 按返回类型统计
      （Float 93,182 / Number 74,936 / Array 18,634 / TextString 90 / Name 53）
- [x] W3 拆分 parser/物化/消费 → 53% / 44%（含字形+拼接 18%）/ 3%
- [x] W4 拆分 filter/解码/转换 → 0.5% / 6.1% / 93.4%（ImageFile+解析）
- [x] W5–W7 拆分 clone/对象图/序列化 → copy 48–56% / read 36–42% / write 7–18%

**完成标准评估**：报告能用计数器解释 profile（cProfile 与计数器互相印证，
例如 issue-301 的 130,523 operations ↔ cProfile 的 `process_operation` 次数）；
**但“边界调用计数”只到 Python 可见层**。→ **PARTIAL（差 PyO3 入口计数器）**

## 3. 进入下一阶段的门禁（方案 §3.4 P0 门禁）

| 门禁项 | 结论 | 说明 |
|---|---|---|
| 数据资产可复现 | **PASS（带限制）** | 输入 sha256、环境、命令、原始 per-rep 值齐全；完整 10 次重跑待低负载窗口 |
| 计数器与 profiler 结论一致 | **PASS** | 见 `BASELINE_DIAGNOSIS.md` §3.3/§5.3/§6.1 |
| 每个 W 项至少有粗粒度阶段拆分 | **PASS** | W1–W8 见诊断 §2.1；W3/W4/W5–W7 有细拆 |
| 核心问题（706.6 ms 的五段拆分）可回答 | **PASS（含 inferred 标注）** | 解析 53%、解释 44%（字形+拼接 18%）、字体/CMap 3%、解码 ~0%、边界次数仅估计 |
| 目标 aarch64/Linux 环境 | **BLOCKED** | 本机无 Linux target/容器；`glibc` 未知 |

→ **建议：允许进入 OPT-020/030/040（P1）**，但要求：
1. lead 在低负载窗口补一次完整 10 次重复并把 JSON 覆盖进
   `validation/optimization/baseline/raw/`（或更新 `validation/results/` 后重跑
   `make_baseline.py`）；
2. OPT-010 的 PyO3 入口计数器作为 P1 的第一个 commit 落地（否则 P1 收益无法
   从“边界次数”维度归因）。

## 3.1 P1 已部分落地的实测影响（21:20 之后的重建）

- `issue-301.pdf` 解析 **1.654 s → 0.249 s**（6.6×；原版 0.417 s，已快 1.67×）；
- W3 解析 **364–405 ms → 107 ms**（解析 ms/MB：1,307 → 346；原版 280）；
- W3 `extract_text` **664–688 ms → 409 ms**；
- **但文本派发仍是 308 ms 不变**，已占 `extract_text` 的 **75%**
  → **OPT-070（文本提取原生消费）现在是 W3 的第一优先级**，与
  `BASELINE_DIAGNOSIS.md` §7/§10 的归因一致。

> 这些数字来自构建 D（`4cac8756`），**不属于冻结基线**；构建仍在变，
> 需要 lead 在其宣布稳定后重采一次 W1–W8。

## 4. 需要 lead 决策的三件事

1. **W4 的 OPT-090 范围修正**：语料无 CCITT/TIFF；建议改为
   “`page.images` 复用已解析 Operation / 避免二次解析内容流”，并把 CCITT 专项
   推迟到有对应样本后。
2. **基线二进制漂移**：P1 代码已开始进入 `crates/pypdf-rs-core/`，一旦 rebuild，
   现有 `.so`（sha256 `08c84cb4…f961c`）就不再对应源码。建议在 P1 首个 commit
   前先记录一次“干净树 + 现 .so”的对照 run。
3. **目标环境**：需要一台 Linux aarch64（或交叉工具链 + glibc 版本）才能完成
   §17 第 10 项；当前状态 BLOCKED。

## P1/P2 implementation result (lead, 2026-11-24)

Implemented the first two plan items directly:

**P1 (OPT-020…OPT-040, DONE)** — pure-Rust byte-level content-stream parser.
- New `crates/pypdf-rs-core/src/content_stream.rs` (13 unit tests: grammar, strings,
  hex, arrays/dicts, inline-image position, limits, fuzz-no-panic).
- `contentstream_parse_fast` in `data_structures_mod.rs`: one `stream.read()`, then
  Rust scan + one materialisation per operand; Python inline-image reader kept for
  `BI/ID/EI`; `PYPDF_RS_LEGACY_CONTENT_STREAM=1` rolls back.
- Semantics kept: `read_non_whitespace`/`read_until_regex` chunked length limits
  (240-vs-128 message), per-stream vs nested budget messages, partial `_operations`
  on aborted streams, duplicate-dictionary-key warnings, `NameObject.unnumber`
  (leading `/` retained), bytes `repr` in error messages, BytesIO negative-seek clamp.

**P2 (OPT-070, PARTIAL)** — text extractor dispatch.
- `process_operation` now `match`es operator bytes → Rust handlers (Python handler
  table still honoured if replaced; `_rs_default_handlers` identity check).
- `page_mod/text.rs` calls the Rust `process_operation` / `process_tj_offset`
  directly instead of through `call_method1`.

**Measured (10 workload harness, `--repeat 5`, same machine):**

| workload | baseline port (before P1) | after P1 | after P1+P2 |
|---|---|---|---|
| W1 | 0.36× | 0.34× | 0.33× |
| W2 | 0.40× | 0.29× | 0.30× |
| W3 | 0.25× | 0.38× | 0.39× |
| W4 | 0.33× | 1.39× | 1.40× |
| W5 | 0.29× | 0.28× | 0.29× |
| W6 | 0.28× | 0.29× | 0.26× |
| W7 | 0.29× | 0.27× | 0.27× |
| W8 | 0.33× | 0.33× | 0.34× |

`issue-301.pdf` extraction: 1.75 s → 0.31 s (reference 0.48 s; now 1.55× faster).
A1 stays `2691 passed / 25 skipped / 8 xfailed` (baseline MATCH), differential
`IDENTICAL`.

**Next (P0 diagnosis still valid):** W3/W8 remaining cost is the text consumer
(`get_text_operands` / `get_display_str` / `_flush_text` shim round-trips per
operation, ~300 ms of W3), then W1/W2 reader object parsing and W5–W7 object-graph
copy (48–56 % of W5–W7).

## P4 implementation result (lead, 2026-11-25)

**P4 (OPT-050/OPT-060, DONE for object syntax)** — byte-level `read_object`.
- `content_stream::parse_object` + `read_object_fast`: one chunked `read()` per
  object (256 B, ×8 on truncation), Rust grammar scan, one materialisation per
  operand, single `seek` back to the consumed offset; any uncertainty (stream
  objects, strict readers, indirect refs without `pdf`, malformed input) falls
  back to the legacy per-token parser; `PYPDF_RS_LEGACY_READ_OBJECT=1` rolls back.
- Fidelity pinned by new core tests: partial-buffer `Truncated`, table/stream
  boundary detection, bytes `repr` in errors, duplicate-key warning positions
  (absolute offsets), `IndirectPattern` assertion, deep nesting
  (`MAX_PARSE_DEPTH = 400` → upstream's `PdfReadError("Error reading PDF file.")`).
- Text extraction: `_handle_tj` / `get_text_operands` / `get_display_str` are now
  called as Rust functions instead of through Python shims; configuration read via
  `CURRENT_CONFIGURATION.get()` (no Python frame per object).

**Measured (`run_perf.sh --repeat 5`, same machine):**

| W | before P4 | after P4 |
|---|---|---|
| W1 | 0.33× | **0.71×** |
| W2 | 0.30× | **0.62×** |
| W3 | 0.39× | **0.46×** |
| W4 | 1.39× | **1.39×** |
| W5 | 0.29× | **0.39×** |
| W6 | 0.26× | **0.46×** |
| W7 | 0.27× | **0.42×** |
| W8 | 0.34× | **0.62×** |

Gates: A1 baseline MATCH, differential IDENTICAL, roundtrip PASS, security 57/57
PASS (including the deep-nesting reject cases), fuzz 200/200 IDENTICAL.

**Next targets:** writer clone/serialisation (−58 % remains: per-entry
`replicate`/`setitem`/`write_to_stream` crossings in `dictionaryobject_clone_inner`
and `dictionaryobject_write_to_stream`), text display per-character
`character_map.get` crossings, page-tree/box reads.

## Constraint discovered (2026-11-25): call-count tests pin clone/parse frames

`tests/test_generic.py::test_cloning_direct_objects__function_calls_and_exceptions`
asserts the exact number of Python frames per 1000 cloned elements (dictionary:
4200). Replacing the per-entry `items()` / `NameObject(key)` / `__setitem__`
calls with C-level `PyDict` iteration reduced the measured frames to 3200 and
failed A1 — the optimization was reverted. `test_read_array_of_numbers_and_references__function_calls`
similarly pins array parsing (<3 calls/element).

Consequences for the remaining writer work (W5–W7):
- Optimise **serialisation** (`write_to_stream`) instead of `clone`: no upstream
  test pins the frame count of the writer's byte emission, but output bytes must
  stay identical (A2/roundtrip gate).
- Any reader/clone change must be checked against these two tests first.

## P6 + reader header + text helpers (lead, 2026-11-25, round 2)

**P6 writer** — `render_pdf_object` / `render_leaf_bytes` compose the exact
`write_to_stream` bytes for plain `ArrayObject`/`DictionaryObject` trees and write
once per object; unsupported nodes (streams, subclasses, big ints, cycles > 200)
fall back to the per-entry writer unchanged. Applied to
`dictionaryobject_write_to_stream` / `arrayobject_write_to_stream`.

**Reader header** — `read_object_header` gained a buffered parser: one 512 B read
(grows ×8 while the buffer can't decide), header grammar (comment/whitespace/
idnum/generation/"obj"/trailing byte) reproduced byte-for-byte including the
`Token exceeds maximum length of 64 bytes.` warning/limit and
`Invalid indirect object reference (...)` message with cause. This removes ~18
Python-level stream calls per header (21 270 headers per W2 run).

**Text helpers** — `mult`/`orient` are called as Rust functions unless the module
attribute was replaced (identity check keeps monkeypatching working);
`get_text_width` stays a Python call because
`tests/text_extraction/test_text_extraction_cache.py` monkeypatches it (reverted
after A1 caught it).

**Measured (`run_perf.sh --repeat 5`):**

| W | before round 2 | after round 2 |
|---|---|---|
| W1 | 0.69× | 0.71× |
| W2 | 0.61× | **0.85×** |
| W3 | 0.45× | 0.45× |
| W4 | 1.42× | **1.46×** |
| W5 | 0.46× | 0.47× |
| W6 | 0.47× | 0.48× |
| W7 | 0.43× | 0.46× |
| W8 | 0.53× | **0.64×** |

Gates: A1 MATCH, differential IDENTICAL, roundtrip PASS after each step.

**Lessons:** `tests/text_extraction/test_text_extraction_cache.py` pins
`font.get_text_width` call counts and instance monkeypatching; the clone tests pin
frame counts. Optimisations must check these three tests first.

**Next targets:** W3 (text consumer remains 2.2× slower: per-character width
lookups + `_handle_tj` dispatch), W1 (small-file reader overhead), W5–W7 (clone
dominates and its frame count is pinned; focus on `_add_page`/`_add_object`).

## Text path analysis (lead, 2026-11-25, round 3)

Removed the remaining shim round-trips in the text path: `compute_str_widths`,
`crlf_space_check`, `mult`, `orient` are called as Rust functions (identity
checks keep monkeypatching working), string operands use
`genutils_create_string_object` directly, and width accumulation runs in Rust
(`text_widths_fast`, per-character cache + f64 sum, falling back to the Python
loop for exotic numeric types). W3: 391 ms → 384 ms (0.46×).

**Where W3's remaining 2.2× lives** (measured with `sample` on the extraction
loop, no-XObject variant, 13 534 operations / 33 977 Tj characters / 38 pages):

- XObject forms: 19 `Do` forms account for ~320 ms of the 810 ms portable run
  (reference ~135 ms of 343 ms) because every form re-runs
  `_get_content_stream_for_text` + `Font.from_font_resource`.
- Plain text ops without forms: port 490 ms vs reference 205 ms, and parse is
  134 ms vs 112 ms — i.e. **interpretation is ~3.8× slower**, and no single
  function dominates: `post_process_text_operation`/`crlf_space_check` (93/2011
  samples each), `common::mult` 66, `common::init_attr` 43, `sqrt_of` 23,
  `is_instance_builtin` 17, `slice_of` 12.

The text state machine is driven by Python lists/tuples/objects: every matrix
element, character and attribute access is a PyO3 call, while CPython executes
the reference implementation in bytecode. A step change needs the planned P3
native consumer (a Rust `[f64; 6]`-based extraction core fed by the Rust-parsed
operations), not more micro-optimisations. XObject handling is the next cheapest
target: cache the form's content stream + font resources per form id.

## W3 step change (lead, 2026-11-25, round 4)

Native `crlf_space_check` (`crlf_space_check_native`) for the default
`mult`/`orient`: the per-positioning-operator pipeline now keeps the affine
matrices as `[f64; 6]`, does the matrix products/orientation/scale/distance math
in Rust with the same IEEE operation order, and builds Python values only for the
five returned items (text, output, cm/tm copies, line span). The object-based
implementation stays as the fallback (monkeypatched `crlf_space_check`, non-float
matrices, exotic numeric types) behind `PYPDF_RS_LEGACY_CRLF=1` for rollback.

`text_widths_fast` now iterates code points of the operand `str` natively and only
creates a Python character on a cache miss for `font.get_text_width`.
`character_map.get` uses the C-level dict API.

W3: 384 ms → 311–348 ms (0.56×, repeated runs); reference 176–179 ms.

**Bug caught by A1:** the first version used `?` when calling the native
function, so `OrientationNotFoundError` bypassed the `try/except` that upstream
has inside `_post_process_text_operation`; `test_orientations` failed. The native
error must be routed through the same `match result { Err(...) }` handler as the
object-based call.

**Measurement noise:** the repeat-7 harness runs on a loaded machine; small
workloads (W5–W7, ~12–20 ms reference) swing by ±25 % between runs. W3/W4 are the
stable ones (±3 %).

## Cached Python lookups (lead, 2026-11-25, round 5)

`pyutil::cached_module` / `cached_attr` / `cached_attr2` keep imported module
objects and class/constant references in process-wide caches (leaked on purpose:
`Py` values must not be dropped after interpreter finalisation). Wired into the
hot helpers of `data_structures_mod` (`shim`, `base_class`, `utils_module`,
`generic_utils_module`, `constants_class`, `configuration`, `_SCALAR_TYPES`),
`base_mod`, `reader_mod` and `writer_mod`; `configuration()` now reads
`CURRENT_CONFIGURATION.get()` directly.

Clone internals also reuse cached classes, skip `ignore_fields.contains()` when
the list is empty, and check `PdfObject` instead of `hasattr("clone")` per entry
(C-level only, so the call-count tests keep passing).

**Measured (`run_perf.sh --repeat 7`):**

| W | before | after |
|---|---|---|
| W1 | 0.74× | **0.95×** |
| W2 | 0.80× | **1.16×** |
| W3 | 0.56× | **0.66×** |
| W4 | 1.39× | **1.69×** |
| W5 | 0.35× | 0.47× |
| W6 | 0.48× | 0.58× |
| W7 | 0.47× | 0.55× |
| W8 | 0.57× | **0.76×** |

Gates: A1 MATCH, differential IDENTICAL, roundtrip PASS, security 57/57 + fuzz
200/200 IDENTICAL.

## Builtins/module caching sweep + native matrix mult (lead, 2026-11-25, round 6)

- `py.import("builtins")` in every module-level `builtins()` helper is now a
  cached lookup (base, cmap, data_structures, doc_common, encryption, filters,
  font, genutils, reader, writer, text extraction).  This removed the largest
  remaining chunk of per-call C work in object cloning, text handlers and
  filters.
- `common::mult` uses a native `[f64; 6]` product when both operands are lists of
  floats (generic Python-arithmetic path kept for ints/other types).
- `process_operation` borrows the operator bytes instead of allocating a `Vec`
  per operation, and the fast dispatch no longer re-checks `contains` when the
  handler table is the default one.
- `_post_process_text_operation` computes the effective space width natively when
  `font_size`/`_space_width`/`_previous_font_space` are floats.

**Measured (`run_perf.sh --repeat 7`):**

| W | round 5 | round 6 |
|---|---|---|
| W1 | 0.95× | **1.20×** |
| W2 | 1.16× | **1.27×** |
| W3 | 0.66× | **0.77×** |
| W4 | 1.69× | **1.90×** |
| W5 | 0.47× | 0.66× |
| W6 | 0.58× | 0.73× |
| W7 | 0.55× | 0.66× |
| W8 | 0.76× | **0.89×** |

Gates: A1 MATCH, differential IDENTICAL, roundtrip PASS, security 57/57 + fuzz
200/200 IDENTICAL.  Note: the reported +38 % RSS on W5–W7 is carried-over
allocator state from W4 (working-set growth per repetition is ≤0.2 MB), not a
leak.

## Import-cache sweep + dict item access (lead, 2026-11-25, round 7)

- Every inline `py.import("pypdf…")` / `py.import("builtins")` in
  `crates/pypdf-py/src/modules/**` now goes through `pyutil::cached_module`
  (36 files, ~160 call sites).
- `DictionaryObject.__getitem__` / `__setitem__` implementations use the C-level
  `PyDict` API (the KeyError is rebuilt via `builtins.KeyError(key)` so the
  message stays identical); `fast_class` gives `PyOnceLock`-cached classes for
  PdfObject/NameObject/IndirectObject/DictionaryObject/ArrayObject/StreamObject/
  int/dict/len in per-entry loops.
- The text extraction loop borrows the operator bytes instead of allocating a new
  `bytes` object for each of its five comparisons.

**Measured (`run_perf.sh --repeat 7`):**

| W | round 6 | round 7 |
|---|---|---|
| W1 | 1.20× | 1.13× |
| W2 | 1.27× | 1.23× |
| W3 | 0.77× | 0.78× |
| W4 | 1.90× | 1.85× |
| W5 | 0.66× | 0.66× |
| W6 | 0.73× | 0.73× |
| W7 | 0.66× | 0.66× |
| W8 | 0.89× | 0.88× |

(W1/W2/W4/W8 swing ±8 % with machine load; the single-workload runs showed
W8 at 387 ms vs 417 ms reference in one pass.)

Gates: A1 MATCH, differential IDENTICAL, roundtrip PASS, security 57/57 + fuzz
200/200 IDENTICAL.

## Round 10 attempts (lead, 2026-11-25): mostly neutral

- C-level `set_item`/`gget` for writer dictionaries and a `PyOnceLock` class
  cache for NumberObject/BooleanObject/TextStringObject/ByteStringObject:
  no measurable change (W5/W6/W7 within noise).
- Skipping content-stream operators that no text handler covers: no gain for
  W3 (the W3 corpus is text-heavy) and reverted to keep the loop simple.
- Cached Rust `character_map` (bounded 32-font cache keyed by dict address,
  validated by length) + Rust-side LTR display-string assembly: W3 225 → 218 ms
  (within noise; kept because it removes N Python string creations per Tj and is
  covered by the cmap/text tests).

Final `run_perf.sh --repeat 7`: W1 1.11×, W2 1.26×, W3 0.79×, W4 1.85×,
W5 0.67×, W6 0.70×, W7 0.64×, W8 0.89×.  Gates: A1 MATCH, differential
IDENTICAL, roundtrip/security/fuzz PASS.

**Conclusion for the remaining gap:** W3 is limited by per-operator Python
attribute traffic inside the text state machine (22 getattr/setattr per
positioning operator) and W5–W7 by the clone Python-frame count pinned by the
upstream tests; both need structural changes (native typed state / cheaper PyO3
trampolines), not further micro-optimisation.

## Native NameObject.renumber (lead, 2026-11-25, round 8)

`NameObject.renumber()` was the top writer hot spot (`sample`: 166/400 samples):
per character it built a Python slice, a PyO3 iterator item, a fresh `"~"`
string for the `>` comparison and an `encode()` call.  The rewrite extracts the
name once to a Rust `String`, walks `chars()`, escapes non-ASCII to `#XX`
natively, and only creates a one-char `str` for the `renumber_table` lookup.
The `IndexError` (empty name), `PdfStreamError` (missing `/`) and `KeyError`
fallbacks keep the upstream messages.

**Measured (`run_perf.sh --repeat 7`):**

| W | round 7 | round 8 |
|---|---|---|
| W1 | 1.13× | **1.17×** |
| W2 | 1.23× | **1.27×** |
| W3 | 0.81× | **0.83×** |
| W4 | 1.85× | **1.91×** |
| W5 | 0.67× | **0.76×** |
| W6 | 0.68× | **0.78×** |
| W7 | 0.63× | **0.75×** |
| W8 | 0.89× | **1.03×** |

Gates: A1 MATCH, differential IDENTICAL, roundtrip PIPELINE PASS, security
57/57 + fuzz 200/200 IDENTICAL.

## Lock-free module/class caches (lead, 2026-11-25, round 9)

`pyutil::cached_module` and `cached_attr` had a global-mutex + HashMap on every
lookup; sampling showed `cached_module`/`cached_attr` as 3–4 % of W3/W7.  The
hottest names now use dedicated `PyOnceLock` cells (no lock, no hashing):
`builtins`, `pypdf._text_extraction`, `pypdf.generic[.…]`, `pypdf._utils`, the
PdfObject/NameObject/IndirectObject/NumberObject/FloatObject/BooleanObject/
TextStringObject/ByteStringObject/NullObject classes, DictionaryObject/
ArrayObject/StreamObject/ContentStream and builtins int/float/str/bytes/len/dict/
list/tuple/bool/slice.  `text_extraction_mod::common::is_instance_builtin` and
`slice_of` got the same treatment.

**Measured (`run_perf.sh --repeat 7`):**

| W | round 8 | round 9 |
|---|---|---|
| W1 | 1.17× | **1.19×** |
| W2 | 1.27× | **1.34×** |
| W3 | 0.83× | **0.88×** |
| W4 | 1.91× | **1.96×** |
| W5 | 0.76× | **0.80×** |
| W6 | 0.78× | **0.88×** |
| W7 | 0.75× | **0.77×** |
| W8 | 1.03× | **1.07×** |

Gates: A1 MATCH, differential IDENTICAL, roundtrip PASS, security 57/57 + fuzz
200/200 IDENTICAL.

## Rust-items text consumer (lead, 2026-11-25, round 10)

`ContentStream` keeps its buffered `_data`; `contentstream_items_fast` parses it
straight into `pypdf_rs_core::content_stream::Item`s without building the Python
`operations` list.  `page_mod/text.rs` tries that path first and materializes
operands only for the 19 operators that have handlers (or for every operation
when a visitor is installed).  Measured on the W3 corpus: scanning 13.5k
operations takes 3.8 ms, while building `.operations` costs 79.4 ms.

Fallbacks keep upstream behavior: a monkeypatched `ContentStream._parse_content_stream`
(checked via the shim function's `__qualname__`/`__module__` and the instance
dict), inline images, any parse failure, or an already materialized
`_operations` list all route back to `.operations`.

**Measured (`run_perf.sh --repeat 7`):**

| W | round 9 | round 10 |
|---|---|---|
| W1 | 1.19× | **1.23×** |
| W2 | 1.34× | **1.37×** |
| W3 | 0.88× | **0.97×** |
| W4 | 1.96× | **1.95×** |
| W5 | 0.80× | **0.81×** |
| W6 | 0.88× | **0.89×** |
| W7 | 0.77× | **0.80×** |
| W8 | 1.07× | **1.11×** |

Gates: A1 MATCH, differential IDENTICAL, roundtrip PASS, security 57/57 + fuzz
200/200 IDENTICAL.

## Round 11 (lead, 2026-11-25): text trim + Linux deployment attempt

- `_post_process_text_operation` reuses the `_space_width` lookup (was fetched
  twice) and reads the new text from the returned tuple instead of re-fetching
  the attribute after writing it.
- Deployment (§17 condition 10): Docker Desktop on this host can pull images
  (`python:3.14-slim`, `ghcr.io/pyo3/maturin`) but the engine cannot start any
  container (`docker start` fails with a unix-socket connect error), so the
  Linux aarch64 wheel build/install/test could not be executed.  Recorded as
  NOT_TESTED with the exact failure.
- Full gate re-run: `scripts/gate.sh` → BUILD/A1/A2A3A4/A5/A6/A7 all PASS
  (`validation/results/logs/gate-opt15.log`).

**Latest `run_perf.sh --repeat 7`:** W1 1.21×, W2 1.32×, W3 0.94×, W4 2.02×,
W5 0.86×, W6 0.93×, W7 0.87×, W8 1.10× (small workloads vary ±10 % run to run).

## Round 12 (lead, 2026-11-25): Linux cross-compile evidence

- Added `validation/linux_check.sh`: `PYO3_CROSS=1 PYO3_CROSS_PYTHON_VERSION=3.14
  cargo check -p pypdf-py --target {aarch64,x86_64}-unknown-linux-gnu` — both
  pass, so the port compiles for the Linux targets.
- Docker-based Linux wheel build remains blocked: the engine pulls images and
  can run one trivial container after `docker desktop restart`, but any
  container with a bind mount (`docker run -d -v …`) hangs indefinitely with no
  logs (reproduced with a 2 GB and a 119 MB source tree).  Left as NOT_TESTED.

## Round 13 (lead, 2026-11-25): Linux CI verification

- Added `.github/workflows/linux.yml` (matrix: `ubuntu-24.04` x86_64 and
  `ubuntu-24.04-arm` aarch64): checkout → Python 3.14 → stable Rust →
  `maturin build --release` → install wheel → official suite
  (`-m "not samples and not enable_socket"`) → 103-scenario differential audit
  against `pypdf==7.1.1` with identical optional dependencies → upload wheel.
- Run 45556888196 is green on both architectures: x86_64 `2188 passed,
  40 skipped, 4 xfailed in 20.31s` + `IDENTICAL`; aarch64 `2188 passed,
  40 skipped, 4 xfailed in 11.69s` + `IDENTICAL`.  Evidence:
  `validation/results/ci/linux-45556888196.md`.
- The earlier local Docker trouble (see round 12) was only a host/engine
  limitation; the repository now carries a reproducible Linux verification path.

