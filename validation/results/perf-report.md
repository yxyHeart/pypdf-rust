# Performance comparison: reference pypdf 7.1.1 vs pypdf-rust port

- Generated: 2026-11-24 21:41:11 (local)
- Reference run: `reference` – 2026-11-24T21:40:52+0800 – `/Users/yxy/Desktop/pypdf-rust/.venvs/oracle/lib/python3.14/site-packages/pypdf/__init__.py`
- Port run: `rust` – 2026-11-24T21:41:11+0800 – `/Users/yxy/Desktop/pypdf-rust/python/pypdf/__init__.py`
- Repetitions per workload: reference 5 / port 5 (warm-up runs excluded: 1 / 1)
- Per-repetition timeout: 300.0 s
- PYTHONHASHSEED: reference '0' / port '0'
- Machine: arm64 / macOS-26.6.2-arm64-arm-64bit-Mach-O
- Data verdict: **PASS** (data integrity only: missing data / output-size equality; not a performance acceptance decision)

## Environment

| item | reference | port |
|---|---|---|
| backend label | reference | rust |
| Python | 3.14.7 | 3.14.7 |
| executable | /Users/yxy/Desktop/pypdf-rust/.venvs/oracle/bin/python | /Users/yxy/Desktop/pypdf-rust/.venvs/dev/bin/python |
| pypdf version | 7.1.1 | 7.1.1 |
| module | /Users/yxy/Desktop/pypdf-rust/.venvs/oracle/lib/python3.14/site-packages/pypdf/__init__.py | /Users/yxy/Desktop/pypdf-rust/python/pypdf/__init__.py |
| rust extension | None | /Users/yxy/Desktop/pypdf-rust/python/pypdf/_pypdf.cpython-314-darwin.so |
| platform | macOS-26.6.2-arm64-arm-64bit-Mach-O | macOS-26.6.2-arm64-arm-64bit-Mach-O |
| cpu count | 10 | 10 |
| loadavg (1/5/15 min) at start | 34.53 16.43 13.50 | 31.60 16.39 13.51 |
| loadavg (1/5/15 min) at end | 32.00 16.21 13.43 | 28.03 16.32 13.54 |
| shell uptime before | 21:40  up 12 days,  6:28, 2 users, load averages: 34.53 16.43 13.50 | 21:40  up 12 days,  6:28, 2 users, load averages: 32.00 16.21 13.43 |
| shell uptime after | 21:40  up 12 days,  6:28, 2 users, load averages: 32.00 16.21 13.43 | 21:41  up 12 days,  6:28, 2 users, load averages: 28.03 16.32 13.54 |
| extension sha256[:16] | — | 74a3905ae5332ca0 |
| started (local) | 2026-11-24T21:40:52+0800 | 2026-11-24T21:41:11+0800 |

## Commands

- Reference: `cd /Users/yxy/Desktop/pypdf-rust && PYTHONHASHSEED=0 /Users/yxy/Desktop/pypdf-rust/validation/perf/run.py --backend reference --repeat 5 --timeout 300 --out /Users/yxy/Desktop/pypdf-rust/validation/results/perf-reference.json`
- Port: `cd /Users/yxy/Desktop/pypdf-rust && PYTHONHASHSEED=0 /Users/yxy/Desktop/pypdf-rust/validation/perf/run.py --backend rust --repeat 5 --timeout 300 --out /Users/yxy/Desktop/pypdf-rust/validation/results/perf-rust.json`
- Comparison: described by `validation/perf/run_perf.sh`; the baseline sizes in `validation/perf/workloads.json` were measured with the reference run.

## Method

