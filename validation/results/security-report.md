# Security and robustness validation report (§8.2 / §8.3 / §8.4)

Generated: `2026-11-25T03:09:06Z` (UTC)
Repository commit: `22fb0f5`

Differential run of the adversarial corpus against the reference implementation (PyPI pypdf 7.1.1 in `.venvs/oracle`) and the Rust+PyO3 port (`.venvs/dev`). Every case runs in an isolated subprocess with a wall-clock timeout, a CPU rlimit backstop and (where the platform honours it) an address-space rlimit.

## 1. Environment

| Side | Interpreter | pypdf version | pypdf location |
|---|---|---|---|
| oracle | `/Users/yxy/Desktop/pypdf-rust/.venvs/oracle/bin/python` | 7.1.1 | `/Users/yxy/Desktop/pypdf-rust/.venvs/oracle/lib/python3.14/site-packages/pypdf/__init__.py` |
| port | `/Users/yxy/Desktop/pypdf-rust/.venvs/dev/bin/python` | 7.1.1 | `/Users/yxy/Desktop/pypdf-rust/python/pypdf/__init__.py` |

Corpus: 57 generated case(s), 2 skipped (heavy), 5.0 MiB. Heavy mode: **disabled**.

## 2. Summary

| Side | Cases | Handled OK | Raised | LimitReachedError | Timeouts | Crashes | Panics | Env errors | Max RSS | CPU total | Wall total |
|---|---|---|---|---|---|---|---|---|---|---|---|
| oracle | 57 | 32 | 25 | 15 | 0 | 0 | 0 | 0 | 83.2 MiB | 8.15 s | 9.40 s |
| port | 57 | 32 | 25 | 15 | 0 | 0 | 0 | 0 | 101.2 MiB | 11.79 s | 13.35 s |

Comparison: **PASS** — 57 case(s): 57 identical, 0 identical modulo message text, 0 decision difference(s), 0 exception-class difference(s), 0 step-set difference(s), 0 value difference(s), 0 partial-output difference(s), 0 timeout(s), 0 crash(es), 0 not-available. Blocking findings: **0**.

Pass criteria applied (plan §2.5): success/failure decision and exception class must match; message text differences are recorded separately and do not fail the case; timeouts, crashes, panics and missing capabilities are never counted as PASS. Message comparison normalises the object addresses that pypdf embeds in some reprs (``IndirectObject(5, 0, <id(pdf)>)``); the raw texts are kept in the JSON evidence and listed in section 6.

## 3. Coverage of plan §8.2

| Plan requirement | Cases |
|---|---|
| Decompression bombs | `bomb_flate_1mb` |
| Deeply nested arrays/dictionaries | `nested_arrays_d10`, `nested_arrays_d100`, `nested_arrays_d1000`, `nested_arrays_d10000`, `nested_dicts_d10`, `nested_dicts_d100`, `nested_dicts_d1000`, `nested_dicts_d10000` |
| Duplicate / conflicting object numbers | `duplicate_object_numbers`, `xref_conflicting_entries` |
| Excessive stream filter chains | `filter_chain_50_flate`, `filter_chain_16_flate`, `filter_unknown_name` |
| Huge /Length, truncated streams | `stream_length_huge`, `stream_length_huge_indirect`, `stream_length_negative`, `stream_truncated`, `stream_missing_endstream` |
| Indirect reference cycles | `cycle_indirect_self`, `cycle_indirect_two`, `cycle_stream_length_self`, `page_tree_cycle_self`, `page_tree_cycle_two`, `page_tree_depth_150`, `outline_cycle_self`, `outline_self_parent`, `outline_depth_170`, `form_field_cycle_self`, `annotation_cycle` |
| Inline image boundary errors | `inline_image_huge_dimensions`, `inline_image_truncated` |
| Malformed xref and wrong object offsets | `xref_offsets_shifted_1000`, `xref_startxref_garbage`, `xref_table_truncated`, `xref_declared_size_huge`, `xref_trailer_size_too_small`, `xref_nonmonotonic_offsets`, `xref_entry_offset_out_of_range`, `xref_negative_subsection_size`, `duplicate_object_numbers`, `xref_conflicting_entries`, `indirect_ref_number_2_32` |
| Missing PDF header / EOF | `structure_missing_eof`, `structure_eof_trailing_garbage`, `structure_missing_header`, `structure_empty_file`, `structure_header_only`, `structure_trailer_no_root`, `structure_root_not_dictionary`, `structure_catalog_no_pages`, `structure_page_count_lie` |
| Outline / form / annotation tree cycles | `outline_cycle_self`, `outline_self_parent`, `outline_depth_170`, `form_field_cycle_self`, `annotation_cycle` |
| Oversized CMap / page labels / attachment names | `cmap_oversized_mapping`, `cmap_oversized_code_token`, `page_labels_oversized_nums`, `page_label_huge_prefix`, `attachments_oversized_names_array`, `attachment_name_1mb` |
| Page-tree cycles | `page_tree_cycle_self`, `page_tree_cycle_two`, `page_tree_depth_150` |

