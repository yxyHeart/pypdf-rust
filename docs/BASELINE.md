# 基线快照（Baseline）

> 上游 pypdf 7.1.1 在本机的官方测试基线。门禁 A1 要求 Rust 实现的测试结果与本文档记录的集合**逐项一致**（passed / skipped / xfailed 相同，failed / error 为 0）。

## 1. 环境

| 项 | 值 |
|---|---|
| 平台 | macOS arm64（Apple Silicon），10 核，16 GB |
| Python | CPython 3.14.7（Homebrew） |
| pytest | 9.2.0（pytest-cov 7.1.0 / pytest-socket 0.8.1 / pytest-timeout 2.4.0 / pytest-xdist 3.8.0 / PyYAML 6.0.4） |
| Rust | stable 1.100.0（aarch64-apple-darwin），pyo3 0.30.1 |
| 构建 | maturin 1.16.0 |
| 参考实现 | pypdf 7.1.1（PyPI sdist，已 vendor 到 `vendor/pypdf-7.1.1/`） |
| 可选依赖 | cryptography 50.0.2、Pillow 13.0.0、fonttools 4.68.0、brotli 1.2.1、arabic-reshaper 3.1.0、python-bidi 0.6.12、fpdf2 2.9.0 |
| 外部二进制 | poppler（`pdfattach`）、`jbig2dec` |

依赖锁定：`requirements-oracle.lock`（`pip freeze` 全量导出）。

## 2. 运行命令（与上游 CI 一致）

```bash
# 基线（参考实现）
.venvs/oracle/bin/python -m pytest tests -n auto -vv -p no:benchmark \
    --junitxml=.audit/baseline-junit.xml

# Rust 实现（门禁 A1，同一命令行、同一 tests/）
.venvs/dev/bin/python -m pytest tests -n auto -vv -p no:benchmark \
    --junitxml=.audit/rust-junit.xml
```

pytest 配置来自仓库根 `pyproject.toml`，其 `[tool.pytest.ini_options]` 段与上游 7.1.1 完全一致（`addopts = "--disable-socket"`、`filterwarnings = ["error"]`、markers、`xfail_strict = true`），保证 warning 升级为 error 等行为不被改变。

## 3. 结果（2026-11-24，含全部可选依赖）

| 指标 | 值 |
|---|---|
| 测试用例数（含参数化） | **2724** |
| passed | **2691** |
| skipped | **25** |
| xfailed | **8** |
| failed / error | **0** |
| 运行时长 | 23.31s（`-n auto`，下载缓存已预热） |

skip 明细（25）：`tests/scripts/test_make_release.py` 21 个（上游 sdist 不含 `make_release.py`）、`test_dictionary_object__read_from_stream__no_limit{,_path}` 2 个（`RLIMIT_AS` 不可靠）、`test_encrypt_decrypt_with_cipher_class[CryptRC4]`（cryptography 50 移除 RC4）、`test_asciihexdecode__memory`（上游 `importorskip` quirk）。

xfail 明细（8）：JPX CMYK/SMaskInData 4 个、PNG predictor 1/2/4-bit 3 个、`test_text_extraction_issue_1091` 1 个（均为上游已知问题，`xfail_strict = true` 要求 Rust 侧同样失败）。

逐用例结果快照：`.audit/baseline-outcomes.json`（由 `tools/baseline/snapshot.py` 从 JUnit XML 生成，按 `classname::name` 索引）。

## 4. 与上游 CI 的差异说明

1. `tests/scripts/test_make_release.py`（21 个用例）因上游 sdist 不包含 `make_release.py` 而 skip；该脚本不属于移植范围，保持 skip 一致。
2. `tests.test_filters.test_asciihexdecode__memory` 因上游 `importorskip("tracemalloc", "Needs to measure memory usage")` 的 quirk 永远 skip（`tracemalloc` 无 `__version__`）。
3. `tests.test_encryption.test_encrypt_decrypt_with_cipher_class[CryptRC4]` 因 cryptography 50 移除 RC4 而 skip（上游 `HAS_AES`/provider 判定逻辑），行为需在 Rust 侧复刻。
4. jbig2 / poppler / fpdf2 相关用例在缺少对应依赖时会 skip；本基线已安装这些依赖，因此这些用例**要求在 Rust 实现上通过**。
5. 未来若要启用 `make_release.py` 相关用例，需重新采集基线并同步更新本文件。

## 5. 基线的用途

- 门禁 A1：`python tools/baseline/snapshot.py .audit/rust-junit.xml --compare .audit/baseline-outcomes.json` 必须输出 `MATCH`。
- 差分场景（门禁 A2/A3/A4）以同一 oracle 环境生成黄金输出，见 `docs/FIDELITY_CONTRACT.md`。
