# 移植指南（Porting Guide）

本文件是 pypdf 7.1.1 → Rust 的逐模块移植规范。**照抄上游只为参考，不复制代码**：每个函数/方法都要在 Rust 里逐行等价重写，并逐条对应上游语义（包括 bug 与 warning）。

## 0. 铁律

1. **行为优先于优雅**：上游怎么处理（包括"看起来是 bug"的行为）就怎么实现；发现分歧先查上游源码与测试断言。
2. **不得静默降级**：任何无法 100% 对齐的点必须记录在 `docs/KNOWN_ISSUES.md` 并上报，不得用"近似"顶替。
3. **不得 panic**：Python 侧可观察的错误一律 `Result` → Python 异常；`unwrap()` 只允许在已证明不可失败的场景，且需注释原因。
4. **确定性**：任何进入输出（PDF 字节、文本、日志、dict 顺序）的遍历都必须确定。Rust `HashMap`/`HashSet` 禁止用于可能影响输出的地方；需要集合语义时用 Python `dict`/`set`（与 pypdf 完全一致）或保证排序的 `IndexMap`/`BTreeMap`。
5. **随机性只走 `pypdf_rs_core::prng`**：禁止 `rand`/`getrandom` 直接出现在移植代码里（生产模式由统一入口选择系统熵）。
6. **1:1 命名**：Rust 函数/方法名与上游 snake_case 一致；模块与文件按上游模块一一对应，便于逐行对照与覆盖检查。

## 1. 对象模型（最关键）

pypdf 的对象是 Python 对象。我们的 Rust 扩展**直接操作 Python 对象**（PyO3），不自建 Rust 对象图。

| 上游类 | 实现方式 |
|---|---|
| `DictionaryObject` 及其子类（`StreamObject`、`TreeObject`、`PageObject`、`Field`、`Destination`、注释类…） | `#[pyclass(extends=PyDict, subclass, dict)]` + `#[new]` 接受 `*args/**kwargs`（内容由继承的 `dict.__init__` 填充，与 pypdf 完全一致） |
| `ArrayObject` 及其子类（`RectangleObject`、`Transformation` 不是） | `#[pyclass(extends=PyList, subclass, dict)]` |
| `FloatObject` / `NumberObject` / `NameObject` / `TextStringObject` / `ByteStringObject` | Python 骨架类（`python/pypdf/generic/_base.py`，手写、固定），方法体调用 Rust 实现——见 §2 |
| `NullObject` / `BooleanObject` / `IndirectObject` / `IndirectReference` / `PdfReader` / `PdfWriter` / `XmpInformation` / `PaperSize` / `PageRange` / `ContentStream` 等 | 普通 `#[pyclass(subclass, dict)]` |
| 异常类 | `pypdf/errors.py`（手写 Python，逐字保留上游继承关系） |
| `Protocol` / `TypedDict` / 枚举 | Python 骨架（类型与枚举成员对外是 Python API） |

### 1.1 从 Rust 创建对象

优先**通过 Python 类构造**，保证 `__init__`、校验、继承链与 pypdf 一模一样：

```rust
let cls = py.import("pypdf.generic")?.getattr("DictionaryObject")?;
let dict = cls.call1((existing_dict,))?;          // DictionaryObject(existing_dict)
```

高频路径可先构造空对象再填充（与 pypdf 解析器行为一致）：

```rust
let obj = Py::new(py, DictObject::new())?;        // 空 dict 子类
let as_dict: &Bound<PyDict> = obj.bind(py).downcast()?;  // 等价于 BaseType
as_dict.set_item(NameObject::new(py, "/Type")?, ...)?;
```

### 1.2 跨类调用

不要把 Python 逻辑翻译成 Rust 内部函数调用后绕开对象语义：**该调用的方法就调用**（例如 `page.extract_text()` 内部应调用 `content_stream.operations` 等公开行为），保证 monkeypatch/子类覆盖行为与 pypdf 一致（官方测试大量使用）。

### 1.3 动态属性

pypdf 到处挂属性（`indirect_reference`、`pdf`、`original_page`、`_data`…）。pyclass 上用 `#[pyclass(dict)]` 允许任意属性，并对高频属性提供 `#[pyo3(get, set)]` 字段以减少开销。

## 2. 值类型骨架（固定写法）

`FloatObject`/`NumberObject`/`NameObject`/`TextStringObject`/`ByteStringObject` 无法用 PyO3 继承内建类型，按下面方式手写（`python/pypdf/generic/_base.py`），**逻辑全部在 Rust**：

```python
class FloatObject(float):
    def __new__(cls, value="0.0", context=None):
        return _rust.generic.float_new(cls, value, context)
    def myrepr(self):
        return _rust.generic.float_myrepr(float(self))
    def write_to_stream(self, stream):
        return _rust.generic.float_write_to_stream(self, stream)
```

Rust 侧 `#[pyfunction]`（注册在 `pypdf._pypdf.generic` 子模块）负责实际逻辑。禁止在骨架里写业务分支。

## 3. 错误、日志、警告

