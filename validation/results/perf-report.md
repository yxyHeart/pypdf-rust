# Performance comparison: reference pypdf 7.1.1 vs pypdf-rust port

- Generated: 2026-11-25 10:05:43 (local)
- Reference run: `reference` – 2026-11-25T10:05:32+0800 – `/Users/yxy/Desktop/pypdf-rust/.venvs/oracle/lib/python3.14/site-packages/pypdf/__init__.py`
- Port run: `rust` – 2026-11-25T10:05:43+0800 – `/Users/yxy/Desktop/pypdf-rust/python/pypdf/__init__.py`
- Repetitions per workload: reference 5 / port 5 (warm-up runs excluded: 1 / 1)
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
| loadavg (1/5/15 min) at start | 12.32 21.04 23.18 | 11.21 20.51 22.97 |
| loadavg (1/5/15 min) at end | 11.21 20.51 22.97 | 10.10 19.97 22.75 |
| shell uptime before | 10:05  up 10:15, 2 users, load averages: 12.32 21.04 23.18 | 10:05  up 10:15, 2 users, load averages: 11.21 20.51 22.97 |
| shell uptime after | 10:05  up 10:15, 2 users, load averages: 11.21 20.51 22.97 | 10:05  up 10:15, 2 users, load averages: 10.10 19.97 22.75 |
| extension sha256[:16] | — | affd0ba787331a67 |
| started (local) | 2026-11-25T10:05:32+0800 | 2026-11-25T10:05:43+0800 |

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
- Machine load at start (1-min, 5-min, 15-min): reference 12.32 21.04 23.18, port 11.21 20.51 22.97 on 10 CPUs; a 1-min load above the CPU count means the machine was oversubscribed and the two runs did not see identical conditions.
- Measurement conditions: this is a shared macOS workstation with persistent system background load that cannot be quiesced (WindowServer, the DeepSeek Harness renderer, StorageManagement and similar system processes keep the 1-minute load average around 20+ on a 10-CPU machine).  Reference and port ran sequentially in separate processes under the same conditions; no repository builds or test suites were started by the validation team while the final measurement was running.

Metric definitions: `speedup = ref_median / port_median` (higher is better), `latency Δ = (port_median / ref_median - 1) × 100`, `RSS Δ = (port_peak_RSS / ref_peak_RSS - 1) × 100`. Peak RSS is the maximum sampled resident-set size of a counted repetition (sampled every ~1.5 ms while the repetition runs).

## Per-workload results

| workload | name | size class | files | ref median | port median | speedup | latency Δ | ref p95 | port p95 | ref peak RSS | port peak RSS | RSS Δ | output bytes ref/port | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| W1 | reader_init_20_small | small | 20 | 11.1 ms | 15.6 ms | 0.71× | +40.6% | 12.2 ms | 16.3 ms | 49.4 MB | 56.3 MB | +13.8% | 2.3 KiB / 2.3 KiB | ok |
| W2 | page_traverse_5_large | large | 5 | 164.8 ms | 194.4 ms | 0.85× | +18.0% | 195.2 ms | 226.9 ms | 162.3 MB | 176.0 MB | +8.4% | 46.1 KiB / 46.1 KiB | ok |
| W3 | text_extract_12 | mixed | 12 | 176.6 ms | 391.3 ms | 0.45× | +121.6% | 179.5 ms | 395.5 ms | 161.1 MB | 180.1 MB | +11.8% | 34.3 KiB / 34.3 KiB | ok |
| W4 | image_extract_10 | mixed | 10 | 603.6 ms | 413.2 ms | 1.46× | -31.5% | 614.0 ms | 430.4 ms | 221.6 MB | 215.4 MB | -2.8% | 1.55 MiB / 1.55 MiB | ok |
| W5 | merge_append_5 | small | 5 | 10.8 ms | 23.2 ms | 0.47× | +114.9% | 16.5 ms | 30.1 ms | 221.8 MB | 216.0 MB | -2.6% | 181.0 KiB / 181.0 KiB | ok |
| W6 | rotate_crop_merge_write_5 | small | 5 | 14.3 ms | 29.7 ms | 0.48× | +107.4% | 16.3 ms | 31.3 ms | 222.0 MB | 216.2 MB | -2.6% | 240.2 KiB / 240.2 KiB | ok |
| W7 | clone_write_10 | small | 10 | 18.6 ms | 40.2 ms | 0.46× | +115.9% | 21.1 ms | 45.1 ms | 222.3 MB | 216.5 MB | -2.6% | 291.0 KiB / 291.0 KiB | ok |
| W8 | end_to_end_mixed_7 | mixed | 7 | 430.2 ms | 667.8 ms | 0.64× | +55.2% | 472.6 ms | 735.3 ms | 284.8 MB | 296.7 MB | +4.2% | 5.91 MiB / 5.91 MiB | ok |

