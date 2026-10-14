//! HTTP handlers: projects, files, compile, git.

use std::{
    path::{Component, Path as FsPath, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::storage::atomic_write;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{io::AsyncReadExt, process::Command};

use crate::{templates::TEMPLATES, AppState};

// ---------------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}
fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}
fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

pub fn git_available() -> bool {
    which("git")
}

pub async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let engine = pick_engine(&state, None)
        .ok()
        .map(|(kind, program)| serde_json::json!({ "kind": kind, "program": program }));
    Json(serde_json::json!({
        "ok": true,
        "app": "gitleaf",
        "instance": instance_id(&state),
        "version": env!("CARGO_PKG_VERSION"),
        "latexmk": which("latexmk"),
        "tectonic": which("tectonic") || bundled_engine(&state).is_some(),
        "git": which("git"),
        "engine": engine,
    }))
}

fn bundled_engine(state: &AppState) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "tectonic.exe"
    } else {
        "tectonic"
    };
    let p = state.cache.join("bin").join(exe);
    p.is_file().then_some(p)
}

/// Fetch an official Tectonic release and verify GitHub's SHA-256 digest before extracting it.
pub async fn install_engine(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let lock = project_lock(&state, ".engine-install").await;
    let _guard = lock.lock().await;
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-musl"
    } else {
        "x86_64-unknown-linux-musl"
    };
    let stage = state.cache.join(".tectonic-install");
    if stage.exists() {
        tokio::fs::remove_dir_all(&stage).await.map_err(internal)?;
    }
    tokio::fs::create_dir_all(&stage).await.map_err(internal)?;
    let result = async {
        let release = run(&stage, "curl", &["-sSfL", "-m", "30", "--proto", "=https", "--proto-redir", "=https", "-A", "gitleaf", "https://api.github.com/repos/tectonic-typesetting/tectonic/releases/latest"]).await?;
        if !release.ok { return Err(ApiError(StatusCode::BAD_GATEWAY, release.stderr)); }
        let json: serde_json::Value = serde_json::from_str(&release.stdout).map_err(internal)?;
        let asset = json["assets"].as_array().and_then(|assets| assets.iter().find(|asset| {
            asset["name"].as_str().is_some_and(|name| name.contains(target) && (name.ends_with(".tar.gz") || name.ends_with(".zip")))
        })).ok_or_else(|| bad(format!("no Tectonic release for {target}; install it manually")))?;
        let url = asset["browser_download_url"].as_str().ok_or_else(|| bad("missing download URL"))?;
        if !url.starts_with("https://github.com/tectonic-typesetting/tectonic/releases/download/") { return Err(bad("unexpected engine download URL")); }
        let digest = asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:"))
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| bad("this release has no SHA-256 digest; install Tectonic manually instead"))?;
        let archive = stage.join("engine.archive");
        let archive_path = archive.to_string_lossy();
        let download = run(&stage, "curl", &["-sSfL", "-m", "240", "--proto", "=https", "--proto-redir", "=https", "--max-filesize", "134217728", "-o", &archive_path, "--", url]).await?;
        if !download.ok { return Err(ApiError(StatusCode::BAD_GATEWAY, download.stderr)); }
        let bytes = tokio::fs::read(&archive).await.map_err(internal)?;
        if !digest_matches(&bytes, digest) { return Err(bad("Tectonic checksum mismatch; download refused")); }
        let executable = if cfg!(windows) { "tectonic.exe" } else { "tectonic" };
        let extracted = run(&stage, "tar", &["-xf", &archive_path, executable]).await?;
        if !extracted.ok { return Err(internal(format!("could not extract Tectonic: {}", extracted.stderr))); }
        let candidate = stage.join(executable);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755)).map_err(internal)?;
        }
        let version = run(&stage, &candidate.to_string_lossy(), &["--version"]).await?;
        if !version.ok { return Err(bad("downloaded Tectonic could not run on this machine")); }
        let bin = state.cache.join("bin").join(executable);
        write_document(bin.clone(), tokio::fs::read(&candidate).await.map_err(internal)?).await?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).map_err(internal)?;
        }
        Ok(Json(serde_json::json!({ "engine": bin, "version": version.stdout.trim(), "release": json["tag_name"] })))
    }.await;
    let _ = tokio::fs::remove_dir_all(&stage).await;
    result
}

fn digest_matches(bytes: &[u8], expected: &str) -> bool {
    format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
}

pub fn instance_id(state: &AppState) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{}\n{}", state.root.display(), state.cache.display()).as_bytes())
    )
}

// ---------------------------------------------------------------------------
// Running external commands

#[derive(Serialize)]
pub struct CmdOut {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const COMMAND_OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
const UPLOAD_LIMIT: u64 = 64 * 1024 * 1024;
const EXTRACT_LIMIT: u64 = 512 * 1024 * 1024;
const ZIP_ENTRY_LIMIT: usize = 10_000;

pub(crate) async fn run(cwd: &FsPath, program: &str, args: &[&str]) -> Result<CmdOut, ApiError> {
    run_with_timeout(cwd, program, args, COMMAND_TIMEOUT).await
}

async fn run_with_timeout(
    cwd: &FsPath,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<CmdOut, ApiError> {
    use std::process::Stdio;
    let mut command = Command::new(program);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| internal(format!("failed to run {program}: {e}")))?;
    let process_id = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| internal("missing stderr"))?;
    async fn bounded_read(stream: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        stream
            .take(COMMAND_OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() as u64 > COMMAND_OUTPUT_LIMIT {
            return Err(std::io::Error::other("command output exceeded 8 MiB"));
        }
        Ok(bytes)
    }
    let completed = tokio::time::timeout(timeout, async {
        tokio::try_join!(child.wait(), bounded_read(stdout), bounded_read(stderr))
    })
    .await;
    let (status, stdout, stderr) = match completed {
        Ok(Ok(result)) => result,
        result => {
            if let Some(id) = process_id {
                #[cfg(unix)]
                {
                    // The child starts its own group. Kill its compiler/Git descendants too.
                    // SAFETY: kill takes a numeric group ID and does not access memory.
                    unsafe {
                        libc::kill(-(id as i32), libc::SIGKILL);
                    }
                }
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .args(["/PID", &id.to_string(), "/T", "/F"])
                        .output()
                        .await;
                }
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(match result {
                Err(_) => ApiError(
                    StatusCode::GATEWAY_TIMEOUT,
                    format!("{program} timed out after {} seconds", timeout.as_secs()),
                ),
                Ok(Err(e)) => internal(e),
                _ => unreachable!(),
            });
        }
    };
    Ok(CmdOut {
        ok: status.success(),
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

async fn git(cwd: &FsPath, args: &[&str]) -> Result<CmdOut, ApiError> {
    run(cwd, "git", args).await
}

/// First line of trimmed stdout, or None when git fails.
async fn git_line(cwd: &FsPath, args: &[&str]) -> Option<String> {
    let out = git(cwd, args).await.ok()?;
    out.ok.then(|| out.stdout.trim().to_string())
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p).any(|d| {
                let path = d.join(program);
                #[cfg(windows)]
                {
                    path.is_file()
                        || d.join(format!("{program}.exe")).is_file()
                        || d.join(format!("{program}.bat")).is_file()
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::metadata(path)
                        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                        .unwrap_or(false)
                }
            })
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Path safety

fn safe_component(name: &str) -> bool {
    let upper = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !name.contains(['\\', ':'])
        && !name.chars().any(char::is_control)
        && !name.eq_ignore_ascii_case(".git")
        && !matches!(
            upper.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}

fn valid_name(name: &str) -> bool {
    name.len() <= 100
        && !name.starts_with('.')
        && safe_component(name)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn project_dir(state: &AppState, name: &str) -> Result<PathBuf, ApiError> {
    if !valid_name(name) {
        return Err(bad("invalid project name"));
    }
    let path = state.root.join(name);
    if std::fs::symlink_metadata(&path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(bad("project directories cannot be symlinks"));
    }
    let dir = path
        .canonicalize()
        .map_err(|_| not_found(format!("project '{name}' not found")))?;
    if !dir.starts_with(&state.root) {
        return Err(bad("project outside root"));
    }
    let metadata = dir.join(".git");
    if !metadata.is_dir()
        || std::fs::symlink_metadata(&metadata)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(bad(format!("'{name}' is not a standalone git repository")));
    }
    Ok(dir)
}

/// Reject traversal, Git metadata (including aliases), nonportable paths and escaping symlinks.
fn file_in_project(project: &FsPath, rel: &str) -> Result<PathBuf, ApiError> {
    let rel_path = FsPath::new(rel);
    if rel.is_empty()
        || rel.len() > 4096
        || rel.contains('\\')
        || rel_path.is_absolute()
        || !rel_path.components().all(|c| match c {
            Component::Normal(n) => n.to_str().is_some_and(safe_component),
            _ => false,
        })
    {
        return Err(bad("invalid file path"));
    }
    let project = project.canonicalize().map_err(internal)?;
    let mut resolved = project.clone();
    for comp in rel_path.components() {
        resolved.push(comp);
        match std::fs::symlink_metadata(&resolved) {
            Ok(meta) if meta.file_type().is_symlink() => {
                resolved = resolved
                    .canonicalize()
                    .map_err(|_| bad("file path goes through a dangling symlink"))?;
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(internal(e)),
        }
        let relative = resolved
            .strip_prefix(&project)
            .map_err(|_| bad("file path escapes project"))?;
        if relative
            .components()
            .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
        {
            return Err(bad("Git metadata is protected"));
        }
    }
    Ok(resolved)
}

async fn write_document(path: PathBuf, content: Vec<u8>) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || atomic_write(&path, &content))
        .await
        .map_err(internal)?
        .map_err(internal)
}

// ---------------------------------------------------------------------------
// Projects

#[derive(Serialize)]
pub struct ProjectInfo {
    name: String,
    branch: Option<String>,
    remote: Option<String>,
    dirty: bool,
}

pub async fn list_projects(State(state): State<AppState>) -> ApiResult<Vec<ProjectInfo>> {
    let mut names: Vec<String> = std::fs::read_dir(&state.root)
        .map_err(internal)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join(".git").exists())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| valid_name(n))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());

    // Inspect at most eight projects at once; each uses three concurrent Git queries.
    let mut set = tokio::task::JoinSet::new();
    let mut out: Vec<(usize, ProjectInfo)> = Vec::with_capacity(names.len());
    for (i, name) in names.iter().cloned().enumerate() {
        while set.len() >= 8 {
            if let Some(Ok(item)) = set.join_next().await {
                out.push(item);
            }
        }
        let Ok(dir) = project_dir(&state, &name) else {
            continue;
        };
        set.spawn(async move {
            let (branch, remote, status) = tokio::join!(
                git_line(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]),
                git_line(&dir, &["remote", "get-url", "origin"]),
                git_line(&dir, &["status", "--porcelain"])
            );
            (
                i,
                ProjectInfo {
                    name,
                    branch,
                    remote,
                    dirty: status.map(|s| !s.is_empty()).unwrap_or(false),
                },
            )
        });
    }
    while let Some(r) = set.join_next().await {
        if let Ok(item) = r {
            out.push(item);
        }
    }
    out.sort_by_key(|(i, _)| *i);
    Ok(Json(out.into_iter().map(|(_, p)| p).collect()))
}

