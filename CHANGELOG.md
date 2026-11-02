# Changelog

## 0.2.0-alpha.8 — 2026-11-02

### Added

- Locked package imports for the Rust compiler and JavaScript reference frontend.
- Constraint-aware transitive semver selection and dependency-confusion checks.
- `ox pack`, `ox publish`, and `ox workspace` commands.
- Package-aware LSP diagnostics, completion, and definitions.
- Runtime built-in capability enforcement and dependency capability grants.
- Lockfile checksum chains, signature identities, registry checksum validation,
  deterministic `.oxpkg` archives, and end-to-end package security fixtures.

### Changed

- Lockfile format is now version 3. Regenerate alpha.7 lockfiles with `ox lock`.

### Security

- Package imports reject stale lockfiles, corrupted cache objects, mismatched
  registry identities, ungranted capabilities, and optionally unsigned or
  untrusted signing identities.
