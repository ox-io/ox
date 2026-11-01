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

See [ARCHITECTURE.md](./ARCHITECTURE.md) for crate boundaries and frontend flow.

