# Performance comparison: reference pypdf 7.1.1 vs pypdf-rust port

- Generated: 2026-11-24 20:12:08 (local)
- Reference run: `reference` – 2026-11-24T20:11:05+0800 – `/Users/yxy/Desktop/pypdf-rust/.venvs/oracle/lib/python3.14/site-packages/pypdf/__init__.py`
- Port run: `rust` – 2026-11-24T20:11:58+0800 – `/Users/yxy/Desktop/pypdf-rust/python/pypdf/__init__.py`
- Repetitions per workload: reference 10 / port 10 (warm-up runs excluded: 1 / 1)
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
| loadavg (1/5/15 min) at start | 22.42 24.44 23.57 | 28.50 25.71 24.04 |
| loadavg (1/5/15 min) at end | 26.37 25.25 23.87 | 28.36 26.16 24.31 |
| shell uptime before | 20:10  up 12 days,  4:58, 2 users, load averages: 22.42 24.44 23.57 | 20:11  up 12 days,  4:58, 2 users, load averages: 26.37 25.25 23.87 |
| shell uptime after | 20:11  up 12 days,  4:58, 2 users, load averages: 26.37 25.25 23.87 | 20:11  up 12 days,  4:59, 2 users, load averages: 28.36 26.16 24.31 |
| extension sha256[:16] | — | 5bfbcbae0b36352f |
| started (local) | 2026-11-24T20:11:05+0800 | 2026-11-24T20:11:58+0800 |

## Commands

- Reference: `cd /Users/yxy/Desktop/pypdf-rust && PYTHONHASHSEED=0 /Users/yxy/Desktop/pypdf-rust/validation/perf/run.py --backend reference --repeat 10 --timeout 300 --out /Users/yxy/Desktop/pypdf-rust/validation/results/perf-reference.json`
- Port: `cd /Users/yxy/Desktop/pypdf-rust && PYTHONHASHSEED=0 /Users/yxy/Desktop/pypdf-rust/validation/perf/run.py --backend rust --repeat 10 --timeout 300 --out /Users/yxy/Desktop/pypdf-rust/validation/results/perf-rust.json`
- Comparison: described by `validation/perf/run_perf.sh`; the baseline sizes in `validation/perf/workloads.json` were measured with the reference run.

## Method

- Reference and port always run in two separate, sequential processes; each process measures exactly one backend, so imports, caches and allocator state cannot leak between sides.
- Every workload performs one warm-up repetition (discarded) followed by the counted repetitions.  The raw per-repetition wall time, CPU time, sampled peak RSS and output byte count are stored in the two result JSON files.
- `PYTHONHASHSEED=0` on both sides and `tools/oracle/prng.py::install_deterministic_secrets()` is called before every repetition, so written PDFs are reproducible; output sizes and sha256 digests are compared between sides and against the recorded reference baselines.
- Timed region: the workload operation only.  Summary serialization and hashing happen after the clock is stopped, so they do not inflate the measured latency.
- Failures and timeouts are recorded per repetition, counted, and listed; `compare.py` fails the run instead of silently dropping them.  A timeout is delivered with SIGALRM, so a repetition blocked inside native code that never returns to the interpreter can overshoot the wall-clock timeout (inherent limitation of in-process measurement).

## Summary

- Workloads compared: 8
- Port faster: 0; port slower: 8; no timing data: 0
- Output-size mismatches: 0
- Recorded failures/timeouts: 0
- File-size separation: small: W1, W5, W6, W7, large: W2, mixed: W3, W4, W8 (workloads with per-file data are additionally split into large/small files below)
- Machine load at start (1-min, 5-min, 15-min): reference 22.42 24.44 23.57, port 28.50 25.71 24.04 on 10 CPUs; a 1-min load above the CPU count means the machine was oversubscribed and the two runs did not see identical conditions.
- Measurement conditions: this is a shared macOS workstation with persistent system background load that cannot be quiesced (WindowServer, the DeepSeek Harness renderer, StorageManagement and similar system processes keep the 1-minute load average around 20+ on a 10-CPU machine).  Reference and port ran sequentially in separate processes under the same conditions; no repository builds or test suites were started by the validation team while the final measurement was running.

