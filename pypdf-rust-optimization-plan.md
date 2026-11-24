# pypdf → Rust 性能优化完整方案

> **文档类型**：性能优化技术方案 / Agent 执行规范  
> **适用对象**：现有 pypdf Rust 端口（Python API + PyO3 + Rust 核心）  
> **方案版本**：v1.0  
> **制定依据**：用户提供的 `validation/results/perf-report.md`、W1–W8 基准结果及 `issue-301.pdf` 内容流热点分析  
> **优化目标**：在不改变 pypdf 对外 API、PDF 处理语义、安全限制和现有工作流的前提下，消除 Python ↔ Rust 热路径上的逐 Token 往返，并逐步把内容流解析及高收益消费路径迁移为 Rust 内部闭环。

---

## 0. Agent 执行说明

本方案是可执行的工程任务书。Agent 必须先核实仓库现状，再按阶段推进；不能仅根据本文中的示意模块名直接覆盖现有实现。

### 0.1 当前已知基线

| 项目 | 当前已知结果 |
|---|---|
| 基准范围 | W1–W8，共 8 个工作负载 |
| 运行方式 | 两侧独立进程；每侧 1 次预热、10 次重复；取中位数 |
| 结果 | 8/8 工作负载中 Rust 端口慢于原版 pypdf |
| 速度比范围 | 0.25×–0.40×，即端口约慢 2.5–4 倍 |
| 峰值 RSS | 最差约 +12%；W4 约 −0.6% |
| 输出 | 当前 8 个基准工作负载输出字节数和 SHA-256 一致 |
| 环境干扰 | loadavg 约 22–30；repeat=2/5/10 三轮方向一致 |
| 热点样例 | `resources/issue-301.pdf` |
| 已观察到的跨语言调用 | `read_object` 约 18.7 万次；`_check_limits` 约 31.7 万次 |
| 当前优化后 | 相关 Python 层调用从约 50 万降至约 2 万，但仍未消除逐对象 Python 物化 |
| 特殊样例 | `png-predictors.pdf` 的 PNG predictor 解码快 2.64×；`selenium-pypdf-issue-177.pdf` 读取快 1.38× |

这些数字是本轮给定数据，不代表其他机器或其他 PDF 集合上的普遍结论。Agent 必须从仓库和报告重新核实数字、测试命令、样本文件及调用路径。

### 0.2 必须遵守的原则

1. **不改变工作流**：不删步骤、不跳过 PDF 解析或解码、不将原有 Poppler/Pillow 等外围环节合并或替换为捷径。
2. **先保持语义，再优化性能**：解析结果、异常类型/时机、对象可变性、页面顺序、资源引用、加密行为和输出行为均需验证。
3. **不把“无回调”误判为“无开销”**：还要测量 Python 对象创建、拷贝、分配、GIL 持有、缓冲区转换和 Rust/Python 数据物化。
4. **不把 tokenizer 当成全部优化**：W1/W2、W4、W5–W7 有独立的结构解析、解码、对象图和序列化成本，必须分开处理。
5. **不先做无证据的微优化**：类引用缓存、`getattr` 缓存、删除 shim、内存池等必须有独立 profile 证据后才排期。
6. **保留 Python 兼容层**：公开 API 按原版 pypdf 基线工作；Rust 内部可以采用不同的数据结构，但不能把 Rust 内部结构泄漏成破坏兼容性的 Python API。
7. **所有性能结论必须可复现**：保留输入文件哈希、环境、命令、原始测量值和报告生成脚本。

### 0.3 执行前必须完成的仓库审计

```bash
pwd
git status --short
git rev-parse HEAD
find . -maxdepth 4 -type f | sort | head -300
rg -n "read_object|_check_limits|ContentStream|parse_content_stream|operations|extract_text|CCITT|TIFF" . --glob '!**/.venv/**' --glob '!**/target/**'
rg -n "PyO3|pyo3|#[[]pyfunction|Python<'|Py<|Bound<" . --glob '*.rs' --glob 'Cargo.toml'
find validation -maxdepth 4 -type f | sort | head -300
```

形成 `validation/optimization/REPO_AUDIT.md`，至少记录：

- 当前 Git commit、工作区是否干净、Python/Rust/pyo3 版本；
- W1–W8 基准入口、参数、输入样本及输出校验方法；
- `issue-301.pdf` 对应解析调用链和调用次数统计来源；
- 当前 Rust 入口、Python shim、回调函数和对象转换位置；
- 内容流解析的实际入口及 `operations` 对外暴露方式；
- 当前文本提取、图像解码、Writer、Clone 的代码入口；
- A1 的 2691 个测试、103 场景差分、150 个 Round-trip 用例的实际执行命令和覆盖范围；
- 目标机器架构、CPU/NUMA、内存、内核、Python、glibc、Rust target 和离线构建约束。

审计未完成，不得开始大规模重构。

---

## 1. 现状分析与问题拆解

### 1.1 已有证据支持的主要瓶颈

对 `issue-301.pdf`，当前 profile 显示内容流解析路径存在大量逐 Token Python 回调：

- `read_object` 约 18.7 万次；
- `_check_limits` 约 31.7 万次；
- `parse_content_stream` 和 `read_object` 对应 Python 层耗时占比很高；
- 优化回调路径后，Python 层调用数已由约 50 万降低至约 2 万，但性能仍显著落后。

据此，当前可以确认的架构问题是：**Rust 解析过程仍然依赖 Python 完成逐对象解析/构造或频繁交互，跨语言边界和 Python 对象物化处于热路径。**

但不能仅凭 `cProfile` 中某个 Python 函数的累计耗时，就把全部时间都归因于 PyO3 边界。Python 函数可能包含 Rust 执行、对象创建、分配、拷贝等多种成本。后续必须通过显式计数器、采样 profile 和分阶段微基准进一步分解。

