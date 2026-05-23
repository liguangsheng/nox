use std::{fs, path::PathBuf};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nox::{MockNetwork, Runtime, RuntimePermissions};

fn bench_data_path() -> PathBuf {
    let path = std::env::temp_dir().join("nox-runtime-capability-bench-data.txt");
    fs::write(&path, "nox-runtime-capability-bench\n").unwrap();
    path
}

fn bench_runtime_capabilities(c: &mut Criterion) {
    let data_path = bench_data_path();
    let fs_source = format!(
        r#"
        let text: str = read_text("{}");
        len(text);
        "#,
        data_path.display()
    );
    let fs_permissions =
        RuntimePermissions::none().allow_filesystem_read_under(std::env::temp_dir());

    c.bench_function("runtime/fs-read-text", |b| {
        b.iter(|| {
            let mut runtime = Runtime::with_permissions(fs_permissions.clone());
            black_box(runtime.eval(black_box(&fs_source)).unwrap());
        })
    });

    let async_permissions = RuntimePermissions {
        async_tasks: true,
        async_task_max_pending: Some(32),
        ..RuntimePermissions::none()
    };
    const TASK_SOURCE: &str = r#"
        let task: int = task_sleep_ms(0);
        task_ready(task);
    "#;

    c.bench_function("runtime/async-task-ready", |b| {
        b.iter(|| {
            let mut runtime = Runtime::with_permissions(async_permissions.clone());
            black_box(runtime.eval(black_box(TASK_SOURCE)).unwrap());
        })
    });

    let http_permissions = RuntimePermissions {
        network: true,
        ..RuntimePermissions::none()
    };
    const HTTP_SOURCE: &str = r#"
        import "std/http.nox" as http;
        let response: result[(int, str), str] = http.get("http://bench.local/payload", 1);
        match (response) {
            ok(payload) => {
                let (status, body) = payload;
                status + len(body);
            }
            err(_) => { 0; }
        }
    "#;

    c.bench_function("runtime/http-get-mock", |b| {
        b.iter(|| {
            let mut runtime = Runtime::with_permissions(http_permissions.clone());
            runtime.set_mock_network(Some(MockNetwork::new().with_http_text_response(
                "GET",
                "http://bench.local/payload",
                200,
                "bench-body",
            )));
            black_box(runtime.eval(black_box(HTTP_SOURCE)).unwrap());
        })
    });
}

criterion_group!(runtime_capabilities, bench_runtime_capabilities);
criterion_main!(runtime_capabilities);
