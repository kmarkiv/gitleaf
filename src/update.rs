//! Self-update: check GitHub releases, download the binary for this platform, verify its
//! SHA-256 against the release's SHA256SUMS.txt, swap it into place and restart.

use std::time::Instant;

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::AppState;

pub const REPO: &str = "kmarkiv/gitleaf";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub asset: Option<String>,
    /// Bundled Mac apps and source builds update as a whole, rather than replacing one executable.
    pub can_self_update: bool,
    pub manual_update_instructions: Option<String>,
    pub checked_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct UpdateCache {
    pub info: Option<UpdateInfo>,
    pub when: Option<Instant>,
}

/// The release asset name the workflow produces for this platform.
pub fn asset_name() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "gitleaf-macos-arm64"
    } else if cfg!(target_os = "macos") {
        "gitleaf-macos-x86_64"
    } else if cfg!(target_os = "windows") {
        "gitleaf-windows-x86_64.exe"
    } else if cfg!(target_arch = "aarch64") {
        "gitleaf-linux-arm64"
    } else {
        "gitleaf-linux-x86_64"
    }
}

/// (major, minor, patch) from "v1.2.3" or "1.2.3"; None when it does not parse.
pub fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim().strip_prefix('v').unwrap_or(version.trim());
    let core = version.split(['-', '+']).next()?;
    let parts: Vec<_> = core.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        if parts.len() == 3 {
            parts[2].parse().ok()?
        } else {
            0
        },
    ))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn manual_update_instructions(exe: &std::path::Path) -> Option<&'static str> {
    let path = exe.to_string_lossy().replace('\\', "/");
    if path.is_empty() {
        Some("The executable could not be located. Download the new version from the release page.")
    } else if path.to_ascii_lowercase().contains(".app/contents/macos/") {
        Some("Download the new Mac app, quit GitLeaf using the leaf menu, and replace GitLeaf.app in Applications. Your papers stay in the projects folder.")
    } else if path.contains("/target/") {
        Some("This copy runs from a build directory. Update with git pull, npm ci, npm run build and cargo build --release --locked.")
    } else {
        None
    }
}

fn current_manual_update_instructions() -> Option<String> {
    manual_update_instructions(&std::env::current_exe().unwrap_or_default()).map(String::from)
}

fn can_self_update() -> bool {
    current_manual_update_instructions().is_none()
}

fn trusted_asset(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/releases/download/"))
}

fn release_checksum(sums: &str, filename: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == filename && digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| digest.to_ascii_lowercase())
    })
}

async fn fetch_latest(state: &AppState) -> UpdateInfo {
    let mut info = UpdateInfo {
        current: CURRENT.into(),
        latest: None,
        update_available: false,
        url: None,
        notes: None,
        asset: None,
        can_self_update: can_self_update(),
        manual_update_instructions: current_manual_update_instructions(),
        checked_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs()),
        error: None,
    };
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    match state
        .http
        .text(&url, std::time::Duration::from_secs(30))
        .await
    {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(j) => {
                let tag = j["tag_name"].as_str().unwrap_or("").to_string();
                info.latest = Some(tag.trim_start_matches('v').to_string());
                info.url = j["html_url"].as_str().map(String::from);
                info.notes = j["body"].as_str().map(|b| b.chars().take(1500).collect());
                info.asset = j["assets"]
                    .as_array()
                    .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(asset_name())))
                    .and_then(|x| x["browser_download_url"].as_str().map(String::from));
                info.asset = info.asset.filter(|url| trusted_asset(url));
                info.url = info
                    .url
                    .filter(|url| url.starts_with(&format!("https://github.com/{REPO}/releases/")));
                info.update_available = is_newer(&tag, CURRENT)
                    && info.asset.is_some()
                    && j["prerelease"] != true
                    && j["draft"] != true;
            }
            Err(e) => info.error = Some(format!("unexpected response from GitHub: {e}")),
        },
        Err(e) => {
            info.error = Some(if e.to_string().contains("404") {
                "no release published yet".into()
            } else {
                e.to_string()
            })
        }
    }
    info
}

