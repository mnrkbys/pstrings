# CA-001 implementation and upstream submission record

Date: 2026-10-23

All eight Findings from CA-001 have implementations and regression tests, submitted as seven upstream PRs. This record describes implementation and verification; it does not grant independent review approval or change Finding lifecycle statuses. The immutable audit report and its original evidence remain unchanged.

## Submitted changes

| Finding | Internal implementation | Upstream PR | Submitted head |
| --- | --- | --- | --- |
| F-0001 | `fix/F-0001-preserve-input` / `08e4ff5` | [#2: protect input aliases](https://github.com/herosi/pstrings/pull/2) | `e52a5376` |
| F-0002; F-0003 | `fix/F-0002-F-0003-output-state` / `472d940` | [#3: ordered output and boundary reassembly](https://github.com/herosi/pstrings/pull/3) | `8474af1f` |
| F-0004 | `fix/F-0004-global-utf16-parity` / `f290247` | [#8: complete-run UTF-16LE parity](https://github.com/herosi/pstrings/pull/8) | `a817a704` |
| F-0005 | `fix/F-0005-bound-inflight-chunks` / `3da034e` | [#4: bound chunks in flight](https://github.com/herosi/pstrings/pull/4) | `cebbe517` |
| F-0006 | `fix/F-0006-bounded-records` / `afd94f8` | [#7: bounded output memory](https://github.com/herosi/pstrings/pull/7) | `adc01da6` |
| F-0007 | `fix/F-0007-odd-length-input` / `9097d0c` | [#5: odd-length input](https://github.com/herosi/pstrings/pull/5) | `4aaf238b` |
| F-0008 | `fix/F-0008-cancellation-report` / `bb46f60` | [#6: cancellation reporting](https://github.com/herosi/pstrings/pull/6) | `03ff77ba` |

The upstream delivery branches use the names in the corresponding PRs. Audit documents are excluded from every submitted branch. Existing [PR #1](https://github.com/herosi/pstrings/pull/1), fixing wide-character truncation in `utf16le-ascii`, was kept separate.

## Dependencies and implementation choices

- PR #2, #4, #5 and #6 are independently based on upstream main `1f6b267e8b484fe191aa8f613bf30cf969388a55`.
- PR #3 introduces `OutputState`, which tracks unfinished strings and completed rows waiting for an earlier run. Its Rust API state parameter replaces the former pending-record map. CLI options and output columns are preserved.
- PR #7 depends on #3. It adds file-backed payloads above 8 MiB and FIFO queues holding only their heads in memory. Large raw runs are decoded incrementally. This preserves full strings while using temporary disk space and I/O; README documents this tradeoff and `--temp-dir` behavior.
- PR #8 depends on #7 and transitively #3. It retains both UTF-16LE parity streams until runs are complete, applies the existing ASCII/source-span preference to complete candidates, and handles low surrogates already consumed by the preceding chunk.
- The dependent PR descriptions identify their prerequisites and link directly to comparisons containing only the additional changes. They can be rebased after their prerequisites merge.
- Every PR discloses OpenAI Codex assistance. No maintainer was assigned or separately messaged.

## Verification

- Combined verification branch: `fix/ca-001-combined-verification`.
- Combined code target: `5cd2d1b` (full SHA available through Git). This includes all eight fixes and existing PR #1. **271 tests passed** with `cargo test --all-features` on Ubuntu 26.04.1 / WSL2 x86-64, Rust 1.99.1.
- Each of the seven actual submitted heads separately passed its full test suite and `cargo check --target x86_64-pc-windows-msvc --all-targets`.
- Windows runtime tests have not been run; cross-target checking is compilation/type checking, not execution.
- The deterministic chunk comparison passed for all 11 scanners: 40 synthetic inputs each, 2,640 CLI runs. Mixed-scanner comparisons passed separately.
- On synthetic all-`A` input, one worker and 1 MiB chunks, peak RSS was about 19 MiB for both 16 MiB and 64 MiB ASCII inputs; CP932 and ISO-2022-JP were about 26 MiB at both sizes. These observed values demonstrate the tested output-stage scaling and are not general per-worker memory guarantees.
- Regression coverage includes alias preservation, device outputs and write errors, all widths of mixed-scanner fixtures, both string-only and TSV output, Unix FD limits, long-record preservation, all raw decoders, shared-cursor movement, UTF-16LE starting offsets 0–7, surrogate boundaries, odd input tails, very large requested widths, and cancellation/channel shutdown.
- Finding trailers were validated. The combined branch's one textual cherry-pick conflict preserved both output-before-workers and safe `div_ceil` chunk counting.
- Submitted PR heads and full descriptions were read back from GitHub and checked against the local manifest. At submission verification time GitHub returned no status checks for these PRs; local validation above is the available test evidence.

The Rust toolchain, build outputs and detailed logs are under `/tmp/pstrings-rustup`, `/tmp/pstrings-cargo`, `/tmp/pstrings-target` and `/tmp/pstrings-delivery`; these can disappear when `/tmp` is cleared. Source, regression tests and commits are retained in the repository and the submitted branches. The original review registry remains Open pending independent review; no self-approval or upstream merge was performed.
