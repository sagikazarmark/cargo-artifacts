# cargo-artifacts

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/sagikazarmark/cargo-artifacts/ci.yaml?style=flat-square)](https://github.com/sagikazarmark/cargo-artifacts/actions/workflows/ci.yaml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/sagikazarmark/cargo-artifacts/badge?style=flat-square)](https://securityscorecards.dev/viewer/?uri=github.com/sagikazarmark/cargo-artifacts)
[![crates.io](https://img.shields.io/crates/v/cargo-artifacts?style=flat-square)](https://crates.io/crates/cargo-artifacts)
[![docs.rs](https://img.shields.io/docsrs/cargo-artifacts?style=flat-square)](https://docs.rs/cargo-artifacts)

**Copy artifacts from a `cargo build` message stream.**

## Features

- **Stable Cargo artifact collection** without nightly `-Z unstable-options`
- **Cargo subcommand support** via `cargo artifacts`
- **Stream-only operation** from stdin or saved build logs
- **Flat copy export** compatible with Cargo's unstable `--artifact-dir` behavior

## Usage

Collect artifacts from a `cargo build` pipeline:

```bash
cargo build --message-format=json-render-diagnostics | cargo artifacts copy --out-dir ./artifacts
```

or just list the artifact source paths:

```bash
cargo build --message-format=json-render-diagnostics | cargo artifacts list
```

Plain `--message-format=json` streams are also supported:

```bash
cargo build --message-format=json | cargo artifacts list
```

Read a saved output with `--input` or `-i`:

```bash
cargo artifacts list --input build.log
cargo artifacts copy -i build.log --out-dir ./artifacts
```

Use `--input -` or `-i -` to explicitly read from stdin.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