- Reference and port always run in two separate, sequential processes; each process measures exactly one backend, so imports, caches and allocator state cannot leak between sides.
- Every workload performs one warm-up repetition (discarded) followed by the counted repetitions.  The raw per-repetition wall time, CPU time, sampled peak RSS and output byte count are stored in the two result JSON files.
- `PYTHONHASHSEED=0` on both sides and `tools/oracle/prng.py::install_deterministic_secrets()` is called before every repetition, so written PDFs are reproducible; output sizes and sha256 digests are compared between sides and against the recorded reference baselines.
- Timed region: the workload operation only.  Summary serialization and hashing happen after the clock is stopped, so they do not inflate the measured latency.
- Failures and timeouts are recorded per repetition, counted, and listed; `compare.py` fails the run instead of silently dropping them.  A timeout is delivered with SIGALRM, so a repetition blocked inside native code that never returns to the interpreter can overshoot the wall-clock timeout (inherent limitation of in-process measurement).

## Summary

- Workloads compared: 8
- Port faster: 1; port slower: 7; no timing data: 0
- Output-size mismatches: 0
- Recorded failures/timeouts: 0
- File-size separation: small: W1, W5, W6, W7, large: W2, mixed: W3, W4, W8 (workloads with per-file data are additionally split into large/small files below)
- Machine load at start (1-min, 5-min, 15-min): reference 34.53 16.43 13.50, port 31.60 16.39 13.51 on 10 CPUs; a 1-min load above the CPU count means the machine was oversubscribed and the two runs did not see identical conditions.
- Measurement conditions: this is a shared macOS workstation with persistent system background load that cannot be quiesced (WindowServer, the DeepSeek Harness renderer, StorageManagement and similar system processes keep the 1-minute load average around 20+ on a 10-CPU machine).  Reference and port ran sequentially in separate processes under the same conditions; no repository builds or test suites were started by the validation team while the final measurement was running.

Metric definitions: `speedup = ref_median / port_median` (higher is better), `latency Δ = (port_median / ref_median - 1) × 100`, `RSS Δ = (port_peak_RSS / ref_peak_RSS - 1) × 100`. Peak RSS is the maximum sampled resident-set size of a counted repetition (sampled every ~1.5 ms while the repetition runs).

## Per-workload results

| workload | name | size class | files | ref median | port median | speedup | latency Δ | ref p95 | port p95 | ref peak RSS | port peak RSS | RSS Δ | output bytes ref/port | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| W1 | reader_init_20_small | small | 20 | 11.8 ms | 35.7 ms | 0.33× | +202.5% | 12.4 ms | 36.0 ms | 50.8 MB | 57.7 MB | +13.7% | 2.3 KiB / 2.3 KiB | ok |
| W2 | page_traverse_5_large | large | 5 | 173.2 ms | 581.5 ms | 0.30× | +235.8% | 198.2 ms | 785.2 ms | 163.5 MB | 178.0 MB | +8.9% | 46.1 KiB / 46.1 KiB | ok |
| W3 | text_extract_12 | mixed | 12 | 176.7 ms | 448.2 ms | 0.39× | +153.6% | 202.2 ms | 450.3 ms | 164.2 MB | 182.0 MB | +10.8% | 34.3 KiB / 34.3 KiB | ok |
| W4 | image_extract_10 | mixed | 10 | 599.5 ms | 427.2 ms | 1.40× | -28.7% | 610.1 ms | 441.7 ms | 187.7 MB | 234.2 MB | +24.8% | 1.55 MiB / 1.55 MiB | ok |
| W5 | merge_append_5 | small | 5 | 12.1 ms | 41.9 ms | 0.29× | +247.2% | 17.4 ms | 44.1 ms | 159.5 MB | 207.4 MB | +30.0% | 181.0 KiB / 181.0 KiB | ok |
| W6 | rotate_crop_merge_write_5 | small | 5 | 15.4 ms | 59.0 ms | 0.26× | +282.6% | 17.9 ms | 62.8 ms | 159.8 MB | 207.6 MB | +29.9% | 240.2 KiB / 240.2 KiB | ok |
| W7 | clone_write_10 | small | 10 | 20.2 ms | 74.9 ms | 0.27× | +270.1% | 21.7 ms | 75.7 ms | 160.2 MB | 207.8 MB | +29.7% | 291.0 KiB / 291.0 KiB | ok |
| W8 | end_to_end_mixed_7 | mixed | 7 | 437.3 ms | 1.272 s | 0.34× | +190.8% | 489.0 ms | 1.349 s | 233.9 MB | 273.8 MB | +17.1% | 5.91 MiB / 5.91 MiB | ok |

