<p align="center">
  <img src="docs/logo.svg" width="72" alt="">
</p>
<h1 align="center">GitLeaf — Free, Lightweight, Offline LaTeX Editor</h1>
<p align="center">
  A free, lightweight, open-source LaTeX editor for Mac, Windows and Linux that runs on your own computer.<br>
  Write with autosave and a side-by-side PDF preview, keep every paper's history in Git, and work offline after setup.
</p>

<p align="center">
  <a href="https://kmarkiv.com/gitleaf/">Website</a> ·
  <a href="https://kmarkiv.com/gitleaf/guide/">Setup guide</a> ·
  <a href="https://github.com/kmarkiv/gitleaf/releases/latest">Downloads</a>
</p>

<p align="center">
  <a href="https://github.com/kmarkiv/gitleaf/releases/latest"><img src="https://img.shields.io/github/v/release/kmarkiv/gitleaf?label=release&color=32C793" alt="Latest release"></a>
  <a href="https://github.com/kmarkiv/gitleaf/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/kmarkiv/gitleaf/ci.yml?label=tests" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center">
  <img src="docs/screenshot.jpg" width="820" alt="GitLeaf: file explorer, LaTeX editor with tabs, and PDF preview side by side">
</p>

## Get started

**Install Git → start GitLeaf → create a paper → Compile.** Follow the three steps below to write your first paper.

