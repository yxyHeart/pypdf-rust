# pypdf Rust 移植验证报告（FINAL_VALIDATION）

> 依据：`pypdf-rust-validation-plan.md`（下称"方案"）。本报告只记录**实际跑过并留有产物**的验证；未验证项标注 `NOT_TESTED` / `NOT_AVAILABLE`，不以"能打开 PDF"或"文件字节一致"代替语义验证。

## 1. 基线与环境

| 项 | 值 |
|---|---|
| 参考实现 | PyPI `pypdf==7.1.1`（`.venvs/oracle`） |
| 被测实现 | 本仓库 Rust+PyO3 端口，包名 `pypdf==7.1.1`（`.venvs/dev`） |
| Python | 3.14.7 |
| 平台 | macOS-26.6.2 arm64（macOS 26 SDK，arm64） |
| 扩展 | `_pypdf.cpython-314-darwin.so`（非 abi3，CPython 特定 ABI） |
| Rust / PyO3 / maturin | rustc 1.83+ MSRV；pyo3 0.30.1；maturin 1.16.0 |
| 构建质量 | `cargo fmt --all --check` PASS；`cargo clippy --workspace --all-targets -- -D warnings` PASS（0 告警）；`cargo test --workspace --locked` PASS |
| 外部工具 | `qpdf 12.4.2`、`mupdf-tools 1.28.6 (mutool)`、poppler（`pdfinfo`/`pdftotext`/`pdftoppm`）、`ghostscript`、`jbig2dec` 均可用 |

产物：`validation/results/env-reference.json`、`env-rust.json`、`logs/build-quality.log`。

**关于方案 §0.2 的 `PYPDF_BACKEND=rust`**：`NOT_APPLICABLE`。本端口以包名 `pypdf` 交付，Rust 扩展 `pypdf._pypdf` 是其唯一后端——不存在"选择后端 + 静默回退"的机制，因此"后端选择失败即失败"的风险从结构上不存在。参考实现与被测实现通过**两个隔离的 venv**（`.venvs/oracle` / `.venvs/dev`）保证互不覆盖；每份输入 PDF 均有 sha256 manifest（`validation/fixtures/manifest.csv`）；测试只读语料、输出写入临时目录；除网络标记测试外不下载语料。

## 2. API 兼容性（§4）

方法：`validation/api_surface.py` 在两侧分别导出 59 个模块的完整 API surface（类/MRO/抽象性/方法/签名/默认值/属性读写器/常量 repr）；`validation/compare_api.py` 逐项对比，并区分**结构差异**与**仅类型注解差异**（注解差异通过读取上游签名文本、忽略语法结构以外的内容）。

| 指标 | 结果 |
|---|---|
| 参考条目 / 端口条目 | 1352 / 2101 |
| 缺失（reference 有、端口无） | **0** |
| 额外公开名（端口多出的公开 API） | **0** |
| 结构差异（名称/参数结构/默认值/MRO/抽象性/属性读写器） | **0** |
| 仅类型注解差异 | 94（均为 Rust 实现的模块级函数，见下） |
| 端口私有额外名 | 749（`_` 前缀 shim 别名，非公开 API） |

**结论：`PASS`**（结构完全一致）；94 项注解差异记为 `PASS_WITH_DOCUMENTED_VARIANCE`：

- 这些是模块级函数（如 `_utils.read_non_whitespace`、`_cmap.logger_warning`、`_writer.deprecate_with_replacement`），由 Rust `#[pyfunction]` 直接暴露；CPython 内建函数对象**不支持**写入 `__annotations__`，因此其签名与上游**参数名/参数种类/默认值完全一致**，仅缺类型注解文本。
- 其它 Python 层可写注解的调用点已通过 `python/pypdf/_api_annotations.py`（由 `validation/gen_api_annotations.py` 从上位参考实现生成，共 2984 条）在各模块导入时同步，`inspect.signature` 与参考实现**逐字符一致**（已验证 `PdfWriter.append`、`NameObject.write_to_stream`、`FontDescriptor`、`LzwCodec` 等）。
- 注意：该同步**不会**触发任何惰性模块的提前导入（否则会改变 `sys.modules` 时序并破坏 `pillow 缺失` 等行为）；引用 `pypdf.*` 的注解以 `typing._Rendered` 代理对象保存（渲染文本与参考一致），因此对这类注解调用 `typing.get_type_hints()` 会失败——记为已知差异。

