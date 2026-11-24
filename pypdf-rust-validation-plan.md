# pypdf Rust 移植验证方案

> **文档类型**：测试与验收方案 / Agent 执行说明  
> **默认参考基线**：目标仓库锁定版本；未锁定时使用 `pypdf==7.1.1`  
> **验证原则**：差分验证 + 结构验证 + round-trip + 安全验证 + 端到端性能验证。  
> **禁止**：仅凭“能打开 PDF”“输出文件存在”或“文件字节一致”判定移植正确。

---

## 0. Agent 验证约定

### 0.1 验证的对象

需要分别验证：

1. **API 兼容性**：Python 调用形式和返回行为是否与参考 pypdf 一致。
2. **PDF 语义正确性**：读取、编辑和写出后，页面、对象、文本、图片和文档结构是否符合预期。
3. **健壮性和安全性**：损坏文件、恶意结构和超大输入是否被正确处理并受资源限制。
4. **互操作性**：输出是否能被其他 PDF 工具正确读取。
5. **性能**：同一环境、同一输入、同一工作流下是否得到可重复收益。
6. **部署正确性**：目标架构 wheel 能否安装、导入和运行。

### 0.2 测试前置条件

- 参考 pypdf 必须安装在隔离环境中，不能被待测 Rust 包覆盖。
- Rust 后端必须显式选择；`PYPDF_BACKEND=rust` 失败时直接失败，不允许静默回退。
- 每份输入 PDF 都有固定 SHA256 和 manifest。
- 原始输入只读；每个测试写入独立临时目录。
- 网络测试和依赖下载不应出现在常规验证流程中；测试语料和依赖预先准备。
- 不同后端使用完全相同的输入 bytes 和业务参数。

### 0.3 推荐目录

```text
tests/
├── fixtures/
│   ├── manifest.csv
│   ├── valid/
│   ├── encrypted/
│   ├── malformed/
│   ├── large/
│   ├── text/
│   ├── images/
│   ├── forms/
│   └── annotations/
├── unit/
│   ├── parser/
│   ├── objects/
│   ├── filters/
│   ├── crypto/
│   ├── content/
│   └── writer/
├── differential/
├── roundtrip/
├── interop/
├── security/
└── integration/

benchmarks/
├── workloads.json
├── run_reference.py
├── run_rust.py
├── compare_results.py
└── results/
```

---

## 1. 验证总流程和发布门禁

```text
仓库/API 基线冻结
       │
       ▼
依赖和构建验证
       │
       ▼
Rust 单元测试 ──失败──> 修复，不进入下一阶段
       │通过
       ▼
Python API 兼容测试
       │通过
       ▼
差分测试（Python pypdf vs Rust）
       │通过
       ▼
Round-trip + 外部 PDF 工具互操作
       │通过
       ▼
异常/安全/fuzz 验证
       │通过
       ▼
端到端工作流和性能对比
       │通过
       ▼
目标平台 wheel 安装与运行
       │通过
       ▼
灰度切换 / 回滚验证 / 发布
```

### 1.1 发布阻断项

以下任一项失败都阻断发布：

- P0 API 调用点的输入、输出、异常或警告与基线不一致；
- 页面数、页面顺序、页面尺寸/旋转、文本、图像、注释、表单或附件发生无解释丢失；
- Rust 后端崩溃、panic 穿透、死锁或无限循环；
- 资源限制不生效，或同类输入导致超出约定的内存/CPU 工作量；
- write → read round-trip 后对象关系损坏；
- 输出被独立工具判定为损坏；
- 目标架构 wheel 无法安装/导入/执行；
- 端到端性能目标未达到，或性能提升来自改变步骤/减少处理内容。

### 1.2 兼容状态定义

每个 API / 功能项只能标记为：

