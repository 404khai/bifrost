# Bifrost Stage 5 benchmark report

Run date: 2026-10-21 in Africa/Lagos (raw timestamps are UTC).

Borrowing validated unescaped object keys reduced the byte editor's median
latency on the 20 KiB object fixture from **87.35 µs to 51.36 µs (41.2%)** across
three runs per variant. DHAT allocation blocks fell from **2,341 to 222 (90.5%)**.
Peak live heap for that case stayed at **20,769 bytes**. This is a measured
allocation-count and latency improvement, not a universal speed or memory claim.

The optimized byte editor is still slower than the Value pipeline on the
three-hop and large plain-string fixtures. No comparison with Rex is made.

## Reproduce

Requires stable Rust, Cargo, and Python 3 (standard library only for scripts).
The harness uses `std::time::Instant`, `black_box`, and raw JSON samples instead
of Criterion. `dhat = 0.3.3` is a dev-only dependency for the separate allocation
executable; no new production dependency or unsafe code was introduced.

```sh
# Current source: each label must be new; results are never overwritten.
python3 scripts/bench.py --label rerun-1 --allocations --profile-case objects_20k
python3 scripts/bench.py --label rerun-2
python3 scripts/bench.py --label rerun-3

# Rebuild the baseline library with the same current harness and Cargo.lock.
# Uses a temporary directory and leaves the working source untouched.
python3 scripts/bench.py --label rerun-baseline --source-ref ead40e2 --allocations

# A quick subset, and regeneration of the committed six-run summary:
python3 scripts/bench.py --label quick --samples 5 --filter objects_1k
python3 scripts/bench_summary.py

# Latency harness directly (raw JSON on stdout):
cargo bench --bench translator -- --samples 31
```

The runner builds `cargo bench --no-run --message-format=json`, locates the two
benchmark executables, and stores results under `docs/benchmark-results`.
`--source-ref` copies only library source from the requested Git commit into a
temporary build and uses the current benchmark code and locked dependencies.
A five-sample baseline reproduction run verified this path; its streaming source
hash matches the three baseline runs. It is excluded from the main summaries.

Baseline source is Stage 4 commit `ead40e24c4a73f8deb0edf4197f7104bb8fbfd83`.
Both variants were measured before this Stage 5 commit, so their recorded Git
HEAD is that commit; their **streaming source SHA-256 hashes distinguish them**.
Harness and fixture hashes match across all six main runs. Full DHAT profiles
are generated under `target/`; the twelve largest allocation sites and stacks
are preserved in baseline-1/optimized-1 JSON. Preliminary single-operation cold
compilation timings were discarded; the final cold test batches 32 compilations
to reduce timer quantization.

## Environment

- Apple M3, 8 logical CPUs, 16 GiB RAM, arm64.
- macOS 27.0 build 26A428; Darwin 27.0.0.
- rustc 1.96.0 (`ac68faa20`, 2026-05-25), cargo 1.96.0.
- Cargo bench optimized profile, line debug info (`debug = 1`), default system
  allocator in latency executable. No custom RUSTFLAGS or CPU-specific flags.
- One process/thread at a time; no CPU pinning, frequency control, thermal
  monitoring, or exclusion of background applications. These are local results,
  not production service latency or multi-machine evidence.

## Workloads and measurement boundaries

`benches/support/mod.rs` generates deterministic fixtures using LCG seed
`0xB1F2057`. Object fixtures contain terminal data, nested rule paths, metadata,
and an unchanged array of event objects with strings, numbers, booleans, and
nulls. Padding brings them close to 1 KiB, 20 KiB, and 1 MiB. Blob fixtures use
long plain strings. Every case checks byte output against the Value engine before
timing and asserts the generated size is within 128 bytes of its target.

The matrix changes one main factor at a time around the 20 KiB case:

- Rule-path depth: 2, 12, 32 `level` containers, then a target object.
- Rule count/density: 1, 8, 32 containing-object rules.
- Revision hops: identity, one, three; directional field names stay ordered.
- Shared clients: 1, 8, 64 precreated `Arc` handles to the same plan. Clients are
  processed sequentially; latency is normalized per translation. This measures
  batches sharing a plan, not parallel scalability or contention.
- Subtree composition: event objects versus a large string. A changed-blob case
  attaches the long string to the renamed field; an unchanged-blob case puts it
  outside the rule path. Only keys change: neither case rewrites string content.

`value_e2e` parses bytes to Value, calls the public compiled Value translator
(which clones the input), serializes the output, and drops the values/output.
`stream_e2e` calls `translate_bytes` and drops its result. Both use an already
compiled plan; fixture construction, plan setup, client handle creation, and
correctness comparisons are excluded. This comparison includes the Value API's
preservation clone; it is not a consuming parse/edit/serialize implementation.

