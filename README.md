# pypdf-rust

Rust 移植版 **pypdf 7.1.1**（PyO3 形态）：Python 侧 `import pypdf` 即可使用，行为目标与上游 pypdf 7.1.1 **100% 一致**。

## 目标与验收

| 门禁 | 要求 |
|---|---|
| A1 官方测试 | 上游 1,465 个测试函数（2,724 个实际用例）**全部通过**，failed/error = 0，skip/xfail 集合与基线逐项一致 |
| A2 Writer 字节级 | `PdfWriter` 输出与 pypdf 逐字节相同（含 xref、对象流、增量更新、加密） |
| A3 读侧语义级 | 对象图、元数据、页树、outline、注释、表单、XMP、文本提取的规范化 JSON 完全一致 |
| A4 观测级 | 异常类型/消息、`logging` 记录、`warnings` 与上游一致 |
| A5 可选依赖 | cryptography / Pillow / fontTools / brotli / arabic-reshaper / python-bidi 路径一致；图像编码与 Pillow 逐字节一致 |
| A6 符号覆盖 | 上游 1,934 个符号 100% 映射，脚本自动校验 |
| A7 鲁棒性 | 变异模糊测试无 panic，两侧成功/失败判定一致 |

详细的、可执行的判定定义见 [`docs/FIDELITY_CONTRACT.md`](docs/FIDELITY_CONTRACT.md)。

## 目录结构

```
crates/pypdf-rs-core/   # 纯 Rust 算法（PRNG、Python 兼容格式化、过滤器、编解码…）
crates/pypdf-py/        # PyO3 扩展（pypdf._pypdf）
python/pypdf/           # Python 包（模块树镜像上游；类定义 + 一行委托到 Rust）
tests/ resources/       # 上游官方测试与测试资源（原样）
vendor/                 # 上游源码参照与第三方编码器（pypdf / zlib-ng / libjpeg-turbo）
tools/                  # oracle 差分、符号清单、代码生成、探针
docs/                   # 基线、契约、符号覆盖、验收报告
scripts/                # 环境搭建、门禁、探针脚本
```

## 环境

```bash
# 1) Rust 工具链（stable）+ maturin
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

# 2) 两个虚拟环境（oracle 参考实现 / dev 端口实现）
./scripts/setup_venvs.sh

# 3) 构建并安装本端口（venv-dev，绝不与上游 pypdf 共存）
./scripts/build.sh
```

## 使用说明

本库**不是新 API**：打包出的包名就是 `pypdf`（7.1.1），是上游 pypdf 的**直接替代品**，Python API 完全一致。
把项目里的 `pypdf` 换成本库即可，代码无需改动。

> 它与上游 pypdf **不能安装在同一个环境**（同一个 import 名）。装上本库等于替换 pypdf。

### 安装

```bash
# 方式 A：仓库自带脚本（装进 .venvs/dev，用于开发/验证）
./scripts/build.sh

# 方式 B：装进你自己的 venv（maturin 已安装时可离线构建）
pip install --no-build-isolation /path/to/pypdf-rust

# 方式 C：先出 wheel 再安装（wheel 可分发到同平台 + 同 Python 版本）
maturin build --release
pip install target/wheels/pypdf-7.1.1-*.whl
```

要求：Python **≥ 3.10**、Rust **≥ 1.83**、`maturin >= 1.7`。
扩展**未启用 abi3**（绑定本机 CPython 版本，如 `_pypdf.cpython-314-darwin.so`），换 Python 版本或平台需重新编译。
直接 `pip install /path`（不带 `--no-build-isolation`）会创建隔离构建环境并联网下载 maturin。

### 基本用法

```python
from pypdf import PdfReader

reader = PdfReader("input.pdf")          # 也接受 io.BytesIO、pathlib.Path
print(len(reader.pages))
print(dict(reader.metadata))             # {'/Producer': ..., '/CreationDate': ...}
print(reader.pages[0].extract_text())
print(reader.page_labels, reader.outline, list(reader.attachments))
```

```python
from pypdf import PdfReader, PdfWriter

# 合并（pypdf 5+ 已移除 PdfMerger，改用 append）
writer = PdfWriter()
for name in ["a.pdf", "b.pdf"]:
    writer.append(name)
writer.write("merged.pdf")

# 拆分 / 选页 / 插入
reader = PdfReader("merged.pdf")
writer = PdfWriter()
for page in reader.pages[1:3]:
    writer.add_page(page)
writer.insert_page(reader.pages[0], 0)
writer.write("subset.pdf")
```

