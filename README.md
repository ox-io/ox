# ox

Production Ox toolchain monorepo: compiler, runtime, CLI, formatter, linter,
package manager, language server, standard library, documentation generator, test
runner, and installer.

## Status

The dependency-free Rust workspace now contains the first production frontend
foundation: source ownership, diagnostics, lexer, declaration parser, entry-point
checking, formatter boundary, portability backend boundary, runtime metadata, and
the `ox` CLI.

## Scope

- Keep this repository focused on the responsibility above.
- Track compatibility against `ox-spec` and conformance through `ox-tests`.
- Document supported versions, security boundaries, and release policy before the first public release.

## Development

Rust 1.85 or newer is required.

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo run -p ox-cli -- check ../ox-tests/fixtures/core/main.ox
cargo run -p ox-cli -- keywords
```

## Packages

`ox.toml` accepts local, Git, and semver registry dependencies. Resolution writes
an integrity-checked `ox.lock` and stores immutable package content in the global
cache (`$OX_CACHE_HOME`, or `$HOME/.ox/cache/v2`). A registry is either a local
index directory or a Git repository selected by `OX_REGISTRY`; its layout is
`PACKAGE/VERSION/ox.toml`.

```toml
[dependencies]
json = "^1.2.0"
auth = { git = "https://github.com/example/auth.ox.git", tag = "v2.0.0" }
shared = { path = "../shared" }

[capabilities]
network = false
filesystem = false
environment = false
secrets = false
```

```sh
ox add json '^1.2.0'
ox add auth 'git+https://github.com/example/auth.ox.git'
ox remove json
ox lock
ox lock --offline
ox update
ox vendor
ox audit
```

Package capabilities are deny-by-default metadata recorded in the lockfile.
`ox audit` reads `OX_ADVISORY_DB` (default `.ox/advisories.txt`) entries formatted
as `package|version|advisory` and exits unsuccessfully when a locked version is
affected.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for crate boundaries and frontend flow.

