# W5 / W6 / W7 — Writer diagnostics (Rust port vs reference pypdf 7.1.1)

> Scope: OPT-010 (P0 attribution) and P6 (§9 "Merge, page operations, Clone and Writer") of
> `pypdf-rust-optimization-plan.md`, for workloads **W5** (`merge_append_5`), **W6**
> (`rotate_crop_merge_write_5`) and **W7** (`clone_write_10`).
>
> Same measurement harness and caveats as `diag_reader.md` §1 (read that first for the three
> instrumentation passes, the cProfile blindness to Rust→C calls and the CPython-3.14
> `__new__`-patching limitation). This file adds the writer-specific stages and the object-graph /
> serialization split.
>
> Raw evidence: `raw/rw_diag_rust.json`, `raw/rw_diag_reference.json` (identical shared inputs,
> byte-identical outputs — `out_sha256` matches on all three workloads).
>
> **Provenance caveat:** the numbers were measured with the extension built from commit `f4d3c45`
> (sha256 `f4678f30a158b4e7…`); the tree has since advanced to `50daf7a` (P1 content-stream scanner)
> with a rebuilt extension, so W5–W7 must be re-baselined after P1 integration. All (file:line)
> references were re-verified against `50daf7a`.

## 0. Headline

| workload | port | reference | port/reference | build (copy) | serialization (write) |
|---|---:|---:|---:|---:|---:|
| W5 merge/append, 17 pages | **40.6 ms** | **11.4 ms** | **3.55×** | `append` 35.0 ms (86%) | `write` 5.6 ms (14%) |
| W6 rotate/crop + write, 8 pages | **53.3 ms** | **15.4 ms** | **3.47×** | readers 3.2 + `add_page` 39.9 ms | `write` 9.8 ms (18%) |
| W7 clone + write, 22 pages | **70.1 ms** | **19.4 ms** | **3.61×** | readers 4.5 + `add_page` 52.5 ms | `write` 13.1 ms (19%) |

Five facts that decide the P6 design:

1. **The cost is object-graph copy, not serialization.** Copying/cloning is 86% (W5),
   81% (W6) and 81% (W7) of the workload; writing the PDF is
   13.8–18.7% and is 2.5–2.9× slower than the reference, versus
   3.6–3.8× for the copy phase. Fixing the writer alone cannot bring W5–W7 to parity.
2. **Every cloned object pays several crossings.** W5 performs 1,999 clone entry-point calls and
   2,821 `write_to_stream` calls for a 137-object output; W7 performs 3,941 /
   4,945 for 198 objects. Cloning one dictionary entry is: `items()` → `contains` →
   `value.clone()` (Python→Rust) → `NameObject(key)` creation → `dict.__setitem__` (Python→Rust) —
   i.e. 4–6 boundary crossings per entry (`data_structures_mod.rs:922-1100`).
3. **The serialization path writes in tiny fragments.** `DictionaryObject.write_to_stream` does 4 Python
   `stream.write` calls plus 2 Python→Rust method calls per dictionary entry
   (`data_structures_mod.rs:1277-1305`), and `_write_pdf_structure` adds a `stream.tell()` and a header
   write per object (`writer_mod.rs:2002-2047`). W5's write phase crosses the boundary 4,962
   times for 185 KB of output.
4. **Boundary calls track the object graph, not the file size.** W5 29,741 crossings / 137 objects
   = 217 per object; W7 51,918 / 198 = 262. The append/add_page
   phases dominate (24,754 of 29,741 in W5).
5. **No resource merging and no compression happen in these workloads**, so they are not part of the
   gap: `_merge_resources` is never called (0 occurrences in the call counts of all three workloads)
   and the only filter activity on the write side is *decoding* source streams
   (`filters_flate_decode` 6/4/6 calls for W5/W6/W7); the output is written uncompressed. The
   reference does exactly the same.

## 1. Method notes specific to the writer

* Stages are defined on the `PdfWriter` methods the workload actually calls: `append`/`merge` (W5),
  `add_page`/`_add_page` (W6/W7), `write`/`write_stream`, `_write_pdf_structure`,
  `_write_xref_table`, `_write_trailer`, `_add_object`, `_get_page_in_node`, plus the object-graph
  primitives (`clone`, `_clone`, `write_to_stream`) on `DictionaryObject`/`ArrayObject`/`StreamObject`
  and the scalar classes.
* The **wall-phase table is the primary result** for these workloads. The cumulative "layers" pass is
  *not* reproduced here: for W5–W7 each layer rebuilds a complete writer from scratch, so the layer
  difference is dominated by allocator/GC state and produced negative deltas in the first run. The
  direct timers bracket exactly one build and one write inside the same repetition.
* W5's `append` opens its own source readers (the workload passes file paths), so W5's source-reader
  cost is *inside* `append`; for W6/W7 the source readers are timed separately (`read_source`).
* `_add_object` is called 102 (W5) /
  138 (W6) / 176 (W7)
  times for 137 / 155 /
  198 objects in the writer: the remaining objects are the page-tree
  nodes, catalog and page objects that `add_page`/`merge` create through other paths. `_add_object`
  itself is cheap in absolute terms (it appends to `_objects` and creates one `IndirectObject`,
  `writer_mod.rs:1001-1030`); the cost is in the recursive `clone` that precedes it.
* All three workloads write byte-identical output on both backends (`out_sha256` equal, see the facts
  tables), which is the P6 regression gate from §9.3 of the plan.
* One call-count difference deserves a note: on W5 the port resolves **261** indirect objects where the
  reference resolves **321** (`open:get_object`), i.e. the port already skips ~19% of the resolutions
  and still produces the same bytes. Every other stage count matches exactly (146 `DictionaryObject.clone`,
  106 `IndirectObject.clone`, 103 `ArrayObject.clone`, 171 `DictionaryObject.write_to_stream`, ...).


## 2. W5 — merge/append of 5 small PDFs, 17 pages, 185,316 bytes

