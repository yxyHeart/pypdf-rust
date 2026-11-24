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