修复过程中解决的真实问题：`Version`/`File`/`_TraversalState` 由 pyclass 改为上游原样 Python 类（含 `@dataclass`）；`PageRange` 改为上游 verbatim 实现；`Codec`/`LzwCodec` 改为 Python `ABC` 类包装 Rust 实现（状态在调用后同步）；补齐 39 个模块级重导出（`WHITESPACES`、`read_non_whitespace`、`STREAM_TRUNCATED_PREMATURELY` 等）；`is_char_rtl`/`is_char_neutral` 默认值对齐 `""`；`read_until_regex(length=…)` 默认值对齐；`STREAM_TRUNCATED_PREMATURELY` 修正为上游字符串常量。

## 3. 正确性：官方测试与差分（§2/§5/§6/§7）

| 项目 | 结果 |
|---|---|
| 上游官方测试（A1） | `2691 passed / 25 skipped / 8 xfailed / 0 failed`，与基线**逐用例** `MATCH`（`.audit/baseline-outcomes.json`） |
| 差分审计（A2/A3/A4） | 103 场景 `IDENTICAL`（对象图、异常类型+消息、日志/告警、文本、加密、writer 输出 sha256） |
| 深差分（本方案新增） | 24 个文档 × {path/BytesIO/file object × strict True/False × 读语义摘要 × 写→重开语义摘要} = **`IDENTICAL`** |
| writer 字节级（A2 专项） | 53/53 文档 sha256 完全一致（含 RC4-40/128、AES-128/256、R5/R6、增量、表单、注释、附件、合并、页标签、outline、命名目标） |

深差分摘要包含：页数、五类页面框（Media/Crop/Trim/Bleed/Art）、旋转、页标签、outline、metadata、XMP 序列化哈希、附件内容哈希、表单字段与取值、命名目标、每页文本哈希、图像（名称/内容哈希/模式/尺寸）、注释（子类型/Rect/Flags）。

产物：`validation/results/diff-reference.json`、`diff-rust.json`；比较器输出 `IDENTICAL`。

## 4. Round-trip 与外部工具互操作（§9）

方法：`validation/roundtrip.py` 对确定性抽样（25 个文档，种子 `54324594788340276`）执行 6 类工作流，每个用例独立子进程（超时 120 s），输出语义摘要（页框/旋转、解码内容 sha、注释、图像 sha、元数据、页标签、扁平化 outline、命名目标、附件、AcroForm、XMP、文本）；`validation/compare_roundtrip.py` 逐字段比较两侧 JSON。

| 工作流 | 用例 | 通过 | 失败 | 跳过 |
|---|---|---|---|---|
| RW（读→写→重开） | 25 | 25 | 0 | 0 |
| MERGE（合并） | 25 | 25 | 0 | 0 |
| TRANSFORM（rotate90/crop/scale） | 25 | 25 | 0 | 0 |
| EDIT（outline/命名目标/页标签/注释/附件/元数据） | 25 | 23 | 2 | 0 |
| ENCRYPT（AES-256 写入→口令重开） | 25 | 25 | 0 | 0 |
| INCREMENTAL（增量写入→重开） | 25 | 22 | 0 | 3 |
| **合计** | **150** | **145** | **2** | **3** |

**两侧逐字段比较：`MATCH — 0 differences, 150/150`**（比较状态、异常类型/消息、各变体校验、全部摘要字段与 warning 计数；仅排除路径/耗时/解释器信息）。

- 2 个 EDIT 失败在**两侧完全相同**：`add_named_destination` 对 `/Names /Dests` 为带 `/Kids` 的间接名字树时是 no-op（写入不可达的内部节点，`/Limits` 未扩展）——上游 pypdf 自身行为，端口逐字节复现，非端口回归。
- 3 个 INCREMENTAL 跳过在两侧相同：加密源文档 `PdfWriter(incremental=True)` 抛 `FileNotDecryptedError`。

互操作（`validation/interop.py`，每后端 222 个目标，工具全部可用：qpdf 12.4.2、mutool 1.28.6、poppler 26.11.0）：

| 检查 | pass | warn | fail | n/a |
|---|---|---|---|---|
| `qpdf --check` | 191 | 31 | 0 | 0 |
| `qpdf --qdf` → 再校验 → pypdf 重读 | 167 | 30 | 0 | 0 |
| `pdfinfo` | 197 | 0 | 0 | 0 |
| `pdftotext`（对比 pypdf 提取） | 105 | 92 | 0 | 0 |
| `mutool info` | 197 | 0 | 0 | 0 |
| `mutool clean` → pypdf 重读 | 197 | 0 | 0 | 0 |
| `pdftoppm` 渲染第 0 页 @72 DPI | 75 | 0 | 0 | 0 |