- `PASS`：差分和独立验证通过；
- `PASS_WITH_DOCUMENTED_VARIANCE`：有经过批准且不影响业务合同的明确差异；
- `PARTIAL`：部分行为实现，不能宣称兼容；
- `FAIL`：存在回归；
- `NOT_TESTED`：尚未验证；
- `NOT_APPLICABLE`：有证据证明目标基线/业务不适用。

不得把 `PARTIAL`、`NOT_TESTED` 当作通过。

---

## 2. 参考实现与差分测试框架

### 2.1 双环境隔离

建议创建三个环境：

```text
.venv-reference/   # 只安装指定版本的 Python pypdf
.venv-rust/        # 安装正在测试的 Rust-backed 包
.venv-tools/       # pytest、报告工具和辅助 PDF CLI
```

验证前输出：

```bash
python - <<'PY'
import platform, sys
print("python:", sys.version)
print("platform:", platform.platform())
print("machine:", platform.machine())
import pypdf
print("pypdf:", pypdf.__version__)
print("module:", pypdf.__file__)
PY
```

参考环境和 Rust 环境分别执行，保存原始日志和 JSON 结果。

### 2.2 差分测试原则

同一测试输入分别执行：

```text
输入 PDF + 操作参数
      ├── 参考 pypdf → reference.json / reference.pdf
      └── Rust backend → rust.json / rust.pdf
                       │
                       ▼
                semantic comparator
```

比较内容：

- 返回类型、数据结构和必要的对象身份行为；
- 页数、页序、页面 MediaBox/CropBox/Rotate；
- metadata、XMP、页标签、书签和命名目标；
- 文本内容、文本模式、每页文本顺序；
- 图像数量、尺寸、颜色模式、mask 和导出数据；
- annotation/form/attachment 的字段和引用关系；
- 加密状态、权限和密码验证结果；
- 异常类、错误类别、warning 类别；
- 输出 PDF 经重新读取后的语义。

### 2.3 不要比较原始 PDF 字节

通常不应要求 `reference.pdf` 和 `rust.pdf` 的 SHA256 相同。PDF 写出可能因对象编号、对象顺序、xref 类型、压缩参数、时间戳、文件 ID 和序列化格式而不同。

正确比较方式：

1. 分别用参考 pypdf 和 Rust 后端重新打开输出文件；
2. 生成规范化的语义摘要；
3. 比较语义摘要；
4. 再用独立工具校验输出结构和可读性。

只有在 API 明确承诺原始字节透传、未修改内容保持原样等场景，才测试字节级相等。

### 2.4 规范化输出格式

建议每个测试生成如下 JSON：

```json
{
  "input_sha256": "...",
  "backend": "reference|rust",
  "operation": "read_summary",
  "ok": true,
  "result": {
    "page_count": 3,
    "pages": [
      {
        "index": 0,
        "mediabox": [0, 0, 612, 792],
        "cropbox": [0, 0, 612, 792],
        "rotation": 0,
        "text": "...",
        "images": [],
        "annotations": []
      }
    ],
    "metadata": {},
    "outlines": [],
    "attachments": [],
    "fields": []
  },
  "warnings": [],
  "error": null
}
```

此处字段只是示例。Agent 应按实际 API 能力扩充；浮点数使用明确容差，文本不允许随意 trim 或合并空白。

### 2.5 异常差分

异常测试至少记录：

- 异常类名；
- `.args`；
- Warning 类型和出现次数；
- `strict=True/False` 下的差异；
- 输入是否部分读取、file-like 的当前位置是否发生变化；
- 文件输出是否留下不完整或损坏文件。

异常消息文本可以按项目需要设为严格/宽松比较，但异常类型和成功/失败判定必须一致。

---

## 3. 语料体系

### 3.1 语料来源

至少组合四类：

1. **项目真实语料**：现有业务输入 PDF。优先级最高，注意脱敏和访问权限。
2. **pypdf 上游测试语料**：固定版本对应的测试文件和样例，遵守各文件来源和 license。
3. **定向构造语料**：由脚本生成极小 PDF，覆盖单一边界条件。
4. **异常/模糊语料**：损坏 xref、循环引用、错误长度、极端压缩率、畸形 CMap 等。

