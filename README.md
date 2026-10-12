<p align="center">
  <img src="docs/logo.svg" width="72" alt="">
</p>
<h1 align="center">GitLeaf — Free, Lightweight, Offline LaTeX Editor</h1>
<p align="center">
  A free, lightweight, open-source LaTeX editor for Mac, Windows and Linux.<br>
  Write locally with Git history, autosave and a side-by-side PDF preview; work offline after compiler setup.<br>
  Every paper is a plain git repository, so your work lives on GitHub, Overleaf's git bridge, or any remote you like.
</p>

<p align="center">
  <a href="https://kmarkiv.com/gitleaf/">GitLeaf website</a> ·
  <a href="https://kmarkiv.com/gitleaf/guide/">Setup guide</a>
</p>

<p align="center">
  <a href="https://github.com/kmarkiv/gitleaf/releases/latest"><img src="https://img.shields.io/github/v/release/kmarkiv/gitleaf?label=release&color=32C793" alt="Latest release"></a>
  <a href="https://github.com/kmarkiv/gitleaf/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/kmarkiv/gitleaf/ci.yml?label=tests" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center">
  <img src="docs/screenshot.jpg" width="820" alt="gitleaf: file explorer, LaTeX editor with tabs, and PDF preview side by side">
</p>

## Get started