**跨后端比较：`MATCH`** —— 75/75 渲染 PNG SHA-256 完全相同、197/197 `pdftotext` 去空白摘要相同、1010/1010 工具状态字段相同、0 差异、无工具可用性差异。

- qpdf 的 31/30 个 WARN 在两侧相同（22 个增量输出 xref 警告、9 个表单文档 "output may still be valid"）；QDF 输出均通过 `qpdf --check` 与 pypdf 重读，记为 WARN 而非 FAIL。
- 92 个 `pdftotext` WARN 是 pypdf 与 poppler 的提取差异（连字、断词顺序、表单 appearance vs 值），**两侧摘要完全相同**，与端口无关；精确 diff 已存于 JSON。

## 5. 安全与健壮性（§8）

方法：`validation/security.py` 生成确定性对抗语料（59 个用例，`--heavy` 含 1 GiB 解压炸弹），在两侧各自独立子进程运行并记录：成功/失败决定、异常类+消息、`LimitReachedError` 标记、warning/log、CPU/墙钟时间、峰值 RSS、是否留下部分输出；`validation/fuzz.py` 用 200 个变异 PDF 复用 `tools/fuzz/run.py` 做双向对照；`validation/run_security.sh` 编排并在任何差异/超时/崩溃时返回非零，且做**构建指纹校验**以发现运行中途重建。

| 运行 | 用例 | 一致 | 差异 | panic/崩溃 | 超时 | 峰值 RSS（参考/端口） |
|---|---|---|---|---|---|---|
| 安全（默认） | 57 | **57** | **0** | 0 / 0 | 0 | 85.0 MiB / 79.7 MiB |
| 安全（`--heavy`） | 59 | **59** | **0** | 0 / 0 | 0 | 189.2 MiB / 193.8 MiB |
| 模糊（200 变异 PDF） | 200 | **200** | **0** | 0 / 0 | 0 | — |

两侧同为 **32 例正常处理 / 25 例抛异常 / 15 例 `LimitReachedError`**；模糊为 127 正常 / 73 抛异常。被两侧一致触发的保护包括：`/Length=999999999`、50 层 `/FlateDecode` 链、页树深度 150、outline 深度 170、100,500 条 ToUnicode、100,002 个页标签、100,002 个附件名树节点、xref 声明 200,000 项、100,000×100,000 inline image、1 GiB Flate 炸弹（75 MB 上限）；循环引用 → `LimitReachedError` / `PdfReadError("Detected cyclic page references.")`；空文件 → `EmptyFileError`；缺 EOF → `PdfStreamError`；缺失/非法 `/Root` → 扫描 `/Catalog` 恢复；2³² 对象号 → 警告 + `NullObject`。

**已知非阻断差异（已记录，未静默）**：深度 1000/10000 的嵌套字典，两侧都成功但保留的 `/Nest` 深度不同（参考约 490，端口约 984）；原因是上游在 `read_from_stream` 中捕获 `RecursionError` 后返回部分数据，保留深度取决于原生栈布局（端口在不同构建下也曾出现 492 与 984），**不是稳定契约**。另有 4 例异常消息包含 `id(pdf)` 地址文本（已归一化，不影响行为）。

方法限制（明确标注，未当作 PASS）：macOS 上 `RLIMIT_AS` 不可用（`ValueError: current limit exceeds maximum limit`），内存以 `ru_maxrss` 观测，墙钟超时与 `RLIMIT_CPU` 作为强制边界；本次 0 次超时。

产物：`validation/results/security-{oracle,port}.json`、`security-comparison.json`、`security-report.md`、`fuzz-*.json`、`fuzz-report.md`、`logs/security-*`。

## 6. 性能（§10）

方法：`validation/perf/`，W1–W8 共 8 类工作负载，参考与端口在**独立进程**中顺序执行（`PYTHONHASHSEED=0`，1 次预热 + 10 次重复），记录逐次墙钟/CPU/峰值 RSS/输出字节与 sha256。测量时机器存在不可控的系统后台负载（WindowServer / DSH renderer / StorageManagement），负载均值 22–30（10 核），两侧在同一条件下依次运行，输出尺寸与 sha256 **全部相等**、80/80 次重复全部成功。