### 1.2 八个工作负载的独立问题

| 工作负载 | 原版 pypdf | Rust 端口 | 端口/原版 | 当前延迟变化 | 主要调查方向 |
|---|---:|---:|---:|---:|---|
| W1 Reader 初始化 + 元数据 | 12.6 ms | 34.9 ms | 0.36× | +177% | Header、xref、trailer、对象解析、Reader 初始化和 Python 包装开销 |
| W2 全页页框遍历 | 221.7 ms | 561.1 ms | 0.40× | +153% | 页树遍历、间接引用解析、继承属性、对象缓存 |
| W3 文本提取 | 178.5 ms | 706.6 ms | 0.25× | +296% | 内容流解析、Token 物化、操作符解释、字体/CMap/文本拼接 |
| W4 图像枚举/提取 | 608.2 ms | 1.865 s | 0.33× | +207% | XObject 枚举、过滤器链、CCITT/TIFF 解码、数据复制 |
| W5 合并 | 11.8 ms | 40.8 ms | 0.29× | +247% | 对象图复制、引用去重、资源合并和写入 |
| W6 页面操作后写出 | 15.2 ms | 54.1 ms | 0.28× | +255% | 页面变换、资源处理、对象图修改和序列化 |
| W7 Clone 写出 | 20.9 ms | 70.8 ms | 0.29× | +240% | Clone 语义、对象图复制、间接引用和序列化 |
| W8 端到端 | 445.8 ms | 1.361 s | 0.33× | +205% | 多阶段成本叠加；必须通过阶段计时拆分 |

**结论**：P0 优先处理 W3 的内容流热路径，同时保证这项改造不把 W1/W2/W4/W5–W7 的其他瓶颈掩盖。不能声称 tokenizer 优化必然解决全部八项。

### 1.3 目标架构

```text
Python public API / pypdf-compatible facade
                  |
                  | one coarse-grained call per stream / operation batch
                  v
          PyO3 boundary layer
                  |
                  v
        Rust Content Stream Engine
        ├── byte scanner / tokenizer
        ├── PDF content-object parser
        ├── compact token / operation representation
        ├── inline-image boundary handler
        ├── resource / limit checks
        ├── optional native text-content consumer
        └── lazy Python materialization at compatibility boundary
                  |
                  v
        Python pypdf-compatible objects
        only when public API or Python callbacks require them
```

关键不是简单地把 Python 循环搬进 Rust，而是减少跨边界粒度：

- 当前：一个 Token/对象一次或多次 Python↔Rust 交互；
- 目标：一个完整内容流或一个批次一次进入 Rust；
- Rust 内部完成扫描、解析、校验、组织数据和可下沉的消费逻辑；
- 只有公开 Python API、visitor/callback 或需要 Python 对象语义的路径才物化对象。

---

## 2. 总体技术决策

### 2.1 先做的事

1. 对当前 8 个基准补充分阶段计时和跨语言边界计数；
2. 建立纯 Rust 字节级内容流 Scanner/Tokenizer；
3. 用 Rust 原生紧凑表示保存 Token 和 Operation；
4. 保留 Python 兼容适配层，按需将 Rust 表示物化为原版对象；
5. 让文本提取热路径尽量直接消费 Rust Operation，避免先物化成 Python 对象再重新遍历；
6. 单独处理图像解码、Reader/页树、Writer/对象图，不与 tokenizer 混合提交。

### 2.2 不建议的做法

- 不建议只写一个 Rust tokenizer，解析后立即对每个 Token 调用 `Py::new` 或 `PyObject` 构造 Python 对象，再返回 Python `list`。这会消除回调，却保留大量 Python 对象分配和转换成本。
- 不建议在第一阶段重写整个 PDF Reader/Writer、字体/CMap、图像解码和文本布局系统。这会扩大正确性风险，也无法归因收益。
- 不建议直接把 `.operations` 改成 Rust 自定义代理对象，除非仓库审计证明原版 API 的可变性、类型和身份语义都能兼容。优先保持公开属性的类型和行为。
- 不建议为了 W3 优化删除 visitor 回调、文本布局模式、方向过滤、字体回退、错误恢复或资源限制。
- 不建议将 CCITT、LZW、CMap 等实现全部自行重写。优先评估已有依赖和现有实现，依赖更换必须独立验证许可证、平台支持和结果语义。

### 2.3 数据表示原则

Rust 内部可采用紧凑表示，但具体类型必须从实际 pypdf 基线语义推导，不能直接把下列示意结构照搬到生产代码：

```rust
// 示意：真实实现需考虑字节跨度生命周期、递归结构、数字精度和错误语义。
enum ContentValue {
    Null,
    Bool(bool),
    Integer(IntegerValue),
    Real(RealValue),
    Name(ByteSpan),
    ByteString(ByteSpan),
    Array(Vec<ValueId>),
    Dictionary(Vec<(ValueId, ValueId)>),
}

struct Operation {
    operands: Range<ValueId>,
    operator: OperatorId,
}
```

约束：

- 字符串、Name 和 inline image 数据尽可能引用原始输入缓冲区的范围，避免不必要复制；但输入缓冲区生命周期必须由拥有者明确管理，不能保存悬空切片。
- 保留原始字节与解码后文本的区别；不能未经语义确认就统一转成 UTF-8。
- 数字表示要匹配 pypdf 对整数/浮点数的区分、边界值及序列化行为；不能无条件用 `f64` 替代。
- 数组/字典需要支持嵌套、重复键策略和原版迭代顺序语义；内部 HashMap 不能无条件替代有序表示。
- Operation 的操作符应优先使用枚举/紧凑 ID；需要对外展示时再还原为对应 Python bytes/对象。
- 内容流 token 结构与整个 PDF 间接对象模型是两个层次，不要把对象引用、xref 和页面资源逻辑混进 tokenizer。

