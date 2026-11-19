# Security Policy

## Supported versions

impl-scan is at `0.1.x`. Only the latest release receives fixes.

## Reporting a vulnerability

Please do **not** open a public issue for a security problem.

Use GitHub's private vulnerability reporting instead:
[Report a vulnerability](https://github.com/GNP50/impl-scan/security/advisories/new).

This opens a private advisory visible only to the maintainer. Expect an
acknowledgement within a few days; this is a spare-time project, so please
allow reasonable time for a fix before any public disclosure.

## Scope

impl-scan runs inside your build script. It performs no network I/O and sends no
telemetry. It reads the `.rs` files under the directory you pass to `Scanner::new`,
the templates and partials you pass to `Renderer`, and writes only the output paths
you pass to `render_to_file` (plus `$OUT_DIR/impl_scan.json` when `IMPL_SCAN_DUMP` is set).

The parts worth scrutiny are:

- `impl-scan/src/scan.rs`: source parsing, `use` resolution and `#[register]` parsing.
- `impl-scan/src/render.rs`: template rendering and output writing.
- `impl-scan/src/partials/`: the code the built-in partials generate into your crate.

Generated code is ordinary Rust compiled with your crate; it contains no `unsafe`.
Reports of a source tree or template that makes the scanner or renderer misbehave
(panic, hang, write outside the given paths) are in scope and welcome.
