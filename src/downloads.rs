//! Reusable native HTTP clients and bounded, checksum-producing file downloads.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::io::AsyncWriteExt;

pub const TEXT_LIMIT: u64 = 8 * 1024 * 1024;
pub const BINARY_LIMIT: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct HttpClient {
    secure: reqwest::Client,
    imports: reqwest::Client,
}

fn client(https_only: bool) -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(reqwest::Client::builder()
        .https_only(https_only)
        .redirect(reqwest::redirect::Policy::limited(5))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!(
            "Mozilla/5.0 (gitleaf/",
            env!("CARGO_PKG_VERSION"),
            ")"
        ))
        .referer(false)
        .build()?)
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            secure: client(true)?,
            imports: client(false)?,
        })
    }

    async fn response(
        client: &reqwest::Client,
        url: &str,
        timeout: Duration,
        limit: u64,
    ) -> Result<reqwest::Response> {
        let response = client
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| e.without_url())?
            .error_for_status()
            .map_err(|e| e.without_url())?;
        if response.content_length().is_some_and(|size| size > limit) {
            bail!("download exceeds the {limit}-byte limit");
        }
        Ok(response)
    }

    pub async fn text(&self, url: &str, timeout: Duration) -> Result<String> {
        let mut response = Self::response(&self.secure, url, timeout, TEXT_LIMIT).await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.without_url())? {
            if chunk.len() as u64 > TEXT_LIMIT.saturating_sub(bytes.len() as u64) {
                bail!("response exceeds the {TEXT_LIMIT}-byte limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Release downloads and their redirects must remain HTTPS.
    pub async fn download(
        &self,
        url: &str,
        path: &Path,
        timeout: Duration,
        limit: u64,
    ) -> Result<String> {
        self.download_with(&self.secure, url, path, timeout, limit)
            .await
    }

    /// ZIP imports continue to support explicitly supplied HTTP as well as HTTPS URLs.
    pub async fn import(
        &self,
        url: &str,
        path: &Path,
        timeout: Duration,
        limit: u64,
    ) -> Result<String> {
        self.download_with(&self.imports, url, path, timeout, limit)
            .await
    }

    async fn download_with(
        &self,
        client: &reqwest::Client,
        url: &str,
        path: &Path,
        timeout: Duration,
        limit: u64,
    ) -> Result<String> {
        let mut response = Self::response(client, url, timeout, limit).await?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = path.with_extension(format!(
            "download-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .context("could not stage the download")?;
        let cleanup = Temporary(temporary.clone());
        let mut file = tokio::fs::File::from_std(file);
        let digest = tokio::time::timeout(timeout, async {
            let mut size = 0_u64;
            let mut hash = Sha256::new();
            while let Some(chunk) = response.chunk().await.map_err(|e| e.without_url())? {
                if chunk.len() as u64 > limit.saturating_sub(size) {
                    bail!("download exceeds the {limit}-byte limit");
                }
                file.write_all(&chunk).await?;
                hash.update(&chunk);
                size += chunk.len() as u64;
            }
            file.sync_all().await?;
            Ok::<_, anyhow::Error>(format!("{:x}", hash.finalize()))
        })
        .await
        .context("download timed out")?;
        // Close before replacing or removing a file, including on Windows.
        drop(file);
        let digest = digest?;
        tokio::fs::rename(&temporary, path)
            .await
            .context("could not finish the download")?;
        drop(cleanup);
        Ok(digest)
    }
}

struct Temporary(std::path::PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn serve(
        response: &'static [u8],
        pause: Duration,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request).await;
            let _ = socket.write_all(response).await;
            tokio::time::sleep(pause).await;
        });
        (url, task)
    }

    #[tokio::test]
    async fn streams_and_hashes_without_truncating_an_existing_file_on_failure() {
        let dir =
            std::env::temp_dir().join(format!("gitleaf-download-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("binary");
        let http = HttpClient::new().unwrap();
        let (url, task) = serve(
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
            Duration::ZERO,
        )
        .await;
        let digest = http
            .import(&url, &path, Duration::from_secs(2), 3)
            .await
            .unwrap();
        task.await.unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let (url, task) = serve(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\noops\r\n0\r\n\r\n", Duration::ZERO).await;
        assert!(http
            .import(&url, &path, Duration::from_secs(2), 3)
            .await
            .is_err());
        task.await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn reuses_connections_and_reports_http_status_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            // One accepted socket must serve both calls, including through a cloned client.
            let (mut socket, _) = listener.accept().await.unwrap();
            for _ in 0..2 {
                let mut request = Vec::new();
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let mut buffer = [0; 1024];
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                }
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndata")
                    .await
                    .unwrap();
            }
        });
        let secure = client(false).unwrap();
        let http = HttpClient {
            imports: secure.clone(),
            secure,
        };
        assert_eq!(
            http.text(&url, Duration::from_secs(2)).await.unwrap(),
            "data"
        );
        assert_eq!(
            http.clone()
                .text(&url, Duration::from_secs(2))
                .await
                .unwrap(),
            "data"
        );
        server.await.unwrap();
        let (url, server) = serve(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            Duration::ZERO,
        )
        .await;
        assert!(http
            .text(&url, Duration::from_secs(2))
            .await
            .unwrap_err()
            .to_string()
            .contains("404"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_insecure_release_requests_and_declared_oversize_and_slow_bodies() {
        let http = HttpClient::new().unwrap();
        assert!(http
            .text("http://127.0.0.1:1", Duration::from_secs(1))
            .await
            .is_err());
        let dir =
            std::env::temp_dir().join(format!("gitleaf-download-failure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("binary");
        let (url, task) = serve(
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n",
            Duration::ZERO,
        )
        .await;
        assert!(http
            .import(&url, &path, Duration::from_secs(1), 3)
            .await
            .is_err());
        task.await.unwrap();
        let (url, task) = serve(
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\na",
            Duration::from_secs(1),
        )
        .await;
        assert!(http
            .import(&url, &path, Duration::from_millis(50), 3)
            .await
            .is_err());
        task.abort();
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
