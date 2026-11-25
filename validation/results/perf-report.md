# Performance comparison: reference pypdf 7.1.1 vs pypdf-rust port

- Generated: 2026-11-25 11:50:01 (local)
- Reference run: `reference` – 2026-11-25T11:49:50+0800 – `/Users/yxy/Desktop/pypdf-rust/.venvs/oracle/lib/python3.14/site-packages/pypdf/__init__.py`
- Port run: `rust` – 2026-11-25T11:50:00+0800 – `/Users/yxy/Desktop/pypdf-rust/python/pypdf/__init__.py`
- Repetitions per workload: reference 7 / port 7 (warm-up runs excluded: 1 / 1)
- Per-repetition timeout: 300.0 s
- PYTHONHASHSEED: reference '0' / port '0'
- Machine: arm64 / macOS-27.2.1-arm64-arm-64bit-Mach-O
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
| platform | macOS-27.2.1-arm64-arm-64bit-Mach-O | macOS-27.2.1-arm64-arm-64bit-Mach-O |
| cpu count | 10 | 10 |
| loadavg (1/5/15 min) at start | 8.97 7.56 7.21 | 8.43 7.49 7.19 |
| loadavg (1/5/15 min) at end | 8.43 7.49 7.19 | 8.06 7.44 7.17 |
| shell uptime before | 11:49  up 11:59, 2 users, load averages: 8.97 7.56 7.21 | 11:49  up 11:59, 2 users, load averages: 8.43 7.49 7.19 |
| shell uptime after | 11:49  up 11:59, 2 users, load averages: 8.43 7.49 7.19 | 11:50  up 11:59, 2 users, load averages: 8.06 7.44 7.17 |
| extension sha256[:16] | — | 917f8eb9cf9b710e |
| started (local) | 2026-11-25T11:49:50+0800 | 2026-11-25T11:50:00+0800 |

## Commands

- Reference: `cd /Users/yxy/Desktop/pypdf-rust && PYTHONHASHSEED=0 /Users/yxy/Desktop/pypdf-rust/validation/perf/run.py --backend reference --repeat 7 --timeout 300 --out /Users/yxy/Desktop/pypdf-rust/validation/results/perf-reference.json`
- Port: `cd /Users/yxy/Desktop/pypdf-rust && PYTHONHASHSEED=0 /Users/yxy/Desktop/pypdf-rust/validation/perf/run.py --backend rust --repeat 7 --timeout 300 --out /Users/yxy/Desktop/pypdf-rust/validation/results/perf-rust.json`
- Comparison: described by `validation/perf/run_perf.sh`; the baseline sizes in `validation/perf/workloads.json` were measured with the reference run.

## Method

- Reference and port always run in two separate, sequential processes; each process measures exactly one backend, so imports, caches and allocator state cannot leak between sides.
- Every workload performs one warm-up repetition (discarded) followed by the counted repetitions.  The raw per-repetition wall time, CPU time, sampled peak RSS and output byte count are stored in the two result JSON files.
- `PYTHONHASHSEED=0` on both sides and `tools/oracle/prng.py::install_deterministic_secrets()` is called before every repetition, so written PDFs are reproducible; output sizes and sha256 digests are compared between sides and against the recorded reference baselines.
- Timed region: the workload operation only.  Summary serialization and hashing happen after the clock is stopped, so they do not inflate the measured latency.
- Failures and timeouts are recorded per repetition, counted, and listed; `compare.py` fails the run instead of silently dropping them.  A timeout is delivered with SIGALRM, so a repetition blocked inside native code that never returns to the interpreter can overshoot the wall-clock timeout (inherent limitation of in-process measurement).

## Summary

- Workloads compared: 8
- Port faster: 3; port slower: 5; no timing data: 0
- Output-size mismatches: 0
- Recorded failures/timeouts: 0
- File-size separation: small: W1, W5, W6, W7, large: W2, mixed: W3, W4, W8 (workloads with per-file data are additionally split into large/small files below)
- Machine load at start (1-min, 5-min, 15-min): reference 8.97 7.56 7.21, port 8.43 7.49 7.19 on 10 CPUs; a 1-min load above the CPU count means the machine was oversubscribed and the two runs did not see identical conditions.
- Measurement conditions: this is a shared macOS workstation with persistent system background load that cannot be quiesced (WindowServer, the DeepSeek Harness renderer, StorageManagement and similar system processes keep the 1-minute load average around 20+ on a 10-CPU machine).  Reference and port ran sequentially in separate processes under the same conditions; no repository builds or test suites were started by the validation team while the final measurement was running.

Metric definitions: `speedup = ref_median / port_median` (higher is better), `latency Δ = (port_median / ref_median - 1) × 100`, `RSS Δ = (port_peak_RSS / ref_peak_RSS - 1) × 100`. Peak RSS is the maximum sampled resident-set size of a counted repetition (sampled every ~1.5 ms while the repetition runs).