Metric definitions: `speedup = ref_median / port_median` (higher is better), `latency Δ = (port_median / ref_median - 1) × 100`, `RSS Δ = (port_peak_RSS / ref_peak_RSS - 1) × 100`. Peak RSS is the maximum sampled resident-set size of a counted repetition (sampled every ~1.5 ms while the repetition runs).

## Per-workload results

| workload | name | size class | files | ref median | port median | speedup | latency Δ | ref p95 | port p95 | ref peak RSS | port peak RSS | RSS Δ | output bytes ref/port | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| W1 | reader_init_20_small | small | 20 | 12.6 ms | 34.9 ms | 0.36× | +176.9% | 14.0 ms | 37.0 ms | 50.6 MB | 56.7 MB | +12.0% | 2.3 KiB / 2.3 KiB | ok |
| W2 | page_traverse_5_large | large | 5 | 221.7 ms | 561.1 ms | 0.40× | +153.1% | 571.5 ms | 637.7 ms | 177.6 MB | 194.3 MB | +9.4% | 46.1 KiB / 46.1 KiB | ok |
| W3 | text_extract_12 | mixed | 12 | 178.5 ms | 706.6 ms | 0.25× | +295.8% | 204.6 ms | 736.8 ms | 179.5 MB | 190.5 MB | +6.1% | 34.3 KiB / 34.3 KiB | ok |
| W4 | image_extract_10 | mixed | 10 | 608.2 ms | 1.865 s | 0.33× | +206.6% | 760.3 ms | 2.466 s | 174.1 MB | 173.1 MB | -0.6% | 1.55 MiB / 1.55 MiB | ok |
| W5 | merge_append_5 | small | 5 | 11.8 ms | 40.8 ms | 0.29× | +247.1% | 14.5 ms | 45.3 ms | 164.3 MB | 175.0 MB | +6.5% | 181.0 KiB / 181.0 KiB | ok |
| W6 | rotate_crop_merge_write_5 | small | 5 | 15.2 ms | 54.1 ms | 0.28× | +255.3% | 24.9 ms | 62.8 ms | 164.7 MB | 175.4 MB | +6.5% | 240.2 KiB / 240.2 KiB | ok |
| W7 | clone_write_10 | small | 10 | 20.9 ms | 70.8 ms | 0.29× | +239.5% | 23.8 ms | 73.4 ms | 164.7 MB | 175.5 MB | +6.5% | 291.0 KiB / 291.0 KiB | ok |
| W8 | end_to_end_mixed_7 | mixed | 7 | 445.8 ms | 1.361 s | 0.33× | +205.3% | 516.3 ms | 1.407 s | 235.5 MB | 236.3 MB | +0.3% | 5.91 MiB / 5.91 MiB | ok |

## Latency distribution (counted repetitions)

| workload | ref min | ref median | ref p95 | ref max | port min | port median | port p95 | port max |
|---|---|---|---|---|---|---|---|---|
| W1 | 12.0 ms | 12.6 ms | 14.0 ms | 14.0 ms | 33.6 ms | 34.9 ms | 37.0 ms | 37.5 ms |
| W2 | 176.0 ms | 221.7 ms | 571.5 ms | 784.2 ms | 546.4 ms | 561.1 ms | 637.7 ms | 643.8 ms |
| W3 | 175.8 ms | 178.5 ms | 204.6 ms | 216.3 ms | 686.6 ms | 706.6 ms | 736.8 ms | 748.7 ms |
| W4 | 589.5 ms | 608.2 ms | 760.3 ms | 863.5 ms | 1.821 s | 1.865 s | 2.466 s | 2.576 s |
| W5 | 11.0 ms | 11.8 ms | 14.5 ms | 14.6 ms | 40.1 ms | 40.8 ms | 45.3 ms | 46.2 ms |
| W6 | 14.5 ms | 15.2 ms | 24.9 ms | 30.3 ms | 50.1 ms | 54.1 ms | 62.8 ms | 65.9 ms |
| W7 | 19.4 ms | 20.9 ms | 23.8 ms | 24.1 ms | 65.7 ms | 70.8 ms | 73.4 ms | 73.7 ms |
| W8 | 371.2 ms | 445.8 ms | 516.3 ms | 519.7 ms | 1.248 s | 1.361 s | 1.407 s | 1.412 s |