---

## 3. P0：性能基线、归因与观测能力

**目标**：在改代码前确认时间花在哪里，并让后续每项优化可独立归因。

### 3.1 固化测试资产

建立 `validation/optimization/baseline/`：

```text
validation/optimization/baseline/
├── environment.txt
├── git-revision.txt
├── inputs.sha256
├── commands.md
├── raw/
│   ├── W1.csv
│   ├── ...
│   └── W8.csv
└── reports/
    └── baseline-report.md
```

记录：

- 原版 pypdf commit/version 和 Rust 端口 commit；
- Python、Rust、PyO3、操作系统、内核、CPU 型号、核数、SMT、NUMA、内存；
- CPU governor/frequency、容器 quota/cpuset、后台负载、文件系统；
- 所有输入样本 SHA-256；
- 启动命令、环境变量、预热次数、重复次数、进程隔离方式；
- 原始每次测量值，不只保存中位数。

### 3.2 统一性能采集

每个工作负载至少报告：

- wall time：中位数、P10/P90 或 P25/P75、最小/最大值；
- CPU time（user/system）；
- 峰值 RSS；
- 输入字节数、页数、对象数、内容流字节数；
- 输出字节数和 SHA-256；
- Python↔Rust 边界调用次数；
- 物化的 Python PDF 对象数量及类型分布；
- Rust Token/Operation 数量；
- 关键阶段时间：读取、xref/对象解析、内容流扫描、Token 解析、Python 物化、字体/CMap、文本解释、图像解码、Writer。

边界计数器必须在 PyO3 函数入口和 Python shim/回调处统计，不能只用 profiler 推断。

### 3.3 基准执行方式

当前 loadavg 22–30 的环境可以用于发现大幅差异，但不适合精确判断小幅性能变化。优化验收需尽量做到：

1. 原版和端口使用相同 CPU 集合、NUMA 策略、内存限制、输入和环境变量；
2. 参考侧和端口侧独立进程；
3. 使用 ABBA/BAAB 或随机交替顺序，避免总是先跑原版或先跑端口；
4. 至少 1 次预热，正式数据建议每边 15–30 次；资源允许时提高重复数；
5. 在低负载条件下测量，并同时记录 loadavg、CPU idle、频率和 steal/iowait；
6. 若只能在高负载服务器测量，扩大重复数，报告分布和置信区间，不把小于噪声范围的差异当成收益；
7. 同时保留原始脚本和报告生成程序，保证可重跑。

基准本身不能改变原业务负载：不删除任何 PDF 处理步骤，不把图像提取替换成只枚举文件名，不将写出换成空操作。

### 3.4 必须生成的诊断报告

`validation/optimization/reports/BASELINE_DIAGNOSIS.md` 至少回答：

- W1–W8 每项主要耗时阶段；
- W3 中扫描、解析、物化、文本解释分别占多少；
- 每 1 MB 内容流产生多少 Token、Operation、Python 对象和边界调用；
- `issue-301.pdf` 的调用次数是否与本轮描述一致；
- W4 图像提取是否确实被 CCITT/TIFF 解码主导，还是解码前后还有显著开销；
- W5–W7 是否主要受对象图复制/序列化影响；
- 当前 2.5–4× 回退中，可归因到跨语言回调、对象物化、算法差异和其他因素的比例。

**P0 门禁**：数据资产可复现；计数器与 profiler 结论一致；每个 W 项至少有粗粒度阶段拆分。未完成前，不进入大范围重构。

---

## 4. P1：Rust 原生内容流 Scanner / Tokenizer

**优先级：最高。目标：去除 Token 热路径上的 Python 回调。**

### 4.1 责任边界

只负责 PDF 页面内容流语法，不负责整个 PDF 文件结构：

- 输入：解码后的内容流字节缓冲区（必须确认原版 ContentStream 在什么阶段完成 Filter 解码）；
- 输出：Rust 内部 Token/Operation 表示；
- 输入缓冲区生命周期：由 Rust 拥有，或通过有明确生命周期/引用计数的只读缓冲区持有；
- 错误：返回与原版兼容的错误分类和可诊断位置；
- 限制：在 Rust 内实施 token 数、嵌套深度、字符串长度、inline image 长度等限制。

不要把原始压缩流直接送给内容流 tokenizer。必须先按 pypdf 原有流程完成对应过滤器解码，再比较相同输入字节上的 parser 行为。

### 4.2 Scanner 功能拆分

建议分模块实现，名称按仓库现有结构调整：

| 子模块 | 必须覆盖的语法 |
|---|---|
| `scanner` | 空白、PDF 分隔符、注释、字节位置、EOF |
| `number` | 正负整数、小数、前导/尾随小数点、指数规则按基线确认、数值边界和非法值 |
| `name` | `/Name`、`#xx` 转义、保留原始字节及解码行为 |
| `literal_string` | `(...)`、嵌套括号、反斜线转义、八进制转义、CR/LF 续行、未闭合字符串行为 |
| `hex_string` | `<...>`、空白、奇数 nibble 补齐和错误处理 |
| `array` | `[...]`、嵌套值、空数组 |
| `dictionary` | `<<...>>`、键值规则、嵌套结构、重复键行为 |
| `operator` | 操作数与操作符边界、操作符 bytes 语义 |
| `inline_image` | `BI`、图像参数、`ID`、二进制数据和 `EI` 结束判定 |
| `limits` | 深度、Token 数、字节长度、恢复/错误上限 |

### 4.3 Inline Image 必须单独设计

Inline Image 不是普通字符串扫描。图像数据中可能出现与 `EI` 相同的字节序列，不能简单使用 `memmem("EI")` 作为边界。

执行要求：