**Install Git → start GitLeaf → create a paper → Compile.** GitLeaf is a free, lightweight LaTeX
editor that runs on your computer and opens in your browser. **No LaTeX installation is needed
to get started:** if a compiler is missing, your first Compile offers to install Tectonic for you. You can also use a
[full LaTeX installation](#latex-compiler-options) such as MacTeX or TeX Live. You do not need a
GitHub account to write a paper, and the release download does not require Rust, Node.js or a
separate web server. The editor, PDF viewer, autosave and local Git commits work offline.
Offline compilation requires a compiler and all of your paper’s packages already cached or
installed; see [Offline LaTeX editing](#offline-latex-editing).

[Setup](#1-install-git-once) · [Your first paper](#3-create-your-first-paper) ·
[LaTeX options](#latex-compiler-options) · [Troubleshooting](#troubleshooting) · [Build from source](#build-from-source)

### 1. Install Git (once)

Open **Terminal** on macOS/Linux or **PowerShell** on Windows, then check:

```sh
git --version
```

If you see a version number, Git is ready. If Git is missing:

| Computer | Install Git |
|---|---|
| macOS | Run `xcode-select --install`, finish the installer, then reopen Terminal. If you already use Homebrew, `brew install git` is another option. [Git for macOS](https://git-scm.com/install/mac) |
| Windows | Install [Git for Windows](https://git-scm.com/install/windows) with its default options, then reopen PowerShell. |
| Ubuntu / Debian | Run `sudo apt update` followed by `sudo apt install git curl tar`. |
| Other Linux | Follow [Git's Linux instructions](https://git-scm.com/install/linux); also make sure `curl` and `tar` are installed. |

GitLeaf uses Git to keep each paper's history. Before creating your first project, set your name
and email below, replacing both examples with your own details. Skip this if your Git identity is
already configured. These settings apply to all your Git repositories on this computer; they are
an author label, not a login.

```sh
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

### 2. Download and start GitLeaf

**Mac app package:** On the [release page](https://github.com/kmarkiv/gitleaf/releases/latest),
look for `gitleaf-macos-arm64.dmg` (Apple silicon) or `gitleaf-macos-x86_64.dmg` (Intel).
If a standalone GitLeaf is already running in Terminal, stop it with **Ctrl+C** first.
When a DMG is available, open it, drag **GitLeaf.app** into **Applications**, eject the disk image,
and open GitLeaf from Applications. It opens the editor in your browser without a terminal window.
The leaf icon in the menu bar offers **Open GitLeaf in Browser**, **Show GitLeaf Log**, and **Quit GitLeaf**.
Git still needs to be installed and configured as in step 1. For a macOS approval prompt, follow
Apple's [opening instructions](https://support.apple.com/en-us/102445).

Older releases may only include the standalone executables below. A `.app.zip` is also produced
by the Mac packaging workflow: extract it and move GitLeaf.app into Applications.

Choose **one** download for your computer. Each is a single executable file, with no installer.
Use a current desktop browser. Internet access is needed for downloading GitLeaf and, if you do
not already have LaTeX, downloading the compiler and its packages on your first compile.

| Computer | Download |
|---|---|
| Mac with Apple silicon | [gitleaf-macos-arm64](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-arm64) |
| Mac with an Intel processor | [gitleaf-macos-x86_64](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-x86_64) |
| Windows (x64) | [gitleaf-windows-x86_64.exe](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-windows-x86_64.exe) |
| Linux (x86_64, most PCs) | [gitleaf-linux-x86_64](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-linux-x86_64) |
| Linux (ARM64, e.g. a Raspberry Pi with a 64-bit OS) | [gitleaf-linux-arm64](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-linux-arm64) |

Not sure which Mac you have? **Apple menu → About This Mac**: “Chip: Apple M…” means Apple silicon;
“Processor: Intel” means Intel. On Linux, `uname -m` reports `x86_64` or `aarch64` (ARM64).

**Windows:** Download the `.exe` and double-click it. Keep the console window open while using
GitLeaf. If Windows SmartScreen asks, choose **More info → Run anyway** only for the file you
intended to download from this repository. You can move the executable to a permanent folder and
make a desktop shortcut to reopen it later.

**macOS / Linux:** The easiest terminal route downloads the right file, makes it executable and
starts it. Expand your computer's instructions below and paste the complete command into Terminal.
The file is kept in a dedicated folder so you can find it again.

<details>
<summary>Mac with Apple silicon</summary>

```bash
mkdir -p "$HOME/Applications/gitleaf" &&
cd "$HOME/Applications/gitleaf" &&
curl --fail --location --output gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-arm64 &&
chmod +x gitleaf &&
./gitleaf
```

</details>

<details>
<summary>Mac with Intel</summary>

```bash
mkdir -p "$HOME/Applications/gitleaf" &&
cd "$HOME/Applications/gitleaf" &&
curl --fail --location --output gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-x86_64 &&
chmod +x gitleaf &&
./gitleaf
```

</details>

<details>
<summary>Linux (x86_64)</summary>

```bash
mkdir -p "$HOME/.local/bin/gitleaf-app" &&
cd "$HOME/.local/bin/gitleaf-app" &&
curl --fail --location --output gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-linux-x86_64 &&
chmod +x gitleaf &&
./gitleaf
```

</details>

<details>
<summary>Linux (ARM64)</summary>

```bash
mkdir -p "$HOME/.local/bin/gitleaf-app" &&
cd "$HOME/.local/bin/gitleaf-app" &&
curl --fail --location --output gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-linux-arm64 &&
chmod +x gitleaf &&
./gitleaf
```

</details>

<details>
<summary>macOS / Linux: I downloaded with my browser instead</summary>

Open Terminal and go to the folder containing the download (usually `~/Downloads`). On a Mac
with Apple silicon, run:

```bash
cd "$HOME/Downloads" &&
chmod +x gitleaf-macos-arm64 &&
./gitleaf-macos-arm64
```

For an Intel Mac or Linux, replace **both** instances of `gitleaf-macos-arm64` with the exact file
name from the download table. Run only the file for your computer.

GitLeaf is not yet signed or notarized. If macOS blocks opening it, first try the command above,
then follow **System Settings → Privacy & Security → Open Anyway** and run it again. Apple's
[opening instructions](https://support.apple.com/en-us/102445) explain the prompt. Approve only
the download you trust; managed computers may require your administrator's help.

</details>

**You are ready when your browser opens GitLeaf.** If it does not open automatically, visit
[http://127.0.0.1:8077](http://127.0.0.1:8077). Keep the terminal/console window open: it runs the
app even though you edit in the browser. For a download error, use the
[latest release page](https://github.com/kmarkiv/gitleaf/releases/latest) and download an executable
under **Assets**, rather than the “Source code” ZIP. Release checksums are in `SHA256SUMS.txt` there.

### 3. Create your first paper

1. Click **＋ New**. On a fresh installation, the New project dialog opens automatically.
2. In **Built-in templates**, select **Article (plain)**, enter a name such as `my-first-paper`,
   and click **Create**. Leave **Push to (optional)** blank to work locally.
3. Open `main.tex` in the file explorer if it is not already open. Change the title or a sentence
   in the editor. Wait for **Saved**; edits save automatically after you stop typing.
4. Click the green **Compile** button. If no LaTeX compiler is installed, accept the prompt to
   download **Tectonic**. The first compile can take longer while it downloads packages.
5. When compilation succeeds, your PDF appears on the right. Use the PDF download button to save
   a copy. If compilation fails, click an error in **Problems** to jump to its source line.

Already have a paper? Use **＋ New → Zip** to import a source ZIP, or **Git / Overleaf project** to
clone a repository. A source ZIP from Overleaf is a simple way to start without Git credentials.
Private repositories need credentials configured in your terminal before GitLeaf can clone them.

An existing `latexmk` or Tectonic installation is detected automatically. Automatic Tectonic
installation also needs `curl` and `tar` on your system. If installation fails, install
[Tectonic](https://tectonic-typesetting.github.io/) manually, make it available on PATH, and restart
GitLeaf. See [Troubleshooting](#troubleshooting) for common setup problems.

## LaTeX compiler options

**Choose either option.** GitLeaf works with Tectonic or a full LaTeX installation. If you already
have LaTeX, you do not need to install another copy.

| Option | Choose it when | Setup |
|---|---|---|
| **Tectonic — quick start** | You want to start writing with minimal setup. Packages download as needed and are cached for later use. | Click **Compile** and accept the download prompt if no compiler is found. |
| **Full LaTeX — MacTeX / TeX Live** | You want a broad collection of packages and tools installed upfront, including for offline work. | Install the distribution for your computer below, then restart GitLeaf. |

### Install full LaTeX (optional)

A full installation uses several gigabytes and can take a while to download. It is a separate
system installation that you manage through its installer or package manager.

| Computer | Installation |
|---|---|
| macOS | Download and run the full [MacTeX installer](https://tug.org/mactex/mactex-download.html). MacTeX includes TeX Live; choose the full distribution for this route. |
| Windows | Download and run the [TeX Live Windows installer](https://tug.org/texlive/windows.html), keeping the default **full** installation scheme and the option to add its programs to PATH. |
| Ubuntu / Debian | Run the commands below to install the full distribution and `latexmk`. See the [Ubuntu](https://packages.ubuntu.com/noble/texlive-full) or [Debian](https://packages.debian.org/stable/texlive-full) package information. |
| Other Linux | Follow the [TeX Live installation guide](https://tug.org/texlive/quickinstall.html), choose the **full** scheme, and add the binary directory to PATH as instructed by the installer. |

Ubuntu / Debian:

```bash
sudo apt update &&
sudo apt install texlive-full latexmk
```

After installation, close and reopen Terminal or PowerShell, then check:

```sh
latexmk -v
pdflatex --version
```

Both commands should print version information. `latexmk` is the build tool GitLeaf uses with a
full distribution; having only `pdflatex` installed is not enough for automatic detection.
Stop the running GitLeaf with **Ctrl+C**, then start it again from the new terminal so it sees
the updated PATH. Click **Compile**; GitLeaf automatically prefers `latexmk` when it is available,
otherwise it uses Tectonic. An explicitly set `GITLEAF_ENGINE` overrides this choice.

If MacTeX is installed but the commands are missing, check that `/Library/TeX/texbin` is on PATH.
For Windows or Linux, use the binary directory shown by your installer. A full distribution
lets you compile with its installed packages offline; remote Git operations still need internet.

GitLeaf's current full-LaTeX build uses **pdfLaTeX** through `latexmk`, with shell escape and
custom `latexmkrc` files disabled. Installing the full distribution does not enable XeLaTeX,
LuaLaTeX or custom shell-based build steps in the app; those workflows need a separately managed
terminal build.

## Using GitLeaf again

**Start:** For the Mac app, open **GitLeaf** from Applications; click its Dock or menu-bar icon
to reopen the browser. On Windows, double-click the same executable. If you used the terminal setup above,
run the matching command below; no download is needed:

| Computer | Reopen in Terminal |
|---|---|
| macOS | `"$HOME/Applications/gitleaf/gitleaf"` |
| Linux | `"$HOME/.local/bin/gitleaf-app/gitleaf"` |

If you downloaded with your browser, run the executable from wherever you saved it instead.
Your projects appear in the project menu when the app opens. Starting GitLeaf again while it is
already running reopens the browser after checking that it is the same GitLeaf configuration.

**Stop:** Wait for **Saved**. For the Mac app, choose **Quit GitLeaf** from the leaf menu or press
**⌘Q** while GitLeaf is active. For a standalone executable, press **Ctrl+C** in its terminal/console.
Closing only the browser tab leaves the app running. Restart it when you want to continue writing.

**Find your papers:** By default they are in `Documents/gitleaf` in your home folder, with one
folder per project. The terminal prints the actual projects folder at startup. If
`Documents/Codex/Overleaf` already exists, GitLeaf uses that folder for compatibility. If
`Documents/gitleaf` is occupied by a file, it uses `Documents/gitleaf-projects` instead. Moving
the GitLeaf executable does not move your papers.

**Keep a history:** Autosave writes your files to disk. **Git → Commit** records a version you can
return to; **Git → Push** sends committed changes to a configured remote. A remote is optional.
Back up the projects folder, including its hidden `.git` folders.

### Offline LaTeX editing

GitLeaf’s editor and PDF viewer are bundled with the app. You can write, autosave, view an
existing PDF and create local Git commits without an internet connection. GitLeaf is free under
the MIT license, with no subscription or account required for local writing.

**Prepare before going offline:**

1. While online, download GitLeaf and install Git. Install Tectonic through the first Compile
   prompt, or use a full LaTeX installation such as MacTeX or TeX Live with `latexmk`.
2. Compile the paper you plan to work on, including its bibliography and figures. Tectonic
   downloads packages as needed and caches them; a full LaTeX distribution uses installed packages.
3. Disconnect from the internet and try compiling that paper again before relying on the setup
   while travelling or working somewhere without a connection.

Offline compilation needs every required package and tool available locally. Adding a package
that is not cached or installed can require another download. Git Push/Pull, cloning remote
repositories, browsing online templates, compiler downloads and update checks need internet.
For a broad collection of packages installed upfront, see [full LaTeX options](#install-full-latex-optional).

## Updates

Open **View → Check for updates**. Published release binaries offer an **Update** button when a
newer compatible release is available. GitLeaf saves pending edits, verifies the download, then
restarts. Keep the standalone executable in a folder your account can write to. Your papers stay in the
projects folder.

**Mac app package:** Update checks link to the release page. Download the new DMG, quit GitLeaf
from its leaf menu, then replace GitLeaf.app in Applications. The app updates as a whole to
preserve its bundle signature. Your projects remain in their existing folder.

Automatic checks run on startup and every six hours. Set `GITLEAF_NO_UPDATE_CHECK=1` to disable
them for offline or restricted environments; manual checks still work. A failed check does not
block editing. You can also download a newer executable from the release page, stop the running
app, and replace the old executable.

**Built from source?** Stop GitLeaf, then run these commands in your clone and start it again.
On Windows, replace the last command with `.\target\release\gitleaf.exe`:

```bash
git pull
npm ci
npm run build
cargo build --release --locked
./target/release/gitleaf
```

Commit or save any local source changes before pulling.

## What it does

- **Three panels, one button.** File explorer, LaTeX editor, PDF preview, and a prominent Compile. Dividers
  drag, double-click resets, the explorer collapses (⌘B), and Focus mode (⌘⇧F) hides everything else.
- **Projects are git repositories.** The project menu lists them, creates new ones from built-in templates
  (article, ACM sigconf, IEEE, Springer LNCS, beamer, thesis), browses the Overleaf gallery by tag, clones any git
  URL or an Overleaf project link (over Overleaf's git bridge), or unpacks a zip from a URL or disk.
- **Explorer.** Search, collapsible folders, type badges, ★ for the main file (guessed from `\documentclass`),
  new file, new folder, upload or drag-and-drop, right-click for rename, download and delete. Build outputs sit
  in a collapsed "Generated files" group.
- **Editor.** CodeMirror 6 with tabs, bracket matching, find and replace, a status bar, and a Source/Visual
  switch. Grammarly and similar extensions work on it. Edits autosave 1.5 s after you stop typing; saves are ordered, and typing during a save triggers a follow-up save. Files are replaced atomically so a failed write does not truncate the original; the toolbar
  says "Saved" or "● Unsaved", and the Git menu says what is uncommitted and unpushed, so the two are never
  confused.
- **Visual mode.** Prose, headings and lists as rich text; tables, figures, display math and the title rendered;
  captions editable; double-click any rendered block to edit its LaTeX. Untouched LaTeX round-trips byte for byte.
- **Compile.** `latexmk` when installed, else Tectonic. The status beside Compile reads "✓ Compiled successfully
  · 2.3s" or "✕ Compilation failed · 3 errors"; failures open a Problems panel (click a row to jump to the line)
  with a Logs tab. The last PDF that compiled cleanly stays on screen when a new compile fails.
- **PDF viewer.** Page navigation, zoom, fit width or page, text search, download, fullscreen; the position
  survives recompiles. SyncTeX both ways: ⌘J jumps from the cursor to the PDF, double-click in the PDF jumps to
  the source line.
- **Git.** Commit (with a review of changed files), Push, Pull, History, all through the system `git`, so the
  credentials and SSH keys git already uses keep working.

### Importing from Overleaf

For an existing Overleaf paper, download its source ZIP in Overleaf, then use **＋ New → Zip**.
This creates a local copy; it does not sync changes back to Overleaf.

For Git-based syncing, paste the project link into **＋ New → Git / Overleaf project** and clone it.
Set up access to Overleaf's Git bridge in your terminal first; GitLeaf uses your existing Git
credentials and cannot answer interactive login prompts. Pull and Push then keep the repositories
in sync when your Overleaf account supports Git access.

The **Overleaf gallery** searches for a template's source repository on GitHub first. When a
source is unavailable, it guides you through opening the template in Overleaf and importing the
resulting project link or source ZIP.

## Troubleshooting

| Symptom | Action |
|---|---|
| Git is missing / command not found | Follow [Install Git](#1-install-git-once), reopen your terminal, then confirm `git --version` works. |
| Project creation fails at initial commit | Set your Git name and email using [step 1](#1-install-git-once), then retry with a new project name if the previous folder already exists. |
| Port is occupied | Stop the other service or choose `--port 8078`. An existing GitLeaf with different root/cache also requires another port. |
| Browser did not open / cannot connect | Keep the GitLeaf terminal open and use the localhost address it prints (normally `http://127.0.0.1:8077`). Check the terminal for a startup error. |
| macOS blocks opening the download | See the browser-download instructions in [step 2](#2-download-and-start-gitleaf) and Apple's [opening instructions](https://support.apple.com/en-us/102445). |
| Permission denied when starting on macOS/Linux | Run `chmod +x` on the exact downloaded file, then start it with `./` followed by its name. |
| Download says 404 / fails | Open the [latest release page](https://github.com/kmarkiv/gitleaf/releases/latest) and choose the executable for your computer under Assets. |
| No LaTeX engine / engine download fails | Accept the Compile prompt while online, or follow [full LaTeX setup](#install-full-latex-optional). For automatic Tectonic installation, check `curl --version` and `tar --version`. Restart GitLeaf after changing PATH. |
| Full LaTeX is installed but not detected | In a new terminal, check `latexmk -v` and `pdflatex --version`. Add the distribution's binary directory to PATH if needed, then restart GitLeaf from that terminal. |
| First compile takes a while | Tectonic may be downloading packages. Keep GitLeaf running and check Logs for progress. Subsequent compiles reuse cached packages. |
| Clone, Pull or Push needs a login | Set up Git credentials for the same remote in your terminal, then retry in GitLeaf. A source ZIP avoids credentials when you only need a local copy. |
| Compilation fails | Use Problems and Logs; the previous successful PDF remains available. Compile again after fixing the source. |
| Editor says Save failed | Keep the tab open. Check disk space, file permissions and the server process, then use Save again. |
| Browser requests return 403 | Use the direct `http://127.0.0.1:PORT` or `http://localhost:PORT` address, with the configured port. |
| Assets missing during Rust build | Run `npm ci` and `npm run build` first. |

## Configuration (optional)

The defaults work for a first paper. Use these settings only if you want another projects folder,
port or compiler. On macOS/Linux, run `./gitleaf` from its folder; on Windows, use
`.\gitleaf-windows-x86_64.exe` instead. For example, `./gitleaf --port 8078` opens on port 8078.

```
gitleaf [--root DIR] [--cache DIR] [--port N] [--no-open]
```

| Variable / flag | Default | Meaning |
|-----------------|---------|---------|
| `GITLEAF_ROOT`, `--root` | `~/Documents/gitleaf` | Folder holding one git repository per paper; existing `~/Documents/Codex/Overleaf` is used for compatibility; falls back to `~/Documents/gitleaf-projects` when `gitleaf` is a file |
| `GITLEAF_CACHE`, `--cache` | OS cache folder | Compile outputs and the downloaded Tectonic |
| `GITLEAF_PORT`, `--port` | `8077` | Listen port, on 127.0.0.1 only |
| `GITLEAF_ENGINE` | auto | `latexmk` or `tectonic` |
| `GITLEAF_NO_OPEN`, `--no-open` | | Do not open the browser on start |
| `GITLEAF_NO_UPDATE_CHECK` | unset | Disable automatic GitHub release checks; manual checks remain available |

## Build from source

Use this route if you want to edit GitLeaf or run source changes that have not been released yet.
For everyday use, follow [Get started](#get-started) above.

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

## Operating GitLeaf

GitLeaf is a local application for one trusted user. It binds to `127.0.0.1` and has no login or
multi-user permission model. Open it directly at the printed localhost address. Public hosting and
reverse-proxy deployments are not supported by this configuration.

- Back up the projects root, including each repository's `.git` directory. Autosave writes to disk;
  it does not create a Git commit or push to a remote. Uncommitted or never-committed deletions cannot
  be recovered from Git. Commit and push important milestones.
- Keep root and cache in separate directories; nested or identical paths are rejected at startup.
  The cache contains derived outputs and an optional downloaded engine, and can be regenerated.
- Existing cache files from v0.3.0 use the previous naming layout. Compile once after upgrading to
  generate outputs in the new per-main-file cache; projects and source files are unchanged.
- Ctrl-C and SIGTERM drain active requests before shutting down. External commands time out after
  five minutes; their output is bounded to 8 MiB per stream. Unix process groups and Windows process
  tree termination are used when a command times out or exceeds its output budget.
- JSON requests are limited to 8 MiB; binary uploads and source ZIPs to 64 MiB. ZIP extraction refuses
  symlinks and nonportable paths, allows at most 10,000 entries, and bounds each expanded file to
  64 MiB and the total to 512 MiB. Project ZIP downloads are capped at 512 MiB of source data.
- Project paths protect `.git` at every depth and through symlink aliases. Standalone repositories
  are supported; Git worktrees whose `.git` is a pointer file are not.
- Browser requests must use the app's own localhost host, port and origin. Cross-origin requests,
  DNS rebinding hosts, and embedding in other pages are rejected. Uploaded SVG content is sandboxed.
  These protections do not authenticate other programs running on the same machine.
- LaTeX is executable input. Compile trusted projects only. Tectonic uses `--untrusted`; latexmk
  disables shell escape and local initialization files (`-no-shell-escape`, `-norc`). These options
  reduce risk but are not an OS sandbox. Projects requiring shell escape or custom latexmkrc behavior
  need a separately managed trusted build workflow.
- Tectonic installation requires `curl` and `tar`, an official HTTPS release URL and a GitHub-provided
  SHA-256 digest. Releases without a digest, checksum failures and binaries that fail `--version`
  are refused. Install Tectonic manually on PATH if automatic installation is unavailable.

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

## Limits

- Single trusted user; no authentication, public hosting or real-time collaboration. Collaboration
  uses Git and your existing credentials.
- The bundled editor can work offline; Tectonic still needs cached packages to compile offline.
- Visual mode is a view of the source, not a word processor. Nested environments and custom macros
  may appear as raw LaTeX blocks.
- Compiler output and large project operations are bounded. A request beyond a limit returns an
  error; long compiles may need a separate trusted terminal workflow.
- Git credentials are used from the system configuration. Interactive credential prompts are
  disabled, so establish credentials in a terminal before cloning/pushing protected repositories.
- Modern desktop browsers are required. Chromium is exercised in the automated browser suite;
  Safari and Firefox need separate release smoke tests.

## License

[MIT](LICENSE) © 2026 Vikram Cannanure
