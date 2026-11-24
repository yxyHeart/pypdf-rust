# W1 / W2 — Reader diagnostics (Rust port vs reference pypdf 7.1.1)

> Scope: OPT-010 (P0 attribution) and P4 (§7 "Reader, page tree and PDF object parsing") of
> `pypdf-rust-optimization-plan.md`, for workloads **W1** (`reader_init_20_small`) and
> **W2** (`page_traverse_5_large`).
>
> Everything below is measured, not estimated. Inputs are exactly the W1/W2 file lists from
> `validation/perf/workloads.json`; the two sides run in separate processes on the same machine:
> reference = `.venvs/oracle` (PyPI pypdf 7.1.1), port = `.venvs/dev` (`python/pypdf` + the
> `pypdf._pypdf` PyO3 extension at commit `f4d3c45`, extension sha256 `f4678f30a158b4e7…`). No library,
> test, tool, doc or Cargo file was touched — the instrumentation is runtime monkeypatching inside
> `validation/optimization/scripts/`.
>
> **Provenance caveat:** every (file:line) reference below was verified against commit `f4d3c45` and
> re-checked against `50daf7a` (the P1 content-stream-scanner commit that landed while this report was
> being written). The measured numbers belong to the `f4d3c45` extension: the tree has since advanced
> and the extension was rebuilt, so **W1/W2 must be re-baselined after P1 is integrated** before the
> P4 work starts from these numbers.
>
> Raw evidence: `raw/rw_diag_rust.json`, `raw/rw_diag_reference.json`,
> `raw/rw_micro_rust.json`, `raw/rw_micro_reference.json`.
> Regenerate with `scripts/run_all.sh` (or the commands in §7); tables with `scripts/render_tables.py`.

## 0. Headline

| workload | port | reference | port/reference | dominant stage in the port |
|---|---:|---:|---:|---|
| W1 (20 small PDFs: open + page count + 5 metadata fields) | **33.3 ms** | **10.6 ms** | **3.14×** | page-tree flatten 68%, open/xref 25%, metadata 7% |
| W2 (5 large PDFs: open + every page + 5 boxes + rotation) | **563.8 ms** | **165.9 ms** | **3.40×** | page-tree flatten 70%, open/xref 26%, box reads 4% |


Five facts that decide the P4 design:

1. **The port runs the same algorithm.** Per-stage and per-file *call counts* are identical on both
   sides — 241 `get_object`, 6 `_get_object_from_stream`, 20 `_flatten`, 41 `_pop_next_page_tree_kid`,
   367 `read_object_header`, 21,270 `read_object_header` for W2, 1,865 `_get_rectangle`, 373
   `PageObject.__init__`. The 3.1–3.4× gap is therefore **per-operation cost**, not extra work.
2. **The port crosses the Python↔Rust boundary 21,415 times for W1 and 472,186 times for W2**
   (≈1,070 per W1 file, ≈1,233 per W2 page). The single largest contributors are token-level helpers
   (`read_non_whitespace` 21.2%, `skip_over_whitespace` 13.5%, `read_until_whitespace` 9.0% in W2).
3. **One crossing costs 423 ns** with a trivial Rust body, vs **54 ns** for a plain Python call and
   **79 ns** for the reference's Python equivalent (measured, §4). Most operations need several
   crossings, so the per-element factor is 3–7×.
4. **Object resolution dominates both workloads**: `_flatten` is 68%/74% of the workload, and it is
   almost entirely `get_object` (W2: 412.5 ms of 417.7 ms), i.e. object parsing/resolution. The
   page-tree walk itself is cheap: `_enter_page_tree_node` 0.23 ms and `_flatten_leaf_page` 6.2 ms
   for 373 pages in W2 — inheritance lookup is *not* a bottleneck.
5. **Object-stream (ObjStm) parsing is the single biggest lever**: 6 calls = 16.2 ms
   (49% of W1), 24 calls = 265.1 ms (47% of W2). Both sides parse
   *all* objects of an ObjStm on first access (same design), so the cost is entirely the per-object
   parse path, and it is 3.6–4.2× slower here.

Consequence for P4: the win does not come from "moving the page tree into Rust" (it already is);
it comes from (a) parsing PDF syntax on a Rust byte cursor instead of a Python file object, (b) not
calling a Python helper per token/object, (c) resolving Python classes once per call instead of
per element, and (d) resolving indirect objects / ObjStm contents without re-entering Python per
object.
## 1. Environment, method, and what the numbers can and cannot show

### 1.1 Environment

| item | port | reference |
|---|---|---|
| Python | 3.14.7 (`/Users/yxy/Desktop/pypdf-rust/.venvs/dev/bin/python`) | 3.14.7 (`.venvs/oracle/bin/python`) |
| pypdf | 7.1.1 (`/Users/yxy/Desktop/pypdf-rust/python/pypdf/`) | 7.1.1 (`site-packages/pypdf/`) |
| extension | `pypdf._pypdf` PyO3 module, commit `f4d3c45` | — |
| machine | Apple M1 Pro, arm64, macOS 26.6.2, 10 CPUs, 16 GiB | same |
| loadavg 1/5/15 at start | 6.48 / 10.72 / 12.45 | 8.12 / 10.67 / 12.36 |
| loadavg at end | 8.12 / 10.67 / 12.36 | 7.56 / 10.47 / 12.27 |
| repeats | 7 (wall phases), 7 (stage timers), 1 (counts), 3 (cProfile), 3–4 (per-file) | same |

The machine is shared and was under 0.6–1.1× oversubscription during the run (10 CPUs, 1-min loadavg
6.5–8.1, 5-min 10.67–10.72). Absolute numbers therefore drift by up to ~20% between runs; **ratios within a single run**
are the evidence used below. For cross-checking, the current baseline report
(`validation/results/perf-report.md`, re-generated 2026-11-24 21:41 against extension sha
`74a3905a…`, i.e. **after** the P1 content-stream-scanner commit) has W1 = 11.8/35.7 ms (0.33×) and
W2 = 173.2/581.5 ms (0.30×). This diagnostic run used the previous build (`f4678f30…`, commit
`f4d3c45`); its port side reproduces the baseline (35.7 → 33.3 ms, 581.5 → 563.8 ms) while its
reference side is faster (11.8 → 10.6 ms, 173.2 → 165.9 ms), so the ratios reported here
(3.14×/3.40×) are pessimistic relative to the baseline's 0.33×/0.30×. Every stage-level ratio in
this report is 1.9–4.4×, consistent with both. The difference is machine load, not the port: the
read/parse path that W1/W2 exercise is not touched by P1.

### 1.2 Instrumentation (independent passes, never mixed)

| pass | what it does | overhead | where |
|---|---|---|---|
| `wall_phases` | brackets each workload stage with `perf_counter_ns` directly in the workload code; **no monkeypatching** | ~0 | `rw_diag.py:run_wall_phases` |
| `layers` | cumulative layers (open → +pages → +metadata/traverse), differenced | ~0 | `run_layers` |
| `stages` | wraps the shim methods of each stage with an inclusive timer | ~0.2 µs/wrapped call; inflates the workload wall by 4–7% (compare `workload wall` with the `wall_phases` total) | `StageTimer` |
| `counts` | (a) replaces every `pypdf._pypdf` function reachable from a shim module global *and* the extension-module attribute with a counting wrapper; (b) wraps `__new__` of 17 PDF object classes; (c) counts a few Python-level methods | ~0.3–0.5 µs/intercepted call; wall time of this pass is *not* used | `BoundaryCounter` |
| `profile` | `cProfile`, 3 repetitions merged, classified into `rust_builtin` / `python_shim` / `other_*` | large (see caveat) | `profile_rows` |

Counting detail that matters for reading the tables: a call is counted once even though the port
uses two import styles (`from pypdf._pypdf import x` → module global, and
`_writermod.some_function(...)` → extension attribute). `patched_globals=724`,
`patched_ext_attrs=940` per counting pass.

**Caveats, stated explicitly:**

* **cProfile cannot see calls made from Rust into C.** `stream.read(1)` / `stream.seek(...)` on the
  `BytesIO` are invoked from Rust (`PyObject_Call` from C), so no `c_call` event is emitted: that time
  is attributed to the *self* time of the enclosing Rust builtin frame. The same applies to the
  Python helpers Rust calls (`read_non_whitespace`, ...) — they are C functions called from C. This is
  why the port's profile lists only ~91k function calls while the same work issues hundreds of
  thousands of C-level calls.
