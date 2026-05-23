# Language v0

Nox is a small statically typed scripting language. Source files use the `.nox` extension.

The implemented v0 surface includes:

- Typed `let` and `const` bindings.
- Typed functions with explicit parameter and return types, including function-level generics
  such as `fn id<T>(value: T) -> T`.
- Top-level type aliases such as `type UserId = int;` and
  `type Pair = (UserId, str);`.
- User enums / sum types such as `enum LoadState { Loading, Ready(int), Failed(str), }`.
- Blocks, `if`, `else if`, `if let`, `let ... else`, `while`, `while let`, half-open integer `for` ranges, `break`, `continue`, and `return`.
- `int`, `float`, `bool`, `str`, `json`, arrays `[T]`, tuples such as
  `(int, str)`, `map[str, T]`, named `record` and `enum` types, `option[T]`, and `result[T, E]`.
  Array and map value annotations can contain tuple, map, option, and result types.
- Integer bitwise operators `&`, `|`, `^`, `<<`, `>>`, and unary `~`.
- Array and map spread in literals, for example `[...items, value]` and
  `{...defaults, "k": value}`.
- Tuple destructuring with `let (a, b) = pair;` and record destructuring with
  `let { x, y } = point;`.
- Postfix `?` propagation for `option[T]` and `result[T, E]` inside functions.
- Record method call syntax: `record_value.method(args)` is call sugar for
  `method(record_value, args)`.
- Relative imports, `export` visibility, and namespace imports with `import "path" as name;`.
- Limited statement `match` for number/string constants, half-open integer
  ranges, and nested option/result payload patterns.
- String interpolation with `${expr}` placeholders. Placeholders automatically stringify
  `null`, `bool`, `int`, `float`, and `str`; use `\$` for a literal dollar sign.
- Builtins such as `len`, `contains`, `map_get`, `map_keys`, `map_values`,
  `map_has`, and `map_size`.
- Default-runtime output helpers: `print(value: str) -> null` plus
  `to_str_int`, `to_str_float`, `to_str_bool`, `to_str_null`, and `to_str_str`.
- `std/string.nox` helpers: `split`, `join`, `substring`, `trim`, `replace`,
  `starts_with`, `ends_with`, `index_of`, `contains`, `last_index_of`,
  `repeat`, `pad_left`, `pad_right`, `parse_int`, `parse_float`, `lines`,
  `to_upper`, and `to_lower`.
- `std/json.nox` helpers: `parse(value: str) -> result[json, str]`,
  `stringify(value: json) -> str`, `kind`, and basic array/object accessors.
- `std/csv.nox` and `std/tsv.nox` line helpers for parsing and formatting
  delimited text rows.
- `std/array.nox`, `std/map.nox`, `std/option.nox`, and `std/result.nox`
  copy-oriented data-structure helpers without mutation or higher-order functions.
- `std/process.nox` helpers: `argv`, `read_stdin`, `print_err`, and `exit` for
  command-line scripts.
- `std/path.nox` helpers: `join`, `basename`, `dirname`, `extension`, and
  `normalize`; `std/fs.nox` also includes file classification and directory
  listing helpers under the existing filesystem capability.

Runtime capabilities are not language syntax. File, environment, timer, network, and async task helpers are exposed by the default runtime only when explicitly permitted.
`print` writes one line to stdout and returns `null`; `nox run` does not print an extra `null` for a script whose final value is `null`.
The string helpers are pure computation and do not require any runtime capability.

`json` is an opaque standard-library value type for RFC 8259 JSON values:
number, string, bool, null, array, and object. Use `std/json.nox` to parse,
stringify, inspect the kind, and read arrays or objects through helper
functions; direct field/index syntax is intentionally not part of this small v0
surface.

Maps currently use `str` keys. `contains(map, key)` and `map_has(map, key)`
return whether a key exists without raising a missing-key diagnostic.
`map_get(map, key)` returns `option[T]`. `map_keys(map) -> [str]` and
`map_values(map) -> [T]` return arrays in stable lexicographic key order;
`map_size(map) -> int` returns the entry count.

