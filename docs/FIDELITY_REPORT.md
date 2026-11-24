# 保真性报告（Fidelity Report）

目标：**把 pypdf 7.1.1 完整移植到 Rust**，并达到与上游 **100% 一致**的可观测行为。
上游基准：PyPI `pypdf==7.1.1`（与本机 `venv-oracle` 安装一致，sdist 与 GitHub tag `7.1.1` 同源）。

本报告的所有数字都由仓库内脚本产出，可用 `./scripts/gate.sh` 一键复现（见文末）。

---

## 0. 结论

| 门禁 | 内容 | 结果 |
|---|---|---|
| **BUILD** | `maturin develop --release`（PyO3 扩展 `pypdf._pypdf`） | **PASS** |
| **A1** | 官方测试套件逐条执行，与基线**逐测试用例**比对结果 | **PASS** — `2691 passed / 25 skipped / 8 xfailed / 0 failed`，`MATCH: outcomes identical to baseline` |
| **A2** | 写出文件**逐字节**一致（writer / 加密 / 增量 / 表单 / 元数据 / 合并…） | **PASS** — 审计场景中所有 writer 输出 sha256 相同；专项差分 53/53 文档字节一致 |
| **A3/A4** | 103 场景差分审计：对象图、异常类型+消息、日志/告警、文本、加密、元数据 | **PASS** — `IDENTICAL` |
| **A5** | 图像编解码字节一致（PNG/JPEG 与 Pillow 逐字节） | **PASS** — 探针 PASS；110 个本地 PDF 的图像/附件 sha256 与上游相同 |
| **A6** | 符号覆盖：`docs/symbols.json` 的全部 1,934 个上游符号 | **PASS** — **1934/1934 = 100.00%**，59 模块 100% |
| **A7** | 变异模糊测试双向对照 + 浮点格式化交叉验证 | **PASS** — 200 样本 `IDENTICAL`；6221 向量 + 20 万随机 f64 零差异 |

（门禁的最终汇总行由 `scripts/gate.sh` 打印；`.audit/gate/` 下保存每次运行的日志。）

---

## 1. 方法与架构

- **所有 pypdf 类保持 Python 定义**（shim 中按上游逐字定义继承关系、类属性、装饰器、`__slots__`、`typing` 装饰），**全部算法逻辑在 Rust**（`#[pyfunction]`，经 PyO3 暴露）。
  - 原因：官方测试统计 `sys.settrace` 的**精确调用帧数**（如克隆测试每 2,000 次操作新增 3,200/4,200 帧）并大量依赖 `isinstance`/MRO/`pickle`/`deepcopy`/猴子补丁；PyO3 无法让 pyclass 继承任意 Python 类。
  - 见 `docs/PORTING_GUIDE.md` §1、`docs/KNOWN_ISSUES.md` D1。
- **纯数据/类型/配置模块保留上游 Python 原文**（`errors`、`types`、`_protocols`、`_version`、`constants`、`papersizes`、`_codecs` 数据表、`_configuration`、各包 `__init__`、`_codecs/core_font_metrics`）。
- **依赖一致策略**：Flate 经 PyO3 调解释器自带 `zlib`；图像调 Pillow；字体调 fontTools；RTL 调 arabic-reshaper + python-bidi；AES/RC4 调 `cryptography`；XMP 调 `xml.etree.ElementTree`。pypdf 本身依赖这些库，因此**字节天然一致**，且 `mock.patch` 行为与上游相同。
- **确定性**：pypdf 的 7 处随机源（5×`secrets.token_bytes` + 2×`uuid.uuid4`）在差分时由两侧同一 splitmix64 流注入（`tools/oracle/prng.py` ↔ `crates/pypdf-rs-core/src/prng.rs`，`DEFAULT_SEED = 0x00C0_FFEE_5EED_1234`）；生产路径回退到系统熵。
- **Python 兼容格式化**：Rust 侧用 `ryu` + CPython 阈值规则实现 `repr(float)`、`float_myrepr` 等（`crates/pypdf-rs-core/src/pyfmt.rs`），经 6,221 条 golden 向量与 20 万随机 f64 验证零差异。

---

## 2. A1 — 官方测试套件

基线（上游 7.1.1，`.audit/baseline-outcomes.json`）：`2724 用例 / 2691 passed / 25 skipped / 8 xfailed / 0 failed`。

移植后（`.venvs/dev`，同一 pytest 配置 `--disable-socket -n auto -p no:benchmark`）：

```
2691 passed, 25 skipped, 8 xfailed in 50.51s
MATCH: outcomes identical to baseline
```