### 2.1 Wall-clock phases (direct timers, 7 reps)

| phase | port median | port min–max | port share | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|---:|
| `total_pipeline` | 40.64 ms | 39.39–43.56 | 100.0% | 11.44 ms | 10.86–12.64 | 3.55× |
| `append` | 35.04 ms | 34.04–37.72 | 86.2% | 9.21 ms | 8.45–10.30 | 3.81× |
| `write` | 5.61 ms | 5.35–6.21 | 13.8% | 2.25 ms | 2.21–2.41 | 2.50× |
| `build` | 35.04 ms | 34.04–37.72 | 86.2% | 9.21 ms | 8.45–10.30 | 3.81× |
| **total** | **40.64 ms** | 39.40–43.56 | 100% | **11.44 ms** | 10.86–12.65 | **3.55×** |

### 2.2 Stage timers (inclusive)

| stage | calls (port / ref) | port ms | reference ms | port/reference | stage % of workload (port) |
|---|---:|---:|---:|---:|---:|
| `graph:DictionaryObject.clone` | 146 / 146 | 42.24 | 11.87 | 3.56× | 97% |
| `graph:DictionaryObject._clone` | 146 / 146 | 41.31 | 11.54 | 3.58× | 95% |
| `write:append` | 5 / 5 | 36.36 | 9.89 | 3.68× | 83% |
| `write:merge` | 5 / 5 | 36.33 | 9.89 | 3.68× | 83% |
| `graph:IndirectObject.clone` | 106 / 106 | 29.28 | 8.08 | 3.62× | 67% |
| `open:get_object` | 261 / 321 | 22.87 | 5.67 | 4.03× | 52% |
| `tree:get_num_pages` | 23 / 23 | 18.11 | 4.50 | 4.02× | 42% |
| `tree:_flatten` | 5 / 5 | 18.07 | 4.49 | 4.02× | 41% |
| `open:_get_object_from_stream` | 3 / 3 | 15.91 | 3.91 | 4.07× | 36% |
| `write:add_page` | 17 / 17 | 11.92 | 3.28 | 3.64× | 27% |
| `write:_add_page` | 17 / 17 | 11.89 | 3.27 | 3.64× | 27% |
| `write:write` | 1 / 1 | 6.54 | 3.24 | 2.02× | 15% |
| `write:write_stream` | 1 / 1 | 6.53 | 3.24 | 2.02× | 15% |
| `write:_write_pdf_structure` | 1 / 1 | 6.45 | 3.17 | 2.03× | 15% |
| `graph:DictionaryObject.write_to_stream` | 171 / 171 | 5.81 | 2.71 | 2.14× | 13% |
| `graph:ArrayObject.clone` | 103 / 103 | 5.37 | 1.48 | 3.62× | 12% |
| `graph:NameObject.write_to_stream` | 965 / 965 | 3.50 | 1.39 | 2.51× | 8% |
| `open:__init__` | 5 / 5 | 2.26 | 0.86 | 2.61× | 5% |
| `open:_initialize_stream` | 5 / 5 | 2.15 | 0.82 | 2.62× | 5% |
| `open:read` | 5 / 5 | 2.13 | 0.81 | 2.62× | 5% |
| `graph:ArrayObject.write_to_stream` | 130 / 130 | 1.66 | 1.01 | 1.65× | 4% |
| `open:_read_xref_tables_and_trailers` | 5 / 5 | 1.47 | 0.58 | 2.55× | 3% |
| **workload wall** | | **43.62** | **13.18** | **3.31×** | 100% |

_Stage rows are inclusive: a stage that is called from another stage (e.g. `get_object` from `_flatten`) is contained in its caller, so rows must not be summed._

### 2.3 Calls per workload phase

| phase | Python→Rust boundary calls | share | materialised PDF objects |
|---|---:|---:|---:|
| `append` | 24,754 | 83.3% | 6,695 |
| `write` | 4,962 | 16.7% | 75 |
| **total** | **29,716** | 100.0% | **6,770** |

_The reference has no extension boundary: its equivalent work is ordinary Python calls counted by cProfile (see the profile tables). `py:` counters appear on both backends and are reported in the call-count table instead._

### 2.4 cProfile, classified

