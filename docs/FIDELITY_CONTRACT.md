# 一致性契约（Fidelity Contract）

本文档定义「与 pypdf 7.1.1 100% 一致」的可执行含义，以及 P0 阶段通过实测确定的技术约束。任何实现与本文档冲突时，以本文档为准；本文档的修改需要重新跑对应探针并更新证据。

## 1. 一致性档位

| 档位 | 含义 | 适用 |
|---|---|---|
| **L2 字节级** | 输出字节流完全一致（sha256 相同） | `PdfWriter.write()/write_stream()` 全部输出（xref、trailer、对象流、增量更新、加密字典）、各过滤器编码、加密后的对象字节 |
| **L1 语义级** | 规范化 JSON 完全一致（类型标签 + 值 + 插入序） | 读侧对象图、元数据、页树、页标签、outline、注释、表单、附件、XMP、`extract_text()` 输出、图像流信息 |
| **L0 观测级** | 行为契约一致 | 异常类名/消息/顺序、`logging` 记录（logger 名/级别/渲染文本/顺序）、`warnings` 类别/消息、`Configuration` 全局状态、`repr()` 文本 |

**所有档位均要求 100%**；L0 之外不存在「看起来一样即可」的路径。

## 2. 规范化格式（差分比较用）

`tools/oracle/canon.py` 实现，规则：

- 每个值输出 `{"t": <Python 类型名>, "v": <载荷>}`；
- `bytes` → 小写 hex；`int` → 十进制字符串（支持任意精度）；`float` → `repr()`（最短往返表示）；
- `dict` → 按插入序的 `[key, value]` 对列表；`list`/`tuple` → 值列表；
- `set`/`frozenset` → **按规范化后的 JSON 排序**（消除哈希序）；`IndirectObject` → `[idnum, generation]`（`repr` 里的 `id(pdf)` 不进入比较）；
- 流对象附带 `data`：`get_data()` 的 sha256；
- 未知对象回退到 `repr`，并将内存地址（`0x…`、长整数）替换为 `ADDR`；
- 环检测：重复出现的容器输出 `{"t": "cycle", "v": <序号>}`。

差分运行要求：两侧使用**同一** `PYTHONHASHSEED`（建议 0）与同一确定性 PRNG 种子；输出目录中只比较 `audit.json`。

## 3. 确定性 PRNG（消除 `secrets` / `uuid` 的随机性）

pypdf 有 6 处运行时随机：
- `PdfWriter.generate_file_identifiers()` → `secrets.token_bytes(16)`（/ID）
- `_encryption` AlgV5 → `secrets.token_bytes(16)`（R5/R6 盐）
- `_encryption` R6 `/Perms` 随机位 → `secrets.token_bytes(4)`
- `_encryption` 文件密钥 → `secrets.token_bytes(Length/8)`
- `JavaScript` 动作名 → `uuid.uuid4()`
- 注释 `/NM` 名 → `uuid.uuid4()`

**实现**：`tools/oracle/prng.py` 用 splitmix64 替换 `secrets.token_bytes`/`randbits`/`token_hex`/`choice` 与 `uuid.uuid4`（`uuid.UUID(bytes=token_bytes(16), version=4)`）。`crates/pypdf-rs-core/src/prng.rs` 必须给出**逐字节相同**的流，并在**相同调用点、相同顺序**消费。生产模式使用系统熵（`getrandom`）。

splitmix64 定义（大端）：

```
state := seed
next() -> state += 0x9E3779B97F4A7C15 (mod 2^64)
          z = state
          z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9 (mod 2^64)
          z = (z ^ (z >> 27)) * 0x94D049BB133111EB (mod 2^64)
          return z ^ (z >> 31)
token_bytes(n) = 拼接 ceil(n/8) 个 next() 的大端字节，取前 n 字节
```

## 4. Python 语义边界（P0 实测结论）

| 需求 | 结论 | 采用方案 |
|---|---|---|
| `DictionaryObject` 必须是 `dict` | PyO3 `#[pyclass(extends=PyDict, subclass, dict)]` 可行：`isinstance(x, dict)` 为真、`dict` 方法完整、可被 Python 子类化、可挂任意属性、`deepcopy` 正常 | 用 `extends=PyDict`，`#[new]` 接受 `*args/**kwargs` 并忽略参数；**继承的 `dict.__init__` 会自动按 pypdf 语义填充内容**（已实测：`DictionaryObject({...})`、`**kwargs`、`[("k",1)]` 均与 dict 一致，非法入参报同样的 `TypeError`） |
| `ArrayObject` 必须是 `list` | 同上（`extends=PyList`），继承的 `list.__init__` 负责填充 | `#[new]` 接受 `*args/**kwargs` 并忽略 |
| `FloatObject` 必须是 `float` | **不可行**：`extends=PyFloat` 的 `tp_new` 由基类以空参数创建，值恒为 `0.0`，无法携带构造参数 | Python 骨架类 `class FloatObject(float)`，`__new__`/逻辑转发到 Rust |
| `NumberObject(int)` / `NameObject(str)` / `TextStringObject(str)` / `ByteStringObject(bytes)` | PyO3 不支持继承 `int`/`str`/`bytes` | Python 骨架类（与 pypdf 定义方式一致），方法体为对 Rust 的调用 |
| macOS 扩展模块链接 | 普通 `cargo build` 缺 `-undefined dynamic_lookup` | 仓库根 `.cargo/config.toml` 已配置；maturin 会自动处理 |
| abi3 | `extends=` 依赖完整 CPython ABI | **禁用 abi3**，仅面向 CPython 3.14 |