## Latency distribution (counted repetitions)

| workload | ref min | ref median | ref p95 | ref max | port min | port median | port p95 | port max |
|---|---|---|---|---|---|---|---|---|
| W1 | 11.0 ms | 11.1 ms | 12.2 ms | 12.3 ms | 14.6 ms | 15.6 ms | 16.3 ms | 16.5 ms |
| W2 | 162.0 ms | 164.8 ms | 195.2 ms | 202.4 ms | 189.9 ms | 194.4 ms | 226.9 ms | 234.7 ms |
| W3 | 172.2 ms | 176.6 ms | 179.5 ms | 179.8 ms | 384.2 ms | 391.3 ms | 395.5 ms | 396.0 ms |
| W4 | 584.7 ms | 603.6 ms | 614.0 ms | 615.2 ms | 400.0 ms | 413.2 ms | 430.4 ms | 431.8 ms |
| W5 | 10.7 ms | 10.8 ms | 16.5 ms | 18.0 ms | 23.0 ms | 23.2 ms | 30.1 ms | 31.2 ms |
| W6 | 13.9 ms | 14.3 ms | 16.3 ms | 16.7 ms | 28.4 ms | 29.7 ms | 31.3 ms | 31.6 ms |
| W7 | 18.5 ms | 18.6 ms | 21.1 ms | 21.3 ms | 38.3 ms | 40.2 ms | 45.1 ms | 46.1 ms |
| W8 | 364.1 ms | 430.2 ms | 472.6 ms | 479.7 ms | 608.0 ms | 667.8 ms | 735.3 ms | 741.0 ms |

## CPU time and peak memory

