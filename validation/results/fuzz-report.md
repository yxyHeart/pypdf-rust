# Mutation fuzzing report (§8.4)

Generated: `2026-11-25T01:24:07Z` (UTC)  
Repository commit: `9ba12e9`

## Corpus

* 200 mutated PDF(s), seed `20261124`, source PDFs under `resources/`: 72
* per-file timeout: 10 s
* corpus directory: `/var/folders/l1/29gl0tr57s1fvhb0j566k5h00000gn/T/pypdf-fuzz-corpus-2mya3tak/corpus`

## Per-side execution

| Side | Files | Handled | Raised | Timeouts | Panics | Hard crash | Wall |
|---|---|---|---|---|---|---|---|
| oracle | 200 | 127 | 73 | 0 | 0 | False | 5.763 s |
| port | 200 | 127 | 73 | 0 | 0 | False | 8.161 s |

## Comparison (must be identical)

Result: **IDENTICAL** — 0 missing file(s), 0 decision difference(s), 0 message difference(s), 0 canonical-result difference(s).

## Reproduction

```bash
python validation/fuzz.py --count 200 --seed 20261124 --timeout 10
```

Raw evidence: `validation/results/fuzz-raw-oracle.json`, `validation/results/fuzz-raw-port.json`, `validation/results/fuzz-comparison.json`.
