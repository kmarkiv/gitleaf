# Security and operating notes

GitLeaf is a local application for one trusted user. These notes explain how it protects your
files and where its limits are. For setup, see the [README](../README.md).

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