/// GET /api/update/check — cached for six hours; `?force=1` refreshes.
pub async fn check(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<UpdateInfo> {
    let force = q
        .get("force")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    let mut cache = state.update.lock().await;
    if let (Some(info), Some(when), false) = (&cache.info, cache.when, force) {
        if when.elapsed() < std::time::Duration::from_secs(6 * 3600) {
            return Json(info.clone());
        }
    }
    if !force && std::env::var_os("GITLEAF_NO_UPDATE_CHECK").is_some() {
        return Json(UpdateInfo {
            current: CURRENT.into(),
            latest: None,
            update_available: false,
            url: None,
            notes: None,
            asset: None,
            can_self_update: can_self_update(),
            manual_update_instructions: current_manual_update_instructions(),
            checked_at: None,
            error: Some("automatic update checks are disabled".into()),
        });
    }
    let info = fetch_latest(&state).await;
    cache.info = Some(info.clone());
    cache.when = Some(Instant::now());
    Json(info)
}

fn err(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (code, Json(serde_json::json!({ "error": msg.into() })))
}

/// Download and verify before replacing the executable on its own filesystem, then drain and restart.
pub async fn apply(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !can_self_update() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            current_manual_update_instructions()
                .unwrap_or_else(|| "Download the new version from the release page.".into()),
        ));
    }
    let lock = crate::api::project_lock(&state, ".app-update").await;
    let _guard = lock.lock().await;
    if state.restart.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(err(StatusCode::CONFLICT, "an update is already restarting"));
    }
    let info = fetch_latest(&state).await;
    if !info.update_available {
        return Err(err(
            StatusCode::BAD_REQUEST,
            info.error
                .unwrap_or_else(|| format!("no newer release than {CURRENT}")),
        ));
    }
    let (Some(url), Some(latest)) = (info.asset, info.latest) else {
        return Err(err(
            StatusCode::BAD_GATEWAY,
            "release metadata is incomplete",
        ));
    };
    let directory = state.cache.join("update");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let downloaded = directory.join(format!("{}.new", asset_name()));
    let result = async {
        let actual = state
            .http
            .download(
                &url,
                &downloaded,
                std::time::Duration::from_secs(60),
                crate::downloads::BINARY_LIMIT,
            )
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, e.to_string()))?;
        let (base, _) = url
            .rsplit_once('/')
            .ok_or_else(|| err(StatusCode::BAD_GATEWAY, "invalid release URL"))?;
        let sums = state
            .http
            .text(
                &format!("{base}/SHA256SUMS.txt"),
                std::time::Duration::from_secs(30),
            )
            .await
            .map_err(|e| {
                err(
                    StatusCode::BAD_GATEWAY,
                    format!("required release checksum unavailable: {e}"),
                )
            })?;
        let expected = release_checksum(&sums, asset_name()).ok_or_else(|| {
            err(
                StatusCode::BAD_GATEWAY,
                "release is missing a valid checksum for this platform",
            )
        })?;
        if actual != expected {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "checksum mismatch; nothing was changed",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&downloaded, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        let version = crate::api::run(&directory, &downloaded.to_string_lossy(), &["--version"])
            .await
            .map_err(|_| err(StatusCode::BAD_GATEWAY, "verified update could not run"))?;
        if !version.ok || version.stdout.trim() != format!("gitleaf {latest}") {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "downloaded binary version does not match the release",
            ));
        }
        let bytes = tokio::fs::read(&downloaded)
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let exe = std::env::current_exe()
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        #[cfg(unix)]
        {
            // Atomic replacement happens beside the executable, even when cache is on another disk.
            let destination = exe.clone();
            tokio::task::spawn_blocking(move || crate::storage::atomic_write(&destination, &bytes))
                .await
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        #[cfg(windows)]
        {
            let staged = exe.with_extension("new.exe");
            crate::storage::atomic_write(&staged, &bytes)
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let backup = exe.with_extension("old.exe");
            if backup.exists() {
                std::fs::remove_file(&backup)
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            std::fs::rename(&exe, &backup)
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if let Err(error) = std::fs::rename(&staged, &exe) {
                let recovery = std::fs::rename(&backup, &exe);
                let detail = match recovery {
                    Ok(()) => "restored previous executable".to_string(),
                    Err(problem) => format!(
                        "restore failed ({problem}); backup is at {}",
                        backup.display()
                    ),
                };
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("could not install update ({error}); {detail}"),
                ));
            }
        }
        state
            .restart
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let shutdown = state.shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            shutdown.notify_one();
        });
        Ok(Json(
            serde_json::json!({"ok": true, "from": CURRENT, "to": latest, "restarting": true}),
        ))
    }
    .await;
    let _ = tokio::fs::remove_file(downloaded).await;
    result
}