## Per-workload results

| workload | name | size class | files | ref median | port median | speedup | latency Δ | ref p95 | port p95 | ref peak RSS | port peak RSS | RSS Δ | output bytes ref/port | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| W1 | reader_init_20_small | small | 20 | 11.2 ms | 10.2 ms | 1.11× | -9.6% | 11.8 ms | 11.0 ms | 50.2 MB | 55.5 MB | +10.4% | 2.3 KiB / 2.3 KiB | ok |
| W2 | page_traverse_5_large | large | 5 | 165.0 ms | 131.1 ms | 1.26× | -20.6% | 215.8 ms | 188.1 ms | 191.6 MB | 178.7 MB | -6.7% | 46.1 KiB / 46.1 KiB | ok |
| W3 | text_extract_12 | mixed | 12 | 171.1 ms | 217.8 ms | 0.79× | +27.3% | 194.3 ms | 245.2 ms | 192.9 MB | 183.7 MB | -4.7% | 34.3 KiB / 34.3 KiB | ok |
| W4 | image_extract_10 | mixed | 10 | 581.1 ms | 314.9 ms | 1.85× | -45.8% | 604.0 ms | 324.6 ms | 234.9 MB | 240.1 MB | +2.2% | 1.55 MiB / 1.55 MiB | ok |
| W5 | merge_append_5 | small | 5 | 11.3 ms | 17.0 ms | 0.67× | +50.3% | 13.1 ms | 19.6 ms | 235.2 MB | 240.7 MB | +2.4% | 181.0 KiB / 181.0 KiB | ok |
| W6 | rotate_crop_merge_write_5 | small | 5 | 14.7 ms | 20.9 ms | 0.70× | +42.6% | 17.2 ms | 24.1 ms | 235.5 MB | 241.1 MB | +2.4% | 240.2 KiB / 240.2 KiB | ok |
| W7 | clone_write_10 | small | 10 | 19.4 ms | 30.3 ms | 0.64× | +55.9% | 21.0 ms | 46.3 ms | 236.6 MB | 241.9 MB | +2.3% | 291.0 KiB / 291.0 KiB | ok |
| W8 | end_to_end_mixed_7 | mixed | 7 | 416.0 ms | 464.9 ms | 0.89× | +11.7% | 462.6 ms | 505.5 ms | 309.9 MB | 338.6 MB | +9.3% | 5.91 MiB / 5.91 MiB | ok |

## Latency distribution (counted repetitions)

| workload | ref min | ref median | ref p95 | ref max | port min | port median | port p95 | port max |
|---|---|---|---|---|---|---|---|---|
| W1 | 11.1 ms | 11.2 ms | 11.8 ms | 12.0 ms | 10.0 ms | 10.2 ms | 11.0 ms | 11.3 ms |
| W2 | 163.3 ms | 165.0 ms | 215.8 ms | 225.4 ms | 130.7 ms | 131.1 ms | 188.1 ms | 198.2 ms |
| W3 | 170.0 ms | 171.1 ms | 194.3 ms | 203.9 ms | 213.3 ms | 217.8 ms | 245.2 ms | 256.1 ms |
| W4 | 573.6 ms | 581.1 ms | 604.0 ms | 605.7 ms | 302.0 ms | 314.9 ms | 324.6 ms | 325.0 ms |
| W5 | 11.2 ms | 11.3 ms | 13.1 ms | 13.8 ms | 16.3 ms | 17.0 ms | 19.6 ms | 20.2 ms |
| W6 | 14.2 ms | 14.7 ms | 17.2 ms | 17.3 ms | 20.2 ms | 20.9 ms | 24.1 ms | 24.4 ms |
| W7 | 19.2 ms | 19.4 ms | 21.0 ms | 21.0 ms | 26.7 ms | 30.3 ms | 46.3 ms | 53.0 ms |
| W8 | 362.2 ms | 416.0 ms | 462.6 ms | 469.1 ms | 397.6 ms | 464.9 ms | 505.5 ms | 508.3 ms |

## CPU time and peak memory