## CPU time and peak memory

| workload | ref CPU median | port CPU median | ref peak RSS (max rep) | port peak RSS (max rep) | RSS Δ | ref working-set growth | port working-set growth | growth Δ | ref ru_maxrss | port ru_maxrss |
|---|---|---|---|---|---|---|---|---|---|---|
| W1 | 12.8 ms | 35.1 ms | 50.6 MB | 56.7 MB | +12.0% | 0.2 MB | 0.3 MB | +45.5% | 50.7 MB | 56.8 MB |
| W2 | 186.5 ms | 557.6 ms | 177.6 MB | 194.3 MB | +9.4% | 25.5 MB | 28.6 MB | +11.9% | 177.8 MB | 194.3 MB |
| W3 | 178.0 ms | 700.9 ms | 179.5 MB | 190.5 MB | +6.1% | 0.4 MB | 0.9 MB | +92.6% | 179.5 MB | 194.8 MB |
| W4 | 606.3 ms | 1.836 s | 174.1 MB | 173.1 MB | -0.6% | 13.9 MB | 17.4 MB | +25.1% | 184.4 MB | 194.8 MB |
| W5 | 11.8 ms | 40.9 ms | 164.3 MB | 175.0 MB | +6.5% | 0.1 MB | 0.2 MB | +55.6% | 184.4 MB | 194.8 MB |
| W6 | 15.3 ms | 53.9 ms | 164.7 MB | 175.4 MB | +6.5% | 0.1 MB | 0.2 MB | +11.1% | 184.4 MB | 194.8 MB |
| W7 | 21.0 ms | 71.0 ms | 164.7 MB | 175.5 MB | +6.5% | 0.2 MB | 0.3 MB | +35.7% | 184.4 MB | 194.8 MB |
| W8 | 445.3 ms | 1.347 s | 235.5 MB | 236.3 MB | +0.3% | 24.3 MB | 21.1 MB | -13.0% | 235.5 MB | 236.4 MB |

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
| W1 | 10/10 | 10/10 | 0 | 0 | — | — |
| W2 | 10/10 | 10/10 | 0 | 0 | — | — |
| W3 | 10/10 | 10/10 | 0 | 0 | — | — |
| W4 | 10/10 | 10/10 | 0 | 0 | — | — |
| W5 | 10/10 | 10/10 | 0 | 0 | — | — |
| W6 | 10/10 | 10/10 | 0 | 0 | — | — |
| W7 | 10/10 | 10/10 | 0 | 0 | — | — |
| W8 | 10/10 | 10/10 | 0 | 0 | — | — |

No failures or timeouts were recorded.

## Per-file latency

The reader/text/image workloads time each input file separately inside every repetition (the timed region is identical for both backends). Files at or above 244.1 KiB are reported as *large*, the rest as *small*. For W8 the per-file time covers open + text extraction + append for that file; the shared writer serialization is only included in the workload total.