不能只使用从网上下载的正常 PDF；不能只用少数业务文件推断完整 API 正确。

### 3.2 Manifest

`tests/fixtures/manifest.csv` 至少包含：

```text
case_id,path,sha256,size_bytes,pages,encrypted,pdf_version,features,source,license,priority
```

`features` 可多值，例如 `xref_stream;object_stream;to_unicode;acroform`。

每次新增或替换样本必须更新 manifest 和来源说明。测试过程中不得覆盖原始 fixture。

### 3.3 必测文档分类

| 类别 | 覆盖内容 |
|---|---|
| 基础结构 | 最小 PDF、单页、多页、空白页、无 metadata |
| xref | classic table、xref stream、hybrid-reference、多个 `/Prev`、free entry、错误 offset |
| 对象流 | object stream、压缩对象、间接 Length、重复引用、循环引用 |
| 增量更新 | 多次保存、旧 xref 链、修改后覆盖或增量追加 |
| 字符串 | literal string、escaped parentheses、octal escape、hex string、UTF-16 BOM、PDFDocEncoding |
| 页面树 | 多层 kids、继承 Resources/MediaBox/Rotate、非法节点、空树 |
| 流过滤器 | Flate、Predictor、ASCIIHex、ASCII85、LZW、RunLength、CCITT、DCT、JPX、JBIG2 |
| 字体与文本 | Type1、TrueType、CID、Type0、ToUnicode、Differences、CJK、RTL、嵌入字体、缺失字体映射 |
| 图像 | XObject、inline image、CMYK、灰度、Indexed、软遮罩、多个 filter |
| 加密 | 无密码、空密码、用户密码、所有者密码、错误密码、不同加密修订 |
| 页面编辑 | 添加、删除、插入、重排、merge、crop、rotate、scale、translate |
| 文档对象 | outlines、destination、page labels、annotation、AcroForm、attachment、XMP、viewer preferences |
| 输入形式 | path、str path、Path、bytes、BytesIO、真实文件对象、可 seek 和不可 seek 的流（按基线支持情况） |
| 异常输入 | 截断文件、伪造 header、错误 EOF、超大长度、畸形嵌套、未知 filter |

### 3.4 最小/边界用例

每个模块除正常样例外，至少补充：

- 空输入；
- 最小合法输入；
- 最大/最小边界值；
- 缺失关键字段；
- 字段类型错误；
- 重复引用；
- 循环引用；
- 被截断输入；
- 超出资源限制输入；
- optional dependency 缺失场景。

---

## 4. Python API 兼容测试

### 4.1 导入兼容

逐条检查基线支持的导入形式：

- `import pypdf`；
- `from pypdf import PdfReader, PdfWriter`；
- 基线公开的类、常量、异常和模块导入；
- `pypdf.generic` 和 annotations 等子模块导入；
- `__version__`、包元数据和模块路径。

### 4.2 签名和默认值

对照 API surface 清单检查：

- 参数名称；
- positional / keyword-only 规则；
- 默认值；
- 返回类型；
- deprecated API 的行为；
- 不支持参数时的异常类型。

### 4.3 Python 对象协议

重点覆盖：

- DictionaryObject 的 get/set/delete/iteration/update；
- ArrayObject 的 index/slice/append/insert/delete；
- NameObject、TextStringObject、ByteStringObject 等值类型；
- `IndirectObject.get_object()` 和引用身份；
- PageObject 属性访问和变更；
- PageRange、RectangleObject、Transformation 的算术与比较；
- 修改对象后写出 PDF 是否反映修改；
- 多个 wrapper 指向同一 PDF 间接对象时，修改是否可见。

禁止仅验证 Python 类型名称相同；必须验证实际协议和状态变化。

### 4.4 文件对象协议

用 `io.BytesIO` 和真实文件分别测试：

