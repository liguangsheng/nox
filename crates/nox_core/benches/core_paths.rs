use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nox_core::Engine;

const CHECK_SOURCE: &str = r#"
fn fib(n: int) -> int {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

let answer: int = fib(10);
answer;
"#;

const LOOP_SOURCE: &str = r#"
let total: int = 0;
let i: int = 0;
while (i < 20000) {
    total = total + i;
    i = i + 1;
}
total;
"#;

const CONTAINER_SOURCE: &str = r#"
fn build(count: int) -> int {
    let i: int = 0;
    let captured: int = 0;
    while (i < count) {
        let values: [int] = [i, i, i, i];
        let scores: map[str, int] = {
            "first": values[0],
            "second": values[1],
        };
        captured = captured + scores["first"];
        i = i + 1;
    }
    return captured;
}

build(1000);
"#;

const LAMBDA_SOURCE: &str = r#"
fn apply(f: fn(int) -> int, n: int) -> int {
    let total: int = 0;
    let i: int = 0;
    while (i < n) {
        total = total + f(i);
        i = i + 1;
    }
    return total;
}

let double: fn(int) -> int = fn(x: int) -> int { return x * 2; };
apply(double, 1000);
"#;

fn bench_core_paths(c: &mut Criterion) {
    c.bench_function("core/check-recursion", |b| {
        b.iter(|| {
            let mut engine = Engine::new();
            engine.check(black_box(CHECK_SOURCE)).unwrap();
        })
    });

    c.bench_function("core/eval-loop", |b| {
        b.iter(|| {
            let mut engine = Engine::new();
            black_box(engine.eval(black_box(LOOP_SOURCE)).unwrap());
        })
    });

    c.bench_function("core/eval-containers", |b| {
        b.iter(|| {
            let mut engine = Engine::new();
            black_box(engine.eval(black_box(CONTAINER_SOURCE)).unwrap());
        })
    });

    c.bench_function("core/eval-lambda", |b| {
        b.iter(|| {
            let mut engine = Engine::new();
            black_box(engine.eval(black_box(LAMBDA_SOURCE)).unwrap());
        })
    });
}

criterion_group!(core_paths, bench_core_paths);
criterion_main!(core_paths);
