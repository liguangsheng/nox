use nox::{Runtime, RuntimePermissions};
use nox_core::{Diagnostic, Engine, HostFunctionBuilder, Span, Type, Value};
use std::{env, error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.register_host_function(HostFunctionBuilder::new("host_config", Type::Str), |_| {
        Err(Diagnostic::new(
            "config unavailable",
            Span { start: 0, end: 0 },
        ))
    })?;

    let err = engine.eval("host_config();").unwrap_err();
    assert_eq!(err.code, "host.callback");
    assert!(err.message.contains("host function 'host_config'"));

    let dir = env::temp_dir().join(format!("nox-rust-embedding-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let data = dir.join("input.txt");
    let script = dir.join("main.nox");
    fs::write(&data, "from host")?;
    fs::write(
        &script,
        format!(
            r#"import "std/fs.nox" as fs;

fn load(path: str) -> str {{
    let loaded: result[str, str] = fs.try_read_text(path);
    match (loaded) {{
        ok(body) => {{
            return body;
        }}
        err(message) => {{
            return message;
        }}
    }}
}}

load("{}");
"#,
            data.display()
        ),
    )?;

    let mut runtime =
        Runtime::with_permissions(RuntimePermissions::none().allow_filesystem_read_under(&dir));
    let value = runtime.eval_file(&script)?;
    assert_eq!(value, Value::string("from host"));

    let denied_script = dir.join("denied.nox");
    fs::write(
        &denied_script,
        r#"import "std/fs.nox" as fs;

fs.try_read_text("../Cargo.toml");
"#,
    )?;
    let denied = runtime.eval_file(&denied_script).unwrap_err();
    assert!(denied.message.contains("filesystem read permission denied"));

    let mut tasks = Runtime::with_permissions(RuntimePermissions {
        async_tasks: true,
        ..RuntimePermissions::none()
    });
    let before = tasks.pending_async_task_count();
    let err = tasks
        .eval("task_sleep_ms(60000); task_ready(999);")
        .unwrap_err();
    assert!(err.message.contains("unknown async task id"));
    assert_eq!(tasks.pending_async_task_count(), before);

    Ok(())
}