Total profiled self time 157.6 ms; rust_builtin 137.0 ms, python_shim 15.0 ms, other_python 3.1 ms, other_builtin 2.5 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_from_stream>` | rust_builtin | 591 | 21.99 | 14.0% | 56.92 |
| 2 | `<built-in method pypdf._pypdf.datastructures_read_object>` | rust_builtin | 4,338 | 15.77 | 10.0% | 75.60 |
| 3 | `<built-in method pypdf._pypdf.basemod_name_read_from_stream>` | rust_builtin | 3,141 | 11.17 | 7.1% | 12.17 |
| 4 | `<built-in method pypdf._pypdf.basemod_name_renumber>` | rust_builtin | 2,895 | 9.75 | 6.2% | 9.75 |
| 5 | `<built-in method pypdf._pypdf.datastructures_arrayobject_read_from_stream>` | rust_builtin | 426 | 7.80 | 5.0% | 29.43 |
| 6 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_setitem>` | rust_builtin | 2,850 | 4.12 | 2.6% | 4.12 |
| 7 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_clone_inner>` | rust_builtin | 438 | 4.01 | 2.5% | 44.54 |
| 8 | `<built-in method pypdf._pypdf.datastructures_arrayobject_clone>` | rust_builtin | 309 | 3.94 | 2.5% | 19.33 |
| 9 | `<built-in method pypdf._pypdf.basemod_number_new>` | rust_builtin | 4,374 | 2.94 | 1.9% | 2.94 |
| 10 | `<built-in method pypdf._pypdf.basemod_float_new>` | rust_builtin | 4,413 | 2.90 | 1.8% | 2.90 |
| 11 | `<built-in method pypdf._pypdf.basemod_pdfobject_reference_clone>` | rust_builtin | 5,139 | 2.60 | 1.6% | 2.63 |
| 12 | `<built-in method pypdf._pypdf.reader_read_object_header>` | rust_builtin | 411 | 2.53 | 1.6% | 2.53 |

Total profiled self time 125.3 ms; python_shim 89.7 ms, other_builtin 34.2 ms, other_python 1.4 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `read_object (pypdf/generic/_data_structures.py:1594)` | python_shim | 9,120 | 12.40 | 9.9% | 55.84 |
| 2 | `renumber (pypdf/generic/_base.py:806)` | python_shim | 2,895 | 7.42 | 5.9% | 11.52 |
| 3 | `_clone (pypdf/generic/_data_structures.py:369)` | python_shim | 438 | 5.99 | 4.8% | 29.60 |
| 4 | `<method 'read' of '_io.BytesIO' objects>` | other_builtin | 57,963 | 5.16 | 4.1% | 5.16 |
| 5 | `read_from_stream (pypdf/generic/_data_structures.py:255)` | python_shim | 426 | 5.09 | 4.1% | 22.75 |
| 6 | `read_until_regex (pypdf/_utils.py:318)` | python_shim | 3,837 | 5.07 | 4.0% | 9.13 |
| 7 | `read_from_stream (pypdf/generic/_data_structures.py:639)` | python_shim | 591 | 4.10 | 3.3% | 41.81 |
| 8 | `<method 'seek' of '_io.BytesIO' objects>` | other_builtin | 31,950 | 3.38 | 2.7% | 3.38 |
| 9 | `<method 'append' of 'list' objects>` | other_builtin | 34,059 | 3.37 | 2.7% | 3.37 |
| 10 | `read_non_whitespace (pypdf/_utils.py:193)` | python_shim | 8,511 | 3.03 | 2.4% | 4.32 |
| 11 | `read_from_stream (pypdf/generic/_base.py:860)` | python_shim | 3,141 | 2.90 | 2.3% | 12.46 |
| 12 | `<method 'encode' of 'str' objects>` | other_builtin | 25,875 | 2.67 | 2.1% | 2.67 |

### 2.5 Output and object-graph facts

| fact | port | reference |
|---|---:|---:|
| `id_translated_pages` | 127 | 127 |
| `id_translated_pdfs` | 5 | 5 |
| `objects_in_writer` | 137 | 137 |
| `out_bytes` | 185316 | 185316 |
| `pages` | 17 | 17 |
| `root_pages_kids` | 17 | 17 |
| `out_sha256` | `14f9f869a1335192…` | `14f9f869a1335192…` |


## 3. W6 — clone + rotate + cropbox + write, 8 pages, 245,923 bytes

### 3.1 Wall-clock phases (direct timers, 7 reps)

| phase | port median | port min–max | port share | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|---:|
| `total_pipeline` | 53.34 ms | 52.54–128.51 | 100.0% | 15.36 ms | 14.47–16.86 | 3.47× |
| `read_source` | 3.19 ms | 3.16–8.56 | 6.0% | 1.32 ms | 1.24–1.59 | 2.42× |
| `add_page` | 39.93 ms | 39.30–115.47 | 74.9% | 10.68 ms | 9.83–12.01 | 3.74× |
| `write` | 9.81 ms | 9.47–10.86 | 18.4% | 3.38 ms | 3.27–3.56 | 2.91× |
| `build` | 43.12 ms | 42.48–118.63 | 80.8% | 11.99 ms | 11.14–13.30 | 3.60× |
| **total** | **53.35 ms** | 52.54–128.51 | 100% | **15.37 ms** | 14.48–16.87 | **3.47×** |

### 3.2 Stage timers (inclusive)

| stage | calls (port / ref) | port ms | reference ms | port/reference | stage % of workload (port) |
|---|---:|---:|---:|---:|---:|
| `graph:DictionaryObject.clone` | 186 / 186 | 90.78 | 24.98 | 3.63× | 159% |
| `graph:DictionaryObject._clone` | 193 / 193 | 89.99 | 24.58 | 3.66× | 157% |
| `graph:IndirectObject.clone` | 192 / 192 | 60.35 | 16.43 | 3.67× | 106% |
| `open:get_object` | 279 / 279 | 32.30 | 8.31 | 3.89× | 56% |
| `write:add_page` | 8 / 8 | 31.82 | 8.92 | 3.57× | 56% |
| `write:_add_page` | 8 / 8 | 31.81 | 8.92 | 3.57× | 56% |
| `graph:ArrayObject.clone` | 133 / 133 | 18.74 | 5.37 | 3.49× | 33% |
| `write:write` | 1 / 1 | 11.39 | 4.71 | 2.42× | 20% |
| `write:write_stream` | 1 / 1 | 11.38 | 4.71 | 2.42× | 20% |
| `tree:get_num_pages` | 102 / 102 | 9.51 | 2.41 | 3.95× | 17% |
| `tree:_flatten` | 5 / 5 | 9.42 | 2.38 | 3.95× | 16% |
| `write:_write_pdf_structure` | 1 / 1 | 8.12 | 3.94 | 2.06× | 14% |
| `graph:DictionaryObject.write_to_stream` | 208 / 208 | 7.53 | 3.50 | 2.15× | 13% |
| `open:_get_object_from_stream` | 2 / 2 | 7.10 | 1.78 | 3.99× | 12% |
| `graph:NameObject.write_to_stream` | 1282 / 1282 | 4.37 | 1.71 | 2.55× | 8% |
| `open:__init__` | 5 / 5 | 3.47 | 1.52 | 2.28× | 6% |
| `open:_initialize_stream` | 5 / 5 | 3.40 | 1.48 | 2.30× | 6% |
| `open:read` | 5 / 5 | 3.15 | 1.22 | 2.59× | 6% |
| `write:_resolve_links` | 1 / 1 | 3.11 | 0.67 | 4.65× | 5% |
| `open:read_object_header` | 363 / 363 | 2.13 | 0.68 | 3.13× | 4% |
| `open:_read_xref_tables_and_trailers` | 5 / 5 | 1.84 | 0.71 | 2.60× | 3% |
| `graph:ArrayObject.write_to_stream` | 152 / 152 | 1.76 | 1.08 | 1.63× | 3% |
| **workload wall** | | **57.17** | **17.93** | **3.19×** | 100% |

_Stage rows are inclusive: a stage that is called from another stage (e.g. `get_object` from `_flatten`) is contained in its caller, so rows must not be summed._

### 3.3 Calls per workload phase

| phase | Python→Rust boundary calls | share | materialised PDF objects |
|---|---:|---:|---:|
| `read_source` | 2,939 | 7.5% | 175 |
| `add_page` | 27,866 | 71.0% | 7,756 |
| `write` | 8,441 | 21.5% | 643 |
| **total** | **39,246** | 100.0% | **8,574** |

_The reference has no extension boundary: its equivalent work is ordinary Python calls counted by cProfile (see the profile tables). `py:` counters appear on both backends and are reported in the call-count table instead._

### 3.4 cProfile, classified

Total profiled self time 202.2 ms; rust_builtin 173.4 ms, python_shim 18.7 ms, other_python 6.7 ms, other_builtin 3.4 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_from_stream>` | rust_builtin | 768 | 26.36 | 13.0% | 88.45 |
| 2 | `<built-in method pypdf._pypdf.datastructures_read_object>` | rust_builtin | 6,264 | 21.68 | 10.7% | 107.44 |
| 3 | `<built-in method pypdf._pypdf.basemod_name_read_from_stream>` | rust_builtin | 4,872 | 17.11 | 8.5% | 18.65 |
| 4 | `<built-in method pypdf._pypdf.basemod_name_renumber>` | rust_builtin | 3,846 | 11.93 | 5.9% | 11.93 |
| 5 | `<built-in method pypdf._pypdf.datastructures_arrayobject_read_from_stream>` | rust_builtin | 582 | 11.21 | 5.5% | 43.52 |
| 6 | `<built-in method pypdf._pypdf.reader_read_object_header>` | rust_builtin | 1,089 | 6.47 | 3.2% | 6.47 |
| 7 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_clone_inner>` | rust_builtin | 579 | 5.75 | 2.8% | 116.35 |
| 8 | `<built-in method pypdf._pypdf.datastructures_arrayobject_clone>` | rust_builtin | 399 | 5.43 | 2.7% | 67.53 |
| 9 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_setitem>` | rust_builtin | 3,342 | 4.92 | 2.4% | 4.92 |
| 10 | `<built-in method pypdf._pypdf.basemod_number_new>` | rust_builtin | 7,317 | 4.78 | 2.4% | 4.78 |
| 11 | `<built-in method pypdf._pypdf.reader_get_object>` | rust_builtin | 837 | 3.42 | 1.7% | 112.64 |
| 12 | `<built-in method pypdf._pypdf.basemod_pdfobject_reference_clone>` | rust_builtin | 6,903 | 3.35 | 1.7% | 3.39 |