1. 先阅读当前 pypdf 基线对 `BI`/`ID`/`EI` 的处理逻辑，列出其判定条件和兼容行为；
2. 从现有测试语料收集 inline image 样本；
3. 明确 `EI` 前后空白、二进制数据中伪 `EI`、截断数据、过滤器参数和异常恢复行为；
4. 以原版 parser 为 oracle 做差分；
5. 若无法证明等价，保留兼容 fallback，不得用激进扫描策略替换。

### 4.4 一次调用处理整个内容流

PyO3 接口优先采用批量边界：

```python
# 概念接口，最终签名需与仓库现有 API 对齐。
parsed = rust_parse_content_stream(decoded_stream_bytes, limits)
```

不允许在 Rust scanner 内逐 Token 调用 Python 的 `read_object()`、`_check_limits()` 或其他 Python 函数。

Rust 内部直接完成 limit 检查。`_check_limits` 现有语义必须通过测试镜像到 Rust，不得因为性能而不执行。

### 4.5 错误和恢复语义

必须基于原版行为逐项核对：

- EOF 出现在 Token 中间；
- 未闭合字面字符串/字典/数组；
- 非法数字、非法 Name 转义；
- 操作符前存在非法操作数；
- inline image 数据截断或 EI 边界不确定；
- 内容流为空或只有注释；
- 连续分隔符、奇异空白字符；
- 内容流包含非 ASCII 字节。

错误兼容至少比较：错误类别、是否抛出、是否 warning、恢复后剩余内容、错误发生阶段。若原版确实依赖异常文本，则按对应测试要求比较文本；不要在所有异常场景机械追求逐字相同。

### 4.6 单测与差分

新增独立 Rust 单测，不依赖 Python/PyO3：

- 每种 Token 的有效/无效输入；
- Token 拼接边界；
- 复杂嵌套；
- 随机字节和 fuzz；
- 深度/长度限制；
- inline image 边界。

Python 差分测试：同一份已解码内容流分别交给原版 pypdf parser 和 Rust parser，比较规范化 Token/Operation 序列。规范化规则必须明确区分 bytes、字符串、整数、浮点数、Name、数组、字典和操作符，不能简单对 `repr()` 做字符串比较。

**P1 门禁**：

- Token 热路径的 Python 回调次数为 0；
- 每个内容流原则上一次进入 Rust（有明确的多段流场景除外）；
- 差分测试覆盖语料全部通过；
- Rust 单测、fuzz、资源限制测试通过；
- W3 的 parser-only microbench 明显改善；
- W1/W2/W4/W5/W6/W7 不允许出现超出测量噪声的显著回退。

---

## 5. P2：Rust 紧凑对象表示与延迟 Python 物化

**优先级：最高，与 P1 紧密衔接。目标：不仅消除回调，还减少 Python 对象创建。**

### 5.1 分清两类 API

必须把接口分成：

1. **内部高性能接口**：给 Rust 内部文本解析、内容流处理等消费者用，直接访问 Rust Token/Operation；
2. **Python 兼容接口**：面向调用方或基线公开对象属性，按原版类型、结构、可变性和身份语义提供 Python 对象。

严禁让内部高性能消费者通过 Python 兼容接口绕一圈。

### 5.2 延迟物化策略

建议执行路径：

```text
content stream bytes
      ↓
Rust scanner/parser
      ↓
Rust compact token/operation arena
      ├── Rust-native consumer (no PyObject creation)
      └── compatibility materializer
              ↓
          Python object graph
```

仅当以下情况发生时才物化：

- Python 用户读取原版公开的 `operations` 或等价属性；
- 原版 visitor/callback 要求 Python 对象；
- 上层 Python 代码需要对 operands 进行直接修改；
- 兼容层无法在 Rust 内完成的扩展点被调用。

### 5.3 对象语义检查

在改变内部存储前，必须调查并测试：

- `.operations` 的确切类型和嵌套结构；
- operands/operator 的实际 Python 类型；
- 返回对象是否可变；
- 修改列表、修改 operand、替换 operation 后，后续写出/解码是否观察到变化；
- 多次读取属性是否返回同一对象或等价对象；
- Name/String/ByteString 的相等性、哈希和编码；
- 是否有下游依赖私有属性或直接访问内部列表。

如果原版提供可变 Python 对象，不能仅把一个只读 Rust 视图伪装成等价 API。

### 5.4 Dirty / fallback 策略

如果公开 Python 对象被修改：

- 标记 Python compatibility representation 为 dirty；
- 后续操作必须以修改后的 Python 对象为真值；
- 可选择重新导入 Rust 内部表示，或对该对象走原有 Python 兼容路径；
- 不得继续使用过期 Rust 缓存覆盖 Python 用户修改。

这套策略必须通过显式测试证明，不要只靠“通常用户不会修改”作为设计依据。

### 5.5 内存布局和拷贝

测量以下成本：

- 每 Token 的 Rust 堆分配次数；
- 每 Operation 的分配次数；
- 原始内容流是否复制；
- 字符串/Name 是否复制；
- Python 对象物化数量；
- parser arena/Vec 的容量增长和峰值内存。

优先通过 arena、连续数组、索引引用和输入字节范围减少小对象分配；不要过早引入复杂内存池。对小 PDF，过大的预分配可能让 W1 变慢或增加 RSS，因此需要按内容流大小分级策略。

**P2 门禁**：Rust 内部文本/解析消费者不创建 Python Token 对象；公开 Python API 的对象类型、可变性和观察行为与基线一致；峰值 RSS 及分配量有明确数据；所有原有兼容测试通过。

---

## 6. P3：文本提取热路径下沉

**优先级：高。目标：让 W3 不再“Rust 解析 → Python 对象列表 → Python 再解释”。**

### 6.1 先拆解 W3

将文本提取计时细分为：