| workload | ref CPU median | port CPU median | ref peak RSS (max rep) | port peak RSS (max rep) | RSS Δ | ref working-set growth | port working-set growth | growth Δ | ref ru_maxrss | port ru_maxrss |
|---|---|---|---|---|---|---|---|---|---|---|
| W1 | 11.2 ms | 15.8 ms | 49.4 MB | 56.3 MB | +13.8% | 0.2 MB | 0.3 MB | +90.0% | 49.4 MB | 56.3 MB |
| W2 | 165.2 ms | 194.3 ms | 162.3 MB | 176.0 MB | +8.4% | 29.1 MB | 28.0 MB | -3.8% | 162.9 MB | 176.0 MB |
| W3 | 176.5 ms | 390.4 ms | 161.1 MB | 180.1 MB | +11.8% | 0.6 MB | 0.3 MB | -55.6% | 163.8 MB | 180.1 MB |
| W4 | 603.1 ms | 414.1 ms | 221.6 MB | 215.4 MB | -2.8% | 24.1 MB | 10.5 MB | -56.6% | 221.6 MB | 227.2 MB |
| W5 | 10.9 ms | 23.3 ms | 221.8 MB | 216.0 MB | -2.6% | 0.1 MB | 0.0 MB | -40.0% | 221.8 MB | 227.2 MB |
| W6 | 14.5 ms | 30.0 ms | 222.0 MB | 216.2 MB | -2.6% | 0.1 MB | 0.1 MB | +33.3% | 222.0 MB | 227.2 MB |
| W7 | 18.8 ms | 40.4 ms | 222.3 MB | 216.5 MB | -2.6% | 0.1 MB | 0.1 MB | +12.5% | 222.3 MB | 227.2 MB |
| W8 | 429.3 ms | 668.4 ms | 284.8 MB | 296.7 MB | +4.2% | 15.8 MB | 25.1 MB | +58.3% | 284.8 MB | 296.7 MB |

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
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 1.04 ms | 1.22 ms | 0.85× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 1.04 ms | 0.37 ms | 2.78× |
| resources/hello-world.pdf | 28.7 KiB | small | 0.34 ms | 0.50 ms | 0.67× |
| resources/multilang.pdf | 28.7 KiB | small | 0.31 ms | 0.47 ms | 0.66× |
| resources/pdflatex-forms.pdf | 27.1 KiB | small | 1.42 ms | 2.58 ms | 0.55× |
| resources/commented-xmp.pdf | 17.5 KiB | small | 0.29 ms | 0.55 ms | 0.52× |
| resources/commented.pdf | 13.1 KiB | small | 1.03 ms | 1.57 ms | 0.66× |
| resources/metadata.pdf | 13.0 KiB | small | 0.73 ms | 1.09 ms | 0.67× |
| resources/crazyones.pdf | 11.2 KiB | small | 1.10 ms | 1.26 ms | 0.88× |
| resources/two-different-pages.pdf | 7.5 KiB | small | 0.35 ms | 0.46 ms | 0.77× |
| resources/box.pdf | 5.3 KiB | small | 0.21 ms | 0.35 ms | 0.61× |
| resources/AutoCad_Simple.pdf | 2.9 KiB | small | 0.39 ms | 0.52 ms | 0.75× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.29 ms | 0.39 ms | 0.74× |
| resources/imagemagick-lzw.pdf | 2.6 KiB | small | 0.27 ms | 0.37 ms | 0.72× |
| resources/imagemagick-CCITTFaxDecode.pdf | 1.8 KiB | small | 0.26 ms | 0.36 ms | 0.73× |
| resources/reportlab-inline-image.pdf | 1.5 KiB | small | 0.25 ms | 0.37 ms | 0.66× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.18 ms | 0.29 ms | 0.62× |
| resources/toy.pdf | 1.1 KiB | small | 0.20 ms | 0.32 ms | 0.61× |
| resources/bytes.pdf | 1.1 KiB | small | 0.45 ms | 0.93 ms | 0.48× |
| resources/issue-297.pdf | 958 B | small | 0.35 ms | 0.51 ms | 0.68× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| small | 20 | 10.50 ms | 14.49 ms | 0.72× |

### W2 – page_traverse_5_large

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 64.44 ms | 59.67 ms | 1.08× |
| sample-files/009-pdflatex-geotopo/GeoTopo.pdf | 5.07 MiB | large | 69.74 ms | 95.70 ms | 0.73× |
| resources/fontsampler.pdf | 3.17 MiB | large | 0.98 ms | 1.16 ms | 0.85× |
| sample-files/018-base64-image/base64image.pdf | 2.16 MiB | large | 0.71 ms | 0.76 ms | 0.93× |
| sample-files/009-pdflatex-geotopo/GeoTopo-komprimiert.pdf | 1.71 MiB | large | 28.99 ms | 35.08 ms | 0.83× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 5 | 164.85 ms | 192.36 ms | 0.86× |