/// Remove only the updater's Windows backup after the new executable starts.
pub fn cleanup_after_update() {
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundles_update_as_a_whole_to_preserve_their_signature() {
        for path in [
            "/Applications/GitLeaf.app/Contents/MacOS/gitleaf",
            "/private/tmp/App Translocation/GitLeaf.app/Contents/MacOS/gitleaf",
        ] {
            assert!(manual_update_instructions(std::path::Path::new(path))
                .unwrap()
                .contains("replace GitLeaf.app"));
        }
        assert!(
            manual_update_instructions(std::path::Path::new("/repo/target/release/gitleaf"))
                .unwrap()
                .contains("build directory")
        );
        assert!(manual_update_instructions(std::path::Path::new(
            r"C:\repo\target\release\gitleaf.exe"
        ))
        .unwrap()
        .contains("build directory"));
        assert!(
            manual_update_instructions(std::path::Path::new("/usr/local/bin/gitleaf")).is_none()
        );
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.4.0", "0.3.0"));
        assert!(is_newer("0.3.10", "0.3.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("garbage", "0.3.0"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
    }

    #[test]
    fn asset_name_matches_release_workflow() {
        let n = asset_name();
        assert!(n.starts_with("gitleaf-"));
        assert!([
            "gitleaf-macos-arm64",
            "gitleaf-macos-x86_64",
            "gitleaf-windows-x86_64.exe",
            "gitleaf-linux-arm64",
            "gitleaf-linux-x86_64"
        ]
        .contains(&n));
    }
    #[test]
    fn malformed_versions_are_not_updates() {
        for version in ["1.2.bad", "1.2.3.4", "vv1.2.3", "", "1"] {
            assert!(parse_version(version).is_none(), "accepted {version}");
        }
    }
    #[test]
    fn checksums_must_name_the_exact_asset() {
        let checksum = "a".repeat(64);
        assert_eq!(
            release_checksum(&format!("{checksum}  {}", asset_name()), asset_name()),
            Some(checksum.clone())
        );
        assert!(
            release_checksum(&format!("{checksum}  other-{}", asset_name()), asset_name())
                .is_none()
        );
        assert!(release_checksum(&format!("bad  {}", asset_name()), asset_name()).is_none());
        assert!(release_checksum("", asset_name()).is_none());
    }
    #[test]
    fn update_urls_are_limited_to_the_official_release_repository() {
        assert!(trusted_asset(
            "https://github.com/kmarkiv/gitleaf/releases/download/v0.3.1/gitleaf-macos-arm64"
        ));
        assert!(!trusted_asset(
            "https://github.com/other/gitleaf/releases/download/v1/a"
        ));
        assert!(!trusted_asset(
            "http://github.com/kmarkiv/gitleaf/releases/download/v1/a"
        ));
    }
}