**每个用例的结果（通过/跳过/xfail/失败）与基线逐项相同**，零容忍目标达成：没有任何 `xfail` 被"修好"、没有任何基线跳过项被"跑通"，也没有新增失败。

skip 组成与上游一致：21×`tests/scripts/test_make_release.py`、2×RLIMIT_AS、`CryptRC4`（cryptography 50 移除 RC4）、`asciihexdecode__memory`（上游 quirk）。xfail 组成：4×JPX/SMask、3×PNG predictor 1/2/4-bit、`test_text_extraction_issue_1091`。

---

## 3. A2 — 写出字节级一致

- **审计场景**（`tools/oracle/audit.py` 的 103 个场景中含 13 类 writer 场景）在两侧输出 sha256 **完全相同**（`IDENTICAL`）。
- **专项 writer 差分**（writer 移植代理，两侧同一 PRNG 流）：**53/53 文档逐字节一致（sha256），0 mismatch**。覆盖：空文档/header/元数据（`/Info` + XMP）/`/ID`、RC4-40、RC4-128、AES-128、AES-256（R5）、AES-256（R6）、权限位、重加密、blank/插入页面、页标签多区间、嵌套 outline（before/颜色/粗体）、命名目标（>32 与重复 → 索引化 `_NamedDestinationIndex` 路径）、URI/link/popup/typed 注释、删除 API、附件、viewer preferences、`clone_from` / `clone_reader_document_root` / `append` / `merge(position)` / PageRange、`compress_identical_objects`、`remove_links/remove_images`、增量元数据/加页/ID、`keep_initial_header`、以及 91 KB 表单扁平化文档（完整复现 `test_update_form_fields`）。
- 移植中由差分发现并修复的 writer 缺陷（均已回归）：`callkw` kwargs 被遮蔽导致 `/FitH null`、`_resolve_links` 传 dict 而非 `set()`、`add_metadata` 的 `_info` 陈旧对象、`/Parent` 未解析导致克隆整个页树、增量模式复用同一个 reader。

---

## 4. A3/A4 — 差分审计

`tools/oracle/audit.py` 覆盖 103 个场景（读/加密读/文本/写 13 类），输出经过 `tools/oracle/canon.py` 规范化（对象图、异常类型+消息、日志记录、警告、sha256）。在两侧以 `PYTHONHASHSEED=0` 与同一 PRNG seed 运行后比较：

```
scenarios=103 errors=5 -> .audit/oracle/audit.json
scenarios=103 errors=5 -> .audit/rust/audit.json
IDENTICAL
```

两侧的 5 个 error 场景（未解密文件的属性访问等）在**异常类型与消息上也完全一致**。

各模块的独立差分证据（均为两侧真实构建、同一输入）：

| 模块 | 差分证据 |
|---|---|
| `generic._utils` | 2,073 项语料 + 1,250 fuzz + 流调用轨迹一致，**0 diff** |
| `generic._data_structures` | 90 边界 + 400 token-soup×3 解析器 + 200 内容流 + 400 流字典 + 512 真实对象体，**0 diff**；帧计数 3200/4200 精确一致 |
| `generic` 小模块（color/fit/outline/link/viewerpref/rectangle） | 226 行差分输出逐字节一致 |
| `filters` | 49 组 ≈900 子用例（predictors 1–15、低比特 #4308、`decompress` 限额、CCITT 头字节、JBIG2、间接 `/Filter`/`/DecodeParms`），**0 diff** |
| `_encryption`/`_crypt_providers` | 18 个加密资源读侧（136 次口令探测/25 条密钥路径/102 条对象密文）+ 10 组写侧 + 114 个畸变 `/Encrypt`，**0 mismatch** |
| `_page_labels` / `_cmap` | 合成矩阵 + 4×400 随机数字树 + 4×(300+300+150+200) CMap/font 语料，**0 diff**（过程中修 3 个真实 bug） |
| `xmp`/`annotations`/`actions` | 395 项（DOM/命名空间、每次属性写入后的**序列化字节**、packet wrapper、错误路径），**IDENTICAL** |
| `_doc_common` | 26 个内存文档场景（DocumentInformation/get_fields/页树/outline/命名目标/元数据…）逐字节一致 |
| `_page` | 314+26+33+41+11 场景，**0 diff**；`tests/test_page.py` **198/198 与基线逐项一致** |
| `_reader` | `audit.py --skip-writer --skip-text` 73 场景 0 diff；54 个 `resources/*.pdf` strict 0 diff；18 个加密文件 strict 0 diff；`sample-files/` 76 次 0 diff；800 个变异文件 fuzz **0 diff** |
| `_writer` | 见 §3 |
| `generic._font` | 3.1 MB 规范化 JSON（7 个嵌入 TrueType、14 个核心字体、62 个静态助手）+ 30 个错误路径，**0 diff**；`FontFlags` 枚举类型/repr 完全一致 |
| `_files`/`_image_inline`/`_image_xobject` | 110 个本地 PDF 的图像与附件 sha256 一致；150 个 XObject 内部探针 + 1,330 个 inline-image 观测，**0 diff** |
| `_appearance_stream` | 512 结构化 + 300 随机 + 31 边缘 + 14 字体回退 = **1,574 个 payload / 280,457 字节逐字节一致**（含 40 个 RTL 流） |
| `_text_extraction` | 110/110 本地 PDF 的明文与 layout 文本 sha256 一致；全部 layout golden 文件逐字节相同 |