### W3 – text_extract_12

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 35.60 ms | 62.77 ms | 0.57× |
| resources/test Orient.pdf | 276.4 KiB | large | 35.88 ms | 30.66 ms | 1.17× |
| sample-files/026-latex-multicolumn/multicolumn.pdf | 76.8 KiB | small | 20.43 ms | 60.00 ms | 0.34× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 28.48 ms | 80.04 ms | 0.36× |
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 2.34 ms | 3.65 ms | 0.64× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 2.80 ms | 3.49 ms | 0.80× |
| resources/hello-world.pdf | 28.7 KiB | small | 2.54 ms | 4.53 ms | 0.56× |
| resources/multilang.pdf | 28.7 KiB | small | 2.55 ms | 5.07 ms | 0.50× |
| sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf | 24.0 KiB | small | 40.85 ms | 127.65 ms | 0.32× |
| resources/crazyones.pdf | 11.2 KiB | small | 3.81 ms | 10.59 ms | 0.36× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.62 ms | 0.98 ms | 0.64× |
| resources/toy.pdf | 1.1 KiB | small | 0.81 ms | 1.50 ms | 0.54× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 2 | 71.48 ms | 93.43 ms | 0.77× |
| small | 10 | 105.21 ms | 297.50 ms | 0.35× |

### W4 – image_extract_10

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| sample-files/023-cmyk-image/cmyk-image.pdf | 433.5 KiB | large | 43.29 ms | 42.40 ms | 1.02× |
| sample-files/031-png-predictors/png-predictors.pdf | 172.7 KiB | small | 17.30 ms | 5.28 ms | 3.28× |
| resources/issue-301.pdf | 119.6 KiB | small | 470.44 ms | 289.93 ms | 1.62× |
| resources/jpeg.pdf | 98.5 KiB | small | 55.67 ms | 54.75 ms | 1.02× |
| sample-files/019-grayscale-image/grayscale-image.pdf | 39.2 KiB | small | 2.71 ms | 2.56 ms | 1.06× |
| resources/git.pdf | 35.5 KiB | small | 8.94 ms | 8.97 ms | 1.00× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.93 ms | 4.89 ms | 0.60× |
| resources/box.pdf | 5.3 KiB | small | 2.50 ms | 3.01 ms | 0.83× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.73 ms | 0.97 ms | 0.75× |
| sample-files/008-reportlab-inline-image/inline-image.pdf | 1.5 KiB | small | 0.53 ms | 0.80 ms | 0.67× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 1 | 43.29 ms | 42.40 ms | 1.02× |
| small | 9 | 561.76 ms | 371.15 ms | 1.51× |

### W8 – end_to_end_mixed_7

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 226.40 ms | 365.06 ms | 0.62× |
| resources/side-by-side-subfig.pdf | 811.7 KiB | large | 5.54 ms | 13.71 ms | 0.40× |
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 21.31 ms | 37.90 ms | 0.56× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 14.30 ms | 39.18 ms | 0.37× |
| resources/multilang.pdf | 28.7 KiB | small | 2.81 ms | 5.32 ms | 0.53× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.81 ms | 6.15 ms | 0.46× |
| resources/crazyones.pdf | 11.2 KiB | small | 4.14 ms | 11.46 ms | 0.36× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 3 | 253.25 ms | 416.67 ms | 0.61× |
| small | 4 | 24.07 ms | 62.10 ms | 0.39× |

## Regressions and anomalies

- **W1** port is slower: speedup 0.71×, latency change +40.6% (11.1 ms → 15.6 ms).
- **W2** port is slower: speedup 0.85×, latency change +18.0% (164.8 ms → 194.4 ms).
- **W3** port is slower: speedup 0.45×, latency change +121.6% (176.6 ms → 391.3 ms).
- **W5** port is slower: speedup 0.47×, latency change +114.9% (10.8 ms → 23.2 ms).
- **W6** port is slower: speedup 0.48×, latency change +107.4% (14.3 ms → 29.7 ms).
- **W7** port is slower: speedup 0.46×, latency change +115.9% (18.6 ms → 40.2 ms).
- **W8** port is slower: speedup 0.64×, latency change +55.2% (430.2 ms → 667.8 ms).
- **W8** port working-set growth is larger: 15.8 MB → 25.1 MB (+58.3%); absolute peak RSS 284.8 MB → 296.7 MB (+4.2%).

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

