// Demonstrates the "host namespace" convention: a host application registers
// multiple host functions under a common prefix and ships its own .nox stub
// module that re-exports them. This keeps the host extension surface explicit
// to scripts (every host function is visible via `import "..." as alias;`)
// while letting embedders ship coherent feature sets without having to expose
// their internal helper names.
//
// Convention:
// - Host functions use a `<namespace>__<function>` name (double underscore as
//   separator) when the host wants to expose them to scripts via a stub.
// - The host ships a small Nox stub (or registers it via a custom module
//   loader) that imports the host functions and re-exports them with the
//   public names the script should see.
// - Scripts only depend on the stub module, not on the underscored host names.

use nox::{Runtime, RuntimePermissions};
use nox_core::{HostFunctionBuilder, Type, Value};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut runtime = Runtime::with_permissions(RuntimePermissions::none());

    runtime.engine_mut().register_host_function(
        HostFunctionBuilder::new("hostmod__greeting", Type::Str).param("name", Type::Str),
        |args| match args {
            [Value::String(name)] => Ok(Value::string(format!("Hello, {name}!"))),
            _ => unreachable!(),
        },
    )?;
    runtime.engine_mut().register_host_function(
        HostFunctionBuilder::new("hostmod__counter", Type::Int),
        |_| Ok(Value::Int(42)),
    )?;

    let value = runtime.eval(
        r#"
        fn greeting(name: str) -> str {
            return hostmod__greeting(name);
        }
        fn counter() -> int {
            return hostmod__counter();
        }
        greeting("Nox") + " count=" + to_str_int(counter());
        "#,
    )?;

    assert_eq!(value, Value::string("Hello, Nox! count=42"));
    println!("rust embedding namespace example: ok ({value})");
    Ok(())
}