1. 读取 PDF / 获取页面；
2. 资源字典和字体资源查找；
3. 内容流 Filter 解码；
4. Tokenizer / parser；
5. Operation 构建；
6. Python 对象物化；
7. 操作符解释；
8. 字体编码/CMap/ToUnicode；
9. 文本块/布局/空格换行计算；
10. 最终字符串组装。

不得在没有拆分数据的情况下把所有 W3 时间都归因到 tokenizer。

### 6.2 分层下沉顺序

**P3-A：无回调的内部 Operation 消费**

Rust 直接遍历紧凑 Operation，不先创建 Python `list[tuple[list[PdfObject], bytes]]`。先实现与现有 Python 逻辑相同的状态机边界。

**P3-B：文本状态和常见操作符**

逐步迁移字体/文本状态机中有明确语义边界的操作符。先覆盖基准数据中实际出现频率高的操作符，但必须保留完整 fallback，不能把未覆盖操作符静默忽略。

**P3-C：字体与编码路径**

先对现有字体对象、CMap、ToUnicode 的数据结构做 Rust 可读视图；再根据 profile 决定是否下沉解码。必须覆盖：

- Simple Font 与 Type0/CID Font；
- Encoding、Differences；
- ToUnicode CMap；
- Identity-H/V 等常见编码；
- 字符宽度、字体缺失和编码回退；
- 非 BMP/多字节映射；
- malformed CMap 的原版行为。

**P3-D：文本输出策略**

根据当前 pypdf 版本实际 API 兼容：普通文本提取、layout 模式、方向/旋转过滤、visitor 回调、页面旋转、文本矩阵和用户空间变换。需要 Python visitor 的场景可以采用批量事件输出，不要每个字符或每个操作符回调 Python。

### 6.3 回调兼容

如果用户传入 visitor 函数：

- Rust 内部先完成可批处理的解析和状态计算；
- 仅在原版定义的事件点批量/按必要频率调用 Python；
- visitor 收到的参数对象类型、顺序、矩阵和资源引用必须匹配基线；
- Python visitor 的副作用顺序必须保持；
- 不允许无 visitor 的默认路径也承担 visitor 的逐操作 Python 回调开销。

### 6.4 分阶段启用

通过功能开关或内部策略区分：

- `legacy_python`：原版 Python 热路径；
- `rust_parse_python_consume`：Rust tokenizer + Python 消费；
- `rust_native_consume`：Rust tokenizer + Rust 消费；
- `compat_fallback`：不支持或风险场景走既有兼容路径。

开关用于测试和回滚，不应变成长期公开 API；默认切换需通过门禁和评审。

**P3 门禁**：W3 在稳定环境下至少达到原版 pypdf break-even；预期目标是优于原版，但不得把预期当成承诺。所有文本差分、visitor、布局、字体和异常测试通过；端到端 W8 不出现显著回退。

---

## 7. P4：Reader、页树和 PDF 对象解析优化（W1/W2）

**优先级：中高。与内容流 tokenizer 分开实施。**

### 7.1 调查重点

- xref table / xref stream 读取；
- trailer 和 root/catalog 获取；
- object stream 解压与对象解析；
- indirect reference 解析及缓存命中率；
- 页树递归、继承属性查找；
- Reader 构造时 eager/lazy 解析差异；
- Python PDF Object 物化数量和跨语言边界数。

### 7.2 优化方向

1. 将可独立的 PDF syntax object parser 下沉到 Rust，但保持与内容流 parser 的职责区分；
2. 对 indirect object 解析建立与原版一致的缓存与循环引用处理；
3. 批量解析页树和属性，避免 Python 每访问一个节点就跨界；
4. 仅在确认原版允许的情况下做惰性解析；不能为了性能延迟原版本应立即抛出的异常；
5. 优化对象缓存键、引用去重和锁竞争；
6. 对小文件单独优化初始化固定成本，避免大文件策略导致 W1 小文件回退。

### 7.3 门禁

- W1、W2 独立报告阶段耗时和 Python 对象数；
- 页数、页面顺序、MediaBox/CropBox/Rotate/UserUnit 等属性一致；
- 页树继承、循环引用、损坏 xref、object stream、增量更新场景通过差分；
- Reader 初始化异常发生时机与基线兼容。

---

## 8. P5：图像枚举与解码优化（W4）

**优先级：中。与 P1/P3 分支并行，但独立提交和独立测量。**

### 8.1 先定位真正热点

为 W4 增加阶段计时：

- 页面资源遍历；
- XObject 查找；
- image stream 读取；
- filter 解码；
- CCITT 参数解析；
- TIFF wrapper/转换；
- Pillow/其他 Python 对象转换；
- 输出缓冲区复制和写文件。

对 `issue-301.pdf` 单独记录每个图像对象尺寸、Filter、DecodeParms、原始字节量、解码后字节量和解码耗时。

### 8.2 CCITT/TIFF 专项

不能直接根据 W4 总耗时就断言所有损失均来自 CCITT。确认后再做：

1. 对照 pypdf 当前 CCITT 解码和 TIFF 封装路径；
2. 分别测纯解码、TIFF 头构造、Pillow 读取和最终输出；
3. 评估现有 Rust 解码库或底层 C 库的正确性、许可证、aarch64 支持、离线构建能力；
4. 对 Group 3/4、K、EndOfLine、EncodedByteAlign、Columns、Rows、BlackIs1 等参数做差分；
5. 保留原有 fallback 处理异常或不支持情况；
6. 对像素结果、尺寸、位深、极性、行填充和输出格式进行验证。

### 8.3 其他图像 Filter

对 Flate/PNG predictor、DCT、JPX、JBIG2 等逐项测量，不能一次性大换库。`png-predictors.pdf` 已显示某条路径有 2.64× 优势，应确保该优势在新架构中保留。

**P5 门禁**：图像枚举数量、顺序、原始 stream 数据和解码图像语义与基线一致；W4 拆分报告可解释改善来源；原有特殊快路径无回退。