## 4. Per-case results

| Case | Category | Expected | Oracle | Port | Status | Notes |
|---|---|---|---|---|---|---|
| `annotation_cycle` | cycles | recover | ok | ok | PASS |  |
| `attachment_name_1mb` | oversized | recover | ok | ok | PASS |  |
| `attachments_oversized_names_array` | oversized | reject | attachments:LimitReachedError | attachments:LimitReachedError | PASS |  |
| `bomb_flate_1mb` | resources | recover | ok | ok | PASS |  |
| `cmap_oversized_code_token` | oversized | reject | text:LimitReachedError | text:LimitReachedError | PASS |  |
| `cmap_oversized_mapping` | oversized | reject | text:LimitReachedError | text:LimitReachedError | PASS |  |
| `cycle_indirect_self` | cycles | recover | ok | ok | PASS |  |
| `cycle_indirect_two` | cycles | recover | ok | ok | PASS |  |
| `cycle_stream_length_self` | cycles | reject | resolve:LimitReachedError | resolve:LimitReachedError | PASS |  |
| `duplicate_object_numbers` | xref | recover | ok | ok | PASS |  |
| `filter_chain_16_flate` | streams | recover | ok | ok | PASS |  |
| `filter_chain_50_flate` | streams | reject | resolve:LimitReachedError, text:LimitReachedError | resolve:LimitReachedError, text:LimitReachedError | PASS |  |
| `filter_unknown_name` | streams | reject | resolve:NotImplementedError | resolve:NotImplementedError | PASS |  |
| `form_field_cycle_self` | cycles | either | ok | ok | PASS |  |
| `indirect_ref_number_2_32` | xref | either | ok | ok | PASS |  |
| `inline_image_huge_dimensions` | streams | reject | operations:LimitReachedError | operations:LimitReachedError | PASS |  |
| `inline_image_truncated` | streams | reject | operations:PdfReadError | operations:PdfReadError | PASS |  |
| `nested_arrays_d10` | nesting | recover | ok | ok | PASS |  |
| `nested_arrays_d100` | nesting | recover | ok | ok | PASS |  |
| `nested_arrays_d1000` | nesting | reject | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError | PASS |  |
| `nested_arrays_d10000` | nesting | reject | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError | PASS |  |
| `nested_dicts_d10` | nesting | recover | ok | ok | PASS |  |
| `nested_dicts_d100` | nesting | recover | ok | ok | PASS |  |
| `nested_dicts_d1000` | nesting | either | ok | ok | PASS |  |
| `nested_dicts_d10000` | nesting | either | ok | ok | PASS |  |
| `outline_cycle_self` | cycles | recover | ok | ok | PASS |  |
| `outline_depth_170` | cycles | reject | outline:LimitReachedError | outline:LimitReachedError | PASS |  |
| `outline_self_parent` | cycles | recover | ok | ok | PASS |  |
| `page_label_huge_prefix` | oversized | recover | ok | ok | PASS |  |
| `page_labels_oversized_nums` | oversized | reject | labels:LimitReachedError | labels:LimitReachedError | PASS |  |
| `page_tree_cycle_self` | cycles | reject | pages:PdfReadError, text:PdfReadError | pages:PdfReadError, text:PdfReadError | PASS |  |
| `page_tree_cycle_two` | cycles | reject | pages:PdfReadError, text:PdfReadError | pages:PdfReadError, text:PdfReadError | PASS |  |
| `page_tree_depth_150` | cycles | reject | pages:LimitReachedError | pages:LimitReachedError | PASS |  |
| `stream_length_huge` | streams | reject | resolve:LimitReachedError, text:LimitReachedError | resolve:LimitReachedError, text:LimitReachedError | PASS |  |
| `stream_length_huge_indirect` | streams | reject | resolve:LimitReachedError | resolve:LimitReachedError | PASS |  |
| `stream_length_negative` | streams | recover | ok | ok | PASS |  |
| `stream_missing_endstream` | streams | reject | resolve:PdfReadError | resolve:PdfReadError | PASS |  |
| `stream_truncated` | streams | reject | pages:PdfStreamError, parse:PdfStreamError, resolve:PdfStreamError | pages:PdfStreamError, parse:PdfStreamError, resolve:PdfStreamError | PASS |  |
| `structure_catalog_no_pages` | structure | reject | pages:PdfReadError, text:PdfReadError | pages:PdfReadError, text:PdfReadError | PASS |  |
| `structure_empty_file` | structure | reject | pages:EmptyFileError, parse:EmptyFileError | pages:EmptyFileError, parse:EmptyFileError | PASS |  |
| `structure_eof_trailing_garbage` | structure | recover | ok | ok | PASS |  |
| `structure_header_only` | structure | reject | pages:PdfReadError, parse:PdfReadError, text:PdfReadError | pages:PdfReadError, parse:PdfReadError, text:PdfReadError | PASS |  |
| `structure_missing_eof` | structure | reject | pages:PdfStreamError, parse:PdfStreamError, text:PdfStreamError | pages:PdfStreamError, parse:PdfStreamError, text:PdfStreamError | PASS |  |
| `structure_missing_header` | structure | either | ok | ok | PASS |  |
| `structure_page_count_lie` | structure | recover | ok | ok | PASS |  |
| `structure_root_not_dictionary` | structure | recover | ok | ok | PASS |  |
| `structure_trailer_no_root` | structure | recover | ok | ok | PASS |  |
| `writer_page_parent_cycle` | structure | either | ok | ok | PASS |  |
| `xref_conflicting_entries` | xref | either | ok | ok | PASS |  |
| `xref_declared_size_huge` | xref | reject | pages:LimitReachedError, parse:LimitReachedError | pages:LimitReachedError, parse:LimitReachedError | PASS |  |
| `xref_entry_offset_out_of_range` | xref | recover | ok | ok | PASS |  |
| `xref_negative_subsection_size` | xref | reject | pages:LimitReachedError, parse:LimitReachedError, text:LimitReachedError | pages:LimitReachedError, parse:LimitReachedError, text:LimitReachedError | PASS |  |
| `xref_nonmonotonic_offsets` | xref | recover | ok | ok | PASS |  |
| `xref_offsets_shifted_1000` | xref | recover | ok | ok | PASS |  |
| `xref_startxref_garbage` | xref | recover | ok | ok | PASS |  |
| `xref_table_truncated` | xref | recover | ok | ok | PASS |  |
| `xref_trailer_size_too_small` | xref | recover | ok | ok | PASS |  |