## Latency distribution (counted repetitions)

| workload | ref min | ref median | ref p95 | ref max | port min | port median | port p95 | port max |
|---|---|---|---|---|---|---|---|---|
| W1 | 11.7 ms | 11.8 ms | 12.4 ms | 12.5 ms | 34.9 ms | 35.7 ms | 36.0 ms | 36.0 ms |
| W2 | 169.1 ms | 173.2 ms | 198.2 ms | 202.9 ms | 565.5 ms | 581.5 ms | 785.2 ms | 814.8 ms |
| W3 | 176.0 ms | 176.7 ms | 202.2 ms | 208.4 ms | 439.7 ms | 448.2 ms | 450.3 ms | 450.6 ms |
| W4 | 585.4 ms | 599.5 ms | 610.1 ms | 611.2 ms | 419.8 ms | 427.2 ms | 441.7 ms | 442.9 ms |
| W5 | 11.0 ms | 12.1 ms | 17.4 ms | 18.8 ms | 40.1 ms | 41.9 ms | 44.1 ms | 44.5 ms |
| W6 | 15.3 ms | 15.4 ms | 17.9 ms | 18.5 ms | 55.3 ms | 59.0 ms | 62.8 ms | 63.6 ms |
| W7 | 19.5 ms | 20.2 ms | 21.7 ms | 21.9 ms | 72.6 ms | 74.9 ms | 75.7 ms | 75.7 ms |
| W8 | 378.5 ms | 437.3 ms | 489.0 ms | 497.0 ms | 1.262 s | 1.272 s | 1.349 s | 1.350 s |

## CPU time and peak memory

| workload | ref CPU median | port CPU median | ref peak RSS (max rep) | port peak RSS (max rep) | RSS Δ | ref working-set growth | port working-set growth | growth Δ | ref ru_maxrss | port ru_maxrss |
|---|---|---|---|---|---|---|---|---|---|---|
| W1 | 11.9 ms | 35.7 ms | 50.8 MB | 57.7 MB | +13.7% | 0.1 MB | 0.3 MB | +100.0% | 50.8 MB | 57.7 MB |
| W2 | 172.1 ms | 574.1 ms | 163.5 MB | 178.0 MB | +8.9% | 29.2 MB | 29.6 MB | +1.1% | 163.7 MB | 178.0 MB |
| W3 | 176.7 ms | 445.0 ms | 164.2 MB | 182.0 MB | +10.8% | 0.8 MB | 0.4 MB | -52.9% | 164.7 MB | 182.0 MB |
| W4 | 595.5 ms | 425.9 ms | 187.7 MB | 234.2 MB | +24.8% | 12.7 MB | 22.2 MB | +74.9% | 192.7 MB | 234.2 MB |
| W5 | 12.0 ms | 42.0 ms | 159.5 MB | 207.4 MB | +30.0% | 0.1 MB | 0.1 MB | -33.3% | 192.7 MB | 234.2 MB |
| W6 | 15.5 ms | 57.5 ms | 159.8 MB | 207.6 MB | +29.9% | 0.1 MB | 0.1 MB | -25.0% | 192.7 MB | 234.2 MB |
| W7 | 20.3 ms | 74.9 ms | 160.2 MB | 207.8 MB | +29.7% | 0.1 MB | 0.1 MB | +0.0% | 192.7 MB | 234.2 MB |
| W8 | 436.8 ms | 1.271 s | 233.9 MB | 273.8 MB | +17.1% | 17.6 MB | 24.4 MB | +38.9% | 233.9 MB | 273.8 MB |

Peak RSS is sampled current RSS, so it is *not* a process-lifetime high-water mark. It can still include pages that the allocator retained from earlier workloads; the working-set growth column (peak RSS minus RSS at repetition start) removes that bias. `ru_maxrss` is the process high-water mark and is shown for reference only.