### W1 – reader_init_20_small

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 1.16 ms | 3.68 ms | 0.31× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 1.10 ms | 0.80 ms | 1.38× |
| resources/hello-world.pdf | 28.7 KiB | small | 0.38 ms | 1.18 ms | 0.33× |
| resources/multilang.pdf | 28.7 KiB | small | 0.34 ms | 0.92 ms | 0.37× |
| resources/pdflatex-forms.pdf | 27.1 KiB | small | 1.58 ms | 6.05 ms | 0.26× |
| resources/commented-xmp.pdf | 17.5 KiB | small | 0.38 ms | 1.14 ms | 0.33× |
| resources/commented.pdf | 13.1 KiB | small | 1.16 ms | 4.11 ms | 0.28× |
| resources/metadata.pdf | 13.0 KiB | small | 0.79 ms | 2.37 ms | 0.34× |
| resources/crazyones.pdf | 11.2 KiB | small | 1.28 ms | 3.38 ms | 0.38× |
| resources/two-different-pages.pdf | 7.5 KiB | small | 0.39 ms | 1.20 ms | 0.33× |
| resources/box.pdf | 5.3 KiB | small | 0.26 ms | 0.71 ms | 0.37× |
| resources/AutoCad_Simple.pdf | 2.9 KiB | small | 0.44 ms | 1.37 ms | 0.32× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.34 ms | 0.84 ms | 0.41× |
| resources/imagemagick-lzw.pdf | 2.6 KiB | small | 0.29 ms | 0.82 ms | 0.35× |
| resources/imagemagick-CCITTFaxDecode.pdf | 1.8 KiB | small | 0.28 ms | 0.76 ms | 0.37× |
| resources/reportlab-inline-image.pdf | 1.5 KiB | small | 0.29 ms | 0.78 ms | 0.38× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.22 ms | 0.67 ms | 0.32× |
| resources/toy.pdf | 1.1 KiB | small | 0.21 ms | 0.57 ms | 0.37× |
| resources/bytes.pdf | 1.1 KiB | small | 0.51 ms | 1.42 ms | 0.36× |
| resources/issue-297.pdf | 958 B | small | 0.47 ms | 1.17 ms | 0.40× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| small | 20 | 11.90 ms | 33.94 ms | 0.35× |

### W2 – page_traverse_5_large

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 86.79 ms | 177.12 ms | 0.49× |
| sample-files/009-pdflatex-geotopo/GeoTopo.pdf | 5.07 MiB | large | 85.34 ms | 275.62 ms | 0.31× |
| resources/fontsampler.pdf | 3.17 MiB | large | 1.20 ms | 1.74 ms | 0.69× |
| sample-files/018-base64-image/base64image.pdf | 2.16 MiB | large | 0.72 ms | 1.29 ms | 0.56× |
| sample-files/009-pdflatex-geotopo/GeoTopo-komprimiert.pdf | 1.71 MiB | large | 31.55 ms | 99.33 ms | 0.32× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 5 | 205.60 ms | 555.09 ms | 0.37× |

### W3 – text_extract_12

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 36.03 ms | 141.56 ms | 0.25× |
| resources/test Orient.pdf | 276.4 KiB | large | 36.59 ms | 138.64 ms | 0.26× |
| sample-files/026-latex-multicolumn/multicolumn.pdf | 76.8 KiB | small | 20.46 ms | 84.45 ms | 0.24× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 28.03 ms | 111.62 ms | 0.25× |
| resources/superscript-pdftex.pdf | 33.8 KiB | small | 2.46 ms | 7.07 ms | 0.35× |
| resources/selenium-pypdf-issue-177.pdf | 33.7 KiB | small | 2.82 ms | 6.70 ms | 0.42× |
| resources/hello-world.pdf | 28.7 KiB | small | 2.61 ms | 8.50 ms | 0.31× |
| resources/multilang.pdf | 28.7 KiB | small | 2.62 ms | 8.46 ms | 0.31× |
| sample-files/004-pdflatex-4-pages/pdflatex-4-pages.pdf | 24.0 KiB | small | 41.09 ms | 173.24 ms | 0.24× |
| resources/crazyones.pdf | 11.2 KiB | small | 3.96 ms | 15.88 ms | 0.25× |
| resources/Sample_Td-matrix.pdf | 1.3 KiB | small | 0.63 ms | 1.92 ms | 0.33× |
| resources/toy.pdf | 1.1 KiB | small | 0.83 ms | 2.47 ms | 0.34× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 2 | 72.63 ms | 280.20 ms | 0.26× |
| small | 10 | 105.52 ms | 420.32 ms | 0.25× |

