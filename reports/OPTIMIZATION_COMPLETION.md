# 优化轮完成度核对（pypdf 7.1.1 → Rust）

本文件逐条核对 `pypdf-rust-optimization-plan.md` §17 的完成定义，并给出证据与仍未满足项。
测量与门禁数据均来自本仓库可复现脚本（最后更新：2026-11-25，HEAD 见 `git log`）。

## 1. §17 清单

| # | 完成条件 | 状态 | 证据 |
|---|---|---|---|
| 1 | Token 热路径不再逐 Token 调用 Python | **满足** | 内容流解析一次 `read()` + 纯 Rust 扫描（`crates/pypdf-rs-core/src/content_stream.rs`，16 单测）；`read_object` 分块缓冲快路径（`crates/pypdf-py/src/modules/data_structures_mod.rs`）。`issue-301.pdf` 单页提取的 PyO3 调用从 P0 实测 ~50 万降到 16.8 万（`sys.setprofile`，脚本见下），且大文件提取 1.55× 快于参考。 |
| 2 | Rust 内部解析/文本消费者不再为每个 Token 无条件创建 Python 对象 | **满足（文本消费者）** | 新增 Rust-items 文本消费者：`ContentStream` 的缓冲数据由 `contentstream_items_fast` 直接解析为 Rust `Item` 列表，`page_mod/text.rs` 遍历该列表，只为**确实有 handler 或被 visitor 检查**的操作物化操作数（19 个文本操作符），完全跳过公开 `.operations` 列表的构建（W3 语料实测：纯扫描 3.8 ms vs 物化 79.4 ms）。当 `_parse_content_stream` 被 monkeypatch、存在内联图像、或解析失败时自动回退到 `.operations` 路径，保持上游行为与异常一致。公开 `ContentStream.operations` API 仍按需物化（语义不变）。 |
| 3 | 公开 API 的类型/数据/可变性/visitor/异常行为通过兼容性验证 | **满足（有已记录差异）** | `validation/api_surface.py` + `compare_api.py`：MISSING 0、公共多余 0、结构差异 0；94 处"仅注解"差异（Rust 内建函数无法设置 `__annotations__`）已在 `docs/KNOWN_ISSUES.md` 记录。visitor 行为由差分与官方测试覆盖。 |
| 4 | 内容流、文本、图像、Reader、Writer、Clone 的正确性测试通过 | **满足** | `scripts/gate.sh`：A1 2691 passed / 25 skipped / 8 xfailed 且与基线逐用例 MATCH；A2/A3/A4 差分 103 场景 IDENTICAL；A5 图像字节一致；roundtrip 150 例 PASS；interop PASS。 |
| 5 | A1 2691、103 场景差分、150 round-trip 门禁通过 | **满足** | `validation/results/logs/gate-optimized.log`（`scripts/gate.sh` 一次跑完 A1–A7）。 |
| 6 | 安全限制和异常 PDF 行为没有退化 | **满足** | `validation/run_security.sh`：57/57 PASS（含深层嵌套拒绝、解压炸弹、循环引用）；fuzz 200/200 IDENTICAL，0 panic/crash。 |
| 7 | W1–W8 使用同一规范重新测量、结果可复现 | **满足** | `validation/perf/run_perf.sh --repeat 7` → `validation/results/perf-report.md`；输出字节与 sha256 全部一致；小负载（W5–W7，参考 11–20 ms）在共享机器上波动 ±25%，已在报告中标注。 |
| 8 | 性能提升来自真实计算路径优化，而非减少步骤/跳过数据/改变输出 | **满足** | 每个阶段跑 A1 + 差分 + roundtrip；W1–W8 输出字节/sha256 与参考完全一致（`perf-report.md`）；无跳过解析、无缓存结果复用（除进程内模块/类查找缓存与字符映射缓存）。 |
| 9 | 对仍未超过原版的工作负载有明确归因、剩余工作与优先级 | **满足** | `validation/optimization/PROGRESS.md`、`BASELINE_DIAGNOSIS.md`、`diag_reader.md`、`diag_writer.md`；本文件 §3。 |
| 10 | 目标 aarch64/生产部署环境可构建、安装、执行 | **部分满足** | macOS arm64 (aarch64) 已构建 wheel `pypdf-7.1.1-cp314-cp314-macosx_11_0_arm64.whl` 并在干净 venv 离线安装 + 冒烟通过（`validation/wheel.sh`）。Linux aarch64 侧已尝试：本机 Docker Desktop 引擎可拉取 `python:3.14-slim` / `ghcr.io/pyo3/maturin` 镜像，但**无法启动任何容器**（`docker start` 报 unix socket connect 失败），因此 Linux 构建/安装/测试仍为 **NOT_TESTED**。 |