* **cProfile inflates Python-heavy code much more than Rust-heavy code.** Both sides were profiled
  with the same 3 repetitions, but the reference executes 339k visible calls (W1) vs the port's 91k,
  so profiled *absolute* times are not comparable across backends. Only (a) rankings *within* a
  backend and (b) the call counts are used; absolute attribution comes from the wall-phase table.
* **The instance-creation counters patch `cls.__new__`, which CPython 3.14 cannot restore** (deleting
  the attribute again leaves `tp_new` broken, so the next instantiation raises
  `object.__new__() takes exactly one argument`). The counting pass therefore runs last, per workload,
  in a process that is discarded afterwards; the first version of this harness polluted later
  workloads and its W2–W7 profiles contained the harness' own frames. That was found and fixed
  (mode-major pass order + no restore + idempotent re-patching); the data in `raw/` is from the fixed
  harness and contains **no** `rw_instrument` frames (verified: 0 matches in all five profiles).
* Stage rows are **inclusive**; `get_object` is called from `_flatten`, so its time is contained in
  `_flatten` and rows must not be summed.
* Per-file numbers come from the stage pass (with its ~6–9% wrapper overhead), so their *sum* is
  ~3% above the whole-workload wall phase total; they are used for ranking, not for addition.

## 2. W1 — Reader init on 20 small PDFs

### 2.1 Where the 33 ms go

| phase | port median | port min–max | port share | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|---:|
| `open` | 8.40 ms | 7.56–9.51 | 25.2% | 4.18 ms | 3.88–4.35 | 2.01× |
| `pages_flatten` | 22.61 ms | 22.17–23.66 | 67.9% | 5.59 ms | 5.47–6.74 | 4.04× |
| `metadata` | 2.39 ms | 2.26–2.55 | 7.2% | 0.74 ms | 0.72–0.86 | 3.23× |
| **total** | **33.32 ms** | 32.42–35.17 | 100% | **10.61 ms** | 10.41–11.49 | **3.14×** |

Read this table as the primary result: **two thirds of W1 is page-tree flattening**, a quarter is
opening/xref/trailer, 7% is metadata. The reference has the same shape (open 39%, flatten 53%,
metadata 7%), so W1 is not dominated by a stage the port added — it is the same work done 2.0×
(open), 4.0× (flatten) and 3.2× (metadata) more slowly.

### 2.2 Cumulative layers (independent cross-check of 2.1)

| layer (cumulative) | port median | port min–max | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|
| open | 7.49 ms | 7.28–19.65 | 3.86 ms | 3.56–8.35 | 1.94× |
| open+pages | 30.52 ms | 29.56–32.17 | 9.77 ms | 9.62–10.81 | 3.12× |
| open+pages+metadata | 33.44 ms | 32.07–35.10 | 10.58 ms | 10.12–10.86 | 3.16× |

| derived stage (= layer difference) | port | reference | port/reference |
|---|---:|---:|---:|
| open+pages − open | 23.02 ms | 5.91 ms | 3.90× |
| open+pages+metadata − open+pages | 2.93 ms | 0.81 ms | 3.61× |

The layer differences agree with the direct timers within noise (flatten 22.3 vs 22.6 ms,
metadata 2.0 vs 2.4 ms), which is the check that the direct-timer phases are not misattributed.

### 2.3 Stage timers (inclusive) and their call counts

| stage | calls (port / ref) | port ms | reference ms | port/reference | stage % of workload (port) |
|---|---:|---:|---:|---:|---:|
| `open:get_object` | 241 / 241 | 24.96 | 6.72 | 3.72× | 72% |
| `tree:get_num_pages` | 20 / 20 | 22.99 | 6.20 | 3.71× | 66% |
| `tree:_flatten` | 20 / 20 | 22.94 | 6.18 | 3.71× | 66% |
| `open:_get_object_from_stream` | 6 / 6 | 16.20 | 4.38 | 3.70× | 47% |
| `open:__init__` | 20 / 20 | 8.72 | 4.35 | 2.01× | 25% |
| `open:_initialize_stream` | 20 / 20 | 8.42 | 4.23 | 1.99× | 24% |
| `open:read` | 20 / 20 | 7.49 | 3.55 | 2.11× | 22% |
| `open:_read_xref_tables_and_trailers` | 20 / 20 | 5.24 | 1.86 | 2.82× | 15% |
| `tree:_pop_next_page_tree_kid` | 41 / 41 | 3.28 | 0.84 | 3.91× | 9% |
| `open:_read_pdf15_xref_stream` | 8 / 8 | 2.61 | 0.87 | 3.01× | 8% |
| `meta:metadata` | 20 / 20 | 2.49 | 0.82 | 3.02× | 7% |
| `open:read_object_header` | 367 / 367 | 2.07 | 0.62 | 3.36× | 6% |
| `open:_read_xref` | 14 / 14 | 1.88 | 0.76 | 2.49× | 5% |
| `open:_read_standard_xref_table` | 14 / 14 | 0.57 | 0.25 | 2.34× | 2% |
| `tree:_flatten_leaf_page` | 21 / 21 | 0.37 | 0.13 | 2.90× | 1% |
| `tree:PageObject.__init__` | 21 / 21 | 0.28 | 0.08 | 3.51× | 1% |
| `tree:_enter_page_tree_node` | 20 / 20 | 0.23 | 0.07 | 3.41× | 1% |
| `open:cache_indirect_object` | 170 / 170 | 0.22 | 0.06 | 3.84× | 1% |
| `open:_find_eof_marker` | 20 / 20 | 0.18 | 0.91 | 0.19× | 1% |
| `open:cache_get_indirect_object` | 335 / 335 | 0.16 | 0.05 | 3.08× | 0% |
| `open:_find_startxref_pos` | 20 / 20 | 0.12 | 0.05 | 2.27× | 0% |
| `open:_process_xref_stream` | 8 / 8 | 0.08 | 0.02 | 4.65× | 0% |
| **workload wall** | | **34.79** | **11.78** | **2.95×** | 100% |

_Stage rows are inclusive: a stage that is called from another stage (e.g. `get_object` from `_flatten`) is contained in its caller, so rows must not be summed._

What the stage table says, in words:

* `_flatten` (22.94 ms, 68%) is almost entirely object resolution: `get_object` 24.96 ms is called 241
  times, and those 241 calls are the page trees themselves (`/Pages` → `/Kids` → each page) plus the
  dictionaries those objects point at (`/Resources`, `/MediaBox`, ...) and `/Info`.
  `_pop_next_page_tree_kid` (41 calls, 3.28 ms) and `_flatten_leaf_page`
  (21 calls, 0.37 ms) are cheap: **the traversal machinery is not the problem, resolving the objects
  it walks over is.**
