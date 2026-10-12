//! gitleaf: a small Overleaf-style LaTeX editor served over HTTP.
//!
//! Every project is a git repository inside one root directory. The browser UI edits files,
//! compiles with latexmk or tectonic, and commits/pushes with the system `git`, so any remote
//! (GitHub, Overleaf, a bare repo on a server) works with the credentials git already has.

mod api;
mod assets;
mod security;
mod storage;
mod templates;
#[cfg(test)]
mod tests;
mod update;

use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use tokio::sync::Mutex;

type GalleryCache = Arc<Mutex<HashMap<String, (std::time::Instant, Vec<api::GalleryItem>)>>>;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub port: u16,
    pub update: Arc<Mutex<update::UpdateCache>>,
    pub shutdown: Arc<tokio::sync::Notify>,
    pub restart: Arc<std::sync::atomic::AtomicBool>,
    /// Directory that holds one git repository per project.
    pub root: PathBuf,
    /// Directory for compile outputs (`<cache>/<project>/main.pdf`), kept out of the repos.
    pub cache: PathBuf,
    /// One lock per project so two compiles never run on the same repo at once.
    pub locks: Arc<Mutex<HashMap<String, std::sync::Weak<Mutex<()>>>>>,
    /// Overleaf gallery pages, cached for half an hour per tag.
    pub gallery_cache: GalleryCache,
}

fn env_path(var: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(default)
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_root() -> PathBuf {
    let docs = home().join("Documents");
    let legacy = docs.join("Codex").join("Overleaf");
    if legacy.is_dir() {
        legacy
    } else {
        docs.join("gitleaf")
    }
}

fn default_cache() -> PathBuf {
    if cfg!(target_os = "macos") {
        home().join("Library").join("Caches").join("gitleaf")
    } else if cfg!(target_os = "windows") {
        env_path("LOCALAPPDATA", || home().join("AppData").join("Local")).join("gitleaf")
    } else {
        env_path("XDG_CACHE_HOME", || home().join(".cache")).join("gitleaf")
    }
}

fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        eprintln!("could not open a browser ({e}); open {url} yourself");
    }
}

const HELP: &str = "gitleaf: a small Overleaf-style LaTeX editor; every paper is a git repository.

USAGE: gitleaf [--root DIR] [--cache DIR] [--port N] [--no-open]

  --root DIR    folder holding one git repository per paper
                (default: ~/Documents/gitleaf, or ~/Documents/Codex/Overleaf when it exists)
  --cache DIR   compile outputs and downloaded tools (default: the OS cache folder)
  --port N      listen port on 127.0.0.1 (default 8077)
  --no-open     do not open the browser on start
Environment variables GITLEAF_ROOT, GITLEAF_CACHE, GITLEAF_PORT, GITLEAF_ENGINE (latexmk|tectonic)
and GITLEAF_NO_OPEN do the same.
";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut root = env_path("GITLEAF_ROOT", default_root);
    let mut cache = env_path("GITLEAF_CACHE", default_cache);
    let mut port: u16 = std::env::var("GITLEAF_PORT")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(8077);
    let mut open = std::env::var_os("GITLEAF_NO_OPEN").is_none();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => {
                root = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--root needs a value"))?,
                )
            }
            "--cache" => {
                cache = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--cache needs a value"))?,
                )
            }
            "--port" => {
                port = args
                    .next()
                    .and_then(|p| p.parse().ok())
                    .ok_or_else(|| anyhow::anyhow!("--port needs a number"))?
            }
            "--no-open" => open = false,
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("gitleaf {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other}\n{HELP}"),
        }
    }

    if port == 0 {
        anyhow::bail!("port must be between 1 and 65535");
    }
    std::fs::create_dir_all(&root)?;
    std::fs::create_dir_all(&cache)?;
    let state = AppState {
        port,
        update: Default::default(),
        shutdown: Default::default(),
        restart: Default::default(),
        root: root.canonicalize()?,
        cache: cache.canonicalize()?,
        locks: Default::default(),
        gallery_cache: Default::default(),
    };

    if state.root == state.cache
        || state.cache.starts_with(&state.root)
        || state.root.starts_with(&state.cache)
    {
        anyhow::bail!("project root and build cache must be separate directories");
    }
    if !api::git_available() {
        anyhow::bail!("git is required; install Git and ensure it is on PATH");
    }

    let app = router(state.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if existing_instance(addr, &state).await {
                let url = format!("http://{addr}");
                println!("gitleaf already running at {url}");
                if open {
                    open_browser(&url);
                }
                return Ok(());
            }
            anyhow::bail!("port {port} is occupied by another service or GitLeaf configuration; choose --port");
        }
        Err(error) => return Err(error.into()),
    };
    println!(
        "gitleaf {} listening on http://{addr}",
        env!("CARGO_PKG_VERSION")
    );
    println!("  projects root: {}", state.root.display());
    println!("  build cache:   {}", state.cache.display());
    println!("  press Ctrl-C to stop");
    if open {
        open_browser(&format!("http://{addr}"));
    }
    update::cleanup_after_update();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown(state.shutdown.clone()))
        .await?;
    if state.restart.load(std::sync::atomic::Ordering::SeqCst) {
        let args = std::env::args()
            .skip(1)
            .filter(|argument| argument != "--no-open");
        std::process::Command::new(std::env::current_exe()?)
            .args(args)
            .arg("--no-open")
            .stdin(std::process::Stdio::null())
            .spawn()?;
    }
    Ok(())
}

