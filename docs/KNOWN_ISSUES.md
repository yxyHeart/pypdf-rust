# 已知偏差（Known Issues / Deliberate Deviations）

本文件记录「与上游 pypdf 7.1.1 可观测差异」的**全部已知情偏差**及其影响评估。门禁 A1（官方测试零容忍）与 A2–A4（差分）通过后，本清单中的条目仍必须满足：**没有任何官方测试断言能观测到它们**；一旦能观测到，必须消除而不是记录。

格式：每条 = 现象 + 为什么这样选 + 可观测性判定 + 处置。

---

## 架构级（全局）

### D1 类保持 Python 定义，Rust 只承载逻辑
- **现象**：所有类（含容器 `DictionaryObject`/`ArrayObject`）都是 Python 类（shim 中按上游定义），方法体一行委托到 Rust `#[pyfunction]`；而不是 Rust pyclass。
- **原因**：官方测试统计 `sys.settrace` 的**精确调用帧数**（如 `test_cloning_direct_objects__function_calls_and_exceptions` 的 3200/4200 帧差断言），并大量依赖 `isinstance`、MRO、`pickle`/`deepcopy`、猴子补丁；PyO3 无法让 pyclass 继承任意 Python 类（`PdfObject` 是 Python 类）。若容器做成 pyclass，`isinstance(x, PdfObject)` 与帧计数都会破。
- **可观测性**：这是**为了满足可观测性**而做的选择，本身不是偏差。
- **处置**：已在 `docs/PORTING_GUIDE.md` §1 固化；`generic/_base`、`_utils`、`generic/_utils`、`_codecs` 已按此实现并通过 oracle 对照。

### D2 `PdfObject` 判定用 `is_pdf_object_like`
- **现象**：上游 `assert isinstance(x, PdfObject)` 的两处 `# mypy` 断言，在端口里改用 `base_mod::is_pdf_object_like`（先真 `isinstance`，再按 PDF 对象surface 判定）。
- **原因**：容器类在 Python 侧继承 `dict/list`（与上游一致），但它们的 `PdfObject` 基类关系通过 shim 显式声明；极端 monkeypatch 场景下 `isinstance` 可能不成立。
- **可观测性**：官方测试不区分该分支（仅防 mypy 断言误报）。
- **处置**：无。

### D3 帧结构近似点
- `_reference_clone` 的 AttributeError 探测与 `renumber_table[...]` 的 KeyError 在 Rust 内被捕获，因此 `sys.settrace` 的 **exception 事件**比 oracle 少 2 个；官方断言只用**帧差相等**（`exceptions_2000 == exceptions_1000`），已验证通过。
- `TextStringObject.write_to_stream` 不再逐字节产生 `typing.cast` 帧（Rust 循环），输出字节逐字节一致；没有测试统计该写路径帧数。
- **处置**：若未来测试开始统计这些路径的帧数，需要按上游补 Python 层 wrapper。

---

## `pypdf._codecs._codecs`

| # | 偏差 | 可观测性 | 处置 |
|---|---|---|---|
| C1 | `Codec` 由 `#[pyclass(subclass)]` 实现而非 Python `ABC`：`Codec()` 抛 `TypeError: cannot create ...`（上游 ABC 文案不同），无 `ABC`/`_abc_impl` | 官方测试只断言 `TypeError` 类型 | 已接受 |
| C2 | 实例状态是 Rust get/set 属性（非 `__dict__`）：`vars(codec)` 为空、`del codec.x` 不支持、`dir(class)` 列描述符 | 无测试观测 | 已接受（换来 3.4× 解码性能） |
| C3 | 数值状态用 `i64`：手工给私有字段赋超大整数会 `OverflowError`（上游为任意精度） | 仅手工构造私有状态时可达 | 已接受 |
| C4 | `inspect.signature(LzwCodec)` 显示 `max_output_length=None` 占位（实际默认 75,000,000） | 无测试观测 | 已接受 |
| C5 | 警告直接 `logging.getLogger("pypdf._codecs._codecs").warning(...)`，因此 monkeypatch `pypdf._utils.logger_warning` 不会拦截（上游同样只在模块加载时绑定，但其模块级名字可被 patch） | 无测试观测 | 已接受 |

`Codec.encode/decode` 抽象方法在端口里抛 `NotImplementedError`（上游抽象桩体返回 `None`），仅显式调用可达。

---

## `pypdf._utils`

| # | 偏差 | 可观测性 | 处置 |
|---|---|---|---|
| U1 | `classproperty` 与 `deprecate_positional_arguments` 留在 shim（`# shim-only`）：PyO3 的 `__get__` 槽中介会拒绝上游的一参形式 `descr.__get__(instance)`；装饰器需要与 `inspect.signature`/`functools.wraps` 完全一致 | 二者是 Python 协议胶水；179/179 官方测试通过 | 已接受（标注 `@pypdf:` 计入 A6） |
| U2 | `Version.__init__` 直接调用 Rust `_parse_version` 而非动态派发 `self._parse_version`（Python 子类覆盖不生效） | 无 in-tree 使用 | 已接受 |
| U3 | `File`/`Version`/`_TraversalState` 为 pyclass 而非 dataclass：无 `__dataclass_fields__`/`__match_args__`/`File.IndirectObject` 类属性；无 `__dict__` 形式的 pickle 协议 | `__init__` 解析、`repr`/`str`/`eq`/`hash` 已按 oracle 对齐；无测试观测 | 已接受 |
| U4 | `File`/`_TraversalState.__eq__` 对异类返回 `False` 而非 `NotImplemented` | 仅当另一操作数自定义 `__eq__` 时可观测 | 已接受 |
| U5 | `is_char_neutral/rtl`/`_TraversalState` 的签名默认值显示为 `None`（行为一致） | `inspect.signature` 才可见 | 已接受 |
| U6 | `read_until_regex(length=…)` 默认 `2**63-1`（= `sys.maxsize`，仅 64 位 CPython 成立） | 目标平台固定 | 已接受 |