Total profiled self time 164.7 ms; python_shim 117.4 ms, other_builtin 45.8 ms, other_python 1.4 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `read_object (pypdf/generic/_data_structures.py:1594)` | python_shim | 13,050 | 17.46 | 10.6% | 86.02 |
| 2 | `read_from_stream (pypdf/generic/_data_structures.py:255)` | python_shim | 582 | 10.64 | 6.5% | 39.17 |
| 3 | `renumber (pypdf/generic/_base.py:806)` | python_shim | 3,846 | 8.98 | 5.5% | 13.90 |
| 4 | `<method 'read' of '_io.BytesIO' objects>` | other_builtin | 84,630 | 7.49 | 4.6% | 7.49 |
| 5 | `read_until_regex (pypdf/_utils.py:318)` | python_shim | 5,118 | 6.93 | 4.2% | 12.36 |
| 6 | `read_from_stream (pypdf/generic/_data_structures.py:639)` | python_shim | 768 | 5.74 | 3.5% | 71.59 |
| 7 | `<method 'seek' of '_io.BytesIO' objects>` | other_builtin | 46,029 | 4.90 | 3.0% | 4.90 |
| 8 | `read_from_stream (pypdf/generic/_base.py:860)` | python_shim | 4,872 | 4.49 | 2.7% | 19.19 |
| 9 | `read_non_whitespace (pypdf/_utils.py:193)` | python_shim | 14,472 | 4.43 | 2.7% | 6.24 |
| 10 | `<method 'append' of 'list' objects>` | other_builtin | 42,357 | 4.18 | 2.5% | 4.18 |
| 11 | `_clone (pypdf/generic/_data_structures.py:369)` | python_shim | 579 | 3.82 | 2.3% | 83.78 |
| 12 | `<method 'encode' of 'str' objects>` | other_builtin | 31,746 | 3.21 | 2.0% | 3.21 |

### 3.5 Output and object-graph facts

| fact | port | reference |
|---|---:|---:|
| `id_translated_pages` | 146 | 146 |
| `id_translated_pdfs` | 5 | 5 |
| `objects_in_writer` | 155 | 155 |
| `out_bytes` | 245923 | 245923 |
| `pages` | 8 | 8 |
| `root_pages_kids` | 8 | 8 |
| `out_sha256` | `98412243edeaaa0f…` | `98412243edeaaa0f…` |


## 4. W7 — clone every page of 10 PDFs into a fresh writer, 22 pages, 298,006 bytes

### 4.1 Wall-clock phases (direct timers, 7 reps)