- 读取前当前位置非零；
- 初始化后当前位置；
- Reader 是否执行 seek；
- Writer 写入后的当前位置；
- 文件由调用方关闭时的行为；
- Reader/Writer 是否错误关闭调用方的 file-like；
- 多次读取和重复写入；
- 异常发生后的文件状态。

---

## 5. Reader / Parser 正确性验证

### 5.1 文件结构

必须覆盖：

- `%PDF-` header 位置；
- `startxref` 定位；
- classic xref table；
- xref stream 和不同 entry type；
- hybrid-reference；
- trailer 的 Root、Info、ID、Encrypt、Prev；
- 增量更新中多个 Prev 链；
- object stream 内对象位置；
- generation number 和 free object；
- 错误 xref 的 strict / non-strict 恢复路径。

### 5.2 对象解析

为每个对象类型测试：

- 正常解析和序列化；
- 空值和边界值；
- 特殊字符；
- 负数、小数、大整数；
- 嵌套数组和字典；
- stream 长度声明与实际长度不同；
- indirect reference 的解析和缓存；
- 对象循环和重复引用。

### 5.3 页树

检查：

- 页数和页面顺序；
- 多层 `/Pages` 节点；
- `/Count` 不可信时的行为；
- Parent/Kids 一致性；
- MediaBox/CropBox/TrimBox/BleedBox/ArtBox；
- 继承资源、旋转和其他页面属性；
- 非法引用、孤立节点和循环页树。

### 5.4 Reader 语义

验证 metadata、PDF 版本、页标签、outlines、named destinations、attachments、annotations、fields、XMP 等读取路径。对未实现的 API 不得返回空值冒充成功。

---

## 6. Writer / 页面编辑验证

### 6.1 基础写出

覆盖：

- 从空文档创建；
- Reader clone；
- 页面 add/append/insert/remove；
- 页面顺序和页数；
- 多个源 PDF 合并；
- metadata、PDF version；
- 写到 path、bytes buffer、file-like；
- 输出后重新读取。

### 6.2 引用和资源

重点验证：

- 同一对象被多个页面引用；
- 页面之间相互引用；
- annotations 的 `/P`、`/Dest`、`/A` 等引用；
- 表单字段树和 widget annotation；
- resource dictionary 的合并和重名冲突；
- Form XObject、字体、图片和 ExtGState 的引用；
- clone 后不再意外引用源文档对象；
- 循环引用不会无限递归。

### 6.3 页面变换

使用带有文字、矢量、图片和 annotation 的页面验证：

- rotate；
- scale；
- translate；
- crop；
- merge page；
- 页面框边界与内容位置；
- 变换矩阵的组合顺序。

页面可视化结果与文本/对象结构两种验证必须同时做。只比较页面框坐标不够。

### 6.4 高级写出

逐项验证：

- outlines / destinations / page labels；
- annotations；
- AcroForm 字段；
- attachments；
- XMP / Document Info；
- viewer preferences；
- JavaScript action 的写入和保存；
- encryption；
- incremental mode（基线支持时）。

注意：JavaScript action 仅作为 PDF 对象读写，不执行其代码。

---

## 7. 文本、字体、图像与 Filter 验证

### 7.1 文本提取

参考实现和 Rust 实现对同一页分别执行相同 `extract_text` 参数，比较：

- 文本字符串；
- 字符顺序；
- 空格、换行和段落边界；
- ligature / Unicode 映射；
- RTL 顺序；
- layout 模式输出；
- visitor callback 的触发次序和参数（若目标 API 使用）；
- 页面无文本、无 text operator、只有图片的处理。

不能做无条件 `.strip()`、合并空格或 Unicode 规范化来让差分通过。确需比较归一化文本时，必须同时保留原始输出差异并说明归一化规则。

### 7.2 字体与 CMap

必测：

