# Language v0

Nox is a small statically typed scripting language. Source files use the `.nox` extension.

The implemented v0 surface includes:

- Typed `let` and `const` bindings.
- Typed functions with explicit parameter and return types.
- Blocks, `if`, `else if`, `while`, half-open integer `for` ranges, `break`, `continue`, and `return`.
- `int`, `float`, `bool`, `str`, arrays `[T]`, `map[str, T]`, named `record` types, `option[T]`, and `result[T, E]`.
- Relative imports, `export` visibility, and namespace imports with `import "path" as name;`.
- Limited statement `match` for integer/string constants and option/result payload handling.
- Builtins such as `len`, `contains`, and `map_get`.

Runtime capabilities are not language syntax. File, environment, timer, network, and async task helpers are exposed by the default runtime only when explicitly permitted.

Current non-goals include JavaScript compatibility, Node.js package compatibility, JIT compilation, browser APIs, mutable arrays, slices, closure/function types, and a package registry.

The detailed Chinese language reference is available in [`../zh_CN/language-v0.md`](../zh_CN/language-v0.md).