#[derive(Deserialize)]
pub struct CloneReq {
    url: String,
    name: Option<String>,
}

pub async fn clone_project(
    State(state): State<AppState>,
    Json(req): Json<CloneReq>,
) -> ApiResult<serde_json::Value> {
    let mut url = req.url.trim().to_string();
    if url.is_empty() || url.starts_with('-') {
        return Err(bad("invalid url"));
    }
    if url.contains("overleaf.com/latex/") {
        return Err(bad(
            "That is an Overleaf gallery page, which cannot be cloned. Open it in Overleaf, press \
             'Open as Template', then paste the new project's link (overleaf.com/project/...).",
        ));
    }
    if url.contains("overleaf.com/read/") {
        return Err(bad("Read-only Overleaf share links cannot be cloned; you need the project link from your own account (overleaf.com/project/...)."));
    }
    if let Some(git_url) = overleaf_project_git_url(&url) {
        url = git_url;
    }
    let name = req
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            url.trim_end_matches('/')
                .rsplit(['/', ':'])
                .next()
                .unwrap_or("")
                .trim_end_matches(".git")
                .to_string()
        });
    if !valid_name(&name) {
        return Err(bad(format!("invalid project name '{name}'")));
    }
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    if std::fs::symlink_metadata(state.root.join(&name)).is_ok() {
        return Err(bad(format!("'{name}' already exists")));
    }
    let destination = state.root.join(&name);
    tokio::fs::create_dir(&destination)
        .await
        .map_err(internal)?;
    let out = match git(&state.root, &["clone", "--", &url, &name]).await {
        Ok(out) => out,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&destination).await;
            return Err(error);
        }
    };
    if !out.ok {
        let _ = tokio::fs::remove_dir_all(state.root.join(&name)).await;
        let mut msg = format!("git clone failed: {}", out.stderr.trim());
        if url.contains("git.overleaf.com")
            && (out.stderr.contains("Authentication")
                || out.stderr.contains("could not read")
                || out.stderr.contains("403"))
        {
            msg.push_str("\n\nOverleaf's git bridge needs your git token once. In a terminal run:\n  git clone ");
            msg.push_str(&url);
            msg.push_str("\nusing 'git' as the user name and the token from Overleaf > Account settings > Git integration as the password. macOS keychain remembers it, then this button works.");
        }
        return Err(ApiError(StatusCode::BAD_GATEWAY, msg));
    }
    Ok(Json(
        serde_json::json!({ "name": name, "stderr": out.stderr }),
    ))
}

#[derive(Deserialize)]
pub struct NewReq {
    name: String,
    template: Option<String>,
    remote: Option<String>,
}

const GITIGNORE: &str =
    "*.aux\n*.log\n*.out\n*.pdf\n*.synctex.gz\n*.bbl\n*.blg\n*.fls\n*.fdb_latexmk\n*.toc\n*.nav\n*.snm\n";

fn clean_remote(remote: Option<String>) -> Option<String> {
    remote
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty() && !r.starts_with('-'))
}

/// `git init` plus a first commit unless the directory already holds a repository; then set origin.
async fn init_repo(dir: &FsPath, remote: Option<String>) -> Result<(), ApiError> {
    if !dir.join(".git").exists() {
        let init = git(dir, &["init", "-q", "-b", "main"]).await?;
        if !init.ok {
            return Err(internal(init.stderr.trim()));
        }
        if !dir.join(".gitignore").exists() {
            tokio::fs::write(dir.join(".gitignore"), GITIGNORE)
                .await
                .map_err(internal)?;
        }
        for args in [
            &["add", "-A"][..],
            &["commit", "-q", "-m", "Initial commit"][..],
        ] {
            let out = git(dir, args).await?;
            if !out.ok {
                return Err(bad(format!(
                    "could not initialize project: {}",
                    out.stderr.trim()
                )));
            }
        }
    }
    if let Some(remote) = remote {
        let _ = git(dir, &["remote", "remove", "origin"]).await;
        let out = git(dir, &["remote", "add", "origin", &remote]).await?;
        if !out.ok {
            return Err(bad(out.stderr.trim()));
        }
    }
    Ok(())
}

/// Pick the root .tex file: main.tex, else the shallowest .tex that contains \documentclass.
fn detect_main(dir: &FsPath) -> String {
    if dir.join("main.tex").is_file() {
        return "main.tex".into();
    }
    let mut candidates: Vec<(usize, String)> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().map(|x| x == "tex").unwrap_or(false)
        })
        .filter(|e| {
            std::fs::read_to_string(e.path())
                .map(|s| s.contains("\\documentclass"))
                .unwrap_or(false)
        })
        .map(|e| {
            let rel = e.path().strip_prefix(dir).unwrap_or(e.path());
            (e.depth(), rel.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .map(|(_, p)| p)
        .unwrap_or_else(|| "main.tex".into())
}

pub async fn list_templates() -> Json<serde_json::Value> {
    Json(
        TEMPLATES
            .iter()
            .map(|t| serde_json::json!({ "id": t.id, "name": t.name, "description": t.description, "main": t.main }))
            .collect(),
    )
}

pub async fn new_project(
    State(state): State<AppState>,
    Json(req): Json<NewReq>,
) -> ApiResult<serde_json::Value> {
    let name = req.name.trim().to_string();
    if !valid_name(&name) {
        return Err(bad("invalid project name"));
    }
    let id = req
        .template
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or("article");
    let template = TEMPLATES
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| bad(format!("unknown template '{id}'")))?;
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = state.root.join(&name);
    if std::fs::symlink_metadata(&dir).is_ok() {
        return Err(bad(format!("'{name}' already exists")));
    }
    tokio::fs::create_dir(&dir).await.map_err(internal)?;
    let result = async {
        for (rel, content) in template.files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(internal)?;
            }
            tokio::fs::write(&path, content).await.map_err(internal)?;
        }
        init_repo(&dir, clean_remote(req.remote)).await
    }
    .await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(e);
    }
    Ok(Json(
        serde_json::json!({ "name": name, "main": template.main }),
    ))
}

#[derive(Deserialize)]
pub struct ImportUrlReq {
    name: Option<String>,
    url: String,
    remote: Option<String>,
}

/// `https://www.overleaf.com/project/<id>` (or without www) -> the git bridge URL; None otherwise.
fn overleaf_project_git_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    if !matches!(host, "www.overleaf.com" | "overleaf.com") {
        return None;
    }
    let id: String = path
        .strip_prefix("project/")?
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    (id.len() == 24).then(|| format!("https://git.overleaf.com/{id}"))
}

