# baseline/commands.md — P0 基线的命令与环境记录

> 本文件记录 `validation/optimization/baseline/` 中每个数字是**怎么来的**，
> 以便任何人重跑。审计细节见 `../REPO_AUDIT.md`，归因见
> `../BASELINE_DIAGNOSIS.md`。

## 1. 冻结的基线数据（不是本次重跑）

| 资产 | 来源 | 生成时间 | 说明 |
|---|---|---|---|
| `raw/W1.csv` … `raw/W8.csv` | `validation/results/perf-reference.json` + `perf-rust.json` | ref `2026-11-24T20:11:05+08:00`，port `2026-11-24T20:11:58+0800` | 已有的 **10 次重复 + 1 次预热** 记录，转换为逐 repetition 的 CSV；**未重跑** |
| `reports/baseline-report.md` | 同上 | 本次 | 中位数 / p95 / speedup / 输出字节一致性表 |

CSV 列：`backend,workload,workload_name,rep,wall_s,cpu_s,peak_rss_bytes,output_bytes,output_sha256,success,timeout,error`

生成命令（只读源 JSON，幂等）：

```bash
cd /Users/yxy/Desktop/pypdf-rust
.venvs/dev/bin/python validation/optimization/scripts/make_baseline.py
```

## 2. harness 复核（本次，`--repeat 2`）

只用来证明“基准脚本仍然可跑、方向与冻结基线一致”，**不作为新基线**。

```bash
cd /Users/yxy/Desktop/pypdf-rust
export PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1

.venvs/oracle/bin/python validation/perf/run.py --backend reference --repeat 2 \
  --out validation/optimization/baseline/raw/quick/perf-reference-repeat2.json

.venvs/dev/bin/python validation/perf/run.py --backend rust --repeat 2 \
  --out validation/optimization/baseline/raw/quick/perf-rust-repeat2.json
```

> 注意：官方脚本 `validation/perf/run_perf.sh` 会把结果写到
> `validation/results/`，本阶段不允许覆盖它，因此这里用 `--out` 重定向到
> `baseline/raw/quick/`。日志：`raw/quick/ref.log`、`raw/quick/rust.log`。

结果（中位数）：

| W | ref (ms) | port (ms) | speedup |
|---|---:|---:|---:|
| W1 | 11.96 | 36.63 | 0.326× |
| W2 | 188.69 | 615.41 | 0.307× |
| W3 | 173.80 | 722.00 | 0.241× |
| W4 | 606.49 | 1891.24 | 0.321× |
| W5 | 12.87 | 42.70 | 0.301× |
| W6 | 16.83 | 57.62 | 0.292× |
| W7 | 21.34 | 75.24 | 0.284× |
| W8 | 401.54 | 1324.23 | 0.303× |

## 3. 环境

`environment.txt`（机器/解释器/工具链/扩展 sha256）与 `git-revision.txt`
（commit、worktree 状态、扩展身份）由 `make_baseline.py` 生成，采集命令是
`sysctl`/`sw_vers`/`uname`/`rustc --version` 等只读调用。

关键值：Apple M1 Pro / arm64 / macOS 26.6.2 / CPython 3.14.7 /
rustc 1.100.0 / cargo 1.100.0 / pyo3 0.30.1 / maturin 1.16.0 /
扩展 sha256 `08c84cb4…f961c`。

## 4. 输入语料哈希

```bash
shasum -a 256 -c validation/optimization/baseline/inputs.sha256   # 全部 OK 才可用
```

覆盖 `validation/perf/workloads.json` 中 W1–W8 引用的 52 个文件（去重）。

## 5. 测量规范（沿用 `validation/perf/` 的定义）

- 每侧**独立进程**；`PYTHONHASHSEED=0`；1 次预热不计入；正式 10 次取中位数。
- 记录 wall / CPU / 峰值 RSS（1.5 ms 采样）/ 输出字节 / 输出 SHA-256。
- 失败与超时**记录但不丢弃**（CSV 中 `success`/`timeout`/`error` 列）。
- 本机 loadavg 16+，只适合判断大比例差异；5% 级结论需要 ABBA 顺序与更多重复
  （方案 §3.3）。

## 6. 本阶段新增的测量脚本

```bash
# issue-301 调用计数 / W3 拆分 / W4 拆分 / W5-W7 拆分 / cProfile
.venvs/dev/bin/python validation/optimization/scripts/probe.py {issue301|w3|w3ops|w4|w4b|w57|stages|cprofile}
.venvs/dev/bin/python validation/optimization/scripts/probe_cprofile_workloads.py
```

输出默认写入 `validation/optimization/baseline/raw/`。完整命令清单见
`../BASELINE_DIAGNOSIS.md` §9。