- Simple Font；
- Type0 / CIDFont；
- Encoding / Differences；
- ToUnicode CMap；
- Identity-H / Identity-V；
- 字体没有 ToUnicode；
- 嵌入子集字体；
- 多字节字符映射；
- CJK 和 RTL 字体；
- malformed CMap 和巨大 CMap。

### 7.3 图像提取

检查：

- 图片资源枚举和引用 ID；
- XObject 与 inline image；
- image mask / soft mask；
- 色彩空间、位深、尺寸和通道数；
- 原始压缩数据与解码像素的区别；
- DCT/JPX/JBIG2 等依赖可用性；
- 同一图片被重复引用时 API 的去重语义；
- 软遮罩的尺寸和应用语义。

若 pypdf 返回的是图像数据或基于 Pillow 的对象，必须逐项确认 Rust 版本如何维持返回合同；不能只验证图片“能打开”。

### 7.4 Stream Filter

每种 filter 至少测试：

- 单独使用；
- 两个及以上 filter 链；
- 正常 DecodeParms；
- 缺失/错误 DecodeParms；
- 空数据；
- 截断数据；
- 非法输入；
- 解压后尺寸达到和超过限制；
- 已知输出与参考 pypdf 对比。

对于压缩炸弹，校验必须证明资源限制在达到危险内存占用之前生效。

---

## 8. 加密、安全与鲁棒性验证

### 8.1 加密矩阵

按基线支持的版本/修订组合建立矩阵：

| 场景 | Reader 预期 | Writer 预期 |
|---|---|---|
| 未加密 PDF | 正常读取 | 正常写出 |
| 空用户密码 | 与基线一致 | 若支持则可输出 |
| 正确用户密码 | 正常解密 | 权限语义一致 |
| 正确所有者密码 | 正常解密 | 权限语义一致 |
| 错误密码 | 明确失败 | 不生成伪成功文件 |
| 加密 object stream | 正确解密/解析 | 可正确写回 |
| 不同加密 revision | 按声明支持范围处理 | 输出可重新读取 |
| 权限标志组合 | 解释与基线一致 | 写出后保持 |

加密能力不能仅使用空密码样本验收。

### 8.2 恶意/异常输入

至少覆盖：

- 极深嵌套数组/字典；
- 间接引用循环；
- 页树循环；
- outline / form / annotation 树循环；
- 畸形 xref 和错误 object offset；
- 极大 `/Length`；
- stream filter 链过长；
- 解压膨胀；
- 超大 CMap / page label / attachment 名称；
- inline image 边界错误；
- truncated stream；
- PDF header / EOF 缺失；
- 非单调或不合理 offset；
- 重复、冲突或恶意对象编号。

### 8.3 资源限制

每次危险输入测试都记录：

- 输入大小；
- 实际对象数量；
- 最大嵌套层数；
- 解压输出字节；
- CPU 时间；
- 峰值 RSS；
- 是否按预期抛出异常/Warning；
- 是否留下可被误认为成功的部分输出。

资源限制必须可配置且默认安全。若需要与 pypdf 的默认限制保持一致，应在 API 层兼容基线，同时在底层设置不会破坏合法文件的保护措施。

### 8.4 Fuzz 和属性测试

- parser token 和对象解析使用 `proptest` 等做结构化 property test。
- 原始 PDF 字节入口建立 `cargo-fuzz` target；将崩溃样本固化为 regression fixture。
- 对 parser / decoder / writer 输入边界执行 sanitizer 或适当的内存安全检查。
- 对所有 panic、超时、OOM 和非预期成功都建立 issue 和固定回归样本。
- fuzz 测试不能取代真实语料差分测试。

---

## 9. Round-trip 和独立工具互操作

### 9.1 Round-trip

至少验证以下链路：

```text
PDF A
  → Rust Reader
  → Rust Writer（不修改）
  → PDF B
  → Rust Reader
  → 语义摘要 B
```

再与参考链路对比：