fn name_from_url(url: &str) -> String {
    let url = url.split(['?', '#']).next().unwrap_or(url);
    let base = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("");
    let base = base.trim_end_matches(".zip").trim_end_matches(".git");
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub async fn import_url(
    State(state): State<AppState>,
    Json(req): Json<ImportUrlReq>,
) -> ApiResult<serde_json::Value> {
    let mut url = req.url.trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(bad("url must start with http:// or https://"));
    }
    if url.contains("overleaf.com/latex/") && !url.ends_with(".zip") {
        return Err(bad(
            "Overleaf gallery pages cannot be downloaded without a login. Open the template in Overleaf \
             (Open as Template), then Menu > Download > Source, and upload that zip here.",
        ));
    }
    if overleaf_project_git_url(&url).is_some() {
        return Err(bad("That is an Overleaf project link: use the Git / Overleaf project tab (it clones over git.overleaf.com) instead of the zip importer."));
    }
    let name = req
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| name_from_url(&url));
    // github.com/owner/repo -> zip of the default branch
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .split('/')
            .collect();
        if parts.len() == 2 {
            url = format!(
                "https://github.com/{}/{}/archive/HEAD.zip",
                parts[0], parts[1]
            );
        }
    }
    if !valid_name(&name) {
        return Err(bad(format!("invalid project name '{name}'")));
    }
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    if std::fs::symlink_metadata(state.root.join(&name)).is_ok() {
        return Err(bad(format!("'{name}' already exists")));
    }
    let tmp = state.cache.join(format!(".download-{name}.zip"));
    let tmp_s = tmp.to_string_lossy().into_owned();
    let dl = run(
        &state.cache,
        "curl",
        &[
            "-sSfL",
            "-m",
            "240",
            "--proto",
            "=https,http",
            "--proto-redir",
            "=https,http",
            "--max-filesize",
            "67108864",
            "-A",
            "gitleaf",
            "-o",
            &tmp_s,
            "--",
            &url,
        ],
    )
    .await?;
    if !dl.ok {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(ApiError(
            StatusCode::BAD_GATEWAY,
            format!("download failed: {}", dl.stderr.trim()),
        ));
    }
    let result = import_zip_file(&state, &name, &tmp, clean_remote(req.remote)).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

#[derive(Deserialize)]
pub struct ImportZipQuery {
    name: String,
    remote: Option<String>,
}

pub async fn import_zip(
    State(state): State<AppState>,
    Query(q): Query<ImportZipQuery>,
    body: Bytes,
) -> ApiResult<serde_json::Value> {
    let name = q.name.trim().to_string();
    if !valid_name(&name) {
        return Err(bad("invalid project name"));
    }
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    if std::fs::symlink_metadata(state.root.join(&name)).is_ok() {
        return Err(bad(format!("'{name}' already exists")));
    }
    if body.is_empty() {
        return Err(bad("empty upload"));
    }
    let tmp = state.cache.join(format!(".upload-{name}.zip"));
    tokio::fs::write(&tmp, &body).await.map_err(internal)?;
    let result = import_zip_file(&state, &name, &tmp, clean_remote(q.remote)).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

async fn import_zip_file(
    state: &AppState,
    name: &str,
    zip_path: &FsPath,
    remote: Option<String>,
) -> ApiResult<serde_json::Value> {
    let dir = state.root.join(name);
    tokio::fs::create_dir(&dir).await.map_err(internal)?;
    let zip_path = zip_path.to_path_buf();
    let dir2 = dir.clone();
    let extracted = tokio::task::spawn_blocking(move || extract_zip(&zip_path, &dir2))
        .await
        .map_err(internal)?;
    if let Err(e) = extracted {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(bad(format!("could not extract zip: {e}")));
    }
    if let Err(e) = init_repo(&dir, remote).await {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(e);
    }
    let detect_dir = dir.clone();
    let main = tokio::task::spawn_blocking(move || detect_main(&detect_dir))
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({ "name": name, "main": main })))
}

/// Extract a zip into `dir`, refusing entries that would escape it, and flatten a single
/// top-level folder (the layout GitHub archives use).
fn extract_zip(zip_path: &FsPath, dir: &FsPath) -> anyhow::Result<usize> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > ZIP_ENTRY_LIMIT {
        anyhow::bail!("archive exceeds 10,000 entries");
    }
    std::fs::create_dir_all(dir)?;
    let mut count = 0;
    let mut expanded = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        if rel
            .components()
            .any(|c| c.as_os_str() == "__MACOSX" || c.as_os_str() == ".git")
        {
            continue;
        }
        if !rel.components().all(|c| match c {
            Component::Normal(n) => n.to_str().is_some_and(safe_component),
            _ => false,
        }) {
            anyhow::bail!("archive contains an invalid path");
        }
        if entry.is_symlink() {
            anyhow::bail!("archive contains a symlink");
        }
        if entry.size() > UPLOAD_LIMIT || entry.size().saturating_add(expanded) > EXTRACT_LIMIT {
            anyhow::bail!("archive exceeds extraction limits (64 MiB per file, 512 MiB total)");
        }
        let out = file_in_project(dir, &rel.to_string_lossy()).map_err(|e| anyhow::anyhow!(e.1))?;
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&out)?;
        let copied = std::io::copy(
            &mut std::io::Read::take(&mut entry, UPLOAD_LIMIT.min(EXTRACT_LIMIT - expanded) + 1),
            &mut f,
        )?;
        expanded += copied;
        if copied > UPLOAD_LIMIT || expanded > EXTRACT_LIMIT {
            anyhow::bail!("archive exceeds extraction limits");
        }
        count += 1;
    }
    if count == 0 {
        anyhow::bail!("archive contains no files");
    }
    let top: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    if top.len() == 1 && top[0].path().is_dir() {
        let inner = top[0].path();
        for e in std::fs::read_dir(&inner)?.filter_map(|e| e.ok()) {
            std::fs::rename(e.path(), dir.join(e.file_name()))?;
        }
        std::fs::remove_dir_all(&inner)?;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Overleaf gallery (anonymous tag pages only; sources need a login, see the UI)

#[derive(Deserialize)]
pub struct GalleryQuery {
    tag: String,
}

#[derive(Serialize, Clone)]
pub struct GalleryItem {
    title: String,
    url: String,
    thumb: String,
    description: String,
}

fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

fn between<'a>(hay: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = hay.find(start)? + start.len();
    let j = hay[i..].find(end)? + i;
    Some(&hay[i..j])
}

fn parse_gallery(html: &str) -> Vec<GalleryItem> {
    let mut out = Vec::new();
    for chunk in html.split("<div class=\"gallery-thumbnail").skip(1) {
        let end = chunk
            .char_indices()
            .map(|(i, _)| i)
            .find(|&i| i >= 6000)
            .unwrap_or(chunk.len());
        let chunk = &chunk[..end];
        let Some(href) = between(chunk, "href=\"", "\"") else {
            continue;
        };
        if !href.starts_with("/latex/templates/") || href.contains("/tagged/") {
            continue;
        }
        let title = between(chunk, "caption-title\">", "</span>")
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            continue;
        }
        let thumb = between(chunk, "<img src=\"", "\"").unwrap_or("");
        let description = between(chunk, "caption-description\">", "</p>")
            .unwrap_or("")
            .trim();
        out.push(GalleryItem {
            title: html_unescape(title),
            url: format!("https://www.overleaf.com{href}"),
            thumb: html_unescape(thumb),
            description: html_unescape(description).chars().take(240).collect(),
        });
    }
    out
}

pub async fn overleaf_gallery(
    State(state): State<AppState>,
    Query(q): Query<GalleryQuery>,
) -> ApiResult<Vec<GalleryItem>> {
    let tag: String = q
        .tag
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if tag.is_empty() || tag.len() > 80 {
        return Err(bad("tag is empty"));
    }
    if let Some((when, items)) = state.gallery_cache.lock().await.get(&tag) {
        if when.elapsed() < std::time::Duration::from_secs(1800) {
            return Ok(Json(items.clone()));
        }
    }
    let url = format!("https://www.overleaf.com/latex/templates/tagged/{tag}");
    let out = run(
        &state.cache,
        "curl",
        &[
            "-sSfL",
            "-m",
            "20",
            "-A",
            "Mozilla/5.0 (gitleaf)",
            "--",
            &url,
        ],
    )
    .await?;
    if !out.ok {
        if out.stderr.contains("404") {
            return Ok(Json(Vec::new()));
        }
        return Err(ApiError(
            StatusCode::BAD_GATEWAY,
            format!(
                "could not reach the Overleaf gallery: {}",
                out.stderr.trim()
            ),
        ));
    }
    let items = parse_gallery(&out.stdout);
    let mut cache = state.gallery_cache.lock().await;
    cache.retain(|_, (when, _)| when.elapsed() < Duration::from_secs(1800));
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(tag, (Instant::now(), items.clone()));
    Ok(Json(items))
}

#[derive(Deserialize)]
pub struct SourceQuery {
    url: String,
    title: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SourceCandidate {
    repo: String,
    url: String,
    stars: u64,
    description: String,
    /// "page" when the Overleaf template page links to it, "search" when GitHub search found it.
    source: String,
}

fn title_words(title: &str) -> Vec<String> {
    title
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| {
            w.len() >= 3
                && !matches!(
                    *w,
                    "template" | "latex" | "the" | "and" | "for" | "with" | "version"
                )
        })
        .map(String::from)
        .collect()
}

