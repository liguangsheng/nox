# Runtime

The `nox` crate provides the default runtime on top of `nox_core`.

By default, runtime permissions are conservative. The CLI grants only the filesystem read access needed to load the entry file and imports. Environment variables, timers, network access, filesystem writes, and async task helpers require explicit permission.

Prefer static standard library modules:

```nox
import "std/fs.nox" as fs;
import "std/env.nox" as env;
import "std/time.nox" as time;
```

Older global functions remain available as compatibility surface, but new code should prefer namespace imports.

The detailed Chinese runtime guide is available in [`../zh_CN/runtime.md`](../zh_CN/runtime.md).