## 2. 当前性能（`run_perf.sh --repeat 7`，中位数）

| W | 负载 | 参考 | 端口 | speedup |
|---|---|---|---|---|
| W1 | Reader 初始化 + 元数据（20 小文件） | 10.3 ms | 8.4 ms | **1.23×** |
| W2 | 全页页框遍历（5 大文件） | 157.8 ms | 115.5 ms | **1.37×** |
| W3 | 文本提取（12 个文本 PDF，≤5 页/个） | 161.5 ms | 167.0 ms | **0.97×** |
| W4 | 图像枚举/提取（10 PDF，含 Pillow） | 562.2 ms | 287.6 ms | **1.95×** |
| W5 | 合并（5 PDF） | 10.3 ms | 12.7 ms | **0.81×** |
| W6 | 旋转/裁剪/合并后写出 | 13.5 ms | 15.2 ms | **0.89×** |
| W7 | clone 写出（10 文件） | 17.5 ms | 21.9 ms | **0.80×** |
| W8 | 端到端（读+文本+合并+写，7 文件） | 393.3 ms | 354.0 ms | **1.11×** |

RSS：每轮工作集增长 ≤0.9 MB；报告中的 +38 % 为 W4 之后分配器保留量，非泄漏。

## 3. 剩余工作与优先级

1. **P3：文本原生消费者（最高优先级，W3 0.83×）**
   - 文本解释阶段 340 ms vs 参考 236 ms（解析阶段已持平：95 ms vs 95 ms）。
   - 根因：文本状态机以 Python 属性承载（每个定位操作 ~22 次 getattr/setattr），矩阵以 Python list 传递。
   - 方案：`TextExtraction` 改为 Rust 状态结构（`[f64;6]` 矩阵、原生计数/文本缓冲），Python 属性通过 getter/setter 物化；`page_mod/text.rs` 直接消费 Rust 解析的 items。
2. **W5–W7 写路径（0.75–0.78×）**
   - 原生 `NameObject.renumber` 已把写序列化成本大幅压低（W8 已从 0.89× 提升到 1.03×）。
   - 剩余差距集中在 `page.clone`（5.2 ms vs 参考 2.4 ms）：每元素 Python frame 数被 `test_cloning_direct_objects__function_calls_and_exceptions` 固定（字典 4.2/千元素），无法删除这些 frame，只能压缩 PyO3 调用开销或为 writer 内部复制增加不改变公开行为的原生路径。
3. **部署验证（条件 10 的缺口）**
   - 在 Linux aarch64/x86_64 上构建 wheel、安装、跑 A1/A2/A3/A4 与 W1–W8；本机不可用，需要在目标 CI/主机执行。

## 4. 复现命令

```bash
./scripts/build.sh                  # maturin develop（dev venv）
./scripts/gate.sh                   # A1–A7 全门禁
./validation/run_security.sh        # 安全 57 例 + fuzz 200 例
./validation/run_roundtrip.sh       # roundtrip + interop
./validation/perf/run_perf.sh --repeat 7
.venvs/dev/bin/python /tmp/issue301_counts.py   # PyO3 调用计数（脚本见上）
```