## 5. Blocking mismatches

None: every case matched on decision and exception class.

## 6. Non-blocking differences

### Message-only differences (not a failure)

None.

### Informational, non-blocking values

These measure how much of an over-deep structure survives before the interpreter's recursion guard fires. The retained depth depends on the native stack layout (Python frames vs. Rust/PyO3 frames), so it is not a stable behavioural contract; both implementations silently truncate the structure rather than raising a `LimitReachedError`.

| Case | Step | Field | Oracle | Port |
|---|---|---|---|---|
| `nested_dicts_d1000` | depth | max_depth | 490 | 398 |
| `nested_dicts_d1000` | depth | nodes | 490 | 398 |
| `nested_dicts_d10000` | depth | max_depth | 490 | 398 |
| `nested_dicts_d10000` | depth | nodes | 490 | 398 |

### Object-address-only message differences

Identical after normalising ``id(pdf)``; recorded for completeness, not a behavioural difference.

| Case | Step | Oracle raw message | Port raw message |
|---|---|---|---|
| `cycle_stream_length_self` | resolve | `Detected loop with self reference for IndirectObject(6, 0, 4498514880).` | `Detected loop with self reference for IndirectObject(6, 0, 4469518224).` |
| `nested_arrays_d1000` | pages | `Detected loop with self reference for IndirectObject(1, 0, 4419576768).` | `Detected loop with self reference for IndirectObject(1, 0, 4416171920).` |
| `nested_arrays_d1000` | depth | `Detected loop with self reference for IndirectObject(1, 0, 4419576768).` | `Detected loop with self reference for IndirectObject(1, 0, 4416171920).` |
| `nested_arrays_d10000` | pages | `Detected loop with self reference for IndirectObject(1, 0, 4456014784).` | `Detected loop with self reference for IndirectObject(1, 0, 4450627472).` |
| `nested_arrays_d10000` | depth | `Detected loop with self reference for IndirectObject(1, 0, 4456014784).` | `Detected loop with self reference for IndirectObject(1, 0, 4450627472).` |
| `stream_length_huge` | text | `Detected loop with self reference for IndirectObject(5, 0, 4421116864).` | `Detected loop with self reference for IndirectObject(5, 0, 4442238864).` |