- **No LaTeX installation needed to begin.** If no compiler is found, your first Compile offers to
  install Tectonic. A [full LaTeX installation](#latex-compiler-options) such as MacTeX or TeX Live also works.
- **No account needed.** Papers stay on your computer; GitHub or another Git remote is optional.
- **Simple download.** Use the Mac app on macOS, or a standalone executable on Windows or Linux.
  GitLeaf runs locally and opens in your browser; no Rust, Node.js or separate web server is needed.
- **Works offline** for editing, autosave, the PDF viewer and local commits. Compiling offline needs your
  compiler and packages installed or cached first; see [Offline LaTeX editing](#offline-latex-editing).

[1. Install Git](#1-install-git-once) · [2. Download](#2-download-and-start-gitleaf) ·
[3. First paper](#3-create-your-first-paper) · [LaTeX options](#latex-compiler-options) ·
[Troubleshooting](#troubleshooting)

### 1. Install Git (once)

Open **Terminal** (macOS/Linux) or **PowerShell** (Windows) and check:

```sh
git --version
```

A version number means Git is ready. Otherwise install it:

| Computer | Install Git |
|---|---|
| macOS | Run `xcode-select --install` and finish the installer, then reopen Terminal. ([Git for macOS](https://git-scm.com/install/mac)) |
| Windows | Install [Git for Windows](https://git-scm.com/install/windows) with the default options, then reopen PowerShell. |
| Ubuntu / Debian | `sudo apt update && sudo apt install git curl tar` |
| Other Linux | Follow [Git's Linux instructions](https://git-scm.com/install/linux); also install `curl` and `tar`. |

Then tell Git who you are, once per computer. Replace both examples with your own details; this
labels your saved versions and is not a login. Skip it if both your Git name and email are already configured.

```sh
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

### 2. Download and start GitLeaf

**On a Mac, the simplest way is the Mac app.** Download the disk image for your Mac:

- [GitLeaf for Mac with Apple silicon (.dmg)](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-arm64.dmg)
- [GitLeaf for older Macs with an Intel processor (.dmg)](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-x86_64.dmg)

If another copy is running, stop it first. Open the downloaded disk image, drag **GitLeaf** into
**Applications**, eject the disk image, then open GitLeaf from Applications. The editor opens in
your browser; the Mac app does not need a Terminal window.
GitLeaf is not yet notarized by Apple, so **the first time** macOS says it "could not verify" the app:
click **Done**, open **System Settings → Privacy & Security**, click **Open Anyway** next to GitLeaf,
and confirm. After that it opens normally. GitLeaf appears as a leaf icon in the menu bar
(**Open GitLeaf in Browser**, **Show GitLeaf Log**, **Quit GitLeaf**) and needs macOS 13 or newer.

**Standalone executable (all platforms).** Use this route on Windows or Linux, or if you prefer
a Terminal launch on macOS. Paste your computer’s command into Terminal (PowerShell on Windows).
It downloads GitLeaf into its own folder and starts it; your browser then opens the editor.

Not sure which Mac you have? **Apple menu → About This Mac**: "Chip: Apple M…" is Apple silicon,
"Processor: Intel" is an Intel Mac. On Linux, `uname -m` prints `x86_64` or `aarch64` (ARM64).

**Mac with Apple silicon (M1 or newer, 2020 onwards)**

```bash
mkdir -p ~/Applications/gitleaf && cd ~/Applications/gitleaf && curl -fL -o gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-arm64 && chmod +x gitleaf && ./gitleaf
```

**Older Mac with an Intel processor**

```bash
mkdir -p ~/Applications/gitleaf && cd ~/Applications/gitleaf && curl -fL -o gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-macos-x86_64 && chmod +x gitleaf && ./gitleaf
```

**Windows (64-bit)**

Download [gitleaf-windows-x86_64.exe](https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-windows-x86_64.exe)
and double-click it. For a download from this repository, a SmartScreen prompt may offer **More info → Run anyway**.
Or paste this into PowerShell:

```powershell
$ErrorActionPreference = 'Stop'; mkdir -Force "$env:LOCALAPPDATA\gitleaf" | Out-Null; cd "$env:LOCALAPPDATA\gitleaf"; Invoke-WebRequest https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-windows-x86_64.exe -OutFile gitleaf.exe; .\gitleaf.exe
```

**Linux (most PCs, x86_64)**

```bash
mkdir -p ~/.local/bin/gitleaf-app && cd ~/.local/bin/gitleaf-app && curl -fL -o gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-linux-x86_64 && chmod +x gitleaf && ./gitleaf
```

**Linux (ARM64, for example a Raspberry Pi with a 64-bit OS)**

```bash
mkdir -p ~/.local/bin/gitleaf-app && cd ~/.local/bin/gitleaf-app && curl -fL -o gitleaf https://github.com/kmarkiv/gitleaf/releases/latest/download/gitleaf-linux-arm64 && chmod +x gitleaf && ./gitleaf
```

**You are ready when your browser shows GitLeaf.** If it does not open by itself, go to
[http://127.0.0.1:8077](http://127.0.0.1:8077). For a standalone executable, keep its Terminal or
console window open while you write. For the Mac app, leave GitLeaf running in the menu bar.

<details>
<summary>I downloaded the file with my browser instead (macOS / Linux)</summary>

Run this in Terminal from the folder holding the download, using the file name you downloaded
(shown here for an Apple silicon Mac):

```bash
cd ~/Downloads && chmod +x gitleaf-macos-arm64 && ./gitleaf-macos-arm64
```

For an Intel Mac or Linux, replace both occurrences of `gitleaf-macos-arm64` with your downloaded
file’s name. If macOS blocks the executable, follow **System Settings → Privacy & Security →
Open Anyway** and try opening it again ([Apple’s instructions](https://support.apple.com/en-us/102445)).
The Mac downloads are not yet notarized; Terminal downloads are not a guarantee of approval.

</details>

All files, including checksums (`SHA256SUMS.txt`), are on the [release page](https://github.com/kmarkiv/gitleaf/releases/latest)
under **Assets**.

### 3. Create your first paper

1. Click **＋ New** (it opens by itself on a fresh installation).
2. Under **Built-in templates**, pick **Article (plain)**, name it, for example `my-first-paper`, and
   click **Create**. Leave **Push to** empty to work locally.
3. Edit a sentence in `main.tex`. Edits save by themselves; wait for **Saved**.
4. Click **Compile**. Without a LaTeX compiler, accept the offer to install **Tectonic**; the first
   compile takes longer while it downloads packages.
5. The PDF appears on the right. If something fails, click an error under **Problems** to jump to the line.

Already have a paper? **＋ New → Zip** imports a source ZIP (for example one downloaded from Overleaf),
and **＋ New → Git / Overleaf project** clones a repository. See [Importing from Overleaf](#importing-from-overleaf).

## LaTeX compiler options

Use either. If you already have LaTeX installed, GitLeaf finds it.

| Option | Choose it when | Setup |
|---|---|---|
| **Tectonic** (quick start) | You want to start writing with little setup. Packages download when first needed and are cached. | Click **Compile** and accept the install offer. |
| **Full LaTeX** (MacTeX / TeX Live) | You want a large set of packages installed up front, for example for offline work. | Install it as below, then restart GitLeaf. |

### Install full LaTeX (optional)

A full installation takes several gigabytes.

| Computer | Installation |
|---|---|
| macOS | Run the [MacTeX installer](https://tug.org/mactex/mactex-download.html). |
| Windows | Run the [TeX Live installer](https://tug.org/texlive/windows.html) with the default full scheme and the option to add it to PATH. |
| Ubuntu / Debian | `sudo apt update && sudo apt install texlive-full latexmk` |
| Other Linux | Follow the [TeX Live guide](https://tug.org/texlive/quickinstall.html), choose the full scheme, and add it to PATH. |

Open a new Terminal or PowerShell and check that `latexmk -v` prints a version. Then restart
GitLeaf: use **Quit GitLeaf** from the leaf menu for the Mac app, or **Ctrl+C** for a standalone
executable. The Mac app finds the standard MacTeX path; restart a standalone executable from
the new terminal so it sees the updated PATH. GitLeaf prefers `latexmk` whenever it is
available and otherwise uses Tectonic; `GITLEAF_ENGINE` forces one or the other. On macOS, if the
command is missing after installing MacTeX, make sure `/Library/TeX/texbin` is on PATH.

With full LaTeX, GitLeaf builds with **pdfLaTeX** through `latexmk`, with shell escape and custom
`latexmkrc` files turned off. XeLaTeX, LuaLaTeX and shell-escape builds need a separate terminal workflow.

## Using GitLeaf again

**Start:** open it the same way again; there is nothing to download.

| Computer | Start GitLeaf |
|---|---|
| Mac app | Open **GitLeaf** from Applications (click the leaf icon in the menu bar to reopen the browser). |
| macOS command | `~/Applications/gitleaf/gitleaf` |
| Linux | `~/.local/bin/gitleaf-app/gitleaf` |
| Windows | Double-click the `.exe` (from PowerShell: `& "$env:LOCALAPPDATA\gitleaf\gitleaf.exe"`) |

If GitLeaf is already running, starting it again just reopens the browser.

**Stop:** wait for **Saved**, then choose **Quit GitLeaf** from the leaf menu (Mac app), or press
**Ctrl+C** in the Terminal or console window. Closing the browser tab alone leaves GitLeaf running.

**Find your papers:** normally in `Documents/gitleaf` in your home folder, one folder per paper.
An existing `Documents/Codex/Overleaf` folder is reused for compatibility; if `Documents/gitleaf`
is a file, GitLeaf uses `Documents/gitleaf-projects`. The startup output shows the exact path;
in the Mac app, choose **Show GitLeaf Log** from the leaf menu. Moving the app does not move your papers.

**Keep versions:** autosave writes to disk. **Git → Commit** saves a version you can return to, and
**Git → Push** sends committed versions to a remote, if you set one up. Back up the projects folder,
including the hidden `.git` folders.

### Offline LaTeX editing

The editor, PDF viewer, autosave and local commits work without internet. To compile offline too:

1. While online, install Git and either Tectonic (via the first Compile) or full LaTeX.
2. Compile the paper you will work on, including its bibliography and figures, so every package it
   needs is cached or installed.
3. Disconnect and compile once more to confirm it works before you rely on it.

Offline compilation needs every required compiler, tool and package available locally. Adding a
package that is not cached or installed needs a connection again. Push, Pull, cloning, the online
template gallery, compiler downloads and update checks need internet.

## Updates

GitLeaf checks for a new release when it starts and every six hours. When one is available, an
**Update** button appears in the toolbar (you can also use **⋯ → Check for updates**).

- **Mac app:** the button opens the release page. Download the new DMG, wait for **Saved**, quit
  through the leaf menu, and replace GitLeaf.app in Applications. This is a manual app update.
- **Standalone executable:** the button saves pending edits, downloads and verifies the new binary,
  then restarts. Keep the executable in a folder you can write to.
- **Source build:** run `git pull`, then rebuild; see [Build from source](#build-from-source).

Your papers stay in the projects folder. Set `GITLEAF_NO_UPDATE_CHECK=1` to turn off automatic checks.

## What it does

- **Three panels, one button.** File explorer, LaTeX editor and PDF preview, with one Compile button.
  Drag the dividers, collapse the explorer (⌘B), or use Focus mode (⌘⇧F).
- **Every paper is a Git repository.** Start from built-in templates (article, ACM sigconf, IEEE,
  Springer LNCS, beamer, thesis), the Overleaf gallery, a Git URL, an Overleaf project link or a ZIP.
- **Explorer.** Search, folders, ★ to set the main file, new file or folder, upload or drag-and-drop,
  and right-click to rename, download or delete.
- **Editor.** Tabs, bracket matching, find and replace, and autosave 1.5 s after you stop typing.
  Grammarly and similar browser extensions work in it. The toolbar shows **Saved** or **● Unsaved**;
  the Git menu separately shows what is not yet committed or pushed.
- **Visual mode.** Edit prose, headings and lists as formatted text, with tables, figures and math
  rendered. LaTeX you do not touch stays exactly as it was.
- **Compile.** Uses `latexmk` when installed, otherwise Tectonic. Errors open a Problems list; click
  one to jump to the line. The last good PDF stays visible when a compile fails.
- **PDF viewer.** Pages, zoom, search, download and fullscreen; your place is kept across compiles.
  ⌘J jumps from the cursor to the PDF; double-click the PDF to jump back to the source.
- **Git.** Commit (with a list of changed files), Push, Pull and History, using the Git login you
  already have.

### Importing from Overleaf

- **A local copy:** in Overleaf, download the project's source ZIP, then use **＋ New → Zip**. This does
  not sync back to Overleaf.
- **Kept in sync:** paste the project's link into **＋ New → Git / Overleaf project**. Set up access to
  Overleaf's Git bridge in your terminal first (GitLeaf uses your existing Git login). Pull and Push then
  sync with Overleaf, if your Overleaf plan includes Git access.
- **Templates:** the **Overleaf gallery** tab fetches a template's source from GitHub when available,
  and otherwise walks you through opening it in Overleaf and importing the result.

## Troubleshooting

| Problem | What to do |
|---|---|
| `git` not found | Follow [Install Git](#1-install-git-once), reopen Terminal or PowerShell, and check `git --version`. |
| Creating a project fails at the first commit | Set your Git name and email ([step 1](#1-install-git-once)), then try again with a new project name. |
| macOS says it "could not verify" GitLeaf | For your intended GitLeaf download, click **Done**, then **System Settings → Privacy & Security → Open Anyway**. Follow [Apple’s instructions](https://support.apple.com/en-us/102445). |
| "Permission denied" when starting (macOS/Linux) | Run `chmod +x` on the file, then start it with `./` and its name. |
| Download fails or shows 404 | Open the [release page](https://github.com/kmarkiv/gitleaf/releases/latest) and pick your file under **Assets** (not "Source code"). |
| Browser did not open | Keep GitLeaf running and open [http://127.0.0.1:8077](http://127.0.0.1:8077), or the configured port. In the Mac app, choose **Open GitLeaf in Browser** from the leaf menu. |
| Mac app cannot start | Open `~/Library/Logs/GitLeaf/gitleaf.log`. If it reports an incomplete app, download a fresh official DMG and replace the app in Applications. |
| "Port is occupied" | Another program uses port 8077. Stop it, or start GitLeaf with `--port 8078`. |
| No LaTeX engine, or the Tectonic install fails | Accept the Compile offer while online, or install [full LaTeX](#install-full-latex-optional). Tectonic's automatic install needs `curl` and `tar`. |
| Full LaTeX installed but not used | In a new terminal check `latexmk -v`, add LaTeX to PATH if needed, and restart GitLeaf from that terminal. |
| First compile is slow | Tectonic is downloading packages; later compiles reuse them. Watch progress under **Logs**. |
| Clone, Pull or Push asks for a login | Sign in to that remote with Git in your terminal first, then retry in GitLeaf. |
| Compile fails | Check **Problems** and **Logs**; the previous PDF stays available while you fix it. |
| "Save failed" | Keep the tab open, check disk space and file permissions, then save again. |
| Requests fail with 403 | Open GitLeaf at `http://127.0.0.1:8077` or `http://localhost:8077` rather than another address. |

## Configuration (optional)

The defaults suit most people. To change them, start GitLeaf with options, for example `./gitleaf --port 8078`:

```
gitleaf [--root DIR] [--cache DIR] [--port N] [--no-open]
```

| Variable / flag | Default | Meaning |
|-----------------|---------|---------|
| `GITLEAF_ROOT`, `--root` | `~/Documents/gitleaf` | Folder holding your papers (uses `~/Documents/Codex/Overleaf` if it exists, or `~/Documents/gitleaf-projects` if `gitleaf` is a file) |
| `GITLEAF_CACHE`, `--cache` | your system's cache folder | Compile outputs and the downloaded Tectonic |
| `GITLEAF_PORT`, `--port` | `8077` | Port on 127.0.0.1 |
| `GITLEAF_ENGINE` | automatic | `latexmk` or `tectonic` |
| `GITLEAF_NO_OPEN`, `--no-open` | | Do not open the browser on start |
| `GITLEAF_NO_UPDATE_CHECK` | | Turn off automatic update checks |

## Limits

- One trusted user on one computer: no accounts, public hosting or live co-editing. Collaborate through Git.
- Compile only projects you trust; LaTeX can run code. GitLeaf turns off shell escape but is not a sandbox.
- Visual mode shows the source; unusual environments and custom macros appear as raw LaTeX.
- Tested in Chromium; use a current desktop browser.

How GitLeaf protects your files, and its request and size limits, are described in
[docs/SECURITY.md](docs/SECURITY.md).

## Build from source

Only needed to change GitLeaf itself. Install [Rust](https://www.rust-lang.org/tools/install),
[Node.js](https://nodejs.org/en/download) 22.13 or newer, and Git, then:

```bash
git clone https://github.com/kmarkiv/gitleaf.git
cd gitleaf
npm ci && npm run build
cargo build --release --locked
./target/release/gitleaf
```

On Windows, start `.\target\release\gitleaf.exe`. Tests, Mac packaging, releases and the code layout
are in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## License

[MIT](LICENSE) © 2026 Vikram Cannanure