---

## 9. P6：合并、页面操作、Clone 和 Writer 优化（W5/W6/W7）

**优先级：中。需要基于对象图 profile，不与内容流 parser 混为一谈。**

### 9.1 建立写入阶段 profile

拆为：

1. 源 PDF Reader 初始化；
2. 页面树定位；
3. 页面对象复制/Clone；
4. indirect reference 重映射；
5. 资源字典合并；
6. 页面变换矩阵/框处理；
7. Writer 对象收集；
8. 对象编号与引用写入；
9. stream 编码/Filter；
10. xref/trailer/startxref；
11. 输出 flush。

### 9.2 优化方向

- 对象图复制时建立源对象身份到目标对象身份的映射，避免重复 clone；
- 循环引用必须通过占位对象/两阶段构造处理；
- 批量处理对象和引用，减少 Python↔Rust 逐对象边界；
- 减少重复序列化前的对象遍历，但必须保留对象更新和增量写入语义；
- 对大 stream 尽量避免不必要的完整复制；
- 对小文件减少初始化固定成本，不能盲目引入大对象池；
- 页面合并、Clone 和普通 Writer 分开基准，不以某一场景代表所有写出。

### 9.3 输出兼容性

当前 W1–W8 输出字节与 SHA-256 一致，应将其保留为这些固定样本的回归条件。但一般 PDF 写出可能存在对象编号、字典顺序、时间戳或压缩结果等非语义差异：

- 对固定基准样本，继续要求字节级一致；
- 对一般语料，要求解析后的对象图/页面/资源/文本/图像语义一致；
- 任何放宽字节一致性范围的决定，必须单独记录原因并经用户/项目负责人批准，不能由 Agent 自行降低标准。

**P6 门禁**：合并页面、资源、引用、注释、表单、书签、附件、页面框和写出结构通过差分；W5–W7 无显著回退；固定样本哈希符合既有门禁。

---

## 10. P7：低优先级微优化

这部分只能在主路径完成后，或 profile 明确证明收益足够大时启动。

| 优化项 | 可能收益点 | 需要的证据 | 风险/注意事项 |
|---|---|---|---|
| 缓存 Python class 引用 | 减少反复 `getattr`/import | profile 显示类查找占比显著 | 解释器生命周期、模块重载和子解释器安全 |
| 移除 Python shim | 减少 Python frame | shim 单独占显著耗时 | 不能破坏 monkeypatch、测试注入和 API 语义 |
| 批量对象物化 | 减少边界调用 | 物化仍是主要成本 | 需保证异常顺序、对象身份和内存峰值 |
| 容量预估/arena | 减少分配 | allocator profile 显示分配热点 | 过度预分配会损害小文件和 RSS |
| GIL 释放 | 并发场景 CPU 时间收益 | 能证明工作段不接触 Python 对象且线程安全 | 不能在持有 Python 引用时释放 GIL；不能假设单线程基准必然变快 |
| 多线程解析 | 大内容流并行 | 内容流彼此独立且顺序/资源语义允许 | 调度、内存和 GIL/线程池开销；先不做 |

对缓存 Python 类引用的建议：只有在 Rust 端仍需构造大量 Python 对象的兼容路径上，才考虑做缓存；如果 P2 成功消除了热路径物化，这项优化优先级自然下降。

---

## 11. 统一兼容性与正确性策略

### 11.1 对照模式

保留可测试的三路模式：

| 模式 | 解析 | 消费 | 用途 |
|---|---|---|---|
| Legacy | 原版 Python | 原版 Python | Oracle / 回滚 |
| Rust parser + Python consumer | Rust | Python 兼容对象 | 分离 parser 收益与物化/消费成本 |
| Rust native | Rust | Rust | 最终热路径 |

不能在生产默认路径静默捕获所有异常后 fallback，否则会掩盖 Rust 缺陷并污染性能结果。Fallback 必须有明确触发原因和计数器。

### 11.2 必须比较的语义

- Token/Operation 数量、顺序、操作符 bytes；
- operand 的类型、值、原始字节和嵌套结构；
- Name/String/编码转换；
- Inline Image 参数和数据边界；
- 文本提取结果及布局模式；
- 页面数量、顺序、框、旋转、资源字典；
- 图片数量、尺寸、像素、颜色空间和解码结果；
- 合并/Clone 后对象引用和资源关系；
- 加密/权限/密码错误行为；
- malformed PDF 的 warning/error/recovery；
- 安全限制和资源上限。

### 11.3 安全要求

- 所有递归结构有深度限制；
- 所有长度、对象数、解压后大小和 token 数有合理上限；
- 处理整数溢出、索引越界和恶意长度字段；
- 防止解压炸弹及异常大的图像缓冲区；
- 不执行 PDF JavaScript、Launch Action 或外部命令；
- 不因 Rust 更快而移除现有安全检查；
- fuzz 测试必须包含极端嵌套、截断、随机字节和长度边界。

---

## 12. 性能验收指标与决策规则

### 12.1 核心指标

对每个 W 项同时报告：

```text
speedup = baseline_pypdf_median / rust_port_median
latency_change = (rust_port_median / baseline_pypdf_median - 1) * 100%
```

- speedup > 1：Rust 更快；
- speedup = 1：持平；
- speedup < 1：Rust 更慢。

不能将 speedup、延迟变化百分比和“快/慢 N 倍”混用。报告中同时展示原始耗时、速度比和变化百分比。

### 12.2 阶段性性能目标

以下是项目建议目标，不是对实际收益的预先承诺：

