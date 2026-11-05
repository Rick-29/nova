# Releasing nova-ta

Releases are driven by `cargo xtask`, a small Rust program in `xtask/` configured by
`release.toml`. It needs only the Rust toolchain plus the per-language tools listed
below, and runs the same way on Windows, Linux and macOS.

```sh
cargo xtask list                  # targets and publishers, and whether this machine can run them
cargo xtask check                 # 1. releasable? git state, versions, tests, warnings, publish dry-run
cargo xtask build                 # 2. build every binding into bindings/<lang>/ and pack dist/
cargo xtask publish --dry-run     # 3. show what a release would do
cargo xtask publish               #    tag, push, crates.io, registries, GitHub + GitLab releases
cargo xtask bump 0.2.0            # set a new version in Cargo.toml and every binding manifest
```

## What `check` enforces

1. **Git.** Clean working tree, on `master`, and the exact HEAD commit is present on every
   remote in `release.toml` (`origin` = GitLab, `main` = GitHub). The release tag must not
   exist locally or on any remote.
2. **One version everywhere.** `Cargo.toml` is the source of truth. Every file listed under
   `version_files` (Kotlin and Java `build.gradle.kts`, the Ruby gemspec, the C++
   `CMakeLists.txt`) must carry the same version. Python reads it from Cargo through
   maturin; Swift packages are versioned by git tag. `--sync-versions` rewrites the
   manifests instead of failing. The version must also not already be on crates.io.
3. **No warnings.** `cargo fmt --check`, `cargo clippy ... -D warnings`,
   `cargo test --workspace --all-features`, `cargo doc` with `RUSTDOCFLAGS=-D warnings`,
   and `cargo publish --workspace --dry-run`.

While iterating locally use `cargo xtask check --skip-git`; it turns the git and
crates.io conditions into warnings so the quality gate can be run on a dirty tree.

## Targets

Each `[[target]]` in `release.toml` is a list of steps (commands, copies, mkdirs) plus
`artifacts` globs that must exist afterwards. Output goes to `bindings/<name>/` and is
zipped into `dist/nova-ta-<version>-<name>-<os>-<arch>.zip` (or copied as-is via `dist`,
as the Python wheel is). Steps and whole targets can be limited with `only_on`, and a
target can declare the executables it `requires`.

| Target | Tool | Notes |
| --- | --- | --- |
| `python` | maturin | PyO3 wheel + stubs; wheel goes straight to `dist/` |
| `kotlin` | uniffi-bindgen (built from this repo) | Gradle project, JNA, library under `src/main/resources/<os-arch>/` |
| `swift` | uniffi-bindgen | SwiftPM layout; XCFramework steps run on macOS only |
| `ruby` | uniffi-bindgen | gem layout with `nova_ta.gemspec` |
| `go` | [uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go) | cgo package; generated code is also written to the tracked `go/nova_ta/` so `go get` works from the tag. `cargo install uniffi-bindgen-go --git https://github.com/NordSecurity/uniffi-bindgen-go --tag v0.7.1+v0.31.0` |
| `cpp` | [uniffi-bindgen-cpp](https://github.com/NordSecurity/uniffi-bindgen-cpp) | C++20 sources + CMake project. `cargo install uniffi-bindgen-cpp --git https://github.com/NordSecurity/uniffi-bindgen-cpp --tag v0.10.0+v0.31.0` |
| `java` | [uniffi-bindgen-java](https://github.com/IronCoreLabs/uniffi-bindgen-java) | **disabled**: needs UniFFI 0.32 metadata, the crate pins 0.31 for Go and C++ |
| `python-uniffi`, `wasm`, `node` | see COMPILE.md | disabled by default; `cargo xtask build -t wasm` builds one anyway |

**UniFFI version.** Third-party generators are built against one UniFFI release, encoded in
their tag (`+v0.31.0`). The `uniffi` dependency in `Cargo.toml` is pinned to the version the
enabled generators support; bump them together.

**Go publishing.** Go has no registry: `go get` fetches the module from the git tag. That is
why the `go` target also writes into the tracked `go/nova_ta/` directory, and why `publish`
refuses to tag when the build changed tracked files: regenerate, commit, push, then publish.

To add a language, add a `[[target]]` block. The placeholders (`{version}`, `{lib}`,
`{out}`, `{platform}`, ...) are documented at the top of `release.toml`. Any generator
that accepts `--library <cdylib>` (uniffi-bindgen-go, uniffi-bindgen-cs, ...) follows the
same shape as the `java` target. Put a tracked, version-bearing manifest in
`packaging/<lang>/` and list it under `version_files` so `check` and `bump` keep it in sync.

## What `publish` does

1. Runs `check` and `build` (skip with `--skip-check` / `--skip-build`).
2. Writes `dist/RELEASE_NOTES.md` from the matching `CHANGELOG.md` section, or from the
   git log since the previous tag.
3. Creates the annotated tag `v<version>` and pushes branch + tag to every remote.
4. `cargo publish --workspace` (nova-bindings-macros first, then nova-ta).
5. Per-target `publish` steps (none enabled by default; the Ruby block shows `gem push`).
6. `[[publisher]]` blocks, each requiring its tools and environment variables:
   - `github`: `gh release create` with every file in `dist/` attached.
   - `gitlab`: `glab release create` against `GITLAB_HOST` (self-hosted). Install
     [glab](https://gitlab.com/gitlab-org/cli) and run `glab auth login --hostname gitlab.chipatrade.com`.
   - `pypi`: `maturin upload` of the wheels in `dist/`, with `MATURIN_PYPI_TOKEN` set.

Credentials live outside the repo. `cargo xtask` loads a gitignored `.env` file from the
repository root (see `.env.example`) for token-style variables such as
`MATURIN_PYPI_TOKEN` and `GITLAB_HOST`; `gh`, `glab`, `cargo` and `gem` keep their own
logins (`gh auth login`, `glab auth login`, `cargo login`, `gem signin`).

Missing tools or variables stop the release with a message; `--skip <publisher>` opts out
explicitly. `--dry-run` prints every command instead of running it.

Builds are per host: a Windows run produces Windows libraries. For a multi-platform
release run `cargo xtask build` on each OS (or in CI) and upload the combined `dist/`,
or let CI run `cargo xtask publish --skip-check` on a macOS runner after the tag exists.