```text
PDF A
  → Python pypdf Reader
  → Python pypdf Writer
  → PDF C
  → Python pypdf Reader
  → 语义摘要 C
```

需要额外覆盖：

- merge 多个 PDF 后重新读取；
- 旋转/裁剪/缩放后重新读取；
- metadata/outline/annotation/form/attachment 修改后重新读取；
- 加密 PDF 解密、修改和重新加密；
- 增量写入后重新读取（基线支持时）。

Round-trip 的目标是语义保持，不是字节保持。

### 9.2 独立工具

按部署可用情况使用至少两种独立工具，建议：

- `qpdf --check`：结构和交叉引用检查；
- `pdfinfo`：页数、版本和基本元数据；
- `pdftotext`：外部文本提取交叉检查；
- `mutool info` / `mutool clean`：结构读取和重写交叉检查；
- PDFium 或 Poppler 渲染输出：视觉结果比对。

工具不可用时记录原因，不能把缺失工具写成“验证通过”。外部工具的结果也不是唯一真值；它们与 pypdf/Rust 任一结果不一致时，必须分析差异来源。

### 9.3 视觉比对

对页面变换、合并、资源处理等可能改变外观的操作：

1. 使用同一渲染器、同一 DPI、同一颜色管理参数分别渲染参考输出和 Rust 输出。
2. 使用像素差异、SSIM 或人工抽查识别页面偏移、裁切、字体、透明度和图像差异。
3. 不能因渲染器自身存在细微差异就直接放宽全部阈值；先建立同一文件重复渲染的噪声基线。
4. 保存差异图和页面级报告。

---

## 10. 性能验证方案

### 10.1 基准原则

- 同机器、同 CPU 频率策略、同内存配置、同输入文件、同参数。
- Python reference 和 Rust backend 分开进程执行。
- 不在同一进程先跑 Python 再跑 Rust，避免缓存和内存状态相互污染。
- `compare` 双执行模式不得用于性能测量。
- 不删掉实际工作流中的 Poppler/Pillow 或其他步骤。
- 不以微基准结果代替端到端结果。

### 10.2 Workload 分层

至少建立：

| 层级 | Workload | 目的 |
|---|---|---|
| W1 | 单 PDF Reader 初始化 + 页数/metadata | 读取路径基线 |
| W2 | 全页遍历与对象访问 | 页树和对象缓存 |
| W3 | 指定页面文本提取 | content stream / font / CMap |
| W4 | 指定页面图像枚举/提取 | image/filter 路径 |
| W5 | PDF merge / append / clone | 对象图和 Writer |
| W6 | 页面 rotate/crop/merge | 页面操作 |
| W7 | PDF 写出 | 序列化/xref |
| W8 | 完整业务工作流 | 最终业务收益 |

只有仓库实际用到的 workload 才能定义为 P0；其他 workload 用于功能完整性和回归。

### 10.3 运行次数和统计

- 先做一次 warm-up，不计入结果；
- 每个 workload 至少重复 5 次，推荐 10 次；
- 报告 median、p95、min、max；
- 记录峰值 RSS、CPU 时间和输出文件大小；
- 大文件和小文件分开报告，不得只汇总成一个平均数；
- 失败和超时不能从统计中静默删除。

### 10.4 性能指标

```text
speedup = reference_elapsed / rust_elapsed
latency_change_pct = (rust_elapsed / reference_elapsed - 1) * 100
memory_change_pct = (rust_peak_rss / reference_peak_rss - 1) * 100
```

报告至少同时给出：

- 单次 API latency；
- 整个工作流 latency；
- peak RSS；
- 输出文件大小；
- 成功率；
- 各测试文件的耗时分布。

### 10.5 性能验收

在首次稳定基准后由项目负责人填写：

```text
P0 workload：
目标端到端加速比：
允许的单项回退上限：
允许的峰值 RSS 变化：
最低成功率：
测试机器和环境：
```

默认质量原则：