---

## `pypdf.generic._base`

| # | 偏差 | 可观测性 | 处置 |
|---|---|---|---|
| B1 | `TextStringObject.write_to_stream` 不产生逐字节 `typing.cast` 帧（见 D3） | 无 | 已接受 |
| B2 | `IndirectObject.clone` 只读一次 `dup.indirect_reference`（属性无副作用） | 无 | 已接受 |
| B3 | `_base` 模块级导入（`binascii`/`codecs`/`iter_unpack`/`_pdfdoc_encoding_rev` 等）不在 shim 中重导出 | 非 API、无测试引用 | 已接受 |
| B4 | `FLOAT_WRITE_PRECISION` 在调用时从 shim 读取后传给 Rust（用户调大/调小仍走同一算法） | 与上游一致（上游也是模块级常量读取） | 无偏差 |

---

## `pypdf.generic._utils`

无已知偏差（2073 项差分语料 + 1250 条 fuzz 零差异，流调用轨迹一致）。

---

## `pypdf.filters`

| # | 偏差 | 可观测性 | 处置 |
|---|---|---|---|
| F1 | `FlateDecode._decode_png_prediction` 对**负 columns**（私有 API）抛 `ValueError("negative count")` 而非模拟 Python 负索引循环 | `_get_parameters` 强制 ≥1，公开路径不可达；无测试 | 已接受（`columns==0`/负 `row_length` 仍复刻上游 `ZeroDivisionError`/`ValueError`） |
| F2 | 配置上限以 `i128` 读取；若用户把配置项覆盖成 float，端口抛 `TypeError` 而非 Python 比较语义 | 官方覆盖值均为 int（含 `sys.maxsize`） | 已接受 |
| F3 | 模块级函数是 PyO3 builtin（`inspect.signature`/`__code__`/`__doc__` 文本不同）；类仍是 Python 且保留上游类属性与 dataclass 语义 | 无测试观测 | 已接受 |
| F4 | `_SINGLE_BYTES` 保留在 shim（API）；Rust 回退路径每次新建 1 字节对象而非索引 | 无 | 已接受 |
| F5 | 旧版 `MAX_*` 常量 / `CCITTParameters` 字段放在 shim（数据）；`filters_mod.rs` 用注释块登记其 A6 注解（56/56 仍覆盖） | 无 | 已接受 |
| F6 | 过滤器名判断用 `name == literal` 而非 tuple-contains 顺序；仅对非 str 的 `/Filter` 覆盖有差异 | 无测试观测 | 已接受 |

差分证据：49 组 / 约 900 个确定性子用例（含 predictors 1–15、低比特深度 #4308 上游 bug、`decompress` 回退与限额消息、CCITT 头字节、JBIG2 错误路径、间接 `/Filter`/`/DecodeParms`）在 oracle 与 dev 下输出**字节一致**。

---

## `pypdf._encryption` / `_crypt_providers`

| # | 偏差 | 可观测性 | 处置 |
|---|---|---|---|
| E1 | PRNG 统一入口：Rust 在 `PYPDF_RS_PRNG_SEED` 或私有 `pypdf._pypdf.cp_prng_set_seed(seed)` 存在时使用 `SplitMix64`，否则经 PyO3 调 `secrets.token_bytes`（生产用系统熵）。上游调用点与顺序完全保留 | 差分两侧用同一确定性流，字节一致（0 mismatch） | 已接受（唯一入口在 `crypt_providers_mod::token_bytes`） |
| E2 | provider 函数在调用时从 `pypdf._crypt_providers` 模块命名空间解析（保持 monkeypatch 生效），而非上游"import 时快照" | 只有在 `_encryption` 导入后 monkeypatch `pypdf._crypt_providers.*` 才可观测 | 已接受 |
| E3 | 纯数据/API（`_PADDING`、`PasswordType`/`EncryptAlgorithm` 成员、`crypt_provider` 等）留在 shim；A6 注解记录在 Rust（同 filters F5 模式） | 无 | 已接受 |
| E4 | `AlgV4`/`AlgV5` 超长逐算法 docstring 被缩短（仅 `__doc__`） | 无测试观测 | 已接受 |
| E5 | `_pycryptodome` 驱动 `Crypto` 经 PyO3，但本机未装 pycryptodome，运行时不可验证（provider 选择走 `cryptography`） | 与上游同样在缺库时 ImportError | 已记录 |
| E6 | 扩展模块新增两个**非 pypdf API** 私有符号：`cp_prng_set_seed`、`CRYPT_DEPENDENCY_ERROR_STR` | 不属于 `pypdf` 公共 API | 已接受 |

差分证据：18 个加密资源读侧（字段/口令探测 136 次/密钥路径 25 条/对象密文 102 条）+ 10 组写侧（RC4-40/RC4-128/AES-128/R5/R6 × owner 两种）+ 114 个畸变 `/Encrypt` 字典的错误/告警路径 → **0 mismatch**。

---

## 工具链备注

- `tools/fuzz/*` 的语料是对 `resources/` 做确定性结构变异生成；在模块未移植完成前，dev 与 oracle 的差异来自 stub 行为（`TypeError: 'NoneType' ...`），属于"未完成"而非偏差。
- stub 生成器只覆盖带 `AUTO-GENERATED stub` 标记的文件；手写 shim 不会被覆盖（`--force` 除外）。
