# Performance engineering and benchmark budgets

## Principles

NOVA will not claim to be faster than another launcher until comparable measurements exist. Launcher overhead and in-game FPS are separate measurements: a launcher cannot guarantee higher FPS merely by changing JVM flags.

## Workload model

Record these phases independently:

1. Application process start to first interactive frame.
2. Cold version-manifest retrieval and parsing.
3. Warm version-manifest retrieval from cache.
4. Instance preparation with all artifacts already present.
5. Cold install of a defined Vanilla version.
6. Warm repair/verification with no corrupted files.
7. Java process creation to Minecraft window visible.
8. Launcher idle CPU and memory while the game is running.

Each benchmark must specify OS build, CPU, memory, storage, network connection, Java build, Minecraft version, loader, instance size, cache state and background workload.

## Initial targets (engineering goals, not measured results)

- Warm dashboard interaction: p95 input-to-response below 100 ms on the reference Windows test machine.
- No blocking filesystem, hashing, archive extraction or process wait on the UI executor.
- Metadata cache: render the last valid local copy immediately, then refresh in the background.
- Download concurrency: bounded and configurable by workload; never create an unbounded task per artifact.
- Large-file range downloads: enabled only when the origin advertises byte ranges and benchmarks show a net benefit.
- File verification: streamed hashing, no whole-file buffering.
- Artifact publication: write to a temporary sibling, verify, then atomically rename.
- Idle mode: no high-frequency polling when no active task or visible screen needs updates.
- Cancellation: stop scheduling new work immediately and cancel in-flight work where safe.

These goals must be validated on repeatable workloads before being treated as release gates.

## Download engine design

- Reuse a long-lived HTTP client with connection pooling.
- Apply per-host and global concurrency limits.
- Use bounded retries with backoff for transient network and incomplete-body failures; do not retry permanent HTTP failures or disk-full errors.
- Resume only when the server supports ranges and the partial file can be validated.
- Use a streaming hash while downloading.
- Verify expected size and digest before publishing.
- Store immutable artifacts by digest when metadata provides a trusted digest.
- Share immutable cached artifacts across instances; keep mutable game configuration and saves instance-local.
- Emit progress at a bounded cadence rather than once per tiny chunk.

## CPU, memory and UI

- Run archive extraction and expensive hashing on a bounded blocking pool.
- Prefer streaming APIs and bounded buffers for large artifacts.
- Parse metadata once and share immutable results.
- Use explicit cancellation tokens for long-running tasks.
- Do not poll CPU, memory or GPU every frame. Sample at a low rate only when a diagnostics view is visible.
- Keep the React renderer focused on presentation. Native operations own filesystem/process/network state.

## JVM and gameplay claims

- Detect the Java version required by the selected Minecraft version and loader.
- Start with conservative per-instance memory recommendations, never an aggressive global default.
- Keep optional JVM tuning explainable, version-aware and reversible.
- Do not promise FPS gains from generic JVM arguments. Measure average FPS, 1% lows, frame-time variance, heap use, garbage-collection pauses and time-to-world on controlled game scenes.

## Instrumentation available today

The native PerformanceMonitor records active and peak concurrent operation counts, completed operation count and cumulative operation time, downloaded and reused bytes, cache hits and misses, and integrity verification failures.

Only native runtime diagnostics currently produce operation timings. Download, cache and integrity counters are intentionally zero until their real services are implemented. These counters are not synthetic benchmark results.

## Benchmark process

For each optimization:
1. Capture a baseline on at least three runs.
2. Change one variable at a time.
3. Run at least five measured repetitions after warm-up.
4. Compare median and p95, not only the best run.
5. Record resource usage and failure/retry rates.
6. Reject optimizations that improve one metric while materially damaging reliability, memory or another target metric.