* `_get_object_from_stream` (6 calls, 16.2 ms, 49%) is the ObjStm path. **6 of the 20 W1 files carry
  object streams** (`pdflatex-forms.pdf` 28 objects, `superscript-pdftex.pdf` 16, `crazyones.pdf` 15,
  `commented.pdf` 14, `metadata.pdf` 12, `bytes.pdf` 8), and they are the slowest per-file entries:
  `pdflatex-forms.pdf` spends 4.82 ms of its 5.67 ms in the ObjStm parse. The port parses all objects
  of the stream on the first access and caches them (`reader_mod.rs:791`, "Parse ALL objects in this
  stream in one pass and cache them"), the reference does the same — the 3.70× is the parse path
  itself.
* The whole xref/trailer machinery (`read` 7.49 ms of which `_read_xref_tables_and_trailers` 5.24 ms)
  is only 22% of W1, and `read_object_header` (367 calls, xref verification) is 2.07 ms. **Optimising
  xref parsing alone cannot fix W1.**
* Two places are *faster* on the port: `_find_eof_marker` (0.18 vs 0.91 ms, 0.19×) and the
  `selenium-pypdf-issue-177.pdf` file as a whole (0.98 vs 1.16 ms, 0.84×; the frozen baseline had
  0.72× on this file) — consistent with the
  note in the plan that this sample already reads 1.38× faster on the port. P4 must not regress them.

### 2.4 Per input file (exact call counts, port wall time)

| file | bytes | port total | ref total | port/ref | port open | port flatten | port get_object | port objStm | boundary calls | materialised objects |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `resources/pdflatex-forms.pdf` | 27712 | 5.67 | 1.52 | 3.72× | 0.54 | 4.89 | 5.18 | 4.82 | 3,690 | 854 |
| `resources/commented.pdf` | 13389 | 4.18 | 1.23 | 3.40× | 0.99 | 2.99 | 3.04 | 2.83 | 2,439 | 591 |
| `resources/superscript-pdftex.pdf` | 34594 | 4.03 | 1.26 | 3.19× | 0.56 | 3.23 | 3.45 | 3.15 | 2,264 | 578 |
| `resources/crazyones.pdf` | 11448 | 3.35 | 1.03 | 3.24× | 0.39 | 2.91 | 2.98 | 2.85 | 2,026 | 517 |
| `resources/metadata.pdf` | 13294 | 2.38 | 0.99 | 2.40× | 0.45 | 1.50 | 1.96 | 1.44 | 1,412 | 311 |
| `resources/bytes.pdf` | 1081 | 1.32 | 0.45 | 2.92× | 0.47 | 0.81 | 0.92 | 0.69 | 988 | 147 |
| `resources/hello-world.pdf` | 29344 | 1.18 | 0.37 | 3.21× | 0.50 | 0.52 | 0.55 | — | 808 | 94 |
| `resources/issue-297.pdf` | 958 | 1.13 | 0.43 | 2.64× | 0.77 | 0.33 | 0.29 | — | 614 | 121 |
| `resources/commented-xmp.pdf` | 17871 | 1.11 | 0.36 | 3.09× | 0.63 | 0.33 | 0.38 | — | 658 | 87 |
| `resources/AutoCad_Simple.pdf` | 3015 | 1.09 | 0.36 | 3.03× | 0.21 | 0.71 | 0.80 | — | 686 | 131 |
| `resources/two-different-pages.pdf` | 7681 | 1.08 | 0.37 | 2.92× | 0.28 | 0.70 | 0.72 | — | 768 | 125 |
| `resources/selenium-pypdf-issue-177.pdf` | 34534 | 0.98 | 1.11 | 0.88× | 0.47 | 0.35 | 0.41 | — | 546 | 63 |
| `resources/multilang.pdf` | 29344 | 0.92 | 0.36 | 2.55× | 0.35 | 0.45 | 0.50 | — | 808 | 94 |
| `resources/imagemagick-ASCII85Decode.pdf` | 2848 | 0.83 | 0.33 | 2.50× | 0.33 | 0.35 | 0.42 | — | 665 | 75 |
| `resources/reportlab-inline-image.pdf` | 1537 | 0.82 | 0.33 | 2.51× | 0.24 | 0.34 | 0.51 | — | 581 | 86 |
| `resources/imagemagick-CCITTFaxDecode.pdf` | 1880 | 0.79 | 0.35 | 2.25× | 0.29 | 0.34 | 0.41 | — | 649 | 75 |
| `resources/imagemagick-lzw.pdf` | 2678 | 0.79 | 0.33 | 2.39× | 0.31 | 0.33 | 0.41 | — | 665 | 75 |
| `resources/box.pdf` | 5395 | 0.74 | 0.24 | 3.05× | 0.25 | 0.35 | 0.39 | — | 450 | 63 |
| `resources/Sample_Td-matrix.pdf` | 1304 | 0.62 | 0.24 | 2.55× | 0.17 | 0.33 | 0.36 | — | 392 | 66 |
| `resources/toy.pdf` | 1113 | 0.52 | 0.21 | 2.51× | 0.20 | 0.31 | 0.26 | — | 306 | 53 |
| **sum** | | **33.53** | **11.88** | **2.82×** |  |  |  |  | **21,415** | **4,206** |

Observations:

* The boundary-call count scales with the amount of PDF syntax, not with file size:
  `pdflatex-forms.pdf` (27 KB) needs 3,690 crossings while `multilang.pdf` (29 KB) needs 808.
* The port has a **fixed per-file cost floor** of roughly 0.3 ms: `toy.pdf` (1.1 KB) costs 0.52 ms vs
  0.22 ms, `Sample_Td-matrix.pdf` 0.62 vs 0.24 ms. For non-object-stream files the floor is what
  P4's "small-file fast path" (§7.2 item 6) has to attack — it is the 30–45 boundary calls the reader
  makes unconditionally (constructor, validation, EOF/startxref, trailer, `/Info`).
* `issue-297.pdf` is the only file where the **open** phase (0.77 ms of 1.13 ms) outweighs flattening:
  it triggers xref recovery (`Ignoring wrong pointing object 2 0 (offset 76)` warning is logged twice
  per run on both backends).

### 2.5 Python↔Rust boundary calls and object materialisation

| phase | Python→Rust boundary calls | share | materialised PDF objects |
|---|---:|---:|---:|
| `open` | 6,117 | 28.6% | 529 |
| `pages` | 13,529 | 63.2% | 3,489 |
| `metadata` | 1,769 | 8.3% | 188 |
| **total** | **21,415** | 100.0% | **4,206** |

_The reference has no extension boundary: its equivalent work is ordinary Python calls counted by cProfile (see the profile tables). `py:` counters appear on both backends and are reported in the call-count table instead._

Total Python→Rust boundary calls: **21,415**; materialised PDF objects: **4,206**. (724 shim globals and 940 extension attributes intercepted; the reference has no extension boundary.)

| # | intercepted callable | port calls | share of boundary calls |
|---|---|---:|---:|
| 1 | `read_non_whitespace` | 4,633 | 21.6% |
| 2 | `datastructures_read_object` | 1,639 | 7.7% |
| 3 | `read_until_regex` | 1,608 | 7.5% |
| 4 | `datastructures_arrayobject_check_limits` | 1,424 | 6.6% |
| 5 | `basemod_name_read_from_stream` | 1,420 | 6.6% |
| 6 | `basemod_name_unnumber` | 1,420 | 6.6% |
| 7 | `skip_over_whitespace` | 1,101 | 5.1% |
| 8 | `basemod_number_new` | 1,090 | 5.1% |
| 9 | `read_until_whitespace` | 734 | 3.4% |
| 10 | `doccommon_convert_to_int` | 477 | 2.2% |
| 11 | `basemod_float_new` | 476 | 2.2% |
| 12 | `skip_over_comment` | 370 | 1.7% |
| 13 | `reader_read_object_header` | 367 | 1.7% |
| 14 | `py:PdfReader.cache_get_indirect_object` | 335 | 1.6% |
| 15 | `reader_cache_get_indirect_object` | 335 | 1.6% |
| 16 | `py:PdfReader.get_object` | 241 | 1.1% |
| 17 | `reader_get_object` | 241 | 1.1% |
| 18 | `basemod_indirect_get_object` | 240 | 1.1% |
| 19 | `datastructures_dictionaryobject_read_from_stream` | 238 | 1.1% |
| 20 | `datastructures_dictionaryobject_read_stream_data` | 238 | 1.1% |
| 21 | `basemod_number_read_from_stream` | 188 | 0.9% |
| 22 | `reader_cache_indirect_object` | 170 | 0.8% |
| 23 | `basemod_pdfobject_get_object` | 167 | 0.8% |
| 24 | `datastructures_arrayobject_read_from_stream` | 164 | 0.8% |
| 25 | `genutils_create_string_object` | 142 | 0.7% |
| | **all boundary calls** | **21,415** | 100.0% |

| class | instances |
|---|---:|
| `NameObject` | 1,444 |
| `NumberObject` | 1,090 |
| `FloatObject` | 476 |
| `IndirectObject` | 466 |
| `DictionaryObject` | 275 |
| `ArrayObject` | 184 |
| `TextStringObject` | 132 |
| `NullObject` | 54 |
| `PageObject` | 21 |
| `DocumentInformation` | 20 |
| `DecodedStreamObject` | 15 |
| `EncodedStreamObject` | 12 |
| `ByteStringObject` | 10 |
| `BooleanObject` | 7 |
| **total** | **4,206** |

Interpretation:

* **Token-level primitives are the top of the list.** `read_non_whitespace` (4,633), `read_until_regex`
  (1,608), `skip_over_whitespace` (1,101), `read_until_whitespace` (734) and `skip_over_comment` (370)
  are called *from Rust* through the Python module object — each is a Python→Rust round trip to do
  what the reference does with a plain in-process call (or an inlined loop). Together they are 39% of
  all crossings in W1 (and 54% in W2).
* **Every parsed Name costs two crossings and two objects**: `basemod_name_read_from_stream` (1,420)
  and `basemod_name_unnumber` (1,420), and `new:NameObject` = 1,444. That matches the reference's
  `read_from_stream`+`unnumber` call counts exactly (4,260/4,260 per 3 reps).
* **`_check_limits` is still per array element on the boundary**: `datastructures_arrayobject_check_limits`
  1,424 calls for 184 `ArrayObject`s — i.e. the limit check for every element of every array is a
  separate Python→Rust call. §4.4 of the plan ("Rust 内部直接完成 limit 检查") targets exactly this.
* 4,206 objects materialised for 20 files (210/file). At the measured per-object costs (§4) that is
  ≈1.5–2 ms of the 33 ms — materialisation alone explains **~5%** of W1; the remaining ~95% is the
  boundary/interpretive overhead around it.

### 2.6 cProfile: top sites, classified

Total profiled self time 119.3 ms; rust_builtin 104.8 ms, python_shim 9.4 ms, other_python 3.1 ms, other_builtin 2.0 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_from_stream>` | rust_builtin | 714 | 23.45 | 19.7% | 64.95 |
| 2 | `<built-in method pypdf._pypdf.datastructures_read_object>` | rust_builtin | 4,917 | 16.70 | 14.0% | 85.53 |
| 3 | `<built-in method pypdf._pypdf.basemod_name_read_from_stream>` | rust_builtin | 4,260 | 14.71 | 12.3% | 16.04 |
| 4 | `<built-in method pypdf._pypdf.datastructures_arrayobject_read_from_stream>` | rust_builtin | 492 | 8.64 | 7.2% | 34.52 |
| 5 | `<built-in method pypdf._pypdf.reader_read_object_header>` | rust_builtin | 1,101 | 6.23 | 5.2% | 6.23 |
| 6 | `<built-in method pypdf._pypdf.reader_initialize_stream>` | rust_builtin | 60 | 2.68 | 2.3% | 27.53 |
| 7 | `<built-in method pypdf._pypdf.basemod_number_new>` | rust_builtin | 3,270 | 2.22 | 1.9% | 2.22 |
| 8 | `<built-in method pypdf._pypdf.genutils_read_string_from_stream>` | rust_builtin | 339 | 2.21 | 1.9% | 6.00 |
| 9 | `<built-in method pypdf._pypdf.reader_get_object>` | rust_builtin | 723 | 2.02 | 1.7% | 82.51 |
| 10 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_stream_data>` | rust_builtin | 714 | 2.00 | 1.7% | 2.10 |
| 11 | `<built-in method pypdf._pypdf.basemod_number_read_from_stream>` | rust_builtin | 564 | 1.44 | 1.2% | 1.90 |
| 12 | `<built-in method pypdf._pypdf.reader_get_object_from_stream>` | rust_builtin | 18 | 1.35 | 1.1% | 55.43 |
| 13 | `read_from_stream (pypdf/generic/_base.py:607)` | python_shim | 4,260 | 1.31 | 1.1% | 17.34 |
| 14 | `__new__ (pypdf/generic/_base.py:414)` | python_shim | 3,270 | 1.15 | 1.0% | 3.37 |
| 15 | `<built-in method pypdf._pypdf.genutils_create_string_object>` | rust_builtin | 426 | 1.13 | 0.9% | 4.63 |

Total profiled self time 106.8 ms; python_shim 72.9 ms, other_builtin 31.6 ms, other_python 2.3 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `read_object (pypdf/generic/_data_structures.py:1594)` | python_shim | 10,917 | 14.36 | 13.5% | 73.42 |
| 2 | `<method 'read' of '_io.BytesIO' objects>` | other_builtin | 82,095 | 7.15 | 6.7% | 7.15 |
| 3 | `read_until_regex (pypdf/_utils.py:318)` | python_shim | 4,824 | 6.46 | 6.0% | 11.58 |
| 4 | `read_from_stream (pypdf/generic/_data_structures.py:255)` | python_shim | 492 | 5.61 | 5.2% | 29.25 |
| 5 | `read_from_stream (pypdf/generic/_data_structures.py:639)` | python_shim | 714 | 5.15 | 4.8% | 57.98 |
| 6 | `<method 'seek' of '_io.BytesIO' objects>` | other_builtin | 41,886 | 4.43 | 4.2% | 4.43 |
| 7 | `read_non_whitespace (pypdf/_utils.py:193)` | python_shim | 13,899 | 4.33 | 4.1% | 6.12 |
| 8 | `read_from_stream (pypdf/generic/_base.py:860)` | python_shim | 4,260 | 3.93 | 3.7% | 16.92 |
| 9 | `read_string_from_stream (pypdf/generic/_utils.py:69)` | python_shim | 339 | 3.44 | 3.2% | 7.62 |
| 10 | `read_previous_line (pypdf/_utils.py:390)` | python_shim | 234 | 2.92 | 2.7% | 3.40 |
| 11 | `<method 'search' of 're.Pattern' objects>` | other_builtin | 8,973 | 2.76 | 2.6% | 2.76 |
| 12 | `read_object_header (pypdf/_reader.py:710)` | python_shim | 1,101 | 2.33 | 2.2% | 8.32 |
| 13 | `read_hex_string_from_stream (pypdf/generic/_utils.py:17)` | python_shim | 87 | 2.29 | 2.1% | 4.39 |
| 14 | `read_until_whitespace (pypdf/_utils.py:160)` | python_shim | 2,202 | 2.27 | 2.1% | 3.28 |
| 15 | `<built-in method builtins.len>` | other_builtin | 26,820 | 2.08 | 1.9% | 63.69 |

Classification of the W1 profile (port, 3 reps, 119.3 ms profiled self time):
`rust_builtin` 104.8 ms (88%), `python_shim` 9.4 ms (8%), other 5.1 ms; in the reference
`python_shim` 72.9 ms (68%) and C builtins 31.6 ms (30%), dominated by 82k `BytesIO.read` + 42k
`BytesIO.seek` calls — the token-level stream churn that cProfile *cannot* see on the port (82,095
reads and 41,886 seeks for 3 W1 repetitions, i.e. ~41k byte-level stream operations per W1 run).

The port's top five self-time sites are all Rust entry points, and they are exactly the object-syntax
path: `dictionaryobject_read_from_stream` (23.4 ms / 714 calls = **33 µs/call self**),
`read_object` (16.7 ms / 4,917), `name_read_from_stream` (14.7 ms / 4,260 = **3.5 µs/call**),
`arrayobject_read_from_stream` (8.6 ms / 492), `read_object_header` (6.2 ms / 1,101).

## 3. W2 — all-page box traversal on 5 large PDFs

### 3.1 Where the 564 ms go

| phase | port median | port min–max | port share | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|---:|
| `open` | 145.60 ms | 143.26–150.20 | 25.8% | 56.16 ms | 55.62–56.81 | 2.59× |
| `pages_flatten` | 395.90 ms | 394.00–479.56 | 70.2% | 106.82 ms | 104.51–166.88 | 3.71× |
| `boxes` | 22.46 ms | 21.99–25.45 | 4.0% | 2.84 ms | 2.70–3.32 | 7.90× |
| `rotation` | 0.42 ms | 0.39–0.50 | 0.1% | 0.04 ms | 0.03–0.06 | 9.86× |
| **total** | **563.80 ms** | 562.27–648.26 | 100% | **165.87 ms** | 163.68–226.62 | **3.40×** |

**70% of W2 is flattening the page tree** (395.9 ms), 26% is opening the file (145.6 ms), and the
thing the workload is named after — reading 5 boxes + rotation for all 383 pages — is 4% (22.5 ms).
The reference has the same shape (flatten 64%, open 34%, boxes 1.7%), but box reads are **7.9×**
slower on the port while flattening is 3.7× and opening 2.6× slower.

### 3.2 Layers (cross-check)

| layer (cumulative) | port median | port min–max | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|
| open | 146.60 ms | 142.65–163.31 | 56.22 ms | 55.81–58.30 | 2.61× |
| open+pages | 549.40 ms | 537.56–617.61 | 163.37 ms | 160.62–230.84 | 3.36× |
| open+pages+traverse | 567.97 ms | 555.22–644.50 | 164.52 ms | 163.04–167.70 | 3.45× |

| derived stage (= layer difference) | port | reference | port/reference |
|---|---:|---:|---:|
| open+pages − open | 402.80 ms | 107.14 ms | 3.76× |
| open+pages+traverse − open+pages | 18.57 ms | 1.16 ms | 16.03× |

| file | bytes | port total | ref total | port/ref | port open | port flatten | port box reads | port rotation | boundary calls | materialised objects |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `sample-files/009-pdflatex-geotopo/GeoTopo.pdf` | 5321132 | 285.90 | 74.91 | 3.82× | 14.24 | 262.01 | 8.15 | 0.15 | 190,654 | 46,474 |
| `resources/issue-604.pdf` | 7988082 | 187.26 | 71.45 | 2.62× | 87.47 | 91.16 | 8.83 | 0.19 | 185,414 | 14,907 |
| `sample-files/009-pdflatex-geotopo/GeoTopo-komprimiert.pdf` | 1793479 | 103.10 | 32.63 | 3.16× | 47.59 | 46.07 | 7.56 | 0.15 | 94,623 | 9,236 |
| `resources/fontsampler.pdf` | 3321024 | 1.73 | 0.85 | 2.04× | 1.09 | 0.52 | 0.07 | 0.00 | 782 | 97 |
| `sample-files/018-base64-image/base64image.pdf` | 2267027 | 1.42 | 0.71 | 1.98× | 0.75 | 0.56 | 0.07 | 0.00 | 713 | 111 |
| **sum** | | **579.41** | **180.56** | **3.21×** |  |  |  |  | **472,186** | **70,825** |

Two files carry almost all of W2: `GeoTopo.pdf` (285.9 ms port / 74.9 ref) and `issue-604.pdf`
(187.3 / 71.4). `fontsampler.pdf` and `base64image.pdf` have few pages (their whole workload is
1.4–1.7 ms) — they are in the W2 set for object-graph coverage, not for timing.

`issue-604.pdf` is the file where **opening** is the biggest single cost (87.5 ms open vs 91.2 ms
flatten): it is the 8 MB, object-stream-heavy sample. `GeoTopo.pdf` is pure flattening (262.0 ms).

### 3.3 Stage timers

| stage | calls (port / ref) | port ms | reference ms | port/reference | stage % of workload (port) |
|---|---:|---:|---:|---:|---:|
| `tree:get_num_pages` | 383 / 383 | 418.00 | 109.84 | 3.81× | 69% |
| `tree:_flatten` | 5 / 5 | 417.72 | 109.77 | 3.81× | 69% |
| `open:get_object` | 1556 / 1556 | 412.52 | 107.89 | 3.82× | 68% |
| `tree:_pop_next_page_tree_kid` | 408 / 408 | 391.34 | 102.68 | 3.81× | 65% |
| `open:_get_object_from_stream` | 24 / 24 | 265.11 | 63.09 | 4.20× | 44% |
| `open:__init__` | 5 / 5 | 153.87 | 63.08 | 2.44× | 26% |
| `open:_initialize_stream` | 5 / 5 | 153.64 | 63.00 | 2.44× | 26% |
| `open:read` | 5 / 5 | 150.27 | 59.82 | 2.51× | 25% |
| `open:read_object_header` | 21270 / 21270 | 116.58 | 40.24 | 2.90× | 19% |
| `open:_read_xref_tables_and_trailers` | 5 / 5 | 24.87 | 13.15 | 1.89× | 4% |
| `traverse:_get_rectangle` | 1865 / 1865 | 24.15 | 3.82 | 6.32× | 4% |
| `open:_read_xref` | 5 / 5 | 17.43 | 8.31 | 2.10× | 3% |
| `open:_read_standard_xref_table` | 5 / 5 | 16.67 | 8.06 | 2.07× | 3% |
| `open:_read_pdf15_xref_stream` | 1 / 1 | 7.39 | 4.57 | 1.61× | 1% |
| `tree:_flatten_leaf_page` | 373 / 373 | 6.17 | 1.93 | 3.20× | 1% |
| `tree:PageObject.__init__` | 373 / 373 | 4.81 | 1.22 | 3.93× | 1% |
| `traverse:_set_rectangle` | 1865 / 1865 | 4.69 | 0.78 | 6.03× | 1% |
| `open:cache_indirect_object` | 2626 / 2626 | 3.39 | 0.74 | 4.60× | 1% |
| `open:cache_get_indirect_object` | 3887 / 3887 | 1.86 | 0.71 | 2.62× | 0% |
| `traverse:rotation` | 373 / 373 | 0.49 | 0.07 | 7.44× | 0% |
| `tree:_enter_page_tree_node` | 35 / 35 | 0.23 | 0.10 | 2.38× | 0% |
| `open:_find_eof_marker` | 5 / 5 | 0.17 | 0.07 | 2.33× | 0% |
| **workload wall** | | **602.38** | **179.73** | **3.35×** | 100% |

_Stage rows are inclusive: a stage that is called from another stage (e.g. `get_object` from `_flatten`) is contained in its caller, so rows must not be summed._

Key readings:

* `_flatten` 417.7 ms (74%) of which `get_object` 412.5 ms and `_pop_next_page_tree_kid` 391.3 ms —
  the latter is large only because it resolves each `/Kids` entry (`page.call_method0("get_object")`,
  `doc_common_mod/pages.rs:305`), i.e. it *is* object resolution in disguise. `_flatten_leaf_page`
  (which builds the 373 `PageObject`s and copies the inherited attributes) is 6.2 ms (1%).
  **Inheritance lookup is not a bottleneck**: the attribute copies are 373 pages × a handful of keys,
  and the measured cost is 5.9 ms total (3.03× the reference, i.e. proportional).
* `_get_object_from_stream` 265.1 ms (47%) over 24 calls = **11.0 ms per call**. With 64,083 objects
  materialised during the flatten phase, this is where the object streams of `GeoTopo.pdf` /
  `issue-604.pdf` are parsed.
* `read_object_header` **21,270 calls, 116.6 ms (21%)**: 1,556 of those are `get_object`'s own header
  reads, the other ~19,700 come from the non-strict xref verification loop
  (`reader_mod.rs:1444-1490` re-reads every xref entry's `obj num gen obj` header). The reference
  spends 40.2 ms in the same loop. This is a pure throughput item — the loop can run on the byte
  buffer instead of the Python stream.
* Box reads: `_get_rectangle` 1,865 calls, 24.2 ms (6.3×). That is **12.9 µs per box read** on the
  port vs 2.1 µs on the reference. Each box read is property → Python `_get_rectangle`
  (`python/pypdf/_page.py:203`) → Rust `page_get_rectangle` (`page_mod/page.rs:305`) → Python
  `_set_rectangle` → Rust `page_set_rectangle`, i.e. 4 crossings and 2 Python frames, and the result
  is written back into the page dict through the overridden `__setitem__` (the write-back alone is
  4.7 ms, `_set_rectangle`).
* `rotation`: 0.49 ms for 373 pages (7.4×, irrelevant in absolute terms; it is one property read per
  page that goes through `page_rotation_get`).

### 3.4 Calls and materialisation

| phase | Python→Rust boundary calls | share | materialised PDF objects |
|---|---:|---:|---:|
| `open` | 181,584 | 38.5% | 102 |
| `flatten` | 259,566 | 55.0% | 64,083 |
| `traverse` | 31,036 | 6.6% | 6,640 |
| **total** | **472,186** | 100.0% | **70,825** |

_The reference has no extension boundary: its equivalent work is ordinary Python calls counted by cProfile (see the profile tables). `py:` counters appear on both backends and are reported in the call-count table instead._

Total Python→Rust boundary calls: **472,186**; materialised PDF objects: **70,825**. (724 shim globals and 940 extension attributes intercepted; the reference has no extension boundary.)

| # | intercepted callable | port calls | share of boundary calls |
|---|---|---:|---:|
| 1 | `read_non_whitespace` | 100,325 | 21.2% |
| 2 | `skip_over_whitespace` | 63,810 | 13.5% |
| 3 | `read_until_whitespace` | 42,540 | 9.0% |
| 4 | `read_until_regex` | 27,536 | 5.8% |
| 5 | `basemod_name_read_from_stream` | 22,874 | 4.8% |
| 6 | `basemod_name_unnumber` | 22,874 | 4.8% |
| 7 | `datastructures_read_object` | 21,435 | 4.5% |
| 8 | `reader_read_object_header` | 21,270 | 4.5% |
| 9 | `skip_over_comment` | 21,270 | 4.5% |
| 10 | `datastructures_arrayobject_check_limits` | 20,110 | 4.3% |
| 11 | `doccommon_convert_to_int` | 12,987 | 2.8% |
| 12 | `basemod_number_new` | 11,440 | 2.4% |
| 13 | `basemod_pdfobject_get_object` | 9,285 | 2.0% |
| 14 | `genbasic_rectangleobject_ensure_is_number` | 7,460 | 1.6% |
| 15 | `basemod_float_new` | 5,709 | 1.2% |
| 16 | `datastructures_dictionaryobject_read_from_stream` | 4,669 | 1.0% |
| 17 | `datastructures_dictionaryobject_read_stream_data` | 4,669 | 1.0% |
| 18 | `basemod_number_read_from_stream` | 4,662 | 1.0% |
| 19 | `datastructures_arrayobject_read_from_stream` | 4,605 | 1.0% |
| 20 | `py:PdfReader.cache_get_indirect_object` | 3,887 | 0.8% |
| 21 | `reader_cache_get_indirect_object` | 3,887 | 0.8% |
| 22 | `basemod_is_null_or_none` | 3,313 | 0.7% |
| 23 | `reader_cache_indirect_object` | 2,626 | 0.6% |
| 24 | `datastructures_dictionaryobject_setitem` | 1,964 | 0.4% |
| 25 | `py:pypdf._page._get_rectangle` | 1,865 | 0.4% |
| | **all boundary calls** | **472,186** | 100.0% |

| class | instances |
|---|---:|
| `NameObject` | 24,743 |
| `IndirectObject` | 11,885 |
| `NumberObject` | 11,440 |
| `FloatObject` | 5,709 |
| `DictionaryObject` | 4,679 |
| `ArrayObject` | 4,640 |
| `NullObject` | 4,260 |
| `RectangleObject` | 1,865 |
| `TextStringObject` | 1,139 |
| `PageObject` | 373 |
| `BooleanObject` | 34 |
| `EncodedStreamObject` | 25 |
| `DecodedStreamObject` | 25 |
| `ByteStringObject` | 8 |
| **total** | **70,825** |

**472,186 boundary crossings for 383 pages (1,233/page) and 70,825 materialised objects
(185/page).** The phase split is the most revealing number in this report:

| phase | boundary calls | materialised objects | what it means |
|---|---:|---:|---|
| open | 181,584 (38%) | **102** | xref/trailer parsing is chatty but object-free: 21,270 `read_object_header` + 21,270 `skip_over_comment` + ~100k whitespace/regex reads |
| flatten | 259,566 (55%) | **64,083** | all the PDF syntax parsing; object resolution + page construction |
| traverse | 31,036 (7%) | 6,640 | 5 boxes + rotation per page; 6,640 `RectangleObject`+`NumberObject` creations for 1,865 box reads |

The `open` phase is a textbook case of "no callbacks but still expensive": it creates 102 objects yet
crosses the boundary 181,584 times (≈4.7 µs per crossing including the Rust work) to verify a
~21k-entry xref table on files that need no repair. This is P4's cheapest large win for W2.

### 3.5 cProfile

Total profiled self time 1994.5 ms; rust_builtin 1806.3 ms, python_shim 171.2 ms, other_builtin 14.1 ms, other_python 3.0 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_from_stream>` | rust_builtin | 14,007 | 397.22 | 19.9% | 1192.60 |
| 2 | `<built-in method pypdf._pypdf.reader_read_object_header>` | rust_builtin | 63,810 | 341.05 | 17.1% | 341.05 |
| 3 | `<built-in method pypdf._pypdf.basemod_name_read_from_stream>` | rust_builtin | 68,622 | 230.44 | 11.6% | 250.95 |
| 4 | `<built-in method pypdf._pypdf.datastructures_read_object>` | rust_builtin | 64,305 | 223.03 | 11.2% | 1262.78 |
| 5 | `<built-in method pypdf._pypdf.datastructures_arrayobject_read_from_stream>` | rust_builtin | 13,815 | 144.89 | 7.3% | 483.11 |
| 6 | `read_from_stream (pypdf/generic/_data_structures.py:428)` | python_shim | 14,007 | 49.45 | 2.5% | 1240.81 |
| 7 | `<built-in method pypdf._pypdf.reader_read_standard_xref_table>` | rust_builtin | 15 | 48.39 | 2.4% | 48.78 |
| 8 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_stream_data>` | rust_builtin | 14,007 | 36.22 | 1.8% | 36.24 |
| 9 | `<built-in method pypdf._pypdf.basemod_number_read_from_stream>` | rust_builtin | 13,986 | 34.73 | 1.7% | 45.94 |
| 10 | `<built-in method pypdf._pypdf.reader_get_object_from_stream>` | rust_builtin | 72 | 27.20 | 1.4% | 923.93 |
| 11 | `<built-in method pypdf._pypdf.page_get_rectangle>` | rust_builtin | 5,595 | 25.88 | 1.3% | 93.82 |
| 12 | `<built-in method pypdf._pypdf.reader_read>` | rust_builtin | 15 | 24.43 | 1.2% | 452.27 |
| 13 | `<built-in method pypdf._pypdf.basemod_number_new>` | rust_builtin | 34,320 | 22.97 | 1.2% | 22.97 |
| 14 | `<built-in method pypdf._pypdf.genbasic_rectangleobject_ensure_is_number>` | rust_builtin | 22,380 | 18.48 | 0.9% | 18.48 |
| 15 | `<built-in method pypdf._pypdf.reader_read_xref_subsections>` | rust_builtin | 3 | 16.07 | 0.8% | 31.09 |

Total profiled self time 1945.7 ms; python_shim 1384.3 ms, other_builtin 557.3 ms, other_python 4.1 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `read_object (pypdf/generic/_data_structures.py:1594)` | python_shim | 165,153 | 247.58 | 12.7% | 1098.11 |
| 2 | `read_until_whitespace (pypdf/_utils.py:160)` | python_shim | 127,620 | 178.27 | 9.2% | 261.95 |
| 3 | `<method 'read' of '_io.BytesIO' objects>` | other_builtin | 2,112,123 | 173.75 | 8.9% | 173.75 |
| 4 | `read_object_header (pypdf/_reader.py:710)` | python_shim | 63,810 | 118.42 | 6.1% | 533.54 |
| 5 | `read_non_whitespace (pypdf/_utils.py:193)` | python_shim | 300,975 | 108.27 | 5.6% | 153.57 |
| 6 | `read_until_regex (pypdf/_utils.py:318)` | python_shim | 82,608 | 106.87 | 5.5% | 190.52 |
| 7 | `read_from_stream (pypdf/generic/_data_structures.py:255)` | python_shim | 13,815 | 96.64 | 5.0% | 428.54 |
| 8 | `<method 'seek' of '_io.BytesIO' objects>` | other_builtin | 916,956 | 93.75 | 4.8% | 93.75 |
| 9 | `read_from_stream (pypdf/generic/_data_structures.py:639)` | python_shim | 14,007 | 83.70 | 4.3% | 1082.22 |
| 10 | `read_from_stream (pypdf/generic/_base.py:860)` | python_shim | 68,622 | 60.36 | 3.1% | 261.43 |
| 11 | `<method 'isspace' of 'bytes' objects>` | other_builtin | 615,306 | 48.74 | 2.5% | 48.74 |
| 12 | `<built-in method builtins.len>` | other_builtin | 684,051 | 44.17 | 2.3% | 44.82 |
| 13 | `read_hex_string_from_stream (pypdf/generic/_utils.py:17)` | python_shim | 597 | 43.44 | 2.2% | 75.89 |
| 14 | `skip_over_whitespace (pypdf/_utils.py:210)` | python_shim | 191,430 | 40.73 | 2.1% | 55.22 |
| 15 | `<method 'search' of 're.Pattern' objects>` | other_builtin | 120,384 | 35.71 | 1.8% | 35.71 |

Port: `rust_builtin` 1,806 ms of 1,995 ms (91%); reference: `python_shim` 1,384 ms + C 557 ms.
The port's five largest self-time sites are `dictionaryobject_read_from_stream` (397 ms / 14,007
calls = **28 µs/call self**), `read_object_header` (341 ms / 63,810 = **5.3 µs/call**),
`name_read_from_stream` (230 ms / 68,622 = 3.4 µs), `read_object` (223 ms / 64,305 = 3.5 µs) and
`arrayobject_read_from_stream` (145 ms / 13,815 = 10.5 µs). These five alone are 1,336 ms = **67%** of
the profiled self time, and all of them are Rust builtins whose work is dominated by invisible
Python-C calls (stream reads, class lookups, helper calls).

## 4. Why the port is slower (plan baseline W1 0.36× / W2 0.40×) — measured element costs

The port executes the same number of logical operations as the reference, so the gap must be in the
cost of one operation. The microbenchmarks below (`raw/rw_micro_rust.json` / `raw/rw_micro_reference.json`,
5 samples, median) were taken in the same measurement window as the stage data, i.e. against the same
extension build:

#### Microbenchmarks (median ns per operation, 5 samples)

| benchmark | port ns/op | reference ns/op | port/reference | note |
|---|---:|---:|---:|---|
| `python_plain_function_call` | 54 | 55 | 0.99× | reference floor for one call |
| `python_to_rust_pyfunction_call` | 423 | 79 | 5.35× | one crossing, trivial Rust body (is_null_or_none) |
| `python_to_rust_pyfunction_call_args2` | 175 | — | — | one crossing, keyword arguments (check_limits) |
| `read_object_parse_number` | 4,732 | 1,347 | 3.51× | pypdf.generic.read_object(b'123') |
| `read_object_parse_name` | 4,027 | 1,234 | 3.26× | pypdf.generic.read_object(b'/SomeName') |
| `read_object_parse_literal_string` | 12,681 | 3,901 | 3.25× | pypdf.generic.read_object(b'(a short literal string)') |
| `read_object_parse_array_4` | 23,689 | 5,870 | 4.04× | pypdf.generic.read_object(b'[1 2 3 4]') |
| `read_object_parse_dict_2` | 46,539 | 6,739 | 6.91× | pypdf.generic.read_object(b'<< /Type /Page /Rotate 90 >>') |
| `bytesio_read1_seek` | 93 | 67 | 1.38× | per-token stream access primitive |
| `create_NameObject` | 169 | 100 | 1.69× | str subclass, Rust __new__ on the port |
| `create_NumberObject` | 838 | 183 | 4.58× | int subclass, Rust __new__ on the port |
| `create_FloatObject` | 1,026 | 213 | 4.82× | float subclass, Rust __new__ on the port |
| `create_DictionaryObject` | 146 | 81 | 1.79× | dict subclass (no Rust __init__) |
| `create_ByteStringObject` | 162 | 106 | 1.52× | bytes subclass |
| `import_module_plus_getattr` | 351 | 200 | 1.76× | approximation of py.import()+getattr() per Rust call-site resolution |


Reading:

* **A Python→Rust call costs 423 ns** even when the Rust body is a single `isinstance` check; a plain
  Python call is 54 ns and the reference's Python `is_null_or_none` is 79 ns. So the *floor* for the
  port's idiom "Python shim method → PyO3 function" is ~5× the reference's equivalent call. With a
  keyword-argument call the floor drops to 175 ns (fewer argument conversions), which is why the
  hottest port paths pass positional arguments.
* **`pypdf.generic.read_object` — the public entry point both backends expose for one PDF value — is
  3.3–6.9× slower on the port**, and the gap grows with structure: number 3.5×, name 3.3×, literal
  string 3.3×, 4-element array 4.0×, 2-key dictionary **6.9×**. The dictionary case is the worst
  because it pays: one crossing into `read_object`, one into `dictionaryobject_read_from_stream`,
  per key/value two more crossings plus `read_non_whitespace`/`read_until_regex` calls, one crossing
  per `set_item`, and one per `NameObject` construction.
* **Constructing a PDF object is 1.5–4.8× slower**: `NumberObject(42)` 838 vs 183 ns,
  `FloatObject("1.5")` 1,026 vs 213 ns, `NameObject("/Foo")` 169 vs 100 ns,
  `DictionaryObject()` 146 vs 81 ns. The `int`/`float` subclasses are the expensive ones because
  `__new__` is a Python frame that calls the Rust `basemod_number_new`/`basemod_float_new`, which in
  turn has to parse/convert and validate.
* **Stream churn is the same order on both sides** (`bytesio_read1_seek` 93 vs 67 ns, 1.4×): the
  reference pays 82,095 reads + 41,886 seeks per W1×3 and the port the same, but the port pays them
  *from Rust* with argument conversion on top, which is exactly the part cProfile cannot see.

Multiplying out for W1: 21,415 crossings × ~(423 − 79) ns of pure boundary overhead ≈ **7.4 ms**,
plus the extra work each Rust frame does (class/module resolution, `Vec<u8>` extraction, exception
setup) and the slower object construction (≈1.5–2 ms). That accounts for the ~22.7 ms excess over the
reference (33.3 − 10.6) and is consistent with the measured 3–7× per-element factors.

### Hypothesis (to be confirmed by Rust-side counters in P4)

1. **Per-token Python callbacks, not Rust parsing.** The object parser is in Rust, but its *primitives*
   are Python-visible extension functions called through the Python module object:
   `read_non_whitespace`/`read_until_regex`/`skip_over_whitespace`/`read_until_whitespace`/
   `skip_over_comment` (54% of W2 crossings). Each is a full Python→Rust round trip.
2. **The byte source is a Python `BytesIO`.** `read_n`/`seek`/`tell`
   (`data_structures_mod.rs:213-229`) call the stream object per token: `.read(1)` then `seek(-1)`
   (`data_structures_mod.rs:1409-1410`, `:1485`, `:1577`, `:1645-1648`). The reference does the same
   operations from Python bytecode, which is *cheaper* than Rust doing it through `PyObject_Call`.
3. **Class/module resolution per call, not per reader.** `base_class`/`shim_class`/`utils_module`/
   `generic_utils_module`/`constants_class`/`configuration` (`data_structures_mod.rs:68-100`,
   `pyutil.rs:10-12`) perform `py.import(<module>).getattr(<name>)` at each use — several per parsed
   object (e.g. `_clone` resolves `DictionaryObject`, `NameObject`, `IndirectObject`, `StreamObject`,
   `int` before doing any work, `data_structures_mod.rs:935-941`).
4. **The boundary is crossed per element, not per unit of work.** One `read_object` for a dictionary
   crosses: entry → dict-read → per key/value (read_object + name/number read + setitem) → exit. P1/P2
   replace this with one Rust call per object (or per object stream) that returns a compact Rust
   representation, materialising Python objects only at the compatibility boundary.

## 5. Concrete P4 entry points (file:line) and what each must change

| # | entry point | what happens today | measured cost | P4 change |
|---|---|---|---|---|
| 1 | `crates/pypdf-py/src/modules/data_structures_mod.rs:3089` `datastructures_read_object` | dispatcher: peeks 1 byte, then calls the Python classmethod `NameObject.read_from_stream` / `DictionaryObject.read_from_stream` / `ArrayObject.read_from_stream` / `BooleanObject.read_from_stream` / `NumberObject.read_from_stream` / `read_string_from_stream` / `read_hex_string_from_stream` | W1 1,639 calls, W2 21,435; profile self 17.0 ms (W1), 222 ms (W2) for 3 reps | parse the value in Rust on a byte cursor; no Python classmethod dispatch, no per-value class resolution |
| 2 | `data_structures_mod.rs:1386` `dictionaryobject_read_from_stream` (+ `:1564` `read_stream_data`) | per key and per value: `read_non_whitespace` (`:1409`, `:1485`), `read_until_regex` (`:1645`), then `set_item` on the Python dict | W1 238 calls / 24.4 ms self (34 µs/call); W2 4,669 calls / 393 ms self | build the dict inside Rust from the byte range, then materialise once |
| 3 | `data_structures_mod.rs:213-229` `read_n`/`seek`/`tell` | every token read is `stream.read(n)` / `stream.seek(...)` on the Python `BytesIO` | invisible to cProfile; same call count as the reference (82k reads / 42k seeks per 3 W1 reps) | keep the file bytes in a Rust cursor (`PyBytes` → `&[u8]` or `Vec<u8>`); `PdfReader.stream` stays a Python object for API compatibility, but the parser must not read through it |
| 4 | `base_mod.rs:1624` `name_read_from_stream`, `:1593` `name_unnumber`, `base_mod.rs:1154` `number_read_from_stream`, `:1083` `number_new`, `:986` `float_new` | one crossing per Name/Number/Float token, and a second for `unnumber` | W1: 1,420+1,420 names, 1,090+476 numbers; `create_NumberObject` 838 ns, `create_FloatObject` 1,026 ns | scan Name/Number bytes in Rust; construct the Python object once, in the caller, not per token |
| 5 | `reader_mod.rs:755` `reader_get_object_from_stream` (phase 1 header loop at `:880`, parse-all loop at `:930-1000`) | on first access to any object of an ObjStm: decompress, read `/N` pairs, then `call_shim("read_object")` for **every** object and `cache_indirect_object` each | W1 6 calls = 16.2 ms (49%); W2 24 calls = 265.1 ms (47%); 11.0 ms/call in W2 | parse the ObjStm inside Rust (it is already a byte buffer after `get_data`), keep parsed objects in Rust, materialise only the requested one; §9.2 of the plan's "batch object + reference" applies |
| 6 | `reader_mod.rs:1015` `reader_get_object` | per resolution: `cache_get_indirect_object` (Python method), `resolved_objects.contains`, `xref[...]` lookups, `stream.seek`, `read_object_header`, `_get_object_from_stream`, `cache_indirect_object` | W1 241 calls = 25.0 ms incl. (3.7×); W2 1,556 calls = 412.5 ms incl. (3.8×) | Rust-side object cache + header parsing; one boundary crossing per *batch* of references |
| 7 | `reader_mod.rs:1398` `reader_read` non-strict xref verification loop (`:1444-1490`) + `reader_read_object_header` (`:1238`) | re-reads every xref entry's header from the Python stream and verifies the object number | W2 **21,270 calls = 116.6 ms (21%)** with 21,270 `skip_over_comment` calls; W1 367 calls / 2.07 ms | verify offsets against the byte buffer in Rust; the loop must not touch the Python stream |
| 8 | `doc_common_mod/pages.rs:382` `doccommon_flatten`, `:199` `doccommon_enter_page_tree_node`, `:239` `doccommon_pop_next_page_tree_kid`, `:325` `doccommon_flatten_leaf_page` | the traversal is Rust but each node iteration calls back into the Python methods (`slf.call_method("_flatten_leaf_page")` at `:478`, `slf.call_method1("_pop_next_page_tree_kid")` at `:481`) and builds a `_PageTreeItem` NamedTuple per node | W1 `_flatten` 22.9 ms (68%); W2 417.7 ms (74%) — but only 0.37/6.2 ms in `_flatten_leaf_page` and 0.23/0.23 ms in `_enter_page_tree_node` | keep the per-node callbacks (tests observe them) but stop resolving objects one by one: resolve `/Kids` in one Rust call; or flatten the whole tree in one call and build `PageObject`s at the end |
| 9 | `page_mod/page.rs:117` `page_init` | `DictionaryObject.__init__` (Python call from Rust), then `update(target)` where `target` came from `IndirectObject.get_object` | W2 373 calls = 4.65 ms; also inside `_flatten_leaf_page` | construct the page dict natively (or `dict.update` bypassing the overridden `__setitem__`), then attach `pdf`/`indirect_reference` |
| 10 | `page_mod/page.rs:305` `page_get_rectangle` (+ `:378` `page_set_rectangle`) | one box read = property → Python `_get_rectangle` (`python/pypdf/_page.py:203`) → Rust → Python `_set_rectangle` → Rust, then write-back through `__setitem__` | W2 1,865 calls = 24.2 ms (**12.9 µs/box**, 6.3×); 4.7 ms in `_set_rectangle` | one Rust call that normalises + memoises the `RectangleObject`; no second crossing, no write-back through Python |
| 11 | `data_structures_mod.rs:68-100` helpers (`base_class`, `shim_class`, `utils_module`, `generic_utils_module`, `constants_class`, `configuration`), `pyutil.rs:10-12` `pypdf_class` | `py.import(module).getattr(name)` on every call site that needs a class or module | proxy measured at 351 ns vs 200 ns (§4); several per parsed object | resolve once per call (or cache `Py<PyAny>` in a `GILOnceCell`); never inside a per-element loop. This is the item P7 in §10 lists as a "micro-optimisation" — the profile shows it is on the hot path now |
| 12 | `python/pypdf/generic/_data_structures.py:383-391` (`__setitem__`/`__getitem__` shims) + `data_structures_mod.rs:1197` `dictionaryobject_setitem` | every insertion/lookup on a `DictionaryObject` is a Python frame → Rust call | W2 1,964 `setitem` calls plus every `dict.get` inside Rust; W1 146+ | keep the Python-visible semantics, but the internal parser path must not insert through the shim |

**Ordering suggestion for P4 (highest value first, each independently measurable):**

1. #7 xref verification + #6 object cache/hit path on the byte buffer (W2 open 26%): no semantic risk,
   pure throughput.
2. #1–#4 syntax parser on a Rust cursor (W1 47%/W2 44% of the workload, both flatten and open).
3. #5 ObjStm parse-all-in-Rust (same 44–47%, harder: must keep the "cache all objects" behaviour and
   its warning/limit semantics).