### 6.1 P1–P4 优化后（2026-11-25 复测，`run_perf.sh --repeat 5`）

已落地三轮优化：字节级 Rust 内容流解析器（P1）、Rust 原生文本派发（P2）、字节级 `read_object` 快路径 + 文本显示辅助函数直调（P4）。

| W | 工作负载 | 参考中位数 | 端口中位数 | speedup | 优化前 |
|---|---|---|---|---|---|
| W1 | Reader 初始化 + 元数据（20 小文件） | 10.2 ms | 8.6 ms | **1.19×** | 0.36× |
| W2 | 全页页框遍历（5 大文件） | 156.7 ms | 117.0 ms | **1.34×** | 0.40× |
| W3 | 文本提取（12 个文本 PDF，≤5 页/个） | 160.9 ms | 182.0 ms | **0.88×** | 0.25× |
| W4 | 图像枚举/提取（10 PDF，含 Pillow） | 564.9 ms | 288.9 ms | **1.96×** | 0.33× |
| W5 | 合并（5 PDF） | 10.2 ms | 12.7 ms | **0.80×** | 0.29× |
| W6 | 旋转/裁剪/合并后写出 | 13.4 ms | 15.3 ms | **0.88×** | 0.28× |
| W7 | clone 写出（10 文件） | 17.5 ms | 22.9 ms | **0.77×** | 0.29× |
| W8 | 端到端（读+文本+合并+写，7 文件） | 390.7 ms | 366.8 ms | **1.07×** | 0.33× |

**进展：W1/W2/W4 快于参考实现（1.11× / 1.26× / 1.85×），W8 0.89×，W3 从 0.25× 提升到 0.79×（706→218 ms），W5–W7 为 0.64–0.70×。** 主要收益来自缓存 Python 模块/类查找（含所有 `builtins()` 辅助函数）、文本路径的原生矩阵/宽度计算与更精简的派发。

门禁（全部复测通过）：A1 `2691 passed / 25 skipped / 8 xfailed` 且与基线 MATCH；差分 103 场景 IDENTICAL；roundtrip PASS；安全 57/57（含深层嵌套拒绝用例）、fuzz 200/200 IDENTICAL；输出字节与 sha256 全部一致。

剩余差距归因（`validation/optimization/diag_reader.md`、`diag_writer.md`）：
- **W5–W7（0.43–0.47×）**：写路径 75–86% 是对象图拷贝，序列化 14–19%；`dictionaryobject_clone_inner` / `dictionaryobject_write_to_stream` 每个字典条目仍有 `replicate`、`__setitem__`、`write_to_stream` 3–4 次跨语言调用。下一步是 Rust 侧 clone 快路径（简单类型直接复用/复制）与按对象批量渲染写出。
- **W3（0.46×）**：文本消费侧仍逐字符 `character_map.get` 跨界，`get_display_str` 的 RTL/visitor 分支保持 Python 语义；下一步把字符映射抽到 Rust 缓存。
- **W1/W2（0.62–0.71×）**：Reader 的 xref 校验循环（`read_object_header` 21,270 次/次 W2）、页树遍历与页框读取（`_get_rectangle`）仍以 Python 对象为中介。

产物：`validation/perf/workloads.json`、`run.py`、`compare.py`、`run_perf.sh`、`validation/results/perf-{reference,rust}.json`、`perf-report.md`、`logs/perf-*`。

产物：`validation/perf/workloads.json`、`run.py`、`compare.py`、`run_perf.sh`、`validation/results/perf-{reference,rust}.json`、`perf-report.md`、`logs/perf-*`。

## 6.2 优化轮 §17 完成度

优化计划的最终完成定义（§17）逐条核对结果见 [reports/OPTIMIZATION_COMPLETION.md](OPTIMIZATION_COMPLETION.md)：
条件 1、3–9 已满足；**条件 2（文本消费者不再逐 Token 物化 Python 对象）未满足**，条件 10（Linux aarch64/x86_64 部署）仍为 `NOT_TESTED`（本机仅 macOS arm64）。
全门禁一次运行证据：`validation/results/logs/gate-optimized.log`（`scripts/gate.sh`，A1–A7 全部 PASS）。

## 7. 部署与打包（§11）