| phase | port median | port min–max | port share | reference median | reference min–max | port/reference |
|---|---:|---:|---:|---:|---:|---:|
| `total_pipeline` | 70.05 ms | 69.33–73.16 | 100.0% | 19.42 ms | 18.92–70.38 | 3.61× |
| `read_source` | 4.49 ms | 4.27–4.79 | 6.4% | 1.84 ms | 1.81–2.05 | 2.44× |
| `add_page` | 52.50 ms | 51.74–53.95 | 74.9% | 13.16 ms | 12.57–63.78 | 3.99× |
| `write` | 13.10 ms | 12.82–16.15 | 18.7% | 4.50 ms | 4.29–4.59 | 2.91× |
| `build` | 57.01 ms | 56.21–58.26 | 81.4% | 14.98 ms | 14.47–65.83 | 3.80× |
| **total** | **70.06 ms** | 69.33–73.17 | 100% | **19.43 ms** | 18.92–70.38 | **3.61×** |

### 4.2 Stage timers (inclusive)

| stage | calls (port / ref) | port ms | reference ms | port/reference | stage % of workload (port) |
|---|---:|---:|---:|---:|---:|
| `graph:DictionaryObject.clone` | 239 / 239 | 131.42 | 35.13 | 3.74× | 171% |
| `graph:DictionaryObject._clone` | 239 / 239 | 129.92 | 34.58 | 3.76× | 169% |
| `graph:IndirectObject.clone` | 184 / 184 | 69.64 | 18.66 | 3.73× | 91% |
| `open:get_object` | 362 / 362 | 44.23 | 10.51 | 4.21× | 58% |
| `write:add_page` | 22 / 22 | 39.58 | 10.26 | 3.86× | 52% |
| `write:_add_page` | 22 / 22 | 39.54 | 10.25 | 3.86× | 52% |
| `tree:get_num_pages` | 139 / 139 | 17.46 | 4.33 | 4.03× | 23% |
| `tree:_flatten` | 10 / 10 | 17.32 | 4.29 | 4.03× | 23% |
| `graph:ArrayObject.clone` | 189 / 189 | 15.34 | 4.13 | 3.71× | 20% |
| `write:write` | 1 / 1 | 15.12 | 6.01 | 2.51× | 20% |
| `write:write_stream` | 1 / 1 | 15.12 | 6.01 | 2.51× | 20% |
| `open:_get_object_from_stream` | 3 / 3 | 11.96 | 2.93 | 4.09× | 16% |
| `graph:DictionaryObject.write_to_stream` | 254 / 254 | 11.74 | 5.25 | 2.24× | 15% |
| `write:_write_pdf_structure` | 1 / 1 | 11.04 | 5.12 | 2.16× | 14% |
| `graph:NameObject.write_to_stream` | 1930 / 1930 | 6.31 | 2.47 | 2.55× | 8% |
| `open:__init__` | 10 / 10 | 4.65 | 1.88 | 2.47× | 6% |
| `open:_initialize_stream` | 10 / 10 | 4.52 | 1.84 | 2.46× | 6% |
| `open:read` | 10 / 10 | 3.99 | 1.47 | 2.71× | 5% |
| `write:_resolve_links` | 1 / 1 | 3.89 | 0.76 | 5.09× | 5% |
| `graph:ArrayObject.write_to_stream` | 200 / 200 | 3.71 | 1.96 | 1.89× | 5% |
| `tree:_pop_next_page_tree_kid` | 32 / 32 | 3.23 | 0.78 | 4.15× | 4% |
| `open:read_object_header` | 440 / 440 | 2.62 | 0.79 | 3.30× | 3% |
| **workload wall** | | **76.66** | **23.34** | **3.28×** | 100% |

_Stage rows are inclusive: a stage that is called from another stage (e.g. `get_object` from `_flatten`) is contained in its caller, so rows must not be summed._

### 4.3 Calls per workload phase

| phase | Python→Rust boundary calls | share | materialised PDF objects |
|---|---:|---:|---:|
| `read_source` | 3,801 | 7.3% | 202 |
| `add_page` | 36,607 | 70.5% | 10,615 |
| `write` | 11,485 | 22.1% | 848 |
| **total** | **51,893** | 100.0% | **11,665** |

_The reference has no extension boundary: its equivalent work is ordinary Python calls counted by cProfile (see the profile tables). `py:` counters appear on both backends and are reported in the call-count table instead._

### 4.4 cProfile, classified

Total profiled self time 273.6 ms; rust_builtin 233.7 ms, python_shim 27.6 ms, other_builtin 6.7 ms, other_python 5.6 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_read_from_stream>` | rust_builtin | 996 | 31.94 | 11.7% | 121.31 |
| 2 | `<built-in method pypdf._pypdf.datastructures_read_object>` | rust_builtin | 9,153 | 29.13 | 10.6% | 138.54 |
| 3 | `<built-in method pypdf._pypdf.basemod_name_read_from_stream>` | rust_builtin | 6,654 | 25.31 | 9.2% | 27.40 |
| 4 | `<built-in method pypdf._pypdf.basemod_name_renumber>` | rust_builtin | 5,790 | 17.78 | 6.5% | 17.78 |
| 5 | `<built-in method pypdf._pypdf.datastructures_arrayobject_read_from_stream>` | rust_builtin | 792 | 15.93 | 5.8% | 64.31 |
| 6 | `<built-in method pypdf._pypdf.datastructures_arrayobject_clone>` | rust_builtin | 567 | 8.11 | 3.0% | 51.75 |
| 7 | `<built-in method pypdf._pypdf.reader_read_object_header>` | rust_builtin | 1,320 | 7.78 | 2.8% | 7.78 |
| 8 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_clone_inner>` | rust_builtin | 717 | 6.56 | 2.4% | 140.58 |
| 9 | `<built-in method pypdf._pypdf.basemod_pdfobject_reference_clone>` | rust_builtin | 10,347 | 6.28 | 2.3% | 6.33 |
| 10 | `<built-in method pypdf._pypdf.datastructures_dictionaryobject_setitem>` | rust_builtin | 4,128 | 6.08 | 2.2% | 6.08 |
| 11 | `<built-in method pypdf._pypdf.basemod_number_new>` | rust_builtin | 8,757 | 5.58 | 2.0% | 5.58 |
| 12 | `<built-in method pypdf._pypdf.reader_get_object>` | rust_builtin | 1,086 | 4.38 | 1.6% | 148.24 |