## 5. 图像/字体（可选依赖）一致性

### 5.1 实测结论（P0 探针，`scripts/probe_image_parity.sh`）

| 项目 | 结论 | 证据 |
|---|---|---|
| JPEG | libjpeg-turbo **3.2.1** + `jpeg_set_defaults` + `jpeg_set_quality(75, TRUE)` → 与 Pillow 13.0.0 输出**逐字节相同** | `.audit/imgprobe/vendored.jpg == pillow.jpg` |
| PNG | zlib-ng **2.3.3**（`ZLIB_COMPAT=ON`、默认 `WITH_NEW_STRATEGIES=ON`）+ level 6 / windowBits 15 / memLevel 9 / `Z_FILTERED`，按行喂入 → 与 Pillow 的 IDAT **逐字节相同** | `.audit/imgprobe/vendored_idat.bin == pillow_idat.bin` |
| Pillow PDF 插件 | RGB 图像嵌入的 DCT 流与独立 JPEG 编码结果**完全相同**（pypdf 的 `ImageFile.replace → save(b, "PDF")` 路径可字节复现） | `probe.json: jpeg_matches_pdf_dct=true` |
| 参考 Pillow 内置库 | libjpeg（turbo 3.2.x 家族）、zlib-ng 2.3.3（报告 zlib 1.3.1 兼容串）、libtiff 4.7.2、openjpeg 2.5.4、libpng16、freetype 2.14.3、harfbuzz 14.5.0、fribidi 1.0.17、libwebp 1.6.0 | `.audit/pillow_versions.json`、`PIL/.dylibs/libz.1.3.1.zlib-ng.dylib` 内嵌 `zlib-ng 2.3.3` |
| 关键陷阱 | **Homebrew 的 zlib-ng-compat 2.3.3 输出与 Pillow 不同**（同为 2.3.3，sha 不同）；必须使用仓库内 `vendor/zlib-ng` 按 `ZLIB_COMPAT=ON` 默认选项自建 | 探针 variant 对比：自建 A/C 命中，brew 版本不命中 |

**依赖策略**：`vendor/zlib-ng`（2.3.3）与 `vendor/libjpeg-turbo`（3.2.1）入库，Rust 侧通过 build 脚本静态链接；**不依赖系统库版本**，保证可复现字节。

### 5.2 Pillow 编码参数（来自 Pillow 13.0.0 源码 `src/libImaging/ZipEncode.c`）

```
compress_level = optimize ? Z_BEST_COMPRESSION : ctx->compress_level   # 默认 6
compress_type  = (mode == ZIP_PNG) ? Z_FILTERED : Z_DEFAULT_STRATEGY
deflateInit2(level, Z_DEFLATED, 15, 9, compress_type)                 # windowBits=15, memLevel=9
每行：deflate(Z_NO_FLUSH)；结束：循环 deflate(Z_FINISH)
PNG 行过滤器（None/Sub/Up/Average/Paeth + 自适应选择）在 C 编码器内部完成
```

### 5.3 fontTools

pypdf 仅**读取**字体（`TTFont` 表访问、`cffLib.CFFFontSet` 解析）用于 `from_truetype_font_file` 与 CFF 编码推导，**不写出、不做子集化**。因此 fontTools 一致性为**表值级**：Rust 解析出的表字段与 fontTools 逐字段相等（在 P4 用样例字体验证）。

## 6. 日志与警告

- `logger_warning(message, source=__name__, **values)` → `logging.getLogger(<pypdf 模块名>).warning(message, values)`；Rust 实现必须用同一 logger 名与同一 `%` 格式消息，官方测试有 563 处 `caplog` 断言。
- `warnings.warn(msg, DeprecationWarning, stacklevel=…)` 必须真实发出 Python 警告（测试配置 `filterwarnings = ["error"]`）。
- `repr` 相关断言：`repr(EmbeddedFile)` 内嵌 `hash(b"")` → Rust 侧通过调用 Python 内建 `hash` 得到一致值。

## 7. 判定与证据链

| 门禁 | 工具 | 证据文件 |
|---|---|---|
| A1 官方测试 | `pytest tests` + `tools/baseline/snapshot.py --compare` | `.audit/rust-junit.xml`、`docs/BASELINE.md` |
| A2/A3/A4 差分 | `tools/oracle/audit.py`（两侧）+ `tools/oracle/compare.py` | `.audit/{oracle,rust}/audit.json` |
| A5 图像/字体 | `scripts/probe_image_parity.sh` + 图像差分场景 | `.audit/imgprobe/*` |
| A6 符号覆盖 | `tools/manifest/check.py --require 100` | `docs/SYMBOL_COVERAGE.md` |
| A7 鲁棒性 | 变异模糊测试 | `.audit/fuzz/report.json` |

任何一项未达 100%，不得声明完成；无法调和的偏差必须先与用户确认后再记录，不得静默降级。