| workload | ref CPU median | port CPU median | ref peak RSS (max rep) | port peak RSS (max rep) | RSS Δ | ref working-set growth | port working-set growth | growth Δ | ref ru_maxrss | port ru_maxrss |
|---|---|---|---|---|---|---|---|---|---|---|
| W1 | 11.4 ms | 10.3 ms | 50.2 MB | 55.5 MB | +10.4% | 0.2 MB | 0.2 MB | +36.4% | 50.2 MB | 55.5 MB |
| W2 | 165.0 ms | 131.3 ms | 191.6 MB | 178.7 MB | -6.7% | 29.1 MB | 25.8 MB | -11.5% | 191.9 MB | 178.7 MB |
| W3 | 171.3 ms | 217.8 ms | 192.9 MB | 183.7 MB | -4.7% | 0.4 MB | 0.9 MB | +130.4% | 192.9 MB | 183.7 MB |
| W4 | 581.2 ms | 315.3 ms | 234.9 MB | 240.1 MB | +2.2% | 11.7 MB | 14.7 MB | +26.2% | 234.9 MB | 240.1 MB |
| W5 | 11.4 ms | 17.1 ms | 235.2 MB | 240.7 MB | +2.4% | 0.1 MB | 0.1 MB | -42.9% | 235.2 MB | 240.7 MB |
| W6 | 14.8 ms | 21.0 ms | 235.5 MB | 241.1 MB | +2.4% | 0.1 MB | 0.1 MB | +14.3% | 235.5 MB | 241.1 MB |
| W7 | 19.5 ms | 30.4 ms | 236.6 MB | 241.9 MB | +2.3% | 0.3 MB | 0.2 MB | -16.7% | 236.6 MB | 241.9 MB |
| W8 | 415.9 ms | 464.7 ms | 309.9 MB | 338.6 MB | +9.3% | 14.9 MB | 18.0 MB | +20.5% | 309.9 MB | 338.6 MB |

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
| W1 | 7/7 | 7/7 | 0 | 0 | — | — |
| W2 | 7/7 | 7/7 | 0 | 0 | — | — |
| W3 | 7/7 | 7/7 | 0 | 0 | — | — |
| W4 | 7/7 | 7/7 | 0 | 0 | — | — |
| W5 | 7/7 | 7/7 | 0 | 0 | — | — |
| W6 | 7/7 | 7/7 | 0 | 0 | — | — |
| W7 | 7/7 | 7/7 | 0 | 0 | — | — |
| W8 | 7/7 | 7/7 | 0 | 0 | — | — |

No failures or timeouts were recorded.

## Per-file latency

The reader/text/image workloads time each input file separately inside every repetition (the timed region is identical for both backends). Files at or above 244.1 KiB are reported as *large*, the rest as *small*. For W8 the per-file time covers open + text extraction + append for that file; the shared writer serialization is only included in the workload total.

### W1 – reader_init_20_small

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 1.00 ms | 0.85 ms | 1.17× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 1.04 ms | 0.28 ms | 3.74× |
| resources/hello-world.pdf | 28.7 KiB | small | 0.33 ms | 0.35 ms | 0.94× |
| resources/multilang.pdf | 28.7 KiB | small | 0.31 ms | 0.29 ms | 1.06× |
| resources/pdflatex-forms.pdf | 27.1 KiB | small | 1.46 ms | 1.44 ms | 1.02× |
| resources/commented-xmp.pdf | 17.5 KiB | small | 0.28 ms | 0.34 ms | 0.82× |
| resources/commented.pdf | 13.1 KiB | small | 1.01 ms | 1.01 ms | 1.00× |
| resources/metadata.pdf | 13.0 KiB | small | 0.75 ms | 0.72 ms | 1.05× |
| resources/crazyones.pdf | 11.2 KiB | small | 1.11 ms | 0.80 ms | 1.39× |
| resources/two-different-pages.pdf | 7.5 KiB | small | 0.30 ms | 0.32 ms | 0.95× |
| resources/box.pdf | 5.3 KiB | small | 0.21 ms | 0.23 ms | 0.89× |
| resources/AutoCad_Simple.pdf | 2.9 KiB | small | 0.35 ms | 0.31 ms | 1.14× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.28 ms | 0.25 ms | 1.14× |
| resources/imagemagick-lzw.pdf | 2.6 KiB | small | 0.26 ms | 0.24 ms | 1.10× |
| resources/imagemagick-CCITTFaxDecode.pdf | 1.8 KiB | small | 0.26 ms | 0.23 ms | 1.17× |
| resources/reportlab-inline-image.pdf | 1.5 KiB | small | 0.27 ms | 0.26 ms | 1.02× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.19 ms | 0.19 ms | 0.97× |
| resources/toy.pdf | 1.1 KiB | small | 0.17 ms | 0.20 ms | 0.88× |
| resources/bytes.pdf | 1.1 KiB | small | 0.44 ms | 0.53 ms | 0.82× |
| resources/issue-297.pdf | 958 B | small | 0.34 ms | 0.32 ms | 1.06× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| small | 20 | 10.36 ms | 9.14 ms | 1.13× |