### W4 – image_extract_10

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| sample-files/023-cmyk-image/cmyk-image.pdf | 433.5 KiB | large | 43.58 ms | 44.98 ms | 0.97× |
| sample-files/031-png-predictors/png-predictors.pdf | 172.7 KiB | small | 17.33 ms | 6.55 ms | 2.64× |
| resources/issue-301.pdf | 119.6 KiB | small | 473.18 ms | 1725.87 ms | 0.27× |
| resources/jpeg.pdf | 98.5 KiB | small | 56.12 ms | 58.03 ms | 0.97× |
| sample-files/019-grayscale-image/grayscale-image.pdf | 39.2 KiB | small | 2.49 ms | 3.11 ms | 0.80× |
| resources/git.pdf | 35.5 KiB | small | 8.65 ms | 10.95 ms | 0.79× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.88 ms | 7.99 ms | 0.36× |
| resources/box.pdf | 5.3 KiB | small | 2.43 ms | 4.04 ms | 0.60× |
| resources/imagemagick-ASCII85Decode.pdf | 2.8 KiB | small | 0.69 ms | 1.59 ms | 0.44× |
| sample-files/008-reportlab-inline-image/inline-image.pdf | 1.5 KiB | small | 0.57 ms | 1.64 ms | 0.35× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 1 | 43.58 ms | 44.98 ms | 0.97× |
| small | 9 | 564.34 ms | 1819.77 ms | 0.31× |

### W8 – end_to_end_mixed_7

| file | bytes | class | ref median | port median | speedup |
|---|---|---|---|---|---|
| resources/issue-604.pdf | 7.62 MiB | large | 233.20 ms | 879.64 ms | 0.27× |
| resources/side-by-side-subfig.pdf | 811.7 KiB | large | 5.74 ms | 19.97 ms | 0.29× |
| resources/GeoBase_NHNC1_Data_Model_UML_EN.pdf | 351.8 KiB | large | 22.19 ms | 84.11 ms | 0.26× |
| resources/pdflatex-outline.pdf | 47.6 KiB | small | 14.92 ms | 57.77 ms | 0.26× |
| resources/multilang.pdf | 28.7 KiB | small | 2.87 ms | 9.36 ms | 0.31× |
| resources/imagemagick-images.pdf | 15.6 KiB | small | 2.71 ms | 9.92 ms | 0.27× |
| resources/crazyones.pdf | 11.2 KiB | small | 4.21 ms | 16.86 ms | 0.25× |

| class | files | ref total (sum of medians) | port total | speedup |
|---|---|---|---|---|
| large | 3 | 261.13 ms | 983.72 ms | 0.27× |
| small | 4 | 24.71 ms | 93.91 ms | 0.26× |

## Regressions and anomalies

- **W1** port is slower: speedup 0.36×, latency change +176.9% (12.6 ms → 34.9 ms).
- **W2** port is slower: speedup 0.40×, latency change +153.1% (221.7 ms → 561.1 ms).
- **W3** port is slower: speedup 0.25×, latency change +295.8% (178.5 ms → 706.6 ms).
- **W4** port is slower: speedup 0.33×, latency change +206.6% (608.2 ms → 1.865 s).
- **W4** port working-set growth is larger: 13.9 MB → 17.4 MB (+25.1%); absolute peak RSS 174.1 MB → 173.1 MB (-0.6%).
- **W5** port is slower: speedup 0.29×, latency change +247.1% (11.8 ms → 40.8 ms).
- **W6** port is slower: speedup 0.28×, latency change +255.3% (15.2 ms → 54.1 ms).
- **W7** port is slower: speedup 0.29×, latency change +239.5% (20.9 ms → 70.8 ms).
- **W8** port is slower: speedup 0.33×, latency change +205.3% (445.8 ms → 1.361 s).

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