- 异常：`crate::errors` 提供与 `pypdf/errors.py` 同名的 `PyErr` 构造器（`pdf_read_error(msg)`, `wrong_password_error()`…），消息模板逐字复制。
- `logger_warning(message, source=__name__, **values)` → 调 Python `logging.getLogger(source).warning(message, values)`，`source` 用上游模块名（如 `"pypdf._reader"`）。
- `warnings.warn(...)` → 调 Python `warnings.warn`，类别与 `stacklevel` 保持一致。
- 官方测试有 563 处 `caplog` 断言，日志文本与顺序都是验收内容。

## 4. 确定性

- 加密随机数与 `/ID`、`uuid4`：只用 `pypdf_rs_core::prng`（`SplitMix64`），调用点与顺序必须与上游一致（`docs/FIDELITY_CONTRACT.md` §3）。
- 依赖 Python `set` 顺序的代码（上游如此）：Rust 侧也必须用 Python `set` 并保持同样的迭代方式；若必须排序，确认上游没有排序。
- 浮点文本：只用 `pypdf_rs_core::pyfmt::{py_float_repr, py_float_fixed, float_myrepr}`（已被 20 万随机值验证与 CPython 一致）。

## 5. 符号覆盖注解

每个移植项上方加一行注解（门禁 A6 依赖它）：

```rust
// @pypdf: pypdf.generic._base.NameObject.write_to_stream
fn write_to_stream(...) { ... }
```

- 类：`// @pypdf: pypdf._page.PageObject`
- 方法：`// @pypdf: pypdf._page.PageObject.extract_text`
- 方法内的私有辅助函数：不需要注解
- 纯数据模块（常量表/编码表）：`// @pypdf-module: pypdf.constants`

`python tools/manifest/check.py --write-report docs/SYMBOL_COVERAGE.md` 生成覆盖报告。

## 6. 测试与验证流程

每个模块移植完成后必须：

1. `./scripts/build.sh` 编译通过、`cargo test -p pypdf-rs-core` 通过；
2. 跑该模块对应的官方测试文件（例如 `tests/generic/test_base.py`）；
3. 跑差分：`tools/oracle/audit.py`（`pypdf` 模块相关场景）；
4. `tools/manifest/check.py` 覆盖数上升，并在 commit message 中注明阶段。

## 7. 文件布局（并行友好，重要）

```
crates/pypdf-py/src/
  lib.rs                 # 公共入口（只由 lead 修改）
  errors.rs pyutil.rs    # 公共助手（只由 lead 修改）
  modules/               # ← 每个移植模块一个文件，可并行新增
    pagerange.rs         #   参考模板（PageRange / parse_filename_page_ranges）
    utils_mod.rs         #   pypdf/_utils.py
    base_mod.rs          #   pypdf/generic/_base.py
    data_structures_mod.rs
    ...
python/pypdf/            # 对应模块的 shim：从 pypdf._pypdf 导入 Rust 实现
```

- **新增模块 = 新增 `src/modules/<name>.rs` + 对应 Python shim**；`build.rs` 会自动扫描目录并调用每个模块的 `register(module)`，**不需要修改 `lib.rs`**，因此多个任务可以并行。
- 每个模块文件必须导出：
  ```rust
  pub fn register(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> { ... }
  ```
- 模块内部若需要多个文件，可在 `src/modules/<name>/` 下建子模块，由 `<name>.rs` 用 `mod` 声明（相对路径解析）。
- 共享助手只用 `crate::errors` 与 `crate::pyutil`；需要新的公共助手时向 lead 申请，不要改 `lib.rs`。
- 完成后把 `python/pypdf/<path>.py` 的 stub 替换为 shim（从 `pypdf._pypdf` 导入），由 lead 负责更新 `port_status.json` 与覆盖率报告。

## 7.1 实战踩坑清单（来自 pagerange 移植）

1. 官方测试会访问**私有属性**（如 `pr._slice`）：pyclass 必须用 `#[getter(_slice)]`/`#[setter(_slice)]` 暴露，命名与原属性一致。
2. Python C-API 细节要照抄：`re.Match.group` 是**变参**（`m.group(4, 6, 8)`），传元组会被当成组名而抛 `IndexError: no such group`。
3. `slice(None, None, None)` 合法且等价 `slice()`；不要"省略"尾随 None，参数个数会影响 `str/repr/indices`。
4. 上游把逻辑写在 `__init__` 里时，Rust `#[new]` 要承载同样校验与副作用；容器类的**内容填充由继承的 `dict/list.__init__` 完成**（不要在 `#[new]` 里重复填充）。
5. 类需要能被子类化 / 挂任意属性：`#[pyclass(subclass, dict)]`。
6. 涉及 Python 对象的比较/加法/字符串化时优先走 Python 运算符（`Bound::gt/eq/...`、`call_method1("__add__", ...)`），保证与上游完全一致。
7. 测试断言 `hash()`/`repr()`/异常消息这类"观测值"时，必须逐字复刻上游表达式（例如 `hash((self.__class__, (start, stop, step)))`）。

## 8. 提交流程

- 一个阶段一个 commit；commit message：`P2: port generic._base (PdfObject/NullObject/... )`。
- 提交前跑：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python tools/manifest/check.py`。