/// `github.com/owner/repo` links in a page, repos first whose name shares a word with the title.
fn github_repos_in(html: &str, title: &str) -> Vec<String> {
    const SKIP: &[&str] = &[
        "sponsors",
        "topics",
        "orgs",
        "features",
        "site",
        "login",
        "join",
        "about",
        "pricing",
        "marketplace",
        "settings",
        "apps",
    ];
    let words = title_words(title);
    let mut found: Vec<String> = Vec::new();
    for chunk in html.split("github.com/").skip(1) {
        let path: String = chunk
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
            .collect();
        let mut parts = path.split('/');
        let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
            continue;
        };
        let repo = repo.trim_end_matches(".git").trim_end_matches('.');
        if owner.is_empty() || repo.is_empty() || SKIP.contains(&owner) || repo == "sponsors" {
            continue;
        }
        let full = format!("{owner}/{repo}");
        if !found.contains(&full) {
            found.push(full);
        }
    }
    found.sort_by_key(|f| {
        let name = f.split('/').nth(1).unwrap_or("").to_lowercase();
        let hit = words.iter().any(|w| name.contains(w.as_str()));
        if hit {
            0
        } else {
            1
        }
    });
    found
}

/// Find the source repository of an Overleaf gallery template without a login: the template page
/// often links to GitHub; otherwise GitHub search by title, star-ranked.
pub async fn overleaf_source(
    State(state): State<AppState>,
    Query(q): Query<SourceQuery>,
) -> ApiResult<Vec<SourceCandidate>> {
    let url = q.url.trim().to_string();
    if !(url.starts_with("https://www.overleaf.com/latex/")
        || url.starts_with("https://overleaf.com/latex/"))
    {
        return Err(bad("not an Overleaf gallery link"));
    }
    let title = q.title.unwrap_or_default();
    let mut out: Vec<SourceCandidate> = Vec::new();
    if let Ok(page) = run(
        &state.cache,
        "curl",
        &[
            "-sSfL",
            "-m",
            "20",
            "-A",
            "Mozilla/5.0 (gitleaf)",
            "--",
            &url,
        ],
    )
    .await
    {
        if page.ok {
            let page_title = between(&page.stdout, "<title>", "</title>")
                .unwrap_or("")
                .trim();
            let t = if title.is_empty() {
                page_title.to_string()
            } else {
                title.clone()
            };
            for repo in github_repos_in(&page.stdout, &t).into_iter().take(3) {
                out.push(SourceCandidate {
                    url: format!("https://github.com/{repo}"),
                    repo,
                    stars: 0,
                    description: "linked from the Overleaf template page".into(),
                    source: "page".into(),
                });
            }
        }
    }
    if out.is_empty() && !title.is_empty() {
        let words = title_words(&title);
        if !words.is_empty() {
            let query = format!("{} latex", words.join("+"));
            let api_url = format!(
                "https://api.github.com/search/repositories?q={}&sort=stars&per_page=5",
                query.replace(' ', "+")
            );
            if let Ok(res) = run(
                &state.cache,
                "curl",
                &["-sSfL", "-m", "20", "-A", "gitleaf", "--", &api_url],
            )
            .await
            {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&res.stdout) {
                    for item in json["items"].as_array().cloned().unwrap_or_default() {
                        let full = item["full_name"].as_str().unwrap_or("").to_string();
                        let name = full.split('/').nth(1).unwrap_or("").to_lowercase();
                        let stars = item["stargazers_count"].as_u64().unwrap_or(0);
                        let desc = item["description"].as_str().unwrap_or("").to_string();
                        let related = words.iter().any(|w| {
                            name.contains(w.as_str()) || desc.to_lowercase().contains(w.as_str())
                        });
                        if !full.is_empty() && related && stars >= 5 {
                            out.push(SourceCandidate {
                                url: format!("https://github.com/{full}"),
                                repo: full,
                                stars,
                                description: desc.chars().take(120).collect(),
                                source: "search".into(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Files

const TEXT_EXT: &[&str] = &[
    "tex",
    "bib",
    "cls",
    "sty",
    "bst",
    "bbx",
    "cbx",
    "dtx",
    "ins",
    "def",
    "clo",
    "ltx",
    "md",
    "txt",
    "json",
    "csv",
    "tsv",
    "py",
    "r",
    "yml",
    "yaml",
    "toml",
    "cfg",
    "ini",
    "html",
    "css",
    "js",
    "sh",
    "log",
    "aux",
    "bbl",
    "blg",
    "xml",
    "svg",
    "latexmkrc",
    "gitignore",
    "gitattributes",
];

fn is_text(path: &FsPath) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    TEXT_EXT.contains(&ext.as_str())
        || name.starts_with('.')
        || matches!(name, "Makefile" | "README" | "LICENSE")
}

#[derive(Serialize)]
pub struct FileEntry {
    path: String,
    size: u64,
    text: bool,
}

pub async fn list_files(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Vec<FileEntry>> {
    let dir = project_dir(&state, &name)?;
    let files = tokio::task::spawn_blocking(move || project_files(&dir))
        .await
        .map_err(internal)??;
    Ok(Json(files))
}

fn project_files(dir: &FsPath) -> Result<Vec<FileEntry>, ApiError> {
    let mut files = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git");
    for entry in walker {
        let entry = entry.map_err(internal)?;
        if files.len() >= ZIP_ENTRY_LIMIT {
            return Err(bad("project exceeds 10,000 files"));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(dir).unwrap_or(entry.path());
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        files.push(FileEntry {
            path: rel.to_string_lossy().replace('\\', "/"),
            size,
            text: is_text(rel),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[derive(Deserialize)]
pub struct FileQuery {
    path: String,
    create: Option<bool>,
}

pub async fn read_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<FileQuery>,
) -> ApiResult<serde_json::Value> {
    let dir = project_dir(&state, &name)?;
    let full = file_in_project(&dir, &q.path)?;
    let metadata = tokio::fs::metadata(&full)
        .await
        .map_err(|_| not_found("file not found"))?;
    if !metadata.is_file() || metadata.len() > UPLOAD_LIMIT {
        return Err(bad("file exceeds the 64 MiB preview limit"));
    }
    let bytes = tokio::fs::read(&full)
        .await
        .map_err(|_| not_found(format!("'{}' not found", q.path)))?;
    let content = String::from_utf8(bytes)
        .map_err(|_| ApiError(StatusCode::UNSUPPORTED_MEDIA_TYPE, "binary file".into()))?;
    Ok(Json(
        serde_json::json!({ "path": q.path, "content": content }),
    ))
}

/// Raw bytes of any project file (images, PDFs in the repo), for previews.
pub async fn raw_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let dir = project_dir(&state, &name)?;
    let full = file_in_project(&dir, &q.path)?;
    let metadata = tokio::fs::metadata(&full)
        .await
        .map_err(|_| not_found("file not found"))?;
    if !metadata.is_file() || metadata.len() > UPLOAD_LIMIT {
        return Err(bad("file exceeds the 64 MiB preview limit"));
    }
    let bytes = tokio::fs::read(&full)
        .await
        .map_err(|_| not_found(format!("'{}' not found", q.path)))?;
    let mime = match full
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'",
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Binary upload (figures, bib files from disk): the request body is the file.
pub async fn upload_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<FileQuery>,
    body: Bytes,
) -> ApiResult<serde_json::Value> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    let full = file_in_project(&dir, &q.path)?;
    if q.create.unwrap_or(false) && full.exists() {
        return Err(bad("file already exists"));
    }
    if let Some(parent) = full.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(internal)?;
    }
    write_document(full, body.to_vec()).await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "path": q.path, "bytes": body.len() }),
    ))
}

#[derive(Deserialize)]
pub struct RenameReq {
    from: String,
    to: String,
}

/// Rename or move a file or folder inside the project (`git mv` when tracked, plain rename otherwise).
pub async fn rename_path(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<RenameReq>,
) -> ApiResult<serde_json::Value> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    let from = file_in_project(&dir, &req.from)?;
    let to = file_in_project(&dir, &req.to)?;
    if !from.exists() {
        return Err(not_found(format!("'{}' not found", req.from)));
    }
    if to.exists() {
        return Err(bad(format!("'{}' already exists", req.to)));
    }
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(internal)?;
    }
    let mv = git(&dir, &["mv", "--", &req.from, &req.to]).await?;
    if !mv.ok || !to.exists() {
        tokio::fs::rename(&from, &to).await.map_err(internal)?;
    }
    Ok(Json(
        serde_json::json!({ "ok": true, "from": req.from, "to": req.to }),
    ))
}

#[derive(Deserialize)]
pub struct WriteReq {
    content: String,
}

pub async fn write_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<FileQuery>,
    Json(req): Json<WriteReq>,
) -> ApiResult<serde_json::Value> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    let full = file_in_project(&dir, &q.path)?;
    if q.create.unwrap_or(false) && full.exists() {
        return Err(bad("file already exists"));
    }
    if let Some(parent) = full.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(internal)?;
    }
    write_document(full, req.content.as_bytes().to_vec()).await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "path": q.path, "bytes": req.content.len() }),
    ))
}

pub async fn delete_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<FileQuery>,
) -> ApiResult<serde_json::Value> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    let full = file_in_project(&dir, &q.path)?;
    tokio::fs::remove_file(&full)
        .await
        .map_err(|_| not_found(format!("'{}' not found", q.path)))?;
    Ok(Json(serde_json::json!({ "ok": true, "path": q.path })))
}

// ---------------------------------------------------------------------------
// Compile

