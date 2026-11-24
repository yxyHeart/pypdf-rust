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
python/pypdf/           # Python 包（模块树镜像上游；未移植模块为自动生成的 stub）
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

见 [`docs/SYMBOL_COVERAGE.md`](docs/SYMBOL_COVERAGE.md)。P0（环境/基线/确定性/可行性探针）已完成，正在按阶段移植。

## 许可证

上游 pypdf 为 BSD-3-Clause；本仓库保留其 LICENSE 与版权声明。详见 `vendor/pypdf-7.1.1/LICENSE`。