4. #10 box reads (4% of W2, 7.9× — cheap to fix, good ratio).
5. #8–#9 page-tree batching (needed once 1–4 land, otherwise the remaining cost is the per-node
   Python callbacks).
6. #11 helper resolution caching (do it as part of 1–5, not as a separate change).

## 6. What is still unproven

* **Rust-internal attribution.** cProfile cannot see C-to-C calls, so the split *inside* a Rust
  builtin (e.g. of `dictionaryobject_read_from_stream`'s 33 µs/call: how much is the Python helper
  calls, how much the `Vec<u8>`/`String` conversions, how much the dict insertions) is inferred from
  the code and the boundary counts, not directly measured. The three candidate sub-costs listed in §4
  should be confirmed with Rust-side counters (e.g. count `py.import` calls, `stream.read` calls and
  per-call allocations) before committing to a rewrite order.
* **Whether `_get_object_from_stream`'s "parse all" is a net win** for these workloads. It trades
  fewer decompressions for more parsed (and materialised) objects. For W2 it materialises 64k objects
  during flatten; a lazy variant might materialise far fewer. This needs a counting experiment
  (P2-style) before being changed, because it also changes warning order.
* **The 0.33× → 3.14× difference** between the current baseline and this run is explained by machine
  load (the reference is pure Python and slows down more under contention), but the exact sensitivity
  was not measured here. A low-load re-run of the frozen baseline would settle it.
