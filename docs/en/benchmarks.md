# Benchmarks

Benchmark smoke tests are intended to catch obvious performance or execution regressions, not to enforce portable hard thresholds.

Run:

```sh
scripts/bench-smoke.sh
```

The smoke covers representative recursion, loop, container, module, and `nox test` workflows. Output is useful for same-machine comparisons between commits.

The detailed Chinese benchmark notes are available in [`../zh_CN/benchmarks.md`](../zh_CN/benchmarks.md).