Total profiled self time 211.9 ms; python_shim 149.8 ms, other_builtin 60.0 ms, other_python 2.1 ms

| # | site | kind | calls | self ms | self % | cum ms |
|---|---|---|---:|---:|---:|---:|
| 1 | `read_object (pypdf/generic/_data_structures.py:1594)` | python_shim | 17,073 | 21.92 | 10.3% | 106.40 |
| 2 | `renumber (pypdf/generic/_base.py:806)` | python_shim | 5,790 | 13.10 | 6.2% | 20.42 |
| 3 | `read_from_stream (pypdf/generic/_data_structures.py:255)` | python_shim | 792 | 11.08 | 5.2% | 53.71 |
| 4 | `<method 'read' of '_io.BytesIO' objects>` | other_builtin | 108,831 | 9.60 | 4.5% | 9.60 |
| 5 | `read_until_regex (pypdf/_utils.py:318)` | python_shim | 7,236 | 9.25 | 4.4% | 16.80 |
| 6 | `read_from_stream (pypdf/generic/_data_structures.py:639)` | python_shim | 996 | 6.94 | 3.3% | 92.96 |
| 7 | `<method 'seek' of '_io.BytesIO' objects>` | other_builtin | 60,702 | 6.42 | 3.0% | 6.42 |
| 8 | `<method 'append' of 'list' objects>` | other_builtin | 61,191 | 6.05 | 2.9% | 6.05 |
| 9 | `read_from_stream (pypdf/generic/_base.py:860)` | python_shim | 6,654 | 6.01 | 2.8% | 25.48 |
| 10 | `read_non_whitespace (pypdf/_utils.py:193)` | python_shim | 14,112 | 4.98 | 2.4% | 7.04 |
| 11 | `<method 'encode' of 'str' objects>` | other_builtin | 45,435 | 4.59 | 2.2% | 4.59 |
| 12 | `_clone (pypdf/generic/_data_structures.py:369)` | python_shim | 717 | 4.37 | 2.1% | 88.50 |

### 4.5 Output and object-graph facts

| fact | port | reference |
|---|---:|---:|
| `id_translated_pages` | 194 | 194 |
| `id_translated_pdfs` | 10 | 10 |
| `objects_in_writer` | 198 | 198 |
| `out_bytes` | 298006 | 298006 |
| `pages` | 22 | 22 |
| `root_pages_kids` | 22 | 22 |
| `out_sha256` | `1ba70bae93b510d9…` | `1ba70bae93b510d9…` |

## 5. Object-graph copy vs serialization — the numbers

| workload | build phase | port build | ref build | build ratio | write phase | port write | ref write | write ratio |
|---|---|---:|---:|---:|---|---:|---:|---:|
| W5 | `append` (open + flatten + clone + `/Pages` build) | 35.04 ms | 9.21 ms | 3.81× | `write` | 5.61 ms | 2.25 ms | 2.50× |
| W6 | `read_source` + `add_page` | 43.12 ms | 11.99 ms | 3.59× | `write` | 9.81 ms | 3.38 ms | 2.91× |
| W7 | `read_source` + `add_page` | 56.99 ms | 15.00 ms | 3.80× | `write` | 13.10 ms | 4.50 ms | 2.91× |

**Answer to the P6 question ("is it the object-graph copy or the serialization?"): the copy.**
Between 86% and 81% of each
workload is reading + cloning + page-tree building, and only 14–19% is
serialization. Serialization is *less* regressed (2.5–2.9×) than the copy phase (3.7–4.0×), because the
write path is a shallower recursion with bigger fragments per crossing.

Serialization throughput (port vs reference): W5 33.0 vs
82.5 MB/s, W6 25.1 vs
72.8 MB/s, W7 22.7 vs
66.3 MB/s — i.e. ~2.5–2.9× less write throughput for
the same bytes and the same object graph.

### 5.1 Clone and write call counts


| workload | boundary calls | materialised objects | object `clone` calls | `_clone` (dict) | `_add_object` | `write_to_stream` calls | objects in writer | bytes written |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| W5 | 29,741 | 6,787 | 1,999 | 146 | 102 | 2,821 | 137 | 185,316 |
| W6 | 39,271 | 8,591 | 2,726 | 193 | 138 | 3,610 | 155 | 245,923 |
| W7 | 51,918 | 11,682 | 3,941 | 239 | 176 | 4,945 | 198 | 298,006 |

`clone` sums the extension-level clone entry points (dictionary / array / stream / scalar); `_clone` is the recursive dictionary clone body, called once per cloned dictionary.

Per workload in detail (port):

| workload | `DictionaryObject.clone` | `_clone` (recursive dict body) | `ArrayObject.clone` | `IndirectObject.clone` | scalar clones (Name/Number/Float/TextString) | `dict.__setitem__` crossings | `write_to_stream` calls | objects in writer | bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| W5 | 146 | 146 | 103 | 106 | 1464 | 950 | 2821 | 137 | 185,316 |
| W6 | 186 | 193 | 133 | 192 | 1973 | 1114 | 3610 | 155 | 245,923 |
| W7 | 239 | 239 | 189 | 184 | 3021 | 1376 | 4945 | 198 | 298,006 |

Reference values for the same counters are identical for the clone entry points (146/186/239
`DictionaryObject.clone`, 103/133/189 `ArrayObject.clone`, 106/192/184 `IndirectObject.clone`) — the
object graph is walked the same way on both sides. The port's difference is that each of those calls
is a Python frame → PyO3 call → Python callbacks, whereas the reference's are plain Python calls
(the scalar clone costs measured in §5.2 of `diag_reader.md` — e.g. `NumberObject` creation 4.6× —
apply here per scalar).


### 5.2 Where the write phase goes

The segment split below comes from the **instrumented stage pass** (one wrapper per call, ~7–10% total
overhead), so its absolutes are slightly higher than the wall-phase table. Use the *shares* (measured
within one pass); the cross-backend ratios are distorted by the wrapper overhead and are discussed
below. `write` = `PdfWriter.write` (whole serialization).