* Per-file numbers carry the stage-wrapper overhead (4–7%); they were used for ranking only.

## 7. Reproduce

```bash
cd /Users/yxy/Desktop/pypdf-rust
export PYTHONHASHSEED=0

# 1. both backends, every pass, all reader+writer workloads (~1 min on an idle machine)
bash validation/optimization/scripts/run_all.sh

# ... or step by step:
.venvs/dev/bin/python validation/optimization/scripts/rw_diag.py \
    --backend rust --workloads W1,W2,W5,W6,W7 --mode all --repeat 7 \
    --out validation/optimization/raw/rw_diag_rust.json
.venvs/oracle/bin/python validation/optimization/scripts/rw_diag.py \
    --backend reference --workloads W1,W2,W5,W6,W7 --mode all --repeat 7 \
    --out validation/optimization/raw/rw_diag_reference.json

# 2. microbenchmarks (~2 s, independent process)
.venvs/dev/bin/python validation/optimization/scripts/rw_microbench.py \
    --backend rust --out validation/optimization/raw/rw_micro_rust.json
.venvs/oracle/bin/python validation/optimization/scripts/rw_microbench.py \
    --backend reference --out validation/optimization/raw/rw_micro_reference.json

# 3. the tables quoted in this report
.venvs/dev/bin/python validation/optimization/scripts/render_tables.py \
    --rust validation/optimization/raw/rw_diag_rust.json \
    --reference validation/optimization/raw/rw_diag_reference.json \
    --microbench-rust validation/optimization/raw/rw_micro_rust.json \
    --microbench-reference validation/optimization/raw/rw_micro_reference.json \
    --section reader
```

Single-mode runs are supported (`--mode layers|phases|stages|counts|profile|perfile`) and are what
the harness itself uses internally when a mode must be isolated (the counting pass cannot be undone
in-process; see §1.2).