## Output-size check

| workload | output kind | recorded baseline | ref bytes | port bytes | ref sha256[:16] | port sha256[:16] | equal |
|---|---|---|---|---|---|---|---|
| W1 | derived_summary | 2,389 B | 2,389 B | 2,389 B | 178c986748074a4e | 178c986748074a4e | yes |
| W2 | derived_summary | 47,166 B | 47,166 B | 47,166 B | 8c0126b43c5c8e17 | 8c0126b43c5c8e17 | yes |
| W3 | extracted_text | 35,165 B | 35,165 B | 35,165 B | b185f13139934de8 | b185f13139934de8 | yes |
| W4 | image_bytes | 1,620,399 B | 1,620,399 B | 1,620,399 B | 527c68f2492812d7 | 527c68f2492812d7 | yes |
| W5 | pdf_bytes | 185,316 B | 185,316 B | 185,316 B | 14f9f869a1335192 | 14f9f869a1335192 | yes |
| W6 | pdf_bytes | 245,923 B | 245,923 B | 245,923 B | 98412243edeaaa0f | 98412243edeaaa0f | yes |
| W7 | pdf_bytes | 298,006 B | 298,006 B | 298,006 B | 1ba70bae93b510d9 | 1ba70bae93b510d9 | yes |
| W8 | pdf_bytes | 6,194,871 B | 6,194,871 B | 6,194,871 B | 843007b176d52368 | 843007b176d52368 | yes |

Recorded baselines come from `validation/perf/workloads.json`; they were captured from the reference implementation on the same corpus. Equality of the `ref`/`port` columns is the pass criterion; a drift of the reference against the recorded baseline is reported as a warning.

## Success and failure counts

| workload | ref ok/total | port ok/total | ref timeouts | port timeouts | ref failures | port failures |
|---|---|---|---|---|---|---|
| W1 | 5/5 | 5/5 | 0 | 0 | — | — |
| W2 | 5/5 | 5/5 | 0 | 0 | — | — |
| W3 | 5/5 | 5/5 | 0 | 0 | — | — |
| W4 | 5/5 | 5/5 | 0 | 0 | — | — |
| W5 | 5/5 | 5/5 | 0 | 0 | — | — |
| W6 | 5/5 | 5/5 | 0 | 0 | — | — |
| W7 | 5/5 | 5/5 | 0 | 0 | — | — |
| W8 | 5/5 | 5/5 | 0 | 0 | — | — |

No failures or timeouts were recorded.

## Per-file latency

The reader/text/image workloads time each input file separately inside every repetition (the timed region is identical for both backends). Files at or above 244.1 KiB are reported as *large*, the rest as *small*. For W8 the per-file time covers open + text extraction + append for that file; the shared writer serialization is only included in the workload total.

### W1 – reader_init_20_small

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 1.14 ms | 3.82 ms | 0.30× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 1.08 ms | 0.75 ms | 1.44× |
| resources/hello-world.pdf | 28.7 KiB | small | 0.36 ms | 1.10 ms | 0.33× |
| resources/multilang.pdf | 28.7 KiB | small | 0.33 ms | 1.23 ms | 0.27× |
| resources/pdflatex-forms.pdf | 27.1 KiB | small | 1.51 ms | 6.15 ms | 0.25× |
| resources/commented-xmp.pdf | 17.5 KiB | small | 0.30 ms | 1.10 ms | 0.27× |
| resources/commented.pdf | 13.1 KiB | small | 1.05 ms | 4.15 ms | 0.25× |
| resources/metadata.pdf | 13.0 KiB | small | 0.75 ms | 2.64 ms | 0.28× |
| resources/crazyones.pdf | 11.2 KiB | small | 1.16 ms | 3.34 ms | 0.35× |
| resources/two-different-pages.pdf | 7.5 KiB | small | 0.34 ms | 1.28 ms | 0.27× |
| resources/box.pdf | 5.3 KiB | small | 0.21 ms | 0.68 ms | 0.31× |
| resources/AutoCad_Simple.pdf | 2.9 KiB | small | 0.42 ms | 1.34 ms | 0.32× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.41 ms | 0.88 ms | 0.47× |
| resources/imagemagick-lzw.pdf | 2.6 KiB | small | 0.33 ms | 0.91 ms | 0.36× |
| resources/imagemagick-CCITTFaxDecode.pdf | 1.8 KiB | small | 0.29 ms | 0.81 ms | 0.36× |
| resources/reportlab-inline-image.pdf | 1.5 KiB | small | 0.27 ms | 0.79 ms | 0.34× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.20 ms | 0.64 ms | 0.31× |
| resources/toy.pdf | 1.1 KiB | small | 0.18 ms | 0.61 ms | 0.30× |
| resources/bytes.pdf | 1.1 KiB | small | 0.44 ms | 1.34 ms | 0.33× |
| resources/issue-297.pdf | 958 B | small | 0.38 ms | 1.15 ms | 0.33× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| small | 20 | 11.16 ms | 34.71 ms | 0.32× |

