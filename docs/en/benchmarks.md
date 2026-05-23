# Benchmarks

Benchmark smoke tests are intended to catch obvious performance or execution regressions, not to enforce portable hard thresholds.

Run:

```sh
scripts/bench-smoke.sh
```

The smoke covers representative recursion, loop, container, module, lambda, permissioned host filesystem helper, and `nox test` workflows. Output is useful for same-machine comparisons between commits.

Stage 15 also adds Criterion benches for selected `nox_core` and runtime paths:

```sh
cargo bench -p nox_core --bench core_paths
cargo bench -p nox --bench runtime_capabilities
```

Current benches cover recursive check/compile, loop execution, container
execution, lambda/function-value execution, filesystem host helpers, async task
host helpers, and mocked HTTP host helpers. Criterion writes statistical output
and an HTML report under `target/criterion/`.

To run Criterion through the smoke script:

```sh
NOX_BENCH_CRITERION=1 scripts/bench-smoke.sh
```

The Criterion benches supplement the existing smoke budgets; they do not replace the fast release-gate thresholds.

`nox profile <file.nox>` provides VM-level function call counts, cumulative function time, and operation counters for hot paths such as host callbacks, container literals, indexing, match patterns, and map helpers. It is separate from these benchmark smokes and does not yet provide parse/typecheck/compile/eval phase profiling.

The detailed Chinese benchmark notes are available in [`../zh_CN/benchmarks.md`](../zh_CN/benchmarks.md).