- 正确性优先于速度；
- 不能用减少处理内容换性能；
- P0 端到端不允许出现未解释的性能回退；
- 只有达到预先记录的端到端目标，才能将移植标记为性能收益达标；
- 如果 Rust 单模块很快但端到端没有收益，必须如实报告，不能只宣传 microbenchmark。

---

## 11. 平台、打包与离线部署验证

### 11.1 构建矩阵

按项目真实支持的平台构建，至少覆盖当前目标部署环境：

- Linux `aarch64`；
- Linux `x86_64`（若项目开发或 CI 使用）；
- 项目实际使用的 Python 版本；
- 与目标机匹配的 glibc / manylinux 基线。

若需要 Windows/macOS，再增加相应平台，不得把未测试平台写为支持。

### 11.2 构建验证

建议使用 maturin/PyO3；具体命令按仓库配置执行。最小检查：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
python -m pytest -q
```

构建 wheel 后：

```bash
python -m pip install --no-deps dist/*.whl
python - <<'PY'
import pypdf
print(pypdf.__version__)
from pypdf import PdfReader, PdfWriter
print(PdfReader, PdfWriter)
PY
```

实际 wheel 应安装到干净虚拟环境，在目标架构执行真实 PDF 读写，不仅仅是 import。

### 11.3 离线验证

- 准备 wheelhouse 和 Cargo crate cache；
- 断网执行构建和安装；
- 禁止构建阶段下载 git 依赖或系统库；
- 记录动态链接依赖；
- 确认 wheel 不依赖构建机的临时路径；
- 在目标服务器验证运行时无需访问外网。

### 11.4 ABI 和运行时

- 明确使用 CPython-specific ABI 还是 abi3；
- 以项目实际 Python 版本验证；
- 检查 extension 与目标 glibc 的兼容性；
- 检查缺少 native library 时的错误是否清晰；
- 不允许在开发机能运行、目标服务器因 glibc 或架构不匹配而加载失败。

---

## 12. 自动化命令和建议脚本

以下为建议入口，Agent 应按仓库真实目录落地；不要求照搬脚本名，但要求保留相同验证能力。

```bash
# 1. Rust 静态检查与单测
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked

# 2. Python 单测与 API 兼容测试
python -m pytest -q tests/unit tests/python_compat

# 3. 差分测试
python scripts/run_differential.py \
  --manifest tests/fixtures/manifest.csv \
  --reference-env .venv-reference \
  --rust-env .venv-rust \
  --output reports/differential.json

# 4. Round-trip 和互操作
python -m pytest -q tests/roundtrip tests/interop

# 5. 安全回归
python -m pytest -q tests/security

# 6. 性能测试（reference / rust 应分开运行）
python benchmarks/run_reference.py --workloads benchmarks/workloads.json --out benchmarks/results/reference.json
python benchmarks/run_rust.py --workloads benchmarks/workloads.json --out benchmarks/results/rust.json
python benchmarks/compare_results.py \
  benchmarks/results/reference.json \
  benchmarks/results/rust.json
```

### 12.1 差分测试脚本的职责

`run_differential.py` 必须：

- 读取 manifest 并按 `case_id` 稳定排序；
- 每个 case 独立子进程执行；
- 限制单 case 超时；
- 不让一个 case 的异常中断剩余用例；
- 输出 reference/rust 两份原始 JSON；
- 输出结构化 diff（字段路径、期望值、实际值）；
- 保存退出码、stderr、warning 和耗时；
- 以非零退出码表示存在未批准差异。

### 12.2 性能脚本的职责

- 语料与参数固定；
- 自动 warm-up 和多次运行；
- reference 与 rust 分开执行；
- 输出原始每轮结果，不只保存平均数；
- 同时记录失败、超时和峰值 RSS；
- 不自动丢弃异常慢的样本；
- 不自动根据结果修改性能门槛。

---

## 13. 缺陷归类和修复规则

发现差异时按以下类别记录，不要直接用“PDF 格式不标准”解释：

| 类别 | 示例 | 处理方式 |
|---|---|---|
| API 差异 | 参数缺失、返回类型不一致 | 修 Python 兼容层 |
| 解析差异 | xref、字符串、对象流不一致 | 修 parser/object layer |
| 过滤器差异 | 解压字节不同、DecodeParms 错误 | 修 filter decoder |
| 字体/文本差异 | CMap、字符顺序、空格不一致 | 修 content/font/text 模块 |
| Writer 差异 | 引用丢失、对象复制错误 | 修 object graph / writer |
| 加密差异 | 密码验证或权限不同 | 修 security 模块 |
| 资源限制差异 | 超时、OOM、未限流 | 修安全边界 |
| 渲染差异 | 页面视觉偏移/裁切 | 定位页面几何或资源处理 |
| 环境差异 | 动态库、glibc、架构加载失败 | 修构建/打包 |
| 测试噪声 | 不稳定时钟、渲染噪声 | 先证明噪声，再调整测试方法 |

每个缺陷必须带：输入 SHA256、复现命令、reference 结果、rust 结果、最小复现样本、根因、修复 commit、回归测试 ID。

不得通过放宽 comparator 或删除失败样本来关闭缺陷，除非有经批准的语义差异说明。

---

## 14. 最终验收报告模板

交付前生成 `reports/FINAL_VALIDATION.md`：

```markdown
# pypdf Rust 移植验证报告

## 1. 基线与环境
- pypdf reference version / wheel SHA256：
- Rust commit：
- Rust / PyO3 / maturin versions：
- Python version：
- OS / kernel / architecture / glibc：
- CPU / memory：

## 2. API 兼容性
- API 总数：
- PASS：
- PASS_WITH_DOCUMENTED_VARIANCE：
- PARTIAL：
- FAIL：
- NOT_TESTED：
- P0 调用点覆盖率：

## 3. 正确性测试
- Rust unit tests：
- Python API tests：
- Differential cases：
- Round-trip cases：
- Interop cases：
- Visual comparison cases：

## 4. 安全和健壮性
- Malformed PDF cases：
- Fuzz duration / corpus：
- Crashes / panics：
- Timeouts：
- OOM：
- Resource-limit checks：

## 5. 性能
- Workload：
- Reference median / p95：
- Rust median / p95：
- Speedup：
- Peak RSS delta：
- End-to-end change：
- 是否达到事先约定目标：

## 6. 部署
- Wheel filename：
- Target platform：
- Clean install：
- Offline install：
- Real PDF smoke test：

## 7. 未解决问题与风险
- ...

## 8. 验收结论
- PASS / PASS WITH DOCUMENTED VARIANCE / FAIL
- 未完成项和批准人：
```

### 14.1 最终通过条件

- P0 功能差分 100% 通过；
- 所有已声明支持的 P1 能力有对应测试和状态；
- 没有未解释的语义差异；
- round-trip 和外部工具互操作通过；
- 安全边界和 fuzz 回归没有未关闭的阻断问题；
- 端到端工作流达到预先定义的性能门槛；
- 目标平台 wheel、干净安装、离线部署和运行测试通过；
- 最终报告完整，未完成项明确披露。

---

## 15. 官方参考资料

- [pypdf 7.1.1 官方文档](https://pypdf.readthedocs.io/en/stable/)
- [pypdf：解析流程](https://pypdf.readthedocs.io/en/stable/dev/pypdf-parsing.html)
- [pypdf：写入流程](https://pypdf.readthedocs.io/en/stable/dev/pypdf-writing.html)
- [pypdf：测试说明](https://pypdf.readthedocs.io/en/stable/dev/testing.html)
- [pypdf：安全说明](https://pypdf.readthedocs.io/en/stable/user/security.html)
- [pypdf：发布记录](https://github.com/py-pdf/pypdf/releases)
- [lopdf 文档与限制](https://docs.rs/lopdf/latest/lopdf/)