### W2 – page_traverse_5_large

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 62.94 ms | 42.47 ms | 1.48× |
| sample-files/009-pdflatex-geotopo/GeoTopo.pdf | 5.07 MiB | large | 70.85 ms | 63.97 ms | 1.11× |
| resources/fontsampler.pdf | 3.17 MiB | large | 0.82 ms | 1.02 ms | 0.81× |
| sample-files/018-base64-image/base64image.pdf | 2.16 MiB | large | 0.66 ms | 0.71 ms | 0.93× |
| sample-files/009-pdflatex-geotopo/GeoTopo-komprimiert.pdf | 1.71 MiB | large | 29.25 ms | 23.31 ms | 1.25× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 5 | 164.52 ms | 131.47 ms | 1.25× |

### W3 – text_extract_12

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 34.74 ms | 35.90 ms | 0.97× |
| resources/test Orient.pdf | 276.4 KiB | large | 34.91 ms | 17.55 ms | 1.99× |
| sample-files/026-latex-multicolumn/multicolumn.pdf | 76.8 KiB | small | 19.74 ms | 32.05 ms | 0.62× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 27.29 ms | 44.37 ms | 0.61× |
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 2.17 ms | 2.64 ms | 0.82× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 2.72 ms | 2.34 ms | 1.16× |
| resources/hello-world.pdf | 28.7 KiB | small | 2.44 ms | 3.14 ms | 0.77× |
| resources/multilang.pdf | 28.7 KiB | small | 2.43 ms | 3.06 ms | 0.79× |
| sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf | 24.0 KiB | small | 39.25 ms | 68.03 ms | 0.58× |
| resources/crazyones.pdf | 11.2 KiB | small | 3.84 ms | 5.86 ms | 0.66× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.58 ms | 0.66 ms | 0.88× |
| resources/toy.pdf | 1.1 KiB | small | 0.74 ms | 0.99 ms | 0.75× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 2 | 69.65 ms | 53.45 ms | 1.30× |
| small | 10 | 101.21 ms | 163.16 ms | 0.62× |

### W4 – image_extract_10

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| sample-files/023-cmyk-image/cmyk-image.pdf | 433.5 KiB | large | 42.52 ms | 41.94 ms | 1.01× |
| sample-files/031-png-predictors/png-predictors.pdf | 172.7 KiB | small | 16.78 ms | 5.15 ms | 3.26× |
| resources/issue-301.pdf | 119.6 KiB | small | 450.34 ms | 192.34 ms | 2.34× |
| resources/jpeg.pdf | 98.5 KiB | small | 54.28 ms | 54.94 ms | 0.99× |
| sample-files/019-grayscale-image/grayscale-image.pdf | 39.2 KiB | small | 2.46 ms | 2.52 ms | 0.98× |
| resources/git.pdf | 35.5 KiB | small | 8.43 ms | 8.69 ms | 0.97× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.83 ms | 3.79 ms | 0.75× |
| resources/box.pdf | 5.3 KiB | small | 2.38 ms | 2.74 ms | 0.87× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.64 ms | 0.68 ms | 0.94× |
| sample-files/008-reportlab-inline-image/inline-image.pdf | 1.5 KiB | small | 0.57 ms | 0.67 ms | 0.86× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 1 | 42.52 ms | 41.94 ms | 1.01× |
| small | 9 | 538.71 ms | 271.51 ms | 1.98× |

### W8 – end_to_end_mixed_7

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 221.45 ms | 271.19 ms | 0.82× |
| resources/side-by-side-subfig.pdf | 811.7 KiB | large | 5.64 ms | 8.61 ms | 0.65× |
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 21.46 ms | 23.78 ms | 0.90× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 14.42 ms | 23.88 ms | 0.60× |
| resources/multilang.pdf | 28.7 KiB | small | 2.60 ms | 3.73 ms | 0.70× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.84 ms | 4.30 ms | 0.66× |
| resources/crazyones.pdf | 11.2 KiB | small | 4.26 ms | 6.86 ms | 0.62× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 3 | 248.55 ms | 303.58 ms | 0.82× |
| small | 4 | 24.12 ms | 38.76 ms | 0.62× |

## Regressions and anomalies

- **W3** port is slower: speedup 0.79×, latency change +27.3% (171.1 ms → 217.8 ms).
- **W4** port working-set growth is larger: 11.7 MB → 14.7 MB (+26.2%); absolute peak RSS 234.9 MB → 240.1 MB (+2.2%).
- **W5** port is slower: speedup 0.67×, latency change +50.3% (11.3 ms → 17.0 ms).
- **W6** port is slower: speedup 0.70×, latency change +42.6% (14.7 ms → 20.9 ms).
- **W7** port is slower: speedup 0.64×, latency change +55.9% (19.4 ms → 30.3 ms).
- **W8** port is slower: speedup 0.89×, latency change +11.7% (416.0 ms → 464.9 ms).

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