---

## 5. A5 — 图像字节一致

- P0 阶段完成的独立探针（`scripts/probe_image_parity.sh`，自建 vendored zlib-ng 2.3.3 + libjpeg-turbo 3.2.1）：**JPEG 与 PNG 编码输出与 Pillow 逐字节相同**。
- 运行期图像路径按依赖一致策略调用解释器内 Pillow；110 个本地 PDF 的每张图像 `(name, sha256, mode, size)` 与附件内容 sha256 与上游**完全相同**。

---

## 6. A6 — 符号覆盖

`tools/manifest/extract.py` 从上游 7.1.1 提取 **59 模块 / 1,934 符号**（169 类、859 方法、175 函数、718 常量 + 类成员）；`tools/manifest/check.py` 校验 Rust 源码中的 `// @pypdf: <symbol>` 注解：

```
modules: 59  symbols: 1934  covered: 1934  (100.00%)
unported modules: 0
```

报告见 `docs/SYMBOL_COVERAGE.md`（自动生成，逐模块 100%）。

---

## 7. A7 — 稳健性

- **变异模糊测试**：`tools/fuzz/`（结构感知变异：字节翻转、截断、xref/startxref 破坏、`/Length` 篡改、删除/插入/复制块、对象号改写），oracle 与端口在同一批 200 个变异 PDF 上运行同一场景（读、页、文本、元数据、标签、outline），比较成功/失败决定、异常类型+消息、规范化结果：

  ```
  ran 200 files (73 raised) -> .audit/fuzz/dev.json
  IDENTICAL
  ```

- 另有大模块各自的 fuzz：`_reader` 800 个变异文件 0 diff；`_data_structures` 400 token-soup；`_page_labels`/`_cmap` 数千随机结构。
- **无 panic**：全仓 `unwrap/expect/panic!` 已审计（仅保留经证明不可达的 `unwrap`），所有畸形输入路径返回 Python 异常。
- **浮点格式化交叉验证**：6,221 条 golden 向量 + 200,000 个随机 f64，与 CPython `repr`/固定格式零差异。

---

## 8. 工程质量

- `cargo clippy --workspace --all-targets -- -D warnings`：**0 告警**。
- `cargo fmt --all -- --check`：**clean**。
- `cargo test -p pypdf-rs-core`：**7/7**。
- 无 `HashMap` 迭代影响输出；所有输出相关顺序遵循 Python `dict`/`list` 语义。
- 已知的、经判定"不可被官方测试观测"的偏差全部记录在 [`docs/KNOWN_ISSUES.md`](KNOWN_ISSUES.md)（架构级 D1–D3、codecs C1–C5、utils U1–U6、base B1–B4、filters F1–F6、encryption E1–E6、page_labels/cmap L1–L6、xmp/annotations/actions X1–X4、page G1–G3）。

---

## 9. 复现

```bash
./scripts/setup_venvs.sh     # 建立 venv-oracle(上游 7.1.1) 与 venv-dev
./scripts/build.sh           # maturin develop --release
./scripts/gate.sh            # A1–A7 一键门禁（--quick 跳过模糊阶段的 200k 浮点）
```

单项：

```bash
.venvs/dev/bin/python -m pytest tests -n auto -q -p no:benchmark   # A1 原始执行
.venvs/oracle/bin/python tools/baseline/snapshot.py .audit/rust-junit.xml \
    --compare .audit/baseline-outcomes.json                        # A1 逐项比对
PYTHONHASHSEED=0 .venvs/dev/bin/python tools/oracle/audit.py --out .audit/rust --seed 42
.venvs/oracle/bin/python tools/oracle/compare.py .audit/oracle .audit/rust   # A2/A3/A4
COUNT=2000 ./scripts/fuzz.sh                                        # A7 深度模糊
```