| segment | W5 port | W5 ref | W7 port | W7 ref | W5 ratio | W7 ratio |
|---|---:|---:|---:|---:|---:|---:|
| `write` / `write_stream` | 6.54 ms | 3.24 ms | 15.12 ms | 6.01 ms | 2.02× | 2.51× |
| of which `_write_pdf_structure` | 6.45 ms | 3.17 ms | 11.04 ms | 5.12 ms | 2.03× | 2.16× |
| `_write_xref_table` + `_write_trailer` | 0.075 ms | 0.061 ms | 0.090 ms | 0.083 ms | — | — |
| `_resolve_links` | 0.002 ms | 0.001 ms | 3.89 ms | 0.76 ms | — | — |
| `DictionaryObject.write_to_stream` (inclusive) | 5.81 ms | 2.71 ms | 11.74 ms | 5.25 ms | 2.14× | 2.24× |

In W5 almost all of the write phase is `_write_pdf_structure` (99%), and that is dominated by
the per-object `write_to_stream` traversal; in W7 `_resolve_links` adds a further 3.9 ms
(26%, the outline-bearing samples) and `_write_pdf_structure` is 73%. The xref table
and trailer together are <1% in every workload and are not worth changing first.
The ratios in this table are *lower bounds*: the per-call wrappers inflate the reference's write phase
more (+44% vs its wall-phase number) than the port's (+17%), because the reference executes the same
2.8k `write_to_stream` calls as plain Python calls that cProfile-style wrappers tax proportionally
more. The authoritative port/reference ratios for the write phase are the wall-phase ones
(2.5–2.9×).

Serialization is **object-count-bound, not byte-bound**: W5 writes 185 KB in 5.61 ms
(33 MB/s) while W7 writes 298 KB in 13.10 ms (23 MB/s) — the per-object cost
(41 µs/object for W5, 66 µs for W7) is what dominates, and it is set by the number of
`write_to_stream` calls (2,821 for W5, 4,945 for W7) times the per-crossing cost.

### 5.3 Object-graph size and bytes

| workload | pages | objects in writer | `_id_translated` pdfs | `_id_translated` pages | bytes written | bytes/object |
|---|---:|---:|---:|---:|---:|---:|
| W5 | 17 | 137 | 5 | 127 | 185,316 | 1353 |
| W6 | 8 | 155 | 5 | 146 | 245,923 | 1587 |
| W7 | 22 | 198 | 10 | 194 | 298,006 | 1505 |

These are identical on the reference side (same pages, same object count, same `_id_translated`
cardinality, same SHA-256), i.e. the port already produces exactly the same object graph — the whole
gap is the cost of producing it.


## 6. Concrete P6 entry points (file:line) and risks

| # | entry point | what happens today | measured cost | P6 change |
|---|---|---|---|---|
| 1 | `crates/pypdf-py/src/modules/writer_mod.rs:4427` `append` / `:4487` `merge` | per source file: create `PdfReader`, then `add_page` per page (which clones the page object graph) and record the source in `_id_translated` | W5 `append` 35.0 ms of 40.6 ms (86%) | keep the per-page API, but batch the clone: one Rust call per source document that walks the page list and clones with a Rust-side identity map |
| 2 | `writer_mod.rs:1274` `add_page` → `:1095` `_add_page` | validates the page, removes a previously translated page from `_id_translated` (`:1117-1120`), calls `page.clone(self, False, [excluded])` (`:1129`), merges the PDF header, inserts into the page tree via `_get_page_in_node` | W6 39.9 ms, W7 52.5 ms (75% of each) | same batching; `_id_translated` lookups must stay (they are the cycle/dedup guard) but can be a Rust `HashMap<(usize, u32), Py<PyAny>>` |
| 3 | `data_structures_mod.rs:892` `dictionaryobject_clone` → `:922` `clone_inner` | per entry: `items()`, `value_is_scalar`, `slf.contains`, `value.clone()` (Python method → Rust) for every non-scalar, `NameObject(key)` construction, `slf.set_item(...)` (goes through `DictionaryObject.__setitem__` → Rust) | W5 1,999 clone entry points / 146 recursions, 950 `__setitem__` crossings; stage timer `graph:DictionaryObject.clone` 42.2 ms inclusive (containment makes this exceed the workload wall — see the note in the stage table) | clone inside Rust with an identity map (`visited`), materialising Python objects only if the public API later touches them; keep the `/Next`,`/Prev`,`/N`,`/V` chain special case (`data_structures_mod.rs:970-1075`) and `force_duplicate` semantics |
| 4 | `data_structures_mod.rs:1197` `dictionaryobject_setitem` + `python/pypdf/generic/_data_structures.py:383` `__setitem__` | every dict insertion during clone/write is a Python frame → Rust call | W5 950, W7 1376 crossings | internal Rust path must use `dict.__setitem__` directly (no shim); public path unchanged |
| 5 | `writer_mod.rs:1001` `_add_object` | checks `obj.indirect_reference`, recurses into `/Contents` if it is a container, appends to `_objects`, creates an `IndirectObject` and sets it on the object | 102–176 calls, cheap individually; it is the recursion (`clone`) before it that costs | keep `_objects` order (byte-identical output depends on it) and the `indirect_reference` mutation (observable), but drive it from the Rust clone |
| 6 | `writer_mod.rs:2002` `_write_pdf_structure` | per object: `stream.tell()`, `write(b"<id> 0 obj\n")`, `obj.write_to_stream(stream)` (Python method → Rust), `write(b"\nendobj\n")`; honours `_encryption.encrypt_object` | 6.45 ms of the 5.61 ms write (W5) | write into a Rust `Vec<u8>`/buffer and hand the finished bytes to the Python stream once per object (or per document); keep object numbering and the exact byte format (`<idnum> 0 obj` + newline, `endobj` + newline) |
| 7 | `data_structures_mod.rs:1277` `dictionaryobject_write_to_stream` (+ `:2325` stream, `:1277` array) | per entry: `key.write_to_stream` (Python→Rust), `write(b" ")`, `value.write_to_stream`, `write(b"\n")` — 4 stream writes + 2 boundary crossings per entry | 2,821 `write_to_stream` calls in W5, of which 965 are `NameObject` (each also calls `renumber`, 965 calls) | serialize the dictionary in one Rust pass; `renumber` is a pure function of the name bytes — compute it in Rust, do not call back |
| 8 | `writer_mod.rs:2064` `_write_xref_table` / `:2099` `_write_trailer` | formats one xref line per object (`%010d %05d n `) and writes each separately | 0.040 / 0.035 ms (W5) — negligible today | leave alone in the first pass; if the buffer rewrite happens, format the whole xref table into the same buffer |
| 9 | `writer_mod.rs:1716` `_resolve_links` | walks named destinations to resolve `/Dest` links before writing | 0.002 ms (W5), 3.89 ms (W7) | out of scope for the first pass; note that W7 spends more here (some samples carry outlines) |
| 10 | `writer_mod.rs:338` `write_bytes` | every fragment is a separate `stream.write(bytes)` Python C call from Rust | part of the 2,821 write-path crossings; ~11,284 Python `write` calls for W5 (4 per `write_to_stream` entry) | batch into one `write` per object, then one per document |