```python
from pypdf import PdfReader, PdfWriter, Transformation

# 旋转 / 裁剪 / 变换（pypdf 7 起，页面需先属于某个 writer）
writer = PdfWriter(clone_from="in.pdf")
page = writer.pages[0]
page.rotate(90)
page.cropbox.lower_left = (10, 10)
page.add_transformation(Transformation().scale(0.5))
writer.write("transformed.pdf")

# 加密 / 解密
writer = PdfWriter()
writer.append("in.pdf")
writer.encrypt("user-pwd", "owner-pwd", algorithm="AES-256")   # 需要 cryptography
writer.write("encrypted.pdf")

reader = PdfReader("encrypted.pdf")
print(reader.is_encrypted)
reader.decrypt("user-pwd")     # 返回 PasswordType.OWNER_PASSWORD / USER_PASSWORD
```

```python
from pypdf import PdfReader, PdfWriter

# 表单
reader = PdfReader("form.pdf")
writer = PdfWriter(clone_from=reader)
writer.update_page_form_field_values(writer.pages[0], {"/Name": "Ada Lovelace"})
writer.write("filled.pdf")

# 图像提取（需要 Pillow）
page = PdfReader("with_images.pdf").pages[0]
for img in page.images:
    open(img.name, "wb").write(img.data)
```

### 可选依赖

与上游一致，按需安装；缺库时的异常/警告行为与 pypdf 完全相同：

| 功能 | 依赖 |
|---|---|
| AES 加密（`algorithm="AES-*"`） | `cryptography` |
| 图像提取 / 替换（`page.images`） | `Pillow` |
| 字体解析、CMap / CFF | `fonttools` |
| Brotli 流解码 | `brotli` |
| RTL 文本与外观流（阿拉伯文、希伯来文） | `arabic-reshaper` + `python-bidi` |

### 确认运行的是 Rust 版

```python
import pypdf, pypdf._pypdf
print(pypdf.__version__)        # 7.1.1
print(pypdf._pypdf.__file__)    # .../_pypdf.cpython-314-darwin.so
print(pypdf._debug_versions)    # pypdf==7.1.1, crypt_provider=(...), PIL=...
```

### 已知限制

- 只保证**行为与字节**和上游一致，不保证性能提升；本仓库未做系统基准测试。
- 存在一个非上游的私有调试接口（`pypdf._pypdf.cp_prng_set_seed(seed)` / `PYPDF_RS_PRNG_SEED`），用于让 `/ID` 等随机源可复现，日常使用不需要。
- 内部实现差异（日志函数名、私有导入别名等）见 [`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md)，均已判定为官方测试不可观测。

## 验证

```bash
# 官方测试（门禁 A1）
.venvs/dev/bin/python -m pytest tests -n auto -vv -p no:benchmark --junitxml=.audit/rust-junit.xml
.venvs/oracle/bin/python tools/baseline/snapshot.py .audit/rust-junit.xml --compare .audit/baseline-outcomes.json

# 差分（门禁 A2/A3/A4）
.venvs/oracle/bin/python tools/oracle/audit.py --out .audit/oracle --seed 42
.venvs/dev/bin/python   tools/oracle/audit.py --out .audit/rust   --seed 42
.venvs/oracle/bin/python tools/oracle/compare.py .audit/oracle .audit/rust

# 图像逐字节一致（门禁 A5）
./scripts/probe_image_parity.sh

# 符号覆盖（门禁 A6）
python tools/manifest/check.py --require 100
```

## 状态

**移植完成，全部门禁通过。** 详细证据见 [`docs/FIDELITY_REPORT.md`](docs/FIDELITY_REPORT.md)。

| 门禁 | 结果 |
|---|---|
| BUILD | PASS（`maturin develop --release`） |
| A1 官方测试 | **PASS** — `2691 passed / 25 skipped / 8 xfailed / 0 failed`，与基线**逐用例** `MATCH` |
| A2/A3/A4 差分 | **PASS** — 103 场景 `IDENTICAL`；writer 专项 53/53 文档**逐字节**一致 |
| A5 图像字节一致 | **PASS** — JPEG/PNG 与 Pillow 逐字节相同；110 个 PDF 图像/附件 sha256 一致 |
| A6 符号覆盖 | **PASS** — **1934/1934 = 100.00%**（59 模块） |
| A7 模糊 + 浮点 | **PASS** — 200 变异 PDF `IDENTICAL`；20 万随机 f64 零差异 |

实测耗时：全量官方测试约 51 秒（10 核并行）；`./scripts/gate.sh` 一键复现全部门禁。

架构：**所有 pypdf 类保持 Python 定义（shim），全部算法逻辑在 Rust**（PyO3 `#[pyfunction]`）——为满足官方测试对 `sys.settrace` 精确帧计数、`isinstance`/MRO/`pickle`/猴子补丁的断言。依赖（zlib/Pillow/fontTools/cryptography/bidi/XMP）全部经 PyO3 调用同一解释器实现，保证字节一致。已知且经判定不可观测的偏差见 [`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md)。

## 许可证

上游 pypdf 为 BSD-3-Clause；本仓库保留其 LICENSE 与版权声明。详见 `vendor/pypdf-7.1.1/LICENSE`。