### W2 – page_traverse_5_large

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 66.62 ms | 183.82 ms | 0.36× |
| sample-files/009-pdflatex-geotopo/GeoTopo.pdf | 5.07 MiB | large | 73.13 ms | 286.71 ms | 0.26× |
| resources/fontsampler.pdf | 3.17 MiB | large | 0.79 ms | 1.92 ms | 0.41× |
| sample-files/018-base64-image/base64image.pdf | 2.16 MiB | large | 0.60 ms | 1.54 ms | 0.39× |
| sample-files/009-pdflatex-geotopo/GeoTopo-komprimiert.pdf | 1.71 MiB | large | 29.46 ms | 102.59 ms | 0.29× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 5 | 170.60 ms | 576.59 ms | 0.30× |

### W3 – text_extract_12

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 36.71 ms | 78.48 ms | 0.47× |
| resources/test Orient.pdf | 276.4 KiB | large | 36.33 ms | 31.79 ms | 1.14× |
| sample-files/026-latex-multicolumn/multicolumn.pdf | 76.8 KiB | small | 20.01 ms | 66.35 ms | 0.30× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 27.60 ms | 90.89 ms | 0.30× |
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 2.35 ms | 6.66 ms | 0.35× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 2.85 ms | 5.62 ms | 0.51× |
| resources/hello-world.pdf | 28.7 KiB | small | 2.49 ms | 6.75 ms | 0.37× |
| resources/multilang.pdf | 28.7 KiB | small | 2.63 ms | 6.23 ms | 0.42× |
| sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf | 24.0 KiB | small | 40.36 ms | 135.10 ms | 0.30× |
| resources/crazyones.pdf | 11.2 KiB | small | 3.98 ms | 13.08 ms | 0.30× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.66 ms | 1.42 ms | 0.46× |
| resources/toy.pdf | 1.1 KiB | small | 0.92 ms | 1.76 ms | 0.52× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 2 | 73.04 ms | 110.26 ms | 0.66× |
| small | 10 | 103.86 ms | 333.87 ms | 0.31× |

### W4 – image_extract_10

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| sample-files/023-cmyk-image/cmyk-image.pdf | 433.5 KiB | large | 43.43 ms | 43.29 ms | 1.00× |
| sample-files/031-png-predictors/png-predictors.pdf | 172.7 KiB | small | 17.10 ms | 6.21 ms | 2.75× |
| resources/issue-301.pdf | 119.6 KiB | small | 464.60 ms | 293.51 ms | 1.58× |
| resources/jpeg.pdf | 98.5 KiB | small | 55.07 ms | 56.32 ms | 0.98× |
| sample-files/019-grayscale-image/grayscale-image.pdf | 39.2 KiB | small | 2.56 ms | 3.07 ms | 0.83× |
| resources/git.pdf | 35.5 KiB | small | 8.74 ms | 10.24 ms | 0.85× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 3.27 ms | 6.85 ms | 0.48× |
| resources/box.pdf | 5.3 KiB | small | 2.43 ms | 3.88 ms | 0.63× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.73 ms | 1.46 ms | 0.50× |
| sample-files/008-reportlab-inline-image/inline-image.pdf | 1.5 KiB | small | 0.59 ms | 1.21 ms | 0.48× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 1 | 43.43 ms | 43.29 ms | 1.00× |
| small | 9 | 555.07 ms | 382.76 ms | 1.45× |