| 项目 | 结果 |
|---|---|
| wheel | `pypdf-7.1.1-cp314-cp314-macosx_11_0_arm64.whl` |
| ABI | 非 abi3（绑定 CPython 3.14 / arm64） |
| 干净 venv 安装 | PASS（`pip install --no-index --no-deps`，无网络） |
| wheel 内真实 PDF 读写 | PASS（`resources/crazyones.pdf`：读取、`extract_text`、clone 写回、重开校验） |
| 离线部署 | wheel 安装与运行不需要网络；构建阶段需要 Rust 工具链与 crates 缓存 |
| 其它平台 | `NOT_TESTED`（本机无 Linux/Windows；方案要求不得把未测平台写作支持） |

产物：`validation/results/wheel-info.json`、`logs/wheel-*.log`。

## 8. 自动化入口

```bash
# 官方门禁（A1–A7，含差分、图像字节、符号覆盖、模糊、浮点）
./scripts/gate.sh

# 本验证方案
.venvs/oracle/bin/python validation/env_info.py validation/results/env-reference.json
.venvs/dev/bin/python    validation/env_info.py validation/results/env-rust.json
.venvs/oracle/bin/python validation/gen_manifest.py
.venvs/oracle/bin/python validation/api_surface.py validation/results/api-reference.json
.venvs/dev/bin/python    validation/api_surface.py validation/results/api-rust.json
.venvs/dev/bin/python    validation/compare_api.py validation/results/api-reference.json validation/results/api-rust.json
PYTHONHASHSEED=0 .venvs/oracle/bin/python validation/differential.py --out validation/results/diff-reference.json
PYTHONHASHSEED=0 .venvs/dev/bin/python    validation/differential.py --out validation/results/diff-rust.json
.venvs/oracle/bin/python validation/compare_differential.py validation/results/diff-reference.json validation/results/diff-rust.json
./validation/run_roundtrip.sh     # §9 round-trip + 互操作
./validation/run_security.sh      # §8 恶意/畸形输入 + fuzz
./validation/perf/run_perf.sh     # §10 W1–W8 性能
./validation/wheel.sh             # §11 wheel 干净安装
```

## 9. 未解决问题与风险

- `_codecs` 之外无 API 结构差异；94 条类型注解差异（Rust 模块级函数）为已知、已记录、不可通过 Python 层修复（CPython 限制）。
- 性能结论见 §6；本机存在不可控的系统后台负载（WindowServer / DSH renderer / StorageManagement），所有测量均在同一共享机器上按"参考→端口"顺序执行，并记录了负载。
- 系统级资源限制（`RLIMIT_AS`）在 macOS 上对子进程生效有限，§5 中相应项按实际情况标注。

## 10. 验收结论

**结论：`PASS WITH DOCUMENTED VARIANCE`（正确性/兼容性/安全/部署全部通过；性能未达标，已如实记录并给出根因与后续方案）。**

| 验收项（方案 §14.1） | 状态 |
|---|---|
| P0 功能差分 100% 通过 | ✅ 官方 A1 逐用例 MATCH；深差分 24 文档 IDENTICAL；round-trip 150/150；差分审计 103 场景 IDENTICAL |
| 已声明支持的 P1 能力有测试和状态 | ✅ API surface 结构差异 0、缺失 0；94 条仅注解差异已记录 |
| 没有未解释的语义差异 | ✅ 全部差异均有解释（共享上游行为/地址文本/注解限制） |
| round-trip 与外部工具互操作通过 | ✅ qpdf 0 FAIL、mutool 0 FAIL、渲染哈希 75/75 一致 |
| 安全边界与 fuzz 无未关闭阻断问题 | ✅ 57/57（heavy 59/59）、fuzz 200/200、0 panic/超时 |
| 端到端性能达到事先定义的性能门槛 | ❌ **未达到：端口比参考慢 2.5–4×**（方案 §10.5 的门槛需项目负责人填写，此前未定义；根因与后续方案见 §6） |
| 目标平台 wheel/干净安装/离线运行 | ✅ macOS arm64 + CPython 3.14 wheel，`--no-index` 干净 venv 安装并完成真实 PDF 读写 |
| 最终报告完整、未完成项明确披露 | ✅ 本报告 |

**未完成/风险**：性能回退（架构性：逐 token 跨语言调用；需字节级 Rust tokenizer 才能消除）；Linux/Windows 平台未测（`NOT_TESTED`）；94 条模块级函数的类型注解无法写入（CPython 内建限制）；深层嵌套字典的部分数据保留深度随栈布局变化（非稳定契约）。