**Ordering suggestion for P6:** (a) #3/#4 clone in Rust with an identity map (largest share, 75–86%),
(b) #1/#2 batched page cloning, (c) #6/#7 serialization into a Rust buffer, (d) #8/#9 last.

### 6.1 Risks (must be handled before P6 lands)

1. **Byte-identical output is the acceptance gate** (`validation/results/perf-report.md` compares output
   size and SHA-256; §9.3 of the plan requires keeping it for these fixed samples). Anything that
   changes object order, object numbering, xref formatting, dictionary key order, or the float/number
   repr breaks it. All three workloads currently match (`out_sha256` equal).
2. **`indirect_reference` is observable and mutable.** `_add_object` sets it on the object
   (`writer_mod.rs:1023`), `_add_page` deletes a translated page from `_id_translated` before cloning
   (`:1117`), and `_clone` writes `value.indirect_reference = None` for streams
   (`data_structures_mod.rs:1094`). A Rust-side graph copy must reproduce these side effects on the
   Python objects that the caller still holds.
3. **Cycle handling is subtle.** `_clone` carries a `visited` set and a separate `/Next`/`/Prev`/`/N`/`/V`
   linked-list path that iterates instead of recursing (`data_structures_mod.rs:970-1075`). A rewrite
   must keep both, plus the `ignore_fields`/`force_duplicate` semantics and the "does not overwrite an
   existing clone" rule (`if length(&destination)? == 0`).
4. **`_id_translated` is keyed by `id(pdf)`** (`writer_mod.rs:1118`) — an identity map on the Rust side
   must not change the number of translation entries: W5 has 5 PDFs / 127 page translations, W6 5/146,
   W7 10/194, identical on both backends. A change here would alter output structure.
5. **Encryption path**: `_write_pdf_structure` calls `encryption.encrypt_object` per object when
   `_encryption` is set. None of W5–W7 is encrypted, so this path is *not* covered by the current
   measurements; a buffer-based serializer must keep the per-object encryption hook.
6. **Incremental updates** (`_write_increment`) share `list_objects_in_increment`/`_original_hash`;
   W5–W7 do not use them. Do not "optimise" them by assumption.
7. **`add_page` copies `/Contents` into `_objects` first** (`_add_object` recursion,
   `writer_mod.rs:1014-1020`), and `_add_page` re-inserts the page into the page tree with
   `_get_page_in_node`. The page-tree shape (`root_pages_kids` = 17/8/22) is part of the output.

## 7. What is still unproven

* **Rust-internal split of the clone cost.** The stage timers show `DictionaryObject.clone`
  42.2 ms inclusive for W5, but because
  cloning is recursive the inclusive totals of `clone`/`_clone`/`IndirectObject.clone` overlap and must
  not be summed. The per-entry breakdown (Python callback vs `NameObject` creation vs `dict.__setitem__`)
  is inferred from the code plus the boundary counts; it should be confirmed with Rust counters before
  the rewrite.
* **The reference is *not* doing less work** — its clone/`write_to_stream` call counts are identical —
  so a Rust clone that still materialises the same Python objects may only recover the boundary
  overhead (≈2.5–3×), not reach parity with a hypothetical "no Python objects" design. P2's lazy
  materialisation is the bigger lever for W5–W7 too.
* **Per-object cost of `write_to_stream` for streams** is not separated here; W5/W6/W7 write few large
  streams (34/49/69 stream objects) and the copy of those bytes may deserve its own measurement before
  deciding on a buffer strategy.
* The current baseline (`validation/results/perf-report.md`, 21:41, post-P1 build) has W5–W7 at
  0.29×/0.26×/0.27× while this diagnostic run (previous build, lower load) measures 3.55×/3.47×/3.61×
  slower; the gap is the machine-load effect described in `diag_reader.md` §1.1. Relative stage
  ratios are stable across runs.

## 8. Reproduce

```bash
cd /Users/yxy/Desktop/pypdf-rust
export PYTHONHASHSEED=0
bash validation/optimization/scripts/run_all.sh          # both backends, all workloads
# or only the writer workloads:
.venvs/dev/bin/python validation/optimization/scripts/rw_diag.py --backend rust \
    --workloads W5,W6,W7 --mode all --repeat 7 \
    --out validation/optimization/raw/rw_diag_rust.json
```

Tables for this report:

```bash
.venvs/dev/bin/python validation/optimization/scripts/render_tables.py \
    --rust validation/optimization/raw/rw_diag_rust.json \
    --reference validation/optimization/raw/rw_diag_reference.json --section writer
```