## 7. Value differences on successful steps

None.

## 8. Observed guard behaviour

Cases where at least one side produced a `LimitReachedError` or a resource-limit warning. This documents that the guard was actually exercised rather than skipped.

| Case | Oracle LimitReached | Oracle classes | Port LimitReached | Port classes | Port limit warnings |
|---|---|---|---|---|---|
| `attachments_oversized_names_array` | True | attachments:LimitReachedError | True | attachments:LimitReachedError |  |
| `cmap_oversized_code_token` | True | text:LimitReachedError | True | text:LimitReachedError |  |
| `cmap_oversized_mapping` | True | text:LimitReachedError | True | text:LimitReachedError |  |
| `cycle_stream_length_self` | True | resolve:LimitReachedError | True | resolve:LimitReachedError |  |
| `filter_chain_50_flate` | True | resolve:LimitReachedError, text:LimitReachedError | True | resolve:LimitReachedError, text:LimitReachedError |  |
| `inline_image_huge_dimensions` | True | operations:LimitReachedError | True | operations:LimitReachedError |  |
| `nested_arrays_d1000` | True | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError | True | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError |  |
| `nested_arrays_d10000` | True | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError | True | depth:LimitReachedError, pages:LimitReachedError, parse:PdfReadError |  |
| `outline_depth_170` | True | outline:LimitReachedError | True | outline:LimitReachedError |  |
| `page_labels_oversized_nums` | True | labels:LimitReachedError | True | labels:LimitReachedError |  |
| `page_tree_depth_150` | True | pages:LimitReachedError | True | pages:LimitReachedError |  |
| `stream_length_huge` | True | resolve:LimitReachedError, text:LimitReachedError | True | resolve:LimitReachedError, text:LimitReachedError |  |
| `stream_length_huge_indirect` | True | resolve:LimitReachedError | True | resolve:LimitReachedError |  |
| `xref_declared_size_huge` | True | pages:LimitReachedError, parse:LimitReachedError | True | pages:LimitReachedError, parse:LimitReachedError |  |
| `xref_negative_subsection_size` | True | pages:LimitReachedError, parse:LimitReachedError, text:LimitReachedError | True | pages:LimitReachedError, parse:LimitReachedError, text:LimitReachedError |  |

