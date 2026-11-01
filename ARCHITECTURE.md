# Ox toolchain architecture

The production frontend follows:

```text
source → tokens → AST → resolved AST → typed HIR → MIR → backend
```

The initial workspace establishes source ownership, diagnostics, lexing, a declaration parser, semantic entry-point validation, formatting, JavaScript code generation, runtime boundaries, and the CLI. Each stage has a narrow API so the reference implementation can be replaced incrementally while conformance tests remain stable.

## Crate boundaries

- `ox-source`: immutable named source text.
- `ox-diagnostics`: source spans and user-facing diagnostics.
- `ox-lexer`: canonical tokens and reserved-keyword recognition.
- `ox-ast`: syntax-independent declaration data.
- `ox-parser`: token-to-AST frontend.
- `ox-types`: semantic checks and, later, typed HIR.
- `ox-formatter`: canonical source formatting.
- `ox-codegen-js`: portability backend during frontend development.
- `ox-runtime`: runtime contracts shared by backends.
- `ox-cli`: stable user command surface.

No framework package may depend on parser internals. Tools consume public compiler APIs or language-server protocols.

