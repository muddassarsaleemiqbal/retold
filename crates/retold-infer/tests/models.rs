//! Offline HTTP fixtures exercise real downloads, range handling and atomic activation.
use retold_infer::models::{Asset, Manifest, ModelStore};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use tokio_util::sync::CancellationToken;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
#[tokio::test]
async fn interrupted_download_resumes_and_corruption_is_refetched() -> Result {
    let bytes = b"pinned-model-fixture";
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
        let mut requests = Vec::new();
        for attempt in 0..3 {
            let (mut stream, _) = listener.accept()?;
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                stream.read_exact(&mut b)?;
                request.push(b[0]);
            }
            let request = String::from_utf8_lossy(&request).to_string();
            requests.push(request);
            if attempt == 0 {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )?;
                stream.write_all(&bytes[..5])?;
                stream.flush()?;
                std::thread::sleep(std::time::Duration::from_millis(100));
            } else if attempt == 1 {
                write!(
                    stream,
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 5-{}/{}\r\nConnection: close\r\n\r\n",
                    bytes.len() - 5,
                    bytes.len() - 1,
                    bytes.len()
                )?;
                stream.write_all(&bytes[5..])?;
            } else {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )?;
                stream.write_all(bytes)?;
            }
        }
        Ok(requests)
    });
    let temp = tempfile::tempdir()?;
    let store = ModelStore {
        root: temp.path().to_owned(),
    };
    let manifest = Manifest {
        name: "fixture".into(),
        version: "v1".into(),
        license: "MIT".into(),
        source: "fixture".into(),
        files: vec![Asset {
            path: "model.bin".into(),
            url: format!("http://127.0.0.1:{port}/model"),
            size: u64::try_from(bytes.len())?,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }],
    };
    let cancel = CancellationToken::new();
    assert!(store.install(&manifest, &cancel).await.is_err());
    assert!(!temp.path().join("fixture/active.json").exists());
    assert_eq!(
        std::fs::read(store.directory(&manifest).join("model.download"))?,
        bytes[..5]
    );
    store.install(&manifest, &cancel).await?;
    store.verify(&manifest)?;
    std::fs::write(
        store.directory(&manifest).join("model.bin"),
        vec![0; bytes.len()],
    )?;
    assert!(store.verify(&manifest).is_err());
    store.install(&manifest, &cancel).await?;
    store.verify(&manifest)?;
    let requests = server.join().map_err(|_| "server thread panicked")??;
    assert!(requests[1].to_lowercase().contains("range: bytes=5-"));
    assert!(!requests[2].to_lowercase().contains("range:"));
    store.remove(&manifest)?;
    assert!(store.verify(&manifest).is_err());
    Ok(())
}
#[tokio::test]
async fn unsafe_paths_and_cancelled_installs_never_activate() -> Result {
    let mut manifest = retold_infer::models::catalog()?.remove(0);
    let temp = tempfile::tempdir()?;
    let store = ModelStore {
        root: temp.path().to_owned(),
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        store.install(&manifest, &cancel).await,
        Err(retold_infer::Error::Cancelled)
    ));
    manifest.files[0].path = "../escape".into();
    assert!(manifest.validate().is_err());
    manifest.name = "../escape".into();
    assert!(store.remove(&manifest).is_err());
    Ok(())
}

#[test]
fn killed_download_helper() -> Result {
    let Ok(root) = std::env::var("RETOLD_DOWNLOAD_FIXTURE") else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(root);
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    tokio::runtime::Runtime::new()?.block_on(
        ModelStore {
            root: root.join("models"),
        }
        .install(&manifest, &CancellationToken::new()),
    )?;
    Ok(())
}
#[tokio::test]
async fn interrupted_download_resumes_its_durable_partial_file() -> Result {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let bytes = vec![42_u8; 1024];
    let root = tempfile::tempdir()?;
    let store = ModelStore {
        root: root.path().join("models"),
    };
    let manifest = Manifest {
        name: "fixture".into(),
        version: "v1".into(),
        license: "MIT".into(),
        source: "fixture".into(),
        files: vec![Asset {
            path: "model.bin".into(),
            url: format!("http://127.0.0.1:{port}/model"),
            size: 1024,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        }],
    };
    std::fs::write(
        root.path().join("manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut first, _) = listener.accept()?;
        let _request = read_headers(&mut first)?;
        write!(
            first,
            "HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\n"
        )?;
        for chunk in bytes.chunks(32) {
            if first.write_all(chunk).is_err() {
                break;
            }
            first.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        drop(first);
        let (mut second, _) = listener.accept()?;
        let request = read_headers(&mut second)?.to_lowercase();
        let range = request
            .lines()
            .find_map(|s| s.strip_prefix("range: bytes="))
            .ok_or_else(|| std::io::Error::other("missing resume range"))?;
        let offset = range
            .trim_end_matches('-')
            .parse::<usize>()
            .map_err(std::io::Error::other)?;
        write!(
            second,
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {offset}-1023/1024\r\nConnection: close\r\n\r\n",
            1024 - offset
        )?;
        second.write_all(&bytes[offset..])?;
        Ok(request)
    });
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "killed_download_helper"])
        .env("RETOLD_DOWNLOAD_FIXTURE", root.path())
        .stdout(std::process::Stdio::null())
        .spawn()?;
    let partial = store.directory(&manifest).join("model.download");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !std::fs::metadata(&partial).is_ok_and(|m| m.len() > 0) {
        if std::time::Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err("download helper failed to start".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    child.kill()?;
    child.wait()?;
    let length = std::fs::metadata(&partial)?.len();
    assert!(length < 1024);
    assert!(!store.root.join("fixture/active.json").exists());
    store.install(&manifest, &CancellationToken::new()).await?;
    store.verify(&manifest)?;
    let request = server.join().map_err(|_| "download server panicked")??;
    assert!(request.contains(&format!("range: bytes={length}-")));
    Ok(())
}

fn read_headers(stream: &mut std::net::TcpStream) -> std::io::Result<String> {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b)?;
        request.push(b[0]);
    }
    Ok(String::from_utf8_lossy(&request).into_owned())
}