Tuple literals use the same fixed arity as their type annotation:
`let pair: (int, str) = (42, "nox");`. Tuple arity mismatches use
`tuple.arity-mismatch`; tuple element type mismatches use
`tuple.element-type-mismatch`.

Type aliases are transparent during type checking and can refer to built-in,
tuple, array, map, option/result, and record types. Cyclic aliases use the
stable diagnostic code `type-alias.cyclic`.

User enum variants are constructed as `EnumName.Variant` or
`EnumName.Variant(value)`. Enum matches must cover every variant and do not
accept `_` defaults. Missing variant coverage uses `match.non-exhaustive`;
unknown variants use `enum.variant-not-found`.

Generic functions support function-level type parameters only. Calls infer type
parameters from argument types and, where needed, from the expected return type.
Inference conflicts or missing inference context use `generic.infer-failed`.
Generic records, generic traits, explicit type arguments, source-level function
type annotations, higher-order functions, and closures are not part of v0.

Bitwise operators require `int` operands and return `int`; non-`int` operands
use `type.bitwise-non-int`. Operations use the 64-bit signed `int`
two's-complement representation. `>>` is arithmetic right shift and preserves
the sign bit. Shift counts must be in `0..64`; out-of-range shifts are runtime
diagnostics.

`expr?` unwraps `some(value)` / `ok(value)` and returns early on `none` / `err(error)`.
For `result[T, E]?`, the enclosing function must return `result[U, E]`; for `option[T]?`,
it must return `option[U]`. Mismatches use the stable diagnostic code
`result.question-mark.mismatch`.

Record method syntax is checked against ordinary visible functions. The method
function must be available in the current module or an imported module, and its
first parameter must match the receiver record type. Missing or mismatched
methods use the stable diagnostic code `record.method-not-found`.

`match` cases support `int`, `float`, and `str` literals. `int` matches also support
half-open range patterns such as `0..10`. `option` and `result` payload patterns can
be nested, for example `some(some(value))`, `some(none)`, and `ok(some(value))`.
Non-exhaustive matches use the stable diagnostic code `match.non-exhaustive`.

`if let`, `let ... else`, and `while let` reuse the same pattern semantics as
`match` for `option`, `result`, and user enum values. Bindings introduced by
`if let` and `while let` are scoped to the successful branch or loop body.
Bindings introduced by `let ... else` are available after the statement, and
the `else` branch must return before those bindings are used. A fallthrough
`let ... else` branch uses `control-flow.let-else-fallthrough`.

Spread in array and map literals always creates a new container and never
mutates the spread source. Array spread requires a `[T]` source; map spread
requires a `map[str, T]` source. Element/value types must match the rest of the
literal. Map merge order follows source order, so later keys overwrite earlier
keys. Spread type mismatches use `type.spread-mismatch`.

Current non-goals include JavaScript compatibility, Node.js package compatibility, JIT compilation, browser APIs, mutable arrays, slices, closure/function types, and a package registry.

Integer literals support decimal, `0xff` hexadecimal, `0b1010` binary,
`0o17` octal, and `_` separators such as `1_000_000`. Malformed integer
literals use the stable diagnostic code `lex.invalid-integer`.

String literals support normal `"..."` strings, multiline `"""..."""` strings,
and raw `r"..."` strings. Normal strings keep the existing escape and `${expr}`
interpolation behavior. Multiline strings preserve embedded newlines. Raw
strings do not interpret escapes or interpolation markers.
Single-quoted character literals such as `'A'`, `'界'`, and `'\n'` lower to
single-Unicode-scalar `str` values. Nox does not currently expose separate
`char` or `bytes` types.

The detailed Chinese language reference is available in [`../zh_CN/language-v0.md`](../zh_CN/language-v0.md).