async fn existing_instance(addr: SocketAddr, state: &AppState) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut stream = tokio::net::TcpStream::connect(addr).await.ok()?;
        stream
            .write_all(
                format!("GET /api/health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .ok()?;
        let mut bytes = Vec::new();
        stream.take(16 * 1024).read_to_end(&mut bytes).await.ok()?;
        let response = String::from_utf8(bytes).ok()?;
        let (headers, body) = response.split_once("\r\n\r\n")?;
        if !headers.starts_with("HTTP/1.1 200 ") {
            return None;
        }
        let health: serde_json::Value = serde_json::from_str(body).ok()?;
        Some(health["app"] == "gitleaf" && health["instance"] == api::instance_id(state))
    })
    .await;
    matches!(result, Ok(Some(true)))
}

async fn shutdown(requested: Arc<tokio::sync::Notify>) {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = interrupt => {}, _ = terminate => {}, _ = requested.notified() => {} }
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(api::index))
        .route("/assets/{*path}", get(assets::serve))
        .route("/api/health", get(api::health))
        .route("/api/engine/install", post(api::install_engine))
        .route("/api/update/check", get(update::check))
        .route("/api/update/apply", post(update::apply))
        .route("/api/projects", get(api::list_projects))
        .route("/api/projects/clone", post(api::clone_project))
        .route("/api/projects/new", post(api::new_project))
        .route("/api/projects/import-url", post(api::import_url))
        .route(
            "/api/projects/import-zip",
            post(api::import_zip).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route("/api/templates", get(api::list_templates))
        .route("/api/overleaf/gallery", get(api::overleaf_gallery))
        .route("/api/overleaf/source", get(api::overleaf_source))
        .route("/api/projects/{name}/files", get(api::list_files))
        .route(
            "/api/projects/{name}/file",
            get(api::read_file)
                .put(api::write_file)
                .delete(api::delete_file),
        )
        .route(
            "/api/projects/{name}/raw",
            get(api::raw_file)
                .put(api::upload_file)
                .layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route("/api/projects/{name}/rename", post(api::rename_path))
        .route(
            "/api/projects/{name}/synctex/forward",
            get(api::synctex_forward),
        )
        .route(
            "/api/projects/{name}/synctex/inverse",
            get(api::synctex_inverse),
        )
        .route("/api/projects/{name}/compile", post(api::compile))
        .route("/api/projects/{name}/pdf", get(api::pdf))
        .route("/api/projects/{name}/zip", get(api::project_zip))
        .route("/api/projects/{name}/git/status", get(api::git_status))
        .route("/api/projects/{name}/git/log", get(api::git_log))
        .route("/api/projects/{name}/git/commit", post(api::git_commit))
        .route("/api/projects/{name}/git/push", post(api::git_push))
        .route("/api/projects/{name}/git/pull", post(api::git_pull))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::protect,
        ))
        .with_state(state)
}