#[derive(Deserialize, Default)]
pub struct CompileReq {
    main: Option<String>,
    engine: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Problem {
    kind: String, // "error" | "warning"
    file: String,
    line: Option<u32>,
    message: String,
}

#[derive(Serialize)]
pub struct CompileResult {
    ok: bool,
    engine: String,
    duration_ms: u128,
    pdf: bool,
    errors: Vec<String>,
    problems: Vec<Problem>,
    log: String,
}

/// Turn the TeX log and the engine's stderr into a short list of problems with file and line.
/// Handles `file:line: message` (latexmk -file-line-error, tectonic), `! message` + `l.N`,
/// and the usual LaTeX warnings (overfull boxes, undefined references and citations).
fn extract_problems(log: &str, stderr: &str, main: &str) -> Vec<Problem> {
    let mut out: Vec<Problem> = Vec::new();
    let push = |out: &mut Vec<Problem>, mut p: Problem| {
        p.message = p.message.trim().trim_end_matches('.').to_string();
        p.file = p.file.trim_start_matches("./").to_string();
        if !out.iter().any(|q| {
            q.kind == p.kind && q.file == p.file && q.line == p.line && q.message == p.message
        }) && out.len() < 200
        {
            out.push(p);
        }
    };
    let file_line = |l: &str| -> Option<(String, u32, String)> {
        let l = l
            .trim_start_matches("error: ")
            .trim_start_matches("warning: ");
        // Find :<line>: without confusing a Windows drive or a colon in the message.
        for (index, _) in l.match_indices(':') {
            let Some((number, message)) = l[index + 1..].split_once(':') else {
                continue;
            };
            let Ok(line) = number.trim().parse::<u32>() else {
                continue;
            };
            let file = l[..index].trim().trim_start_matches("./");
            let message = message.trim();
            if file.contains('.') && !message.is_empty() {
                return Some((file.to_string(), line, message.to_string()));
            }
        }
        None
    };
    for l in stderr.lines() {
        if let Some((file, line, msg)) = file_line(l) {
            push(
                &mut out,
                Problem {
                    kind: if l.starts_with("warning:") {
                        "warning"
                    } else {
                        "error"
                    }
                    .into(),
                    file,
                    line: Some(line),
                    message: msg,
                },
            );
        }
    }
    let lines: Vec<&str> = log.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if let Some((file, line, msg)) = file_line(l) {
            push(
                &mut out,
                Problem {
                    kind: if l.starts_with("warning:") {
                        "warning"
                    } else {
                        "error"
                    }
                    .into(),
                    file,
                    line: Some(line),
                    message: msg,
                },
            );
        } else if let Some(msg) = l.strip_prefix("! ") {
            // Look a few lines ahead for "l.<n>".
            let mut line = None;
            for following in lines.iter().take((i + 8).min(lines.len())).skip(i + 1) {
                if let Some(rest) = following.strip_prefix("l.") {
                    line = rest.split_whitespace().next().and_then(|n| n.parse().ok());
                    break;
                }
            }
            push(
                &mut out,
                Problem {
                    kind: "error".into(),
                    file: main.to_string(),
                    line,
                    message: msg.trim().to_string(),
                },
            );
        } else if l.starts_with("Overfull \\hbox")
            || l.starts_with("Underfull \\hbox")
            || l.starts_with("Overfull \\vbox")
        {
            let line = l
                .split("at lines ")
                .nth(1)
                .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|n| n.parse().ok())
                .or_else(|| {
                    l.split("at line ")
                        .nth(1)
                        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
                        .and_then(|n| n.parse().ok())
                });
            let msg = l.split(" in ").next().unwrap_or(l).trim().to_string();
            push(
                &mut out,
                Problem {
                    kind: "warning".into(),
                    file: main.to_string(),
                    line,
                    message: msg,
                },
            );
        } else if l.starts_with("LaTeX Warning:")
            || l.starts_with("Package ") && l.contains("Warning:")
        {
            let mut msg = l.trim_start_matches("LaTeX Warning:").trim().to_string();
            if msg.is_empty() && i + 1 < lines.len() {
                msg = lines[i + 1].trim().to_string();
            }
            let line = l
                .split("on input line ")
                .nth(1)
                .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|n| n.parse().ok());
            push(
                &mut out,
                Problem {
                    kind: "warning".into(),
                    file: main.to_string(),
                    line,
                    message: msg,
                },
            );
        }
        i += 1;
    }
    out.sort_by(|a, b| {
        (a.kind != "error")
            .cmp(&(b.kind != "error"))
            .then(a.line.cmp(&b.line))
    });
    out
}

// ---- SyncTeX: map source lines to PDF positions and back, by reading <stem>.synctex.gz.

struct SyncRec {
    page: u32,
    tag: u32,
    line: u32,
    x: f64, // PDF points (bp), from the left
    y: f64, // PDF points (bp), from the top
}

struct SyncData {
    files: std::collections::HashMap<u32, String>,
    recs: Vec<SyncRec>,
}

fn read_synctex(path: &FsPath, project: &FsPath) -> anyhow::Result<SyncData> {
    use std::io::Read;
    let raw = std::fs::read(path)?;
    let mut text = String::new();
    if path.extension().map(|e| e == "gz").unwrap_or(false) {
        flate2::read::GzDecoder::new(&raw[..])
            .take(64 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        if text.len() > 64 * 1024 * 1024 {
            anyhow::bail!("SyncTeX data exceeds 64 MiB");
        }
    } else {
        text = String::from_utf8_lossy(&raw).into_owned();
    }
    let mut files = std::collections::HashMap::new();
    let mut recs = Vec::new();
    let mut page = 0u32;
    let mut unit = 1.0f64;
    let (mut xoff, mut yoff) = (0.0f64, 0.0f64);
    let sp_to_bp = |v: f64| v / 65536.0 * 72.0 / 72.27;
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix("Input:") {
            let mut p = rest.splitn(2, ':');
            if let (Some(t), Some(f)) = (p.next(), p.next()) {
                if let Ok(t) = t.parse() {
                    let f = f.trim();
                    let rel = FsPath::new(f)
                        .strip_prefix(project)
                        .map(|r| r.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| f.trim_start_matches("./").to_string());
                    files.insert(t, rel);
                }
            }
        } else if let Some(u) = l.strip_prefix("Unit:") {
            unit = u.trim().parse().unwrap_or(1.0);
        } else if let Some(v) = l.strip_prefix("X Offset:") {
            xoff = v.trim().parse().unwrap_or(0.0);
        } else if let Some(v) = l.strip_prefix("Y Offset:") {
            yoff = v.trim().parse().unwrap_or(0.0);
        } else if let Some(p) = l.strip_prefix('{') {
            page = p.trim().parse().unwrap_or(page);
        } else if let Some(c) = l.chars().next() {
            if matches!(c, 'x' | 'k' | 'g' | '$' | 'v' | 'h' | 'r' | '[' | '(') && page > 0 {
                let body = &l[1..];
                let mut a = body.splitn(2, ':');
                let tl = a.next().unwrap_or("");
                let xy = a.next().unwrap_or("");
                let mut tl = tl.split(',');
                let mut xy = xy.split(':').next().unwrap_or("").split(',');
                if let (Some(t), Some(ln), Some(x), Some(y)) =
                    (tl.next(), tl.next(), xy.next(), xy.next())
                {
                    if let (Ok(tag), Ok(line), Ok(x), Ok(y)) =
                        (t.parse(), ln.parse(), x.parse::<f64>(), y.parse::<f64>())
                    {
                        recs.push(SyncRec {
                            page,
                            tag,
                            line,
                            x: sp_to_bp(x * unit + xoff),
                            y: sp_to_bp(y * unit + yoff),
                        });
                    }
                }
            }
        }
    }
    if recs.is_empty() {
        anyhow::bail!("no SyncTeX records");
    }
    Ok(SyncData { files, recs })
}