## 9. Method limitations (explicit)

* Address-space rlimit (`RLIMIT_AS`) could **not** be applied on this platform (`darwin`): `ValueError: current limit exceeds maximum limit`. Memory pressure is therefore only observed (peak RSS via `ru_maxrss`), not enforced; the wall-clock timeout and the `RLIMIT_CPU` backstop are the enforced bounds. `NOT_ENFORCED` — not a pass.
* peak RSS is read from `resource.getrusage(RUSAGE_SELF).ru_maxrss` inside the child (bytes on macOS, KiB on Linux, normalised to bytes).
* A `TIMEOUT` result means the case could not be decided within the budget; it is reported as TIMEOUT and never as PASS.
* Deeply nested inline **dictionaries** are not covered by an explicit limit in the reference implementation: it catches the interpreter `RecursionError` and returns the partially parsed dictionary, so `PdfReader` 'succeeds' with a truncated value. The retained depth depends on the native stack layout and changed when the PyO3 extension was rebuilt during this session (port retained 492 levels in one build, 984 in another; reference 490). This is recorded as an informational value difference, not as a pass/fail criterion, because neither side guarantees a parse depth.
* `partial_output` is only evaluated for cases that request a writer round-trip (`writer_page_parent_cycle`); read-only cases record `null` (not applicable).
* The corpus is generated fresh in a temporary directory for every run; `security-corpus-manifest.json` records the intent, size and sha256 of every case.

## 10. Mutation fuzzing (§8.4)

Corpus: 200 mutated PDF(s) (seed 20261124, 72 source PDFs), generated by `tools/fuzz/gen_corpus.py` and executed through `tools/fuzz/run.py` in both venvs.

| Side | Files | Handled | Raised | Timeouts | Panics | Hard crash | Wall |
|---|---|---|---|---|---|---|---|
| oracle | 200 | 127 | 73 | 0 | 0 | False | 5.408 s |
| port | 200 | 127 | 73 | 0 | 0 | False | 7.137 s |

Comparison: **IDENTICAL** — 0 decision difference(s), 0 message difference(s), 0 result difference(s), 0 missing file(s).

Full evidence: `validation/results/fuzz-comparison.json`, `validation/results/fuzz-raw-oracle.json`, `validation/results/fuzz-raw-port.json`.

## 11. Findings

* Message text for `cycle_stream_length_self` differs only by embedded object addresses (normalised, not behavioural): resolve: `Detected loop with self reference for IndirectObject(6, 0, 4498514880).` vs `Detected loop with self reference for IndirectObject(6, 0, 4469518224).`
* Message text for `nested_arrays_d1000` differs only by embedded object addresses (normalised, not behavioural): pages: `Detected loop with self reference for IndirectObject(1, 0, 4419576768).` vs `Detected loop with self reference for IndirectObject(1, 0, 4416171920).`; depth: `Detected loop with self reference for IndirectObject(1, 0, 4419576768).` vs `Detected loop with self reference for IndirectObject(1, 0, 4416171920).`
* Message text for `nested_arrays_d10000` differs only by embedded object addresses (normalised, not behavioural): pages: `Detected loop with self reference for IndirectObject(1, 0, 4456014784).` vs `Detected loop with self reference for IndirectObject(1, 0, 4450627472).`; depth: `Detected loop with self reference for IndirectObject(1, 0, 4456014784).` vs `Detected loop with self reference for IndirectObject(1, 0, 4450627472).`
* Informational (non-blocking) value difference for `nested_dicts_d1000` — both sides silently truncate the over-deep structure at the interpreter recursion guard, with a different retained depth: depth.max_depth: oracle=490 port=398, depth.nodes: oracle=490 port=398.
* Informational (non-blocking) value difference for `nested_dicts_d10000` — both sides silently truncate the over-deep structure at the interpreter recursion guard, with a different retained depth: depth.max_depth: oracle=490 port=398, depth.nodes: oracle=490 port=398.
* Message text for `stream_length_huge` differs only by embedded object addresses (normalised, not behavioural): text: `Detected loop with self reference for IndirectObject(5, 0, 4421116864).` vs `Detected loop with self reference for IndirectObject(5, 0, 4442238864).`