Each latency run warms each operation for 5 ms, calibrates a batch toward 2 ms
(clamped to 1–1,024 client-batch operations), and collects **31 batch means**.
There are three baseline and three optimized runs. Tables report the median of
three run medians; ranges show the three run medians' min/max. p05/p95 values are
nearest-index percentiles of run 1's 31 batch means, **not individual-request tail
latencies or confidence intervals**. All raw samples and batch sizes are retained.
Throughput is input MiB/s derived from per-translation median latency, including
all internal passes, not an internal parser bandwidth measurement.

## Latency and throughput

Negative change means lower byte-editor latency after the optimization. Value
numbers are from optimized runs; no Value algorithm was changed.

| Case | Input bytes | Value µs | Byte before µs | Byte after µs | Change | Byte MiB/s | After run-median range µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| objects_1k | 1,024 | 7.62 | 4.37 | 2.80 | -35.8% | 348.3 | 2.68–2.89 |
| objects_20k | 20,480 | 139.10 | 87.35 | 51.36 | -41.2% | 380.3 | 50.17–52.42 |
| objects_1m | 1,048,576 | 7625.96 | 4436.33 | 2733.46 | -38.4% | 365.8 | 2477.12–2763.79 |
| depth_12 | 20,480 | 143.50 | 90.04 | 52.24 | -42.0% | 373.9 | 49.40–60.08 |
| depth_32 | 20,480 | 150.01 | 88.24 | 55.15 | -37.5% | 354.2 | 50.44–55.39 |
| rules_8 | 20,480 | 144.62 | 87.68 | 51.66 | -41.1% | 378.1 | 50.33–54.77 |
| rules_32 | 20,480 | 148.41 | 94.07 | 56.75 | -39.7% | 344.1 | 56.38–58.19 |
| identity | 20,480 | 135.65 | 87.88 | 50.50 | -42.5% | 386.7 | 49.20–53.29 |
| hops_3 | 20,480 | 137.32 | 260.20 | 152.02 | -41.6% | 128.5 | 149.79–154.28 |
| clients_8 | 20,480 | 138.22 | 88.34 | 50.39 | -43.0% | 387.6 | 50.05–51.36 |
| clients_64 | 20,480 | 139.79 | 86.82 | 51.11 | -41.1% | 382.2 | 49.04–51.61 |
| blob_unchanged_20k | 20,479 | 10.66 | 15.97 | 16.04 | +0.5% | 1217.4 | 15.38–16.16 |
| blob_changed_20k | 20,474 | 10.85 | 15.75 | 15.93 | +1.1% | 1225.4 | 15.50–16.26 |
| blob_unchanged_1m | 1,048,575 | 451.12 | 790.35 | 801.71 | +1.4% | 1247.3 | 779.06–1020.65 |

For the optimized 20 KiB object case, run 1's byte p05/p95 batch means are
50.37/55.20 µs; the Value p05/p95 are 136.38/154.99 µs. The before/after byte
run-median ranges are 86.66–87.84 µs and 50.17–52.42 µs respectively. These
observations support the narrow workload claim, without a statistical
significance test.

### Compilation versus reuse

These operations translate no payload. `cold_compile` prepares 32 independent
registries outside each timed sample and times only their first compile, retaining
plans until after the timer. Registry construction, result-vector allocation, and
destruction are excluded; cache insertion and plan allocation are included.
`cache_hit` repeatedly looks up an already compiled pair and drops the returned
Arc. It uses the same adaptive batching as translation. Timer overhead is not
subtracted, so tiny absolute numbers should be treated cautiously.

| Hops | Cold compile median ns | Cache hit median ns | Cold run-1 p05–p95 ns | Hit run-1 p05–p95 ns |
| --- | ---: | ---: | --- | --- |
| 0 | 24.72 | 4.03 | 23.44–28.66 | 4.03–4.11 |
| 1 | 40.38 | 4.39 | 39.06–63.81 | 4.39–4.44 |
| 3 | 53.41 | 5.13 | 52.09–291.66 | 5.21–5.29 |

These are resolution-sensitive microbenchmarks of this small registry, not a
claim about complex schema compilation. Stage 3 correctness tests separately
verify lazy compilation, reuse identity, and invalidation.

## Allocations and live heap (separate instrumented executable)