### W8 – end_to_end_mixed_7

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 268.79 ms | 837.91 ms | 0.32× |
| resources/side-by-side-subfig.pdf | 811.7 KiB | large | 5.78 ms | 16.69 ms | 0.35× |
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 21.57 ms | 63.89 ms | 0.34× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 14.87 ms | 48.11 ms | 0.31× |
| resources/multilang.pdf | 28.7 KiB | small | 2.74 ms | 7.74 ms | 0.35× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.85 ms | 8.97 ms | 0.32× |
| resources/crazyones.pdf | 11.2 KiB | small | 4.37 ms | 14.77 ms | 0.30× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 3 | 296.15 ms | 918.49 ms | 0.32× |
| small | 4 | 24.83 ms | 79.59 ms | 0.31× |

## Regressions and anomalies

- **W1** port is slower: speedup 0.33×, latency change +202.5% (11.8 ms → 35.7 ms).
- **W2** port is slower: speedup 0.30×, latency change +235.8% (173.2 ms → 581.5 ms).
- **W3** port is slower: speedup 0.39×, latency change +153.6% (176.7 ms → 448.2 ms).
- **W4** port working-set growth is larger: 12.7 MB → 22.2 MB (+74.9%); absolute peak RSS 187.7 MB → 234.2 MB (+24.8%).
- **W5** port is slower: speedup 0.29×, latency change +247.2% (12.1 ms → 41.9 ms).
- **W6** port is slower: speedup 0.26×, latency change +282.6% (15.4 ms → 59.0 ms).
- **W7** port is slower: speedup 0.27×, latency change +270.1% (20.2 ms → 74.9 ms).
- **W8** port is slower: speedup 0.34×, latency change +190.8% (437.3 ms → 1.272 s).
- **W8** port working-set growth is larger: 17.6 MB → 24.4 MB (+38.9%); absolute peak RSS 233.9 MB → 273.8 MB (+17.1%).

## Workload definitions

| id | name | operation | size class | output kind | parameters | files |
|---|---|---|---|---|---|---|
| W1 | reader_init_20_small | reader_init | small | derived_summary | metadata_fields | 20 |
| W2 | page_traverse_5_large | page_traverse | large | derived_summary | boxes, include_rotation, max_pages | 5 |
| W3 | text_extract_12 | text_extract | mixed | extracted_text | max_pages | 12 |
| W4 | image_extract_10 | image_extract | mixed | image_bytes | decode_with_pillow, max_pages | 10 |
| W5 | merge_append_5 | merge_append | small | pdf_bytes | — | 5 |
| W6 | rotate_crop_merge_write_5 | page_ops_write | small | pdf_bytes | crop_lower_left, crop_upper_right, rotate_cycle | 5 |
| W7 | clone_write_10 | clone_write | small | pdf_bytes | — | 10 |
| W8 | end_to_end_mixed_7 | end_to_end | mixed | pdf_bytes | text_pages | 7 |

- **W1** Open 20 small PDFs, read the page count and selected document-information fields. Baseline for the read/parse path.
  - files: resources/AutoCad_Simple.pdf, resources/Sample_Td-matrix.pdf, resources/crazyones.pdf, resources/bytes.pdf, resources/box.pdf, resources/commented.pdf, resources/commented-xmp.pdf, resources/issue-297.pdf, resources/hello-world.pdf, resources/toy.pdf, resources/multilang.pdf, resources/metadata.pdf, resources/two-different-pages.pdf, resources/imagemagick-ASCII85Decode.pdf, resources/imagemagick-lzw.pdf, resources/imagemagick-CCITTFaxDecode.pdf, resources/reportlab-inline-image.pdf, resources/selenium-pypdf-issue-177.pdf, resources/superscript-pdftex.pdf, resources/pdflatex-forms.pdf