| 阶段 | 主要目标 | 通过条件 |
|---|---|---|
| P0 | 可靠归因 | 能把 W3 拆成 parser、物化、消费等阶段；边界调用可计数 |
| P1 | 消除逐 Token 回调 | Token 热路径 Python 回调为 0；parser-only microbench 有稳定收益 |
| P2 | 消除不必要物化 | Rust 内部消费者不创建逐 Token Python 对象；公开 API 兼容 |
| P3 | 拉平 W3 回退 | W3 至少达到 pypdf break-even；理想目标 >1.0×，并继续优化至稳定领先 |
| P4 | Reader 提速 | W1/W2 分别达到 break-even 或证明有明确剩余瓶颈和下一步 |
| P5 | 图像提速 | W4 的 CCITT/TIFF 热点有独立证据和专项收益 |
| P6 | Writer 提速 | W5/W6/W7 分项改善且输出正确 |
| Release | 整体领先 | W1–W8 无无法解释的显著回退；核心工作负载加权几何平均 speedup >1；内存满足项目约束 |

“整体领先”不能只靠一个大工作负载掩盖其他工作负载严重退化。必须同时公布每一项数据。加权方式在审计阶段根据真实生产调用占比确定；若无调用占比，报告不应伪造业务权重，使用几何平均和单项结果并列呈现。

### 12.3 回退阈值

建议把超过测量噪声且达到 5% 的单项回退作为调查门槛；内存峰值增加超过 10% 作为调查门槛。阈值不是允许自动退化的额度：

- 若变化落在噪声范围，增加重复测量；
- 若变化稳定超过阈值，必须定位原因；
- 任何明显正确性/安全回归立即阻断；
- 小文件 W1 和大型内容流 W3 必须分别观察，防止一种策略损害另一种。

### 12.4 对当前 8 个 workload 的目标

第一轮架构优化不要求每个 W 都同步超过原版。建议目标顺序：

1. P1/P2 后，`issue-301.pdf` 的 parser 热路径明显提速，逐 Token Python 回调清零；
2. P3 后，W3 达到至少持平，随后追求稳定超过原版；
3. P4、P5、P6 分别解决 W1/W2、W4、W5–W7；
4. 最后评价 W8 端到端是否超过原版。

禁止仅根据局部 tokenizer microbench 就宣布 Rust 端口整体已经更快。

---

## 13. 任务拆分与 Agent 执行清单

Agent 必须按依赖顺序创建任务，不得并行修改共享热路径导致无法归因。

### OPT-000：审计和冻结基线

- [ ] 核查当前分支、commit、工作区和版本；
- [ ] 找到 W1–W8 的真实入口和报告；
- [ ] 固化输入样本哈希和环境信息；
- [ ] 复跑当前基准；
- [ ] 输出 `REPO_AUDIT.md` 和 `BASELINE_DIAGNOSIS.md`。

**完成标准**：代码路径、测试、环境和当前性能报告均可复现。

### OPT-010：增加性能计数器和阶段计时

- [ ] 统计每个内容流长度、Token 数、Operation 数；
- [ ] 统计 PyO3 入口、Python callback 数；
- [ ] 统计 Python PDF 对象物化数量和类型；
- [ ] 对 W3 拆分 parser/物化/消费；
- [ ] 对 W4 拆分图像 Filter/解码/转换；
- [ ] 对 W5–W7 拆分 clone/对象图/序列化。

**完成标准**：报告能用计数器解释 profile；测量开销经空跑或对照确认可接受。

### OPT-020：Rust Scanner 基础语法

- [ ] 空白/分隔符/注释；
- [ ] Number；
- [ ] Name；
- [ ] Literal String；
- [ ] Hex String；
- [ ] Array；
- [ ] Dictionary；
- [ ] Operation 识别；
- [ ] Rust 单测和差分。

**完成标准**：基础语法和边界输入差分通过。

### OPT-030：Inline Image 和资源限制

- [ ] 复刻基线 BI/ID/EI 逻辑；
- [ ] 建立 inline image 专项测试；
- [ ] 实现深度/数量/长度限制；
- [ ] 验证异常恢复和资源耗尽防护。

**完成标准**：inline image 专项差分、fuzz 和限制测试通过。

### OPT-040：批量 PyO3 接口

- [ ] 实现单内容流批量入口；
- [ ] 去掉 Rust 热路径的 Python `read_object` 回调；
- [ ] 去掉逐 Token `_check_limits` 回调并在 Rust 内保留同等限制；
- [ ] 记录边界调用数；
- [ ] 比较 parser-only microbench。

**完成标准**：Token 热路径不再发生逐 Token Python 回调。

### OPT-050：紧凑 Rust Token/Operation 表示

- [ ] 明确数值、Name、String、Array、Dictionary 表示；
- [ ] 明确输入 buffer 生命周期；
- [ ] 实现 Rust 内部 Operation 访问；
- [ ] 评估 arena/索引/切片；
- [ ] 比较分配量、RSS 和吞吐。

**完成标准**：Rust 内部消费者无须创建逐 Token Python 对象。

### OPT-060：Python 兼容物化

- [ ] 核对公开属性精确类型和行为；
- [ ] 实现批量物化；
- [ ] 覆盖对象可变性、身份和重复访问；
- [ ] 实现修改后的 dirty/fallback；
- [ ] 覆盖 visitor 和第三方扩展行为。

**完成标准**：Python API 兼容测试全通过，Rust 内部路径与兼容路径隔离。

### OPT-070：文本提取 Rust 原生消费

- [ ] 拆解 W3 阶段耗时；
- [ ] 实现原生 Operation 遍历；
- [ ] 下沉可明确等价的文本状态机；
- [ ] 验证字体、CMap、ToUnicode；
- [ ] 保留 visitor、layout 和 fallback；
- [ ] 跑文本差分和 W3/W8 性能。

**完成标准**：W3 至少 break-even；文本和 visitor 语义通过。

### OPT-080：Reader / 页树优化