## 12. Appendix: corpus inventory

Every case is regenerated deterministically on each run; the manifest (`validation/results/security-corpus-manifest.json`) is the machine-readable form of this table.

| Case | Category | Intent | Size | SHA-256 (first 16) | Steps |
|---|---|---|---|---|---|
| `nested_arrays_d10` | nesting | Deeply nested inline array (depth 10) reached through /Nest. | 641.0 B | `6b38a17db8b17862` | parse,pages,depth |
| `nested_arrays_d100` | nesting | Deeply nested inline array (depth 100) reached through /Nest. | 821.0 B | `1dd9f5a73a4d09a3` | parse,pages,depth |
| `nested_arrays_d1000` | nesting | Deeply nested inline array (depth 1000) reached through /Nest. | 2.6 KiB | `f833b770dc4f5387` | parse,pages,depth |
| `nested_arrays_d10000` | nesting | Deeply nested inline array (depth 10000) reached through /Nest. | 20.1 KiB | `072ac962ff238807` | parse,pages,depth |
| `nested_dicts_d10` | nesting | Deeply nested inline dictionaries (depth 10) reached through /Nest. | 711.0 B | `1af4e21cdd58101b` | parse,pages,depth |
| `nested_dicts_d100` | nesting | Deeply nested inline dictionaries (depth 100) reached through /Nest. | 1.5 KiB | `f68ba3754daf5947` | parse,pages,depth |
| `nested_dicts_d1000` | nesting | Deeply nested inline dictionaries (depth 1000) reached through /Nest. | 9.4 KiB | `265f6bbdb6555649` | parse,pages,depth |
| `nested_dicts_d10000` | nesting | Deeply nested inline dictionaries (depth 10000) reached through /Nest. | 88.5 KiB | `69b6881d47ca2645` | parse,pages,depth |
| `cycle_indirect_self` | cycles | Indirect object /Cycle resolves to a dictionary whose /Next points at itself. | 701.0 B | `3369ef0e71157ea0` | parse,pages,resolve,cycle |
| `cycle_indirect_two` | cycles | Two indirect objects reference each other through /Next (A->B->A). | 776.0 B | `5afb5a8d2a8c1a41` | parse,pages,resolve,cycle |
| `cycle_stream_length_self` | cycles | Stream declares /Length 6 0 R where object 6 is the stream itself. | 703.0 B | `a78a90548ea81d81` | parse,pages,resolve |
| `page_tree_cycle_self` | cycles | /Pages node lists itself in /Kids. | 613.0 B | `d6de033545720c80` | parse,pages,text |
| `page_tree_cycle_two` | cycles | Two /Pages nodes reference each other in /Kids (A->B->A). | 690.0 B | `c6546ba867b1133a` | parse,pages,text |
| `page_tree_depth_150` | cycles | Page tree nested 150 levels deep (limit is 100). | 14.4 KiB | `8caaa270b6423a2d` | parse,pages |
| `outline_cycle_self` | cycles | Outline items chain to themselves via /Next and /Prev. | 968.0 B | `8872331ef98e1644` | parse,pages,outline |
| `outline_self_parent` | cycles | Outline item is its own /Parent and points at itself in /Dest. | 893.0 B | `9975499dd41fa630` | parse,pages,outline |
| `outline_depth_170` | cycles | Outline nested 170 levels deep (limit is 100). | 18.3 KiB | `04c0b88760378791` | parse,pages,outline |
| `form_field_cycle_self` | cycles | AcroForm field lists itself in /Kids. | 715.0 B | `5b7201ac49a7b69b` | parse,pages,fields |
| `annotation_cycle` | cycles | Two annotations reference each other through /Popup and /Parent. | 844.0 B | `929a03205011962a` | parse,pages,annotations |
| `xref_offsets_shifted_1000` | xref | Every classic-xref object offset is 1000 bytes too large. | 613.0 B | `400e838666f6c1b5` | parse,pages,text |
| `xref_startxref_garbage` | xref | startxref points at a non-existent offset (999999999). | 619.0 B | `1bb1da41a87f3c0e` | parse,pages,text |
| `xref_table_truncated` | xref | File is cut inside the classic xref table; no trailer/startxref. | 613.0 B | `2019b8211ccdc2d8` | parse,pages,text |
| `xref_declared_size_huge` | xref | Classic xref declares 200000 entries in a file of a few hundred bytes. | 618.0 B | `c8584273f45f5cb2` | parse,pages |
| `xref_trailer_size_too_small` | xref | Trailer /Size is 3 while objects up to 5 exist. | 613.0 B | `90509d9f830d1c8e` | parse,pages,text |
| `xref_nonmonotonic_offsets` | xref | Object bodies are stored out of order, so xref offsets are non-monotonic. | 613.0 B | `121ed8152046ba47` | parse,pages,text |
| `xref_entry_offset_out_of_range` | xref | xref entry for object 5 points past the end of the file. | 613.0 B | `47d12e24fb214442` | parse,pages,text |
| `xref_negative_subsection_size` | xref | Classic xref subsection declares a negative entry count (0 -3); the parser desynchronises and reads an entry line as the next subsection size, which the declared-size guard then rejects. | 614.0 B | `2905f7a723d2cf2e` | parse,pages,text |
| `duplicate_object_numbers` | xref | Object 5 is defined twice; the xref points at a duplicate copy after %%EOF. | 734.0 B | `c7ac685ed19c2f91` | parse,pages,resolve |
| `xref_conflicting_entries` | xref | xref defines object 5 twice (main subsection plus an extra 5 1 subsection). | 707.0 B | `09bdb5c64a9d8920` | parse,pages,resolve |
| `indirect_ref_number_2_32` | xref | Indirect reference uses object number 2**32, beyond 32-bit and not in the xref. | 635.0 B | `520ad31b23a66fa0` | parse,pages,resolve |
| `stream_length_huge` | streams | Content stream declares /Length 999999999 (limit is 75 MB). | 589.0 B | `f3b7ded970903a1d` | parse,pages,resolve,text |
| `stream_length_huge_indirect` | streams | Stream /Length is an indirect reference to the number 999999999. | 630.0 B | `85daedb2b5fc2fc0` | parse,pages,resolve |
| `stream_length_negative` | streams | Stream declares /Length -1; data must be found by scanning for endstream. | 582.0 B | `2f914cba00b14fc4` | parse,pages,resolve |
| `stream_truncated` | streams | Stream declares 5000 bytes but the file ends after 5 bytes; no endstream. | 568.0 B | `1151f95da85316e4` | parse,pages,resolve |
| `stream_missing_endstream` | streams | Stream is terminated by endobj instead of endstream. | 579.0 B | `c77a36472603d21e` | parse,pages,resolve |
| `filter_chain_50_flate` | streams | Content stream declares a 50-entry /FlateDecode filter chain (limit is 16). | 1.2 KiB | `f0564e7dc59121e9` | parse,pages,resolve,text |
| `filter_chain_16_flate` | streams | Content stream declares exactly 16 nested /FlateDecode filters (at the limit). | 964.0 B | `a67ee4d66702c1cc` | parse,pages,resolve |
| `filter_unknown_name` | streams | Stream uses an unknown filter /FooBar. | 597.0 B | `828b8c45388aada6` | parse,pages,resolve |
| `inline_image_huge_dimensions` | streams | Inline image declares 100000x100000 pixels (sample buffer > 75 MB limit). | 647.0 B | `a66c9ba0b7b1cb0b` | parse,pages,operations |
| `inline_image_truncated` | streams | Inline image data ends abruptly without the EI operator. | 614.0 B | `8d6d462dff6d81a3` | parse,pages,operations |
| `structure_missing_eof` | structure | File has no %%EOF marker at all. | 607.0 B | `078fb3058ef1f0de` | parse,pages,text |
| `structure_eof_trailing_garbage` | structure | Bytes follow the %%EOF marker (incremental-update leftovers). | 634.0 B | `999f738db40b768e` | parse,pages,text |
| `structure_missing_header` | structure | File does not start with a %PDF- header. | 598.0 B | `96fc7accdb3beb9f` | parse,pages,text |
| `structure_empty_file` | structure | Zero-byte input file. | 0 | `e3b0c44298fc1c14` | parse,pages |
| `structure_header_only` | structure | File contains only a %PDF- header and an %%EOF marker. | 15.0 B | `1e7313ace78f0fb4` | parse,pages,text |
| `structure_trailer_no_root` | structure | Trailer has no /Root entry. | 601.0 B | `e8c11d2418cf660a` | parse,pages |
| `structure_root_not_dictionary` | structure | /Root points at the number 42 instead of a catalog dictionary. | 651.0 B | `a11af28598779943` | parse,pages |
| `structure_catalog_no_pages` | structure | Catalog dictionary has no /Pages entry. | 600.0 B | `23ac59591243c680` | parse,pages,text |
| `structure_page_count_lie` | structure | /Pages /Count claims 999999 pages but only one kid exists. | 618.0 B | `8e49ce951fa84ff0` | parse,pages,text |
| `bomb_flate_1mb` | resources | Flate stream inflates to 1 MiB from a few KiB (bounded, within limits). | 1.7 KiB | `1f7f64f886595a36` | parse,pages,resolve |
| `bomb_flate_60mb` | resources | Flate stream inflates to 60 MiB, just below the 75 MB output limit. | skipped (requires --heavy) | - | parse,pages,resolve |
| `bomb_flate_1gb` | resources | Flate stream inflates to 1 GiB from ~1 MiB of input; /Length is realistic. | skipped (requires --heavy) | - | parse,pages,resolve |
| `cmap_oversized_mapping` | oversized | ToUnicode CMap defines 100500 bfchar mappings (limit is 100000). | 1.4 MiB | `28f39242ccbc6b45` | parse,pages,text |
| `cmap_oversized_code_token` | oversized | ToUnicode bfchar uses a 34-hex-digit source code (limit is 16). | 945.0 B | `80808c9c9d01b706` | parse,pages,text |
| `page_labels_oversized_nums` | oversized | /PageLabels /Nums holds 100002 array elements (array limit is 100000). | 868.7 KiB | `3f1e27d09cf7e192` | parse,pages,labels |
| `page_label_huge_prefix` | oversized | Page label prefix string is 1 MB long. | 977.3 KiB | `93df31dcd6a70d64` | parse,pages,labels |
| `attachments_oversized_names_array` | oversized | EmbeddedFiles /Names array holds 100002 elements (array limit is 100000). | 733.3 KiB | `a156f3ae467db2bc` | parse,pages,attachments |
| `attachment_name_1mb` | oversized | Single embedded-file name string is 1 MB long. | 977.4 KiB | `cd6a3cc8ecca59f9` | parse,pages,attachments |
| `writer_page_parent_cycle` | structure | Page's /Parent points at the page itself; writer round-trip must not leave a broken file. | 613.0 B | `9f2ede5e6f4fc2b8` | parse,pages,write |

## 13. Reproduction

```bash
bash validation/run_security.sh              # security + fuzz
bash validation/run_security.sh --heavy      # includes decompression bombs
python validation/security.py --heavy --keep-corpus   # keep the corpus
python validation/security.py --cases '^(bomb|cmap)'  # subset by regex
python validation/fuzz.py --count 200 --seed 20261124
```