fn compile_directory(state: &AppState, name: &str, main: &str) -> Result<PathBuf, ApiError> {
    let project = project_dir(state, name)?;
    file_in_project(&project, main)?;
    if FsPath::new(main).extension().and_then(|s| s.to_str()) != Some("tex") {
        return Err(bad("main file must be a .tex file"));
    }
    let key = format!("{:x}", Sha256::digest(main.as_bytes()));
    Ok(state.cache.join(name).join(key))
}
fn synctex_path(state: &AppState, name: &str, main: &str) -> Result<PathBuf, ApiError> {
    if !valid_name(name) {
        return Err(bad("invalid project name"));
    }
    let stem = FsPath::new(main)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    let dir = compile_directory(state, name, main)?;
    for cand in [
        format!("{stem}.ok.synctex.gz"),
        format!("{stem}.ok.synctex"),
    ] {
        let p = dir.join(cand);
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(ApiError(
        StatusCode::NOT_FOUND,
        "no SyncTeX data for this compile (compile once more; latexmk and tectonic both write it)"
            .into(),
    ))
}

#[derive(Deserialize)]
pub struct ForwardQuery {
    main: Option<String>,
    file: String,
    line: u32,
}

/// Source line -> PDF page and position.
pub async fn synctex_forward(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<ForwardQuery>,
) -> ApiResult<serde_json::Value> {
    let main = q.main.clone().unwrap_or_else(|| "main.tex".into());
    let path = synctex_path(&state, &name, &main)?;
    let project = project_dir(&state, &name)?;
    let data = tokio::task::spawn_blocking(move || read_synctex(&path, &project))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let want = q.file.trim_start_matches("./");
    let tags: Vec<u32> = data
        .files
        .iter()
        .filter(|(_, f)| {
            f == &want || f.ends_with(&format!("/{want}")) || want.ends_with(f.as_str())
        })
        .map(|(t, _)| *t)
        .collect();
    if tags.is_empty() {
        return Err(not_found(format!(
            "'{want}' is not part of the last compile"
        )));
    }
    // Closest line at or after the requested one, else the closest before.
    let best = data
        .recs
        .iter()
        .filter(|r| tags.contains(&r.tag))
        .min_by_key(|r| {
            if r.line >= q.line {
                (0u8, r.line - q.line, r.page)
            } else {
                (1u8, q.line - r.line, r.page)
            }
        })
        .ok_or_else(|| not_found("no position for that line"))?;
    Ok(Json(
        serde_json::json!({ "page": best.page, "x": best.x, "y": best.y, "line": best.line }),
    ))
}

#[derive(Deserialize)]
pub struct InverseQuery {
    main: Option<String>,
    page: u32,
    x: f64,
    y: f64,
}

/// PDF position -> source file and line.
pub async fn synctex_inverse(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<InverseQuery>,
) -> ApiResult<serde_json::Value> {
    if !q.x.is_finite() || !q.y.is_finite() || q.page == 0 {
        return Err(bad("invalid PDF coordinates"));
    }
    let main = q.main.clone().unwrap_or_else(|| "main.tex".into());
    let path = synctex_path(&state, &name, &main)?;
    let project = project_dir(&state, &name)?;
    let data = tokio::task::spawn_blocking(move || read_synctex(&path, &project))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let best = data
        .recs
        .iter()
        .filter(|r| {
            r.page == q.page
                && data
                    .files
                    .get(&r.tag)
                    .is_some_and(|f| !FsPath::new(f).is_absolute())
        })
        .min_by(|a, b| {
            let da = (a.y - q.y).abs() * 3.0 + (a.x - q.x).abs();
            let db = (b.y - q.y).abs() * 3.0 + (b.x - q.x).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| not_found("nothing on that page"))?;
    let file = data.files.get(&best.tag).cloned().unwrap_or(main);
    Ok(Json(serde_json::json!({ "file": file, "line": best.line })))
}

/// Returns (kind, program): kind is "latexmk" or "tectonic", program is what to execute.
fn pick_engine(state: &AppState, requested: Option<&str>) -> Result<(String, String), ApiError> {
    let wanted = requested
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(String::from)
        .or_else(|| std::env::var("GITLEAF_ENGINE").ok());
    let tectonic_program = || -> Option<String> {
        if which("tectonic") {
            Some("tectonic".into())
        } else {
            bundled_engine(state).map(|p| p.to_string_lossy().into_owned())
        }
    };
    match wanted.as_deref() {
        Some("latexmk") => return Ok(("latexmk".into(), "latexmk".into())),
        Some("tectonic") => {
            return tectonic_program()
                .map(|p| ("tectonic".into(), p))
                .ok_or_else(|| {
                    ApiError(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "no LaTeX engine found".into(),
                    )
                })
        }
        Some(other) => {
            return Err(bad(format!(
                "unknown engine '{other}' (use latexmk or tectonic)"
            )))
        }
        None => {}
    }
    if which("latexmk") {
        Ok(("latexmk".into(), "latexmk".into()))
    } else if let Some(p) = tectonic_program() {
        Ok(("tectonic".into(), p))
    } else {
        Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "no LaTeX engine found: install TeX Live (latexmk) or let gitleaf download Tectonic"
                .into(),
        ))
    }
}