The [dhat allocator](https://docs.rs/dhat/0.3.3/dhat/) wraps the system allocator.
Each case/method runs once in a fresh process. Profiling starts after input,
registry, plan, client handles, and correctness checks are built and stops before
printing JSON. It captures transient application allocations during translation
and deallocation, but excludes existing inputs/plans and allocator metadata.
Latency tables never use this instrumented executable.

Total allocated bytes describe cumulative allocation traffic; peak live bytes
describe simultaneous tracked heap. They are separate quantities. These are not
OS resident-set size measurements, nor total process-memory bounds. All measured
regions ended with zero live tracked bytes. Shared-client allocation totals are
for the entire sequential batch in raw JSON; divide by client count for per-call
traffic. Peak live heap is not divided, since clients run sequentially.

Selected cases (one client) show the distinction:

| Case | Method/variant | Blocks | Total allocated bytes | Peak live bytes |
| --- | --- | ---: | ---: | ---: |
| objects_1k | Value | 206 | 24,053 | 21,909 |
| objects_1k | Byte before | 119 | 5,525 | 1,546 |
| objects_1k | Byte after | 20 | 5,061 | 1,516 |
| objects_20k | Value | 3,851 | 422,499 | 381,699 |
| objects_20k | Byte before | 2,341 | 90,429 | 20,769 |
| objects_20k | Byte after | 222 | 81,077 | 20,769 |
| objects_1m | Value | 192,935 | 22,284,727 | 19,663,447 |
| objects_1m | Byte before | 117,885 | 4,521,821 | 1,048,865 |
| objects_1m | Byte after | 10,726 | 4,050,293 | 1,048,865 |
| hops_3 | Value | 3,875 | 422,700 | 381,706 |
| hops_3 | Byte before | 7,023 | 271,298 | 41,257 |
| hops_3 | Byte after | 666 | 243,238 | 41,257 |
| blob_unchanged_1m | Value | 56 | 5,249,719 | 4,200,665 |
| blob_unchanged_1m | Byte before | 29 | 1,050,470 | 1,048,864 |
| blob_unchanged_1m | Byte after | 12 | 1,050,372 | 1,048,864 |

## Profile, one change, and tradeoffs

On the 20 KiB object case, the baseline DHAT call stacks attributed 1,045 blocks
to decoding object key Strings and another 1,045 blocks to cloning those Strings
into duplicate-key sets. These 2,090 blocks were 89.3% of 2,341 blocks. The
largest byte site was still BTreeSet node allocation (58,520 bytes in 209 nodes).
This was an **allocation profile**, not a sampled CPU profile.

The single production-code optimization uses `Cow<'a, str>` for decoded keys:
a validated unescaped key borrows its input slice; an escaped key remains an
owned decoded String. Borrowed clones copy references. Relevant diagnostic paths
and duplicate-key errors own strings when needed. Existing UTF-8/string checks,
duplicate semantics, error order, copy spans, resource limits, and ordered passes
remain intact. No parser-validation work was removed and no unsafe code added.

The change removed 2,119 blocks on this fixture (90.5%; slightly more than the
2,090 unchanged-array key blocks because relevant metadata keys also benefit).
Cumulative allocated bytes fell from 90,429 to 81,077 (10.3%), while peak live
heap did not change. Remaining BTreeSet nodes and output-buffer memory explain
why reducing allocation count is not the same as reducing peak live memory.

Object-heavy byte medians improved about 36–43% across the measured variations.
The optimized one-hop 20 KiB byte case is about 2.7× faster than this Value API
pipeline. Three hops still scan/copy three times: 152.02 µs versus Value's
137.32 µs. The 1 MiB plain-string case remains slower: 801.71 µs versus
451.12 µs. String scanning, rather than object-key allocation, dominates that
fixture; no additional string-scanner optimization was attempted.

The three plain-string cases show observed before/after median increases of
0.5–1.4%; run ranges overlap and the 1 MiB case has a wide range. These runs do
not establish a reliable regression, and certainly do not show a speedup on
those cases. Escaped keys still allocate and now take an extra escape-presence
check; there is no dedicated escaped-key timing workload, so no performance
claim is made for that distribution. Duplicate validation also imposes work the
Value parser does not retain after duplicate keys collapse.

## Limits and review checkpoint

The fixture matrix is a selected one-factor study, not a full factorial sweep.
Only one hardware/OS/toolchain environment and one local measurement session
were used. Runs were not randomized or statistically modeled; before runs all
preceded after runs. No network, concurrent-client, production p99, process RSS,
or allocator metadata measurement is included. End-to-end cache-miss translation
can be derived only approximately from the separate phase timings; it was not
measured as a combined operation. The report explicitly avoids claims beyond
these fixtures and public API boundaries.

Final checks passed: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test` (54 tests including
doctests), `cargo doc --no-deps`, Python syntax checks, raw-data/hash validation,
and `git diff --check`. Stage 5's implementation is ready for review.
Understand how a batch mean differs from a request percentile,
why DHAT traffic differs from peak live heap, and why an optimization helping
object keys does not establish a general JSON speedup before extending it.

Raw data: [baseline 1](benchmark-results/baseline-1.json),
[baseline 2](benchmark-results/baseline-2.json),
[baseline 3](benchmark-results/baseline-3.json),
[optimized 1](benchmark-results/optimized-1.json),
[optimized 2](benchmark-results/optimized-2.json),
[optimized 3](benchmark-results/optimized-3.json),
[computed summary](benchmark-results/summary.json).