- [ ] 拆解 W1/W2；
- [ ] 分析 xref、object stream、indirect reference；
- [ ] 优化引用解析缓存；
- [ ] 优化页树遍历与继承属性；
- [ ] 复测 W1/W2。

**完成标准**：W1/W2 有独立性能结论和完整差分证据。

### OPT-090：图像专项

- [ ] 拆解 W4；
- [ ] 验证 CCITT/TIFF 是否主导；
- [ ] 评估依赖或现有 Rust 解码实现；
- [ ] 覆盖 Group 3/4 和 DecodeParms；
- [ ] 保留 PNG predictor 已有优势；
- [ ] 复测 W4。

**完成标准**：图像语义正确，W4 优化收益可归因。

### OPT-100：Writer / Clone 优化

- [ ] 拆解 W5/W6/W7；
- [ ] 实现/优化对象图映射；
- [ ] 验证循环引用和资源合并；
- [ ] 优化写出遍历和序列化；
- [ ] 保留固定样本字节级一致性；
- [ ] 复测 W5–W7。

**完成标准**：输出和对象关系正确，分项性能有可复现改善。

### OPT-110：最终验收与发布

- [ ] 运行 A1 2691 测试；
- [ ] 运行 103 场景差分；
- [ ] 运行 150 个 Round-trip；
- [ ] 运行 fuzz/安全门禁；
- [ ] 运行 W1–W8 全量性能；
- [ ] 验证 aarch64 和目标部署环境；
- [ ] 生成最终性能报告和已知限制。

**完成标准**：所有正确性门禁通过；所有性能回退已解释；发布策略有回滚开关。

---

## 14. 分支、提交和回滚策略

### 14.1 建议提交粒度

每个优化点独立提交：

1. `perf: add content-stream profiling counters`；
2. `feat: add rust content-stream scanner`；
3. `feat: parse content operations in rust`；
4. `perf: add compact rust operation representation`；
5. `feat: preserve python content-stream compatibility`；
6. `perf: consume content operations in rust text path`；
7. 后续 Reader、Image、Writer 各自单独提交。

提交标题只是示例，Agent 需遵守仓库现有规范。不要把 P1–P6 所有修改塞入一个超大提交。

### 14.2 回滚策略

- 保留 legacy Python 路径作为对照和短期回滚方案；
- Rust parser、Rust native consumer 分别可关闭；
- 每个阶段先 shadow/differential 验证，再切默认；
- 出现语义不一致时先回退对应阶段，不允许通过吞异常或静默 fallback 隐藏问题；
- 每次切换记录配置、版本和性能数据。

---

## 15. 每阶段交付报告模板

每完成一个 OPT 任务，更新 `validation/optimization/PROGRESS.md`，并附独立报告：

```markdown
# OPT-xxx 阶段报告

## 1. 目标
## 2. 代码变更
- 文件：
- 关键入口：
- 新增/删除的跨语言调用：

## 3. 行为兼容
- 对照基线：
- 新增测试：
- 测试命令：
- 通过/失败/跳过：
- 差异和处理：

## 4. 性能结果
| workload | baseline | before | after | speedup | latency change | RSS |
|---|---:|---:|---:|---:|---:|---:|

## 5. 归因
- 哪个阶段变快：
- 哪个阶段未变化：
- 是否有回退：
- 边界调用/对象物化变化：

## 6. 风险与未完成项
## 7. 门禁结论
- PASS / FAIL / BLOCKED：
- 是否允许进入下一阶段：
```

---

## 16. Agent 第一轮必须实际执行的动作

接到本方案后，不要立刻重写 parser。第一轮只完成以下任务：

1. 审计当前代码和实际版本；
2. 找出 W1–W8 基准入口和全部输入样本；
3. 对 `issue-301.pdf` 建立可重复的调用计数和阶段耗时；
4. 核对 `read_object`、`_check_limits` 调用统计是否包括递归调用、是否存在重复计数；
5. 检查当前所谓“Python 层调用从约 50 万降到约 2 万”的改动具体落在哪里；
6. 明确当前 Rust parser 返回的数据结构，以及谁创建 `FloatObject`/`NameObject`；
7. 输出 `BASELINE_DIAGNOSIS.md`，给出 P1/P2 的准确代码修改点和风险；
8. 等 P0 门禁成立后再进入 OPT-020。

第一轮报告必须回答这个核心问题：

> 在当前 706.6 ms 的 W3 中，分别有多少时间消耗在内容流扫描/语法解析、Python↔Rust 边界、Python 对象物化、文本操作符解释、字体/CMap 和最终字符串构建？哪些开销已被测量，哪些仍只是推测？

如果 Agent 不能回答，说明证据还不够，不能直接进入大规模重写。

---

## 17. 最终完成定义

只有同时满足以下条件，才能宣布本轮优化完成：

1. Token 热路径不再逐 Token 调用 Python；
2. Rust 内部解析/文本消费者不再为每个 Token 无条件创建 Python 对象；
3. Python 公开 API 的类型、数据、可变性、visitor 和异常行为通过兼容性验证；
4. 内容流、文本、图像、Reader、Writer 和 Clone 的正确性测试通过；
5. A1 2691 个测试、103 场景差分、150 个 Round-trip 门禁通过；
6. 安全限制和异常 PDF 行为没有退化；
7. W1–W8 使用同一基准规范重新测量，所有结果可复现；
8. 性能提升来自真实计算路径优化，而不是减少处理步骤、跳过数据或改变输出要求；
9. 对仍未超过原版的工作负载有明确归因、剩余工作和优先级；
10. 目标 aarch64/生产部署环境可以构建、安装和执行。

**最终原则：把跨语言边界从逐 Token 降到逐内容流/批次，把内部计算从 Python 对象模型迁移到 Rust 原生表示；但 Python 兼容语义只在边界物化，任何时候都不能以牺牲 PDF 正确性、安全性或既有工作流来换取速度。**