pub(crate) async fn project_lock(state: &AppState, name: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = state.locks.lock().await;
    if locks.len() >= 128 {
        locks.retain(|_, lock| lock.strong_count() > 0);
    }
    let key = name.to_ascii_lowercase();
    if let Some(lock) = locks.get(&key).and_then(std::sync::Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

/// Pull the decisive lines out of a LaTeX log: `! ...` errors (plus the line after) and
/// `file:line: ...` messages produced by -file-line-error.
fn extract_errors(log: &str) -> Vec<String> {
    let lines: Vec<&str> = log.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() && out.len() < 50 {
        let l = lines[i];
        if l.starts_with('!') {
            let mut msg = l.to_string();
            if let Some(next) = lines.get(i + 1).filter(|n| !n.trim().is_empty()) {
                msg.push_str("  ");
                msg.push_str(next.trim());
            }
            out.push(msg);
        } else if l.contains(".tex:")
            && l.split(':')
                .nth(1)
                .map(|n| n.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
            || l.starts_with("error:")
        {
            out.push(l.to_string());
        }
        i += 1;
    }
    out
}

pub async fn compile(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<CompileReq>,
) -> ApiResult<CompileResult> {
    let dir = project_dir(&state, &name)?;
    let main = req
        .main
        .clone()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "main.tex".into());
    let main_path = file_in_project(&dir, &main)?;
    if !main_path.is_file() {
        return Err(not_found(format!("'{main}' not found")));
    }
    let (engine, program) = pick_engine(&state, req.engine.as_deref())?;
    let outdir = compile_directory(&state, &name, &main)?;
    tokio::fs::create_dir_all(&outdir).await.map_err(internal)?;
    let outdir_s = outdir.to_string_lossy().into_owned();
    let stem = FsPath::new(&main)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main")
        .to_string();

    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    for extension in ["pdf", "log", "synctex", "synctex.gz"] {
        let _ = tokio::fs::remove_file(outdir.join(format!("{stem}.{extension}"))).await;
    }
    let started = Instant::now();
    let out = match engine.as_str() {
        "latexmk" => {
            let outarg = format!("-outdir={outdir_s}");
            run(
                &dir,
                "latexmk",
                &[
                    "-norc",
                    "-pdf",
                    "-no-shell-escape",
                    "-interaction=nonstopmode",
                    "-file-line-error",
                    "-synctex=1",
                    &outarg,
                    "--",
                    &main,
                ],
            )
            .await?
        }
        _ => {
            run(
                &dir,
                &program,
                &[
                    "--untrusted",
                    "--outdir",
                    &outdir_s,
                    "--keep-logs",
                    "--keep-intermediates",
                    "--synctex",
                    "--",
                    &main,
                ],
            )
            .await?
        }
    };
    let duration_ms = started.elapsed().as_millis();

    let log_file = tokio::fs::read_to_string(outdir.join(format!("{stem}.log")))
        .await
        .unwrap_or_default();
    let mut errors = extract_errors(&log_file);
    errors.extend(extract_errors(&out.stderr));
    errors.dedup();

    let mut log = String::new();
    log.push_str(&out.stdout);
    if !out.stderr.is_empty() {
        log.push_str("\n--- stderr ---\n");
        log.push_str(&out.stderr);
    }
    if !log_file.is_empty() {
        log.push_str("\n--- ");
        log.push_str(&stem);
        log.push_str(".log ---\n");
        log.push_str(&log_file);
    }
    const LIMIT: usize = 30_000;
    if log.len() > LIMIT {
        let cut = log.len() - LIMIT;
        let cut = log
            .char_indices()
            .map(|(i, _)| i)
            .find(|&i| i >= cut)
            .unwrap_or(cut);
        log = format!("[... {cut} bytes trimmed ...]\n{}", &log[cut..]);
    }

    let problems = extract_problems(&log_file, &out.stderr, &main);
    // Keep the last PDF that compiled cleanly, so a typo never blanks the preview.
    let fresh = outdir.join(format!("{stem}.pdf"));
    let good = outdir.join(format!("{stem}.ok.pdf"));
    let succeeded = out.ok && fresh.is_file();
    if succeeded {
        let bytes = tokio::fs::read(&fresh).await.map_err(internal)?;
        write_document(good.clone(), bytes).await?;
        for suffix in ["synctex.gz", "synctex"] {
            let source = outdir.join(format!("{stem}.{suffix}"));
            let target = outdir.join(format!("{stem}.ok.{suffix}"));
            if source.is_file() {
                write_document(target, tokio::fs::read(source).await.map_err(internal)?).await?;
            } else {
                let _ = tokio::fs::remove_file(target).await;
            }
        }
    }
    let pdf = good.is_file();
    Ok(Json(CompileResult {
        ok: succeeded,
        engine,
        duration_ms,
        pdf,
        errors,
        problems,
        log,
    }))
}

#[derive(Deserialize)]
pub struct PdfQuery {
    main: Option<String>,
    download: Option<bool>,
}

pub async fn pdf(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<PdfQuery>,
) -> Result<Response, ApiError> {
    if !valid_name(&name) {
        return Err(bad("invalid project name"));
    }
    let stem = q
        .main
        .as_deref()
        .and_then(|m| FsPath::new(m).file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("main")
        .to_string();
    if !safe_component(&stem) {
        return Err(bad("invalid main file"));
    }
    let dir = compile_directory(&state, &name, q.main.as_deref().unwrap_or("main.tex"))?;
    let good = dir.join(format!("{stem}.ok.pdf"));
    let path = good;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| not_found("no PDF yet; compile first"))?;
    let safe_stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = if stem == "main" {
        format!("{name}.pdf")
    } else {
        format!("{name}-{safe_stem}.pdf")
    };
    let disposition = if q.download.unwrap_or(false) {
        "attachment"
    } else {
        "inline"
    };
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("{disposition}; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// The whole project (minus `.git`) as a zip, for handing to co-authors or uploading elsewhere.
pub async fn project_zip(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let dir = project_dir(&state, &name)?;
    let bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut total = 0u64;
            let mut zw = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let walker = walkdir::WalkDir::new(&dir)
                .follow_links(false)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| e.file_name() != ".git");
            for entry in walker.filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() {
                    continue;
                }
                total = total.saturating_add(entry.metadata()?.len());
                if total > EXTRACT_LIMIT {
                    anyhow::bail!("project exceeds the 512 MiB download limit");
                }
                let rel = entry
                    .path()
                    .strip_prefix(&dir)?
                    .to_string_lossy()
                    .replace('\\', "/");
                zw.start_file(rel, opts)?;
                std::io::copy(&mut std::fs::File::open(entry.path())?, &mut zw)?;
            }
            zw.finish()?;
        }
        Ok(buf.into_inner())
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}.zip\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Git

#[derive(Serialize)]
pub struct GitStatus {
    branch: Option<String>,
    remote: Option<String>,
    changes: Vec<String>,
    ahead: u32,
    behind: u32,
    last_commit: Option<String>,
}

pub async fn git_status(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<GitStatus> {
    let dir = project_dir(&state, &name)?;
    let (branch, remote, status, counts, last_commit) = tokio::join!(
        git_line(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]),
        git_line(&dir, &["remote", "get-url", "origin"]),
        git(&dir, &["status", "--porcelain"]),
        git_line(
            &dir,
            &["rev-list", "--left-right", "--count", "HEAD...@{u}"]
        ),
        git_line(
            &dir,
            &["log", "-1", "--date=short", "--pretty=format:%h %ad %s"]
        )
    );
    let changes = status
        .map(|o| o.stdout.lines().map(String::from).collect())
        .unwrap_or_default();
    let (ahead, behind) = counts
        .and_then(|s| {
            let mut it = s.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .unwrap_or((0, 0));
    Ok(Json(GitStatus {
        branch,
        remote,
        changes,
        ahead,
        behind,
        last_commit,
    }))
}

#[derive(Serialize)]
pub struct LogEntry {
    hash: String,
    date: String,
    author: String,
    subject: String,
}

pub async fn git_log(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Vec<LogEntry>> {
    let dir = project_dir(&state, &name)?;
    let out = git(
        &dir,
        &[
            "log",
            "-n",
            "40",
            "--date=short",
            "--pretty=format:%h%x09%ad%x09%an%x09%s",
        ],
    )
    .await?;
    let entries = out
        .stdout
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\t');
            Some(LogEntry {
                hash: p.next()?.into(),
                date: p.next()?.into(),
                author: p.next()?.into(),
                subject: p.next().unwrap_or("").into(),
            })
        })
        .collect();
    Ok(Json(entries))
}

#[derive(Deserialize)]
pub struct CommitReq {
    message: String,
}

pub async fn git_commit(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<CommitReq>,
) -> ApiResult<CmdOut> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    let message = req.message.trim();
    if message.is_empty() {
        return Err(bad("commit message is empty"));
    }
    let add = git(&dir, &["add", "-A"]).await?;
    if !add.ok {
        return Ok(Json(add));
    }
    Ok(Json(git(&dir, &["commit", "-m", message]).await?))
}

pub async fn git_push(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<CmdOut> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    let out = git(&dir, &["push"]).await?;
    if !out.ok && out.stderr.contains("no upstream branch") {
        return Ok(Json(git(&dir, &["push", "-u", "origin", "HEAD"]).await?));
    }
    Ok(Json(out))
}

pub async fn git_pull(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<CmdOut> {
    let lock = project_lock(&state, &name).await;
    let _guard = lock.lock().await;
    let dir = project_dir(&state, &name)?;
    Ok(Json(git(&dir, &["pull", "--rebase", "--autostash"]).await?))
}

// ---------------------------------------------------------------------------
// Tests (pure helpers; nothing here touches the network or the user's projects)

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gitleaf-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    #[test]
    fn project_names_are_validated() {
        assert!(valid_name("my-paper"));
        assert!(valid_name("6ab276a09b30df4bbc6c1e49"));
        assert!(valid_name("v1.2_final"));
        assert!(!valid_name(""));
        assert!(!valid_name(".git"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("a b"));
        assert!(!valid_name(&"x".repeat(101)));
    }

    #[test]
    fn overleaf_project_links_are_rewritten_only_for_the_real_host() {
        assert_eq!(
            overleaf_project_git_url("https://www.overleaf.com/project/6ab276a09b30df4bbc6c1e49")
                .as_deref(),
            Some("https://git.overleaf.com/6ab276a09b30df4bbc6c1e49")
        );
        assert_eq!(
            overleaf_project_git_url("https://overleaf.com/project/6ab276a09b30df4bbc6c1e49?x=1")
                .as_deref(),
            Some("https://git.overleaf.com/6ab276a09b30df4bbc6c1e49")
        );
        assert!(overleaf_project_git_url(
            "https://evil-overleaf.com/project/6ab276a09b30df4bbc6c1e49"
        )
        .is_none());
        assert!(overleaf_project_git_url("https://www.overleaf.com/project/short").is_none());
        assert!(overleaf_project_git_url(
            "https://www.overleaf.com/latex/templates/x/abcdefghijkl"
        )
        .is_none());
        assert!(overleaf_project_git_url("https://github.com/user/repo").is_none());
    }

    #[test]
    fn names_derive_from_urls() {
        assert_eq!(
            name_from_url("https://github.com/user/my-repo.git"),
            "my-repo"
        );
        assert_eq!(
            name_from_url("https://github.com/user/repo.git#readme"),
            "repo"
        );
        assert_eq!(
            name_from_url("https://example.org/dl/template.zip?x=1"),
            "template"
        );
        assert_eq!(name_from_url("git@github.com:user/paper.git"), "paper");
        assert_eq!(name_from_url("https://h/a b.zip"), "a-b");
    }

    #[test]
    fn file_paths_cannot_escape_the_project() {
        let project = tmp("paths");
        std::fs::write(project.join("main.tex"), "x").unwrap();
        assert!(file_in_project(&project, "main.tex").is_ok());
        assert!(file_in_project(&project, "sections/new.tex").is_ok());
        assert!(file_in_project(&project, "../other.tex").is_err());
        assert!(file_in_project(&project, "/etc/passwd").is_err());
        assert!(file_in_project(&project, ".git/config").is_err());
        assert!(file_in_project(&project, "").is_err());
        #[cfg(unix)]
        {
            let outside = tmp("paths-outside");
            std::os::unix::fs::symlink(&outside, project.join("link")).unwrap();
            assert!(
                file_in_project(&project, "link/secret.tex").is_err(),
                "symlink escape must be refused"
            );
            std::os::unix::fs::symlink(outside.join("does-not-exist"), project.join("dangling"))
                .unwrap();
            assert!(
                file_in_project(&project, "dangling").is_err(),
                "dangling symlink must be refused"
            );
            std::fs::create_dir_all(project.join("real")).unwrap();
            std::os::unix::fs::symlink(project.join("real"), project.join("inside")).unwrap();
            assert!(
                file_in_project(&project, "inside/x.tex").is_ok(),
                "symlink inside the project is fine"
            );
        }
    }

    #[test]
    fn text_files_are_recognised() {
        assert!(is_text(FsPath::new("main.tex")));
        assert!(is_text(FsPath::new("refs.bib")));
        assert!(is_text(FsPath::new(".gitignore")));
        assert!(is_text(FsPath::new("Makefile")));
        assert!(!is_text(FsPath::new("figures/plot.png")));
        assert!(!is_text(FsPath::new("paper.pdf")));
    }

    #[test]
    fn main_file_is_detected_from_documentclass() {
        let d = tmp("main");
        std::fs::create_dir_all(d.join("chapters")).unwrap();
        std::fs::write(d.join("appendix.tex"), "\\section{A}").unwrap();
        std::fs::write(d.join("paper.tex"), "% comment\n\\documentclass{article}").unwrap();
        std::fs::write(d.join("chapters/intro.tex"), "\\documentclass{x}").unwrap();
        assert_eq!(
            detect_main(&d),
            "paper.tex",
            "shallowest file with \\documentclass wins"
        );
        std::fs::write(d.join("main.tex"), "").unwrap();
        assert_eq!(detect_main(&d), "main.tex", "main.tex always wins");
    }

    #[test]
    fn problems_are_parsed_from_log_and_stderr() {
        let log = "This is XeTeX\n! Undefined control sequence.\nl.24 Start here \\undefinedcmd\n\nOverfull \\hbox (12.3pt too wide) in paragraph at lines 40--41\nLaTeX Warning: Citation `foo' on page 1 undefined on input line 12.\n./sections/a.tex:7: Missing $ inserted.\n";
        let stderr = "error: main.tex:24: Undefined control sequence\n";
        let p = extract_problems(log, stderr, "main.tex");
        let errors: Vec<_> = p.iter().filter(|x| x.kind == "error").collect();
        let warnings: Vec<_> = p.iter().filter(|x| x.kind == "warning").collect();
        assert_eq!(
            errors.len(),
            2,
            "duplicate 'Undefined control sequence' collapsed: {errors:?}"
        );
        assert!(errors
            .iter()
            .any(|e| e.file == "main.tex" && e.line == Some(24)));
        assert!(errors.iter().any(|e| e.file == "sections/a.tex"
            && e.line == Some(7)
            && e.message == "Missing $ inserted"));
        assert_eq!(warnings.len(), 2);
        assert!(warnings
            .iter()
            .any(|w| w.line == Some(40) && w.message.starts_with("Overfull")));
        assert!(warnings
            .iter()
            .any(|w| w.line == Some(12) && w.message.contains("Citation")));
        assert_eq!(p[0].kind, "error", "errors sort first");
        let two = extract_problems("./a.tex:1: Same\n./b.tex:1: Same\n", "", "main.tex");
        assert_eq!(
            two.len(),
            2,
            "same message on the same line in different files are two problems"
        );
    }

    #[test]
    fn legacy_error_lines_are_extracted() {
        let log = "! Missing } inserted.\n<inserted text>\n./main.tex:3: Something\nerror: boom\nnormal line";
        let e = extract_errors(log);
        assert_eq!(e.len(), 3);
        assert!(e[0].starts_with("! Missing } inserted."));
    }

    #[test]
    fn synctex_maps_lines_to_pages_and_back() {
        let project = tmp("synctex");
        let text = "SyncTeX Version:1\nInput:1:./main.tex\nInput:2:/usr/share/texmf/article.cls\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n!100\n{1\n[1,1:4736286,4736286:0,0,0\nx1,5:4736286,6000000\nx1,9:4736286,9000000\nx2,3:4736286,9500000\n]\n}1\n{2\n[1,12:4736286,4736286:0,0,0\nx1,12:5000000,7000000\n]\n}2\n";
        let path = project.join("main.synctex.gz");
        {
            let f = std::fs::File::create(&path).unwrap();
            let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            gz.write_all(text.as_bytes()).unwrap();
            gz.finish().unwrap();
        }
        let data = read_synctex(&path, &project).unwrap();
        assert_eq!(data.files.get(&1).map(String::as_str), Some("main.tex"));
        assert_eq!(data.recs.len(), 6);
        // forward: line 7 -> nearest line at or after (9), page 1
        let fwd = data
            .recs
            .iter()
            .filter(|r| r.tag == 1)
            .min_by_key(|r| {
                if r.line >= 7 {
                    (0u8, r.line - 7)
                } else {
                    (1u8, 7 - r.line)
                }
            })
            .unwrap();
        assert_eq!((fwd.page, fwd.line), (1, 9));
        // units: 4736286 sp = 72.27pt = exactly one inch = 72 bp
        assert!((fwd.x - 72.0).abs() < 0.01, "x was {}", fwd.x);
        // inverse: a point on page 2 maps back to line 12
        let inv = data
            .recs
            .iter()
            .filter(|r| r.page == 2)
            .min_by(|a, b| {
                (a.y - 100.0)
                    .abs()
                    .partial_cmp(&(b.y - 100.0).abs())
                    .unwrap()
            })
            .unwrap();
        assert_eq!(inv.line, 12);
    }

    #[test]
    fn gallery_html_is_parsed() {
        let html = r#"<div class="gallery-thumbnail col"><a href="/latex/templates/acm-x/abcdefghijkl"><div class="thumbnail"><img src="https://img/1.jpeg?sig=a&amp;b" alt=""></div><span class="gallery-list-item-title"><span class="caption-title">ACM &amp; Friends</span></span></a><div class="caption"><p class="caption-description">A template.</p></div></div><div class="gallery-thumbnail"><a href="/latex/templates/tagged/acm">tag</a><div class="caption">"#;
        let items = parse_gallery(html);
        assert_eq!(items.len(), 1, "tag links are skipped");
        assert_eq!(items[0].title, "ACM & Friends");
        assert_eq!(
            items[0].url,
            "https://www.overleaf.com/latex/templates/acm-x/abcdefghijkl"
        );
        assert_eq!(items[0].thumb, "https://img/1.jpeg?sig=a&b");
        assert_eq!(items[0].description, "A template.");
    }

    #[test]
    fn zip_extraction_refuses_escapes_and_flattens() {
        let d = tmp("zip");
        let zip_path = d.join("in.zip");
        {
            let f = std::fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let o = zip::write::SimpleFileOptions::default();
            zw.start_file("repo-main/main.tex", o).unwrap();
            zw.write_all(b"\\documentclass{article}").unwrap();
            zw.start_file("repo-main/figs/a.png", o).unwrap();
            zw.write_all(b"png").unwrap();
            zw.start_file("../../evil.txt", o).unwrap();
            zw.write_all(b"x").unwrap();
            zw.start_file("repo-main/.git/config", o).unwrap();
            zw.write_all(b"x").unwrap();
            zw.finish().unwrap();
        }
        let out = d.join("out");
        let n = extract_zip(&zip_path, &out).unwrap();
        assert_eq!(n, 2, "only the two real files are written");
        assert!(
            out.join("main.tex").is_file(),
            "single top-level folder is flattened"
        );
        assert!(out.join("figs/a.png").is_file());
        assert!(!out.join(".git").exists());
        assert!(!d.join("evil.txt").exists() && !d.parent().unwrap().join("evil.txt").exists());
        assert_eq!(detect_main(&out), "main.tex");
    }

    #[test]
    fn github_repos_are_found_on_template_pages() {
        let html = r#"<a href="https://github.com/sponsors/x">s</a> text https://github.com/liantze/AltaCV more <a href="https://github.com/someone/profile-site">p</a> https://github.com/liantze/AltaCV.git"#;
        let repos = github_repos_in(html, "AltaCV Template");
        assert_eq!(
            repos,
            vec![
                "liantze/AltaCV".to_string(),
                "someone/profile-site".to_string()
            ],
            "sponsor link skipped, duplicate merged, title match first"
        );
        assert!(github_repos_in("no links here", "x").is_empty());
        assert_eq!(
            title_words("A Customised CurVe CV Template"),
            vec!["customised", "curve"]
        );
    }

    #[test]
    fn html_entities_are_unescaped() {
        assert_eq!(
            html_unescape("a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#39;"),
            "a & b <c> \"d\" 'e'"
        );
    }

    #[test]
    fn git_metadata_aliases_and_nonportable_paths_are_blocked() {
        let project = tmp("metadata");
        std::fs::create_dir_all(project.join(".git/hooks")).unwrap();
        for path in [
            "nested/.git/config",
            "nested/.GIT/config",
            "figures\\secret",
            "C:main.tex",
            "CON.tex",
            "a/../main.tex",
            "file. ",
        ] {
            assert!(file_in_project(&project, path).is_err(), "accepted {path}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(project.join(".git"), project.join("alias")).unwrap();
            assert!(file_in_project(&project, "alias/config").is_err());
        }
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn cache_keys_distinguish_same_named_main_files() {
        let root = tmp("cache-keys");
        let project = root.join("paper");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let state = AppState {
            root: root.clone(),
            cache: root.join("cache"),
            port: 8077,
            update: Default::default(),
            shutdown: Default::default(),
            restart: Default::default(),
            locks: Default::default(),
            gallery_cache: Default::default(),
            presence: Default::default(),
        };
        let first = compile_directory(&state, "paper", "one/main.tex").unwrap();
        let second = compile_directory(&state, "paper", "two/main.tex").unwrap();
        assert_ne!(first, second);
        assert!(compile_directory(&state, "paper", "../bad.tex").is_err());
        assert!(compile_directory(&state, "paper", "main.pdf").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn downloaded_engine_checksum_is_verified() {
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(digest_matches(b"abc", expected));
        assert!(digest_matches(b"abc", &expected.to_uppercase()));
        assert!(!digest_matches(b"changed", expected));
        assert!(!digest_matches(b"abc", ""));
    }

    #[test]
    fn gallery_parser_handles_multibyte_text_at_its_cutoff() {
        let html = format!("<div class=\"gallery-thumbnail{}", "界".repeat(3000));
        assert!(parse_gallery(&html).is_empty());
    }

    #[test]
    fn warning_stderr_and_windows_paths_are_parsed_correctly() {
        let problems = extract_problems(
            "",
            "warning: chapter.tex:3: A warning\nerror: C:\\paper\\main.tex:5: Broken command\n",
            "main.tex",
        );
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].kind, "error");
        assert_eq!(problems[0].line, Some(5));
        assert_eq!(problems[1].kind, "warning");
    }

    #[test]
    fn zip_symlinks_are_refused() {
        let root = tmp("zip-symlink");
        let archive = root.join("in.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        writer
            .add_symlink(
                "link",
                "../../outside",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.finish().unwrap();
        assert!(extract_zip(&archive, &root.join("out")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zip_entry_count_is_bounded() {
        let root = tmp("zip-count");
        let archive = root.join("in.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        for index in 0..=ZIP_ENTRY_LIMIT {
            writer
                .start_file(
                    format!("{index}.tex"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
        }
        writer.finish().unwrap();
        assert!(extract_zip(&archive, &root.join("out")).is_err());
        assert!(!root.join("out").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn external_commands_time_out() {
        let result = run_with_timeout(
            FsPath::new("/tmp"),
            "sleep",
            &["5"],
            Duration::from_millis(20),
        )
        .await;
        assert!(matches!(
            result,
            Err(ApiError(StatusCode::GATEWAY_TIMEOUT, _))
        ));
    }
}
