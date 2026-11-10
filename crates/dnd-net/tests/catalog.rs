//! The catalog client against a real HTTP endpoint (ADR-0006, ADR-0060): signature first, cache
//! second, and a tampered index never becomes a catalog.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dnd_net::{CatalogClient, cached_path};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair as _};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn an_oversized_signature_is_rejected_before_decoding() {
    let files = Arc::new(Files {
        routes: BTreeMap::from([
            ("/index.json".into(), b"{}".to_vec()),
            ("/index.sig".into(), vec![b'a'; 2048]),
        ]),
        hits: BTreeMap::new(),
    });
    let base = serve(files, TcpListener::bind("127.0.0.1:0").await.unwrap()).await;
    let error = CatalogClient::new(&base, vec![0; 32])
        .unwrap()
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.code(), "download_too_large");
}

#[tokio::test]
async fn a_chunked_index_cannot_bypass_the_download_limit() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let chunk = vec![b' '; 64 * 1024];
        for _ in 0..257 {
            if socket.write_all(b"10000\r\n").await.is_err()
                || socket.write_all(&chunk).await.is_err()
                || socket.write_all(b"\r\n").await.is_err()
            {
                return;
            }
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });
    let error = CatalogClient::new(&base, vec![0; 32])
        .unwrap()
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.code(), "download_too_large");
}

fn key() -> (Ed25519KeyPair, Vec<u8>) {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
    let public = pair.public_key().as_ref().to_vec();
    (pair, public)
}

fn package_bytes() -> Vec<u8> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/theme-midnight");
    dnd_plugin::pack_directory(&dir).unwrap()
}

struct Files {
    routes: BTreeMap<String, Vec<u8>>,
    hits: BTreeMap<String, AtomicUsize>,
}

/// A tiny static server: one request per connection, then close.
async fn serve(files: Arc<Files>, listener: TcpListener) -> String {
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let files = Arc::clone(&files);
            tokio::spawn(async move {
                let mut buffer = [0u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                if let Some(counter) = files.hits.get(&path) {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
                match files.routes.get(&path) {
                    Some(body) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(header.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                    None => {
                        let _ = socket
                            .write_all(
                                b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                            )
                            .await;
                    }
                }
            });
        }
    });
    base
}

fn signed_index(pair: &Ed25519KeyPair, package: &[u8], archive_url: &str) -> (Vec<u8>, Vec<u8>) {
    use sha2::{Digest as _, Sha256};
    let hash = dnd_plugin::signature::encode_hex(&Sha256::digest(package));
    let signature = dnd_plugin::signature::encode_hex(
        pair.sign(&dnd_plugin::signature::plugin_transcript(
            "theme-midnight",
            "0.0.1",
            &hash,
        ))
        .as_ref(),
    );
    let index = format!(
        r#"{{
  "format": 1,
  "key_id": "catalog-test",
  "generated_at": "2026-11-08T00:00:00Z",
  "plugins": [
    {{
      "id": "theme-midnight",
      "version": "0.0.1",
      "kind": "theme",
      "license": "MIT OR Apache-2.0",
      "app_compat": ">=0.0.1",
      "sha256": "{hash}",
      "archive_url": "{archive_url}",
      "signature": "{signature}",
      "revoked": false,
      "dependencies": {{}}
    }}
  ]
}}
"#
    );
    let index_signature = dnd_plugin::signature::encode_hex(
        pair.sign(&dnd_plugin::signature::index_transcript(index.as_bytes()))
            .as_ref(),
    );
    (index.into_bytes(), index_signature.into_bytes())
}

#[tokio::test]
async fn a_signed_catalog_is_fetched_verified_and_cached() {
    let (pair, public) = key();
    let package = package_bytes();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (index, index_signature) =
        signed_index(&pair, &package, &format!("{base}/theme.dndplugin"));
    let files = Arc::new(Files {
        routes: BTreeMap::from([
            ("/index.json".to_owned(), index.clone()),
            ("/index.sig".to_owned(), index_signature),
            ("/theme.dndplugin".to_owned(), package.clone()),
        ]),
        hits: BTreeMap::from([
            ("/index.json".to_owned(), AtomicUsize::new(0)),
            ("/theme.dndplugin".to_owned(), AtomicUsize::new(0)),
        ]),
    });
    serve(Arc::clone(&files), listener).await;

    let client = CatalogClient::new(&base, public.clone()).unwrap();
    let catalog = client.fetch().await.unwrap();
    assert_eq!(catalog.key_id, "catalog-test");
    let entry = catalog
        .installable("theme-midnight", &semver("0.0.1"))
        .unwrap();
    assert!(!entry.revoked);

    let cache = tempfile::tempdir().unwrap();
    let first = client.package_bytes(entry, cache.path()).await.unwrap();
    assert_eq!(
        dnd_plugin::read_package(&first, dnd_plugin::PackageLimits::default())
            .unwrap()
            .manifest
            .id,
        "theme-midnight"
    );
    assert!(cached_path(entry, cache.path()).exists());
    // The second call is served from the cache: the package URL is not requested again.
    let second = client.package_bytes(entry, cache.path()).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(
        files.hits["/theme.dndplugin"].load(Ordering::SeqCst),
        1,
        "a verified cache entry is reused"
    );

    // A corrupted cache file is not trusted: it is downloaded again and replaced.
    let path = cached_path(entry, cache.path());
    std::fs::write(&path, b"corrupted").unwrap();
    let third = client.package_bytes(entry, cache.path()).await.unwrap();
    assert_eq!(third, first);
    assert_eq!(files.hits["/theme.dndplugin"].load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_tampered_index_is_refused() {
    let (pair, public) = key();
    let package = package_bytes();
    let (index, index_signature) =
        signed_index(&pair, &package, "http://example.invalid/theme.dndplugin");
    let tampered = String::from_utf8(index)
        .unwrap()
        .replace("MIT OR Apache-2.0", "proprietary");
    let files = Arc::new(Files {
        routes: BTreeMap::from([
            ("/index.json".to_owned(), tampered.into_bytes()),
            ("/index.sig".to_owned(), index_signature),
        ]),
        hits: BTreeMap::new(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = serve(files, listener).await;
    let client = CatalogClient::new(&base, public).unwrap();
    let error = client.fetch().await.unwrap_err();
    assert_eq!(error.code(), "signature_invalid");
}

#[tokio::test]
async fn a_wrong_key_never_yields_a_catalog() {
    let (pair, _) = key();
    let (_, other_public) = key();
    let package = package_bytes();
    let (index, index_signature) =
        signed_index(&pair, &package, "http://example.invalid/theme.dndplugin");
    let files = Arc::new(Files {
        routes: BTreeMap::from([
            ("/index.json".to_owned(), index),
            ("/index.sig".to_owned(), index_signature),
        ]),
        hits: BTreeMap::new(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = serve(files, listener).await;
    let client = CatalogClient::new(&base, other_public).unwrap();
    assert_eq!(
        client.fetch().await.unwrap_err().code(),
        "signature_invalid"
    );
}

#[tokio::test]
async fn a_missing_index_is_an_http_error() {
    let (listener, base) = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        (listener, base)
    };
    serve(
        Arc::new(Files {
            routes: BTreeMap::new(),
            hits: BTreeMap::new(),
        }),
        listener,
    )
    .await;
    let client = CatalogClient::new(&base, vec![0; 32]).unwrap();
    let error = client.fetch().await.unwrap_err();
    assert!(matches!(error, dnd_net::NetError::Http { status: 404, .. }));
}

fn semver(text: &str) -> semver::Version {
    semver::Version::parse(text).unwrap()
}
