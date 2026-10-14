# Developing GitLeaf

Everything here is for people changing GitLeaf itself. To use GitLeaf, follow
[Get started](../README.md#get-started) instead; release downloads need no Rust, Node.js or npm.

## Build from source

Use this route if you want to edit GitLeaf or run source changes that have not been released yet.
For everyday use, follow [Get started](../README.md#get-started) above.

Install a current stable [Rust toolchain](https://www.rust-lang.org/tools/install),
[Node.js](https://nodejs.org/en/download) 22.13 or newer (Node 24 is used in CI), npm and Git.
The Node.js installer includes npm. Reopen your terminal and check `cargo --version`,
`node --version`, `npm --version` and `git --version` before continuing.
Configure your Git name and email before creating projects; GitLeaf reports a failed initial commit
instead of silently creating an incomplete project. On Windows, replace the last command below
with `.\target\release\gitleaf.exe`.

```bash
git clone https://github.com/kmarkiv/gitleaf.git
cd gitleaf
npm ci
npm run build
cargo build --release --locked
./target/release/gitleaf
```

On Windows, run `target\release\gitleaf.exe`. End users running a release binary do not need Node,
npm or Rust. `static/assets/` is generated and ignored by Git; regenerate it after frontend changes.
The Rust build embeds every asset, so the finished binary does not need a separate web server or
an assets folder at runtime. Third-party license notices are available at
`http://127.0.0.1:8077/assets/THIRD_PARTY_NOTICES.txt`.

`./run.sh --no-open` builds and runs on Unix. Set `GITLEAF_REBUILD_UI=1` after changing the frontend,
or run `npm run build` explicitly. The launcher forwards command-line options to the binary.

## Rust backend and runtime dependencies

Rust owns HTTP downloads, checksum verification, compiler archive extraction, generated-file
classification and main-document selection. A shared reqwest client with Rustls reuses connections;
release URLs and redirects require HTTPS, while user-requested ZIP imports retain HTTP/HTTPS support.
Binary downloads stream to a temporary file with incremental SHA-256 and a 128 MiB limit; ZIP imports
are capped at 64 MiB and text responses at 8 MiB. Failed downloads preserve the previous destination.

Compiler extraction runs on a blocking worker. It reads only a regular executable from TAR.GZ or ZIP,
rejects duplicate/link executables, and bounds file sizes and entry counts. Explorer metadata reports
generated files and the detected main document in the existing file-list response, so opening a
nonstandard main needs one file read rather than a browser loop of file-content probes. Main detection
ignores commented document classes, searches shallow paths first and reads at most 64 KiB per candidate.
Explicit main-file preferences in the browser still take precedence.

Git and LaTeX compilation use the existing system tools and credentials. JavaScript remains responsible
for browser editing, PDF rendering and visual-mode interaction; the Mac launcher remains native Swift.
Source builds no longer require curl or tar for network operations or compiler installation. The
copy-paste initial download commands in the README still use curl or PowerShell.

## Validation

```bash
npm ci
npm run build
npm test
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
npm audit --audit-level=high
cargo install cargo-audit --locked
cargo audit
```

The Rust suite exercises path containment, protected Git metadata, archive extraction, checksum
verification, log parsing, SyncTeX, durable writes and actual HTTP routes, including request limits
and cross-origin protection. The JavaScript suite tests saves that overlap, edits made during a
save, retries after failure, and refusing to compile a continuously changing unsaved document.

For browser checks, install Chromium and run against the freshly built binary:

```bash
npx playwright install chromium
GITLEAF_BIN="$PWD/target/release/gitleaf" npm run test:browser
```

The browser suite starts an isolated server with temporary projects and a deterministic test
compiler. It verifies offline startup (all external browser requests blocked), corrupt preference
recovery, project creation, editing, save races, PDF preservation after failure, visual-mode source
preservation, Focus Mode, project switching and layouts at 900, 1366, 1440 and 1920 pixels.
It does not modify your papers or Git identity and does not substitute for testing a real TeX engine.
To exercise the installed engine, run `GITLEAF_BIN="$PWD/target/release/gitleaf" npm run test:latex`.
This uses temporary projects and a temporary Tectonic package cache; it checks real compilation,
both SyncTeX directions, structured errors and PDF preservation. Set `GITLEAF_REAL_ENGINE=latexmk`
to check a TeX Live installation instead. The first Tectonic run may download packages.
Screenshots are written to `test-artifacts/` (or `GITLEAF_SCREENSHOT_DIR`).

For an optional live download check, run:

```bash
GITLEAF_BIN="$PWD/target/release/gitleaf" npm run test:downloads
```

This uses temporary projects and checks a real HTTPS release request, verified Tectonic installation
and a local HTTP ZIP import. On Unix it removes curl and tar from the test server's PATH, retaining
only Git. It requires internet and downloads a temporary official compiler; it leaves your tools,
papers and Git settings untouched. Windows uses the normal PATH. Network tests are separate from
the default unit suite.

## Building a Mac package

The Mac packaging script creates a Finder-launchable **GitLeaf.app**, a drag-to-Applications
**DMG**, and an **app ZIP**. It requires macOS, Python 3 and Apple's Xcode Command Line Tools
(`xcode-select --install`). Build the frontend and release binary first, as in
[Build from source](#build-from-source), then run on an Apple silicon Mac:

```bash
python3 scripts/package-macos.py --binary target/release/gitleaf --arch arm64 --output dist
```

On an Intel Mac use `--arch x86_64`. The binary architecture and version must match the package.
The generated app requires **macOS 13 or newer** and includes the editor assets and license notices.
The native launcher finds common Homebrew and MacTeX paths, writes startup diagnostics to
`~/Library/Logs/GitLeaf/gitleaf.log`, and shuts down its server when you quit the app. Git and a
LaTeX compiler remain separate dependencies; first Compile can install Tectonic.

Without signing credentials, packages use an **ad-hoc signature** and are **not notarized**;
macOS may require first-launch approval. For distribution without that warning, use an Apple
Developer ID Application identity and a configured `notarytool` keychain profile:

```bash
export MACOS_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export MACOS_NOTARY_PROFILE="gitleaf-notary"
python3 scripts/package-macos.py --binary target/release/gitleaf --arch arm64 --output dist
```

These are examples: configure your own certificate and credentials using Apple's
[notarization guidance](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
The script signs the inner executables and bundle, notarizes and staples the app, creates the
DMG, then signs, notarizes and staples the DMG. Signing or notarization failures stop packaging.
The default release workflow builds ad-hoc packages; trusted signing credentials must be configured
separately before producing a notarized release. For a local launch test, extract the app ZIP into a temporary folder, then run:

```bash
python3 tests/macos-package.py /path/to/GitLeaf.app
```

The test uses temporary projects, blocks automatic update checks, suppresses browser opening,
and checks startup, embedded assets, existing-instance reuse, update instructions and graceful Quit.
Both Mac architecture jobs attach the DMG and
app ZIP alongside the existing executable, and their checksums are included in `SHA256SUMS.txt`.

## Releasing

CI runs frontend tests, Rust formatting, static checks, unit/HTTP tests and release builds on Linux,
macOS and Windows. macOS also builds and smoke-tests the native app package; Linux runs browser regressions. A separate job checks RustSec advisories.
These workflows must pass on the release commit before shipping; local testing cannot validate all
three operating systems.

Keep `Cargo.toml`, `package.json` and their lockfiles on the same version. Tag a validated commit
`vX.Y.Z` and push it. `.github/workflows/release.yml` builds macOS (Apple silicon and Intel), Linux
(x86_64 and ARM64) and Windows binaries with locked dependencies. It verifies version/tag agreement,
attaches `SHA256SUMS.txt`, and publishes a GitHub release. Review platform validation before pushing a release tag.
The download links above point to the latest published release, not untagged source changes.

The updater requires the exact asset checksum from `SHA256SUMS.txt`, validates the executable's
version, and replaces it on the executable's own filesystem before a graceful restart. Source
builds must be rebuilt manually. Unit tests cover version comparison, URL validation and exact
checksum selection; actual replacement/restart needs a release smoke test on each platform.

The Mac packaging script supports signing and notarization, but the default CI/release jobs do
not have Apple credentials configured. Windows code signing is not configured. Review platform smoke tests,
artifact checksums and release notes before tagging a release.

## Layout

- `src/main.rs`: configuration, router, existing-instance verification, browser launch and shutdown.
- `src/api.rs`: projects, files, compilation, SyncTeX, Git and template discovery.
- `src/update.rs`: cached release checks and verified, graceful self-updates.
- `src/downloads.rs`: reusable Rust HTTP clients, bounded downloads and streaming checksums.
- `src/engine_archive.rs`: compiler-only TAR.GZ/ZIP extraction with limits and no link unpacking.
- `src/latex_files.rs`: generated-file classification and bounded main-document detection.
- `src/security.rs`: localhost/origin checks and browser security headers.
- `src/storage.rs`: durable atomic file replacement.
- `src/assets.rs` and `build.rs`: embedded browser assets.
- `src/tests.rs`: regression tests through the actual HTTP router.
- `src/templates.rs` and `templates/`: built-in project templates.
- `static/index.html`: application markup.
- `frontend/`: editor behavior, styles and the save coordinator.
- `scripts/build-ui.mjs`: reproducible local browser bundling and license notices.
- `scripts/package-macos.py` and `packaging/macos/`: Mac app launcher, icon and packaging pipeline.
- `tests/`: JavaScript unit tests and isolated browser regressions.