- **W2** Open 5 large PDFs and walk every page of the page tree, touching all five page boxes and the rotation attribute. Exercises page-tree traversal and object caching on large documents.
  - files: resources/issue-604.pdf, sample-files/009-pdflatex-geotopo/GeoTopo.pdf, sample-files/009-pdflatex-geotopo/GeoTopo-komprimiert.pdf, resources/fontsampler.pdf, sample-files/018-base64-image/base64image.pdf
- **W3** Extract text from up to 5 pages of 12 text-bearing PDFs (small and large mixed) and concatenate it. Exercises content-stream parsing, fonts and CMaps.
  - files: resources/crazyones.pdf, resources/multilang.pdf, resources/hello-world.pdf, resources/toy.pdf, resources/Sample_Td-matrix.pdf, resources/superscript-pdftex.pdf, resources/selenium-pypdf-issue-177.pdf, sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf, resources/pdflatex-outline.pdf, sample-files/026-latex-multicolumn/multicolumn.pdf, resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf, resources/test Orient.pdf
- **W4** Enumerate and extract every image of 10 image-bearing PDFs: read the raw image bytes and decode them through Pillow, recording name, byte length, pixel size and mode.
  - files: resources/imagemagick-images.pdf, resources/jpeg.pdf, resources/box.pdf, resources/issue-301.pdf, sample-files/031-png-predictors/png-predictors.pdf, resources/imagemagick-ASCII85Decode.pdf, sample-files/008-reportlab-inline-image/inline-image.pdf, sample-files/019-grayscale-image/grayscale-image.pdf, resources/git.pdf, sample-files/023-cmyk-image/cmyk-image.pdf
- **W5** Merge 5 small multi-page PDFs with PdfWriter.append in one writer and serialize the result into memory. Exercises object-graph copying and the writer.
  - files: resources/two-different-pages.pdf, sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf, resources/pdflatex-outline.pdf, sample-files/015-arabic/habibi-rotated.pdf, sample-files/026-latex-multicolumn/multicolumn.pdf
- **W6** Clone the pages of 5 small PDFs into a writer, rotate each page by a 90/180/270 cycle, replace the cropbox, then serialize the merged document.
  - files: resources/FormTestFromOo.pdf, resources/attachment.pdf, resources/form.pdf, resources/commented.pdf, resources/outline-without-title.pdf
- **W7** Clone every page of a fixed 10-PDF set (20 pages) into a fresh PdfWriter and write it. Serialization/xref baseline for a typical archive rebuild.
  - files: sample-files/001-trivial/minimal-document.pdf, sample-files/002-trivial-libre-office-writer/002-trivial-libre-office-writer.pdf, sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf, sample-files/006-pdflatex-outline/pdflatex-outline.pdf, sample-files/011-google-doc-document/google-doc-document.pdf, sample-files/013-reportlab-overlay/reportlab-overlay.pdf, sample-files/014-outlines/mistitled_outlines_example.pdf, sample-files/015-arabic/habibi-rotated.pdf, sample-files/016-libre-office-link/libre-office-link.pdf, sample-files/024-annotations/annotated_pdf.pdf
- **W8** Full business workflow on a mixed 7-PDF set: open each file, extract text from the first pages, merge all pages with PdfWriter.append, write the result and re-open it to verify the page count.
  - files: resources/issue-604.pdf, resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf, resources/side-by-side-subfig.pdf, resources/crazyones.pdf, resources/multilang.pdf, resources/imagemagick-images.pdf, resources/pdflatex-outline.pdf

---

Raw per-repetition data: `validation/results/perf-reference.json`, `validation/results/perf-rust.json`. Logs: `validation/results/logs/`.

