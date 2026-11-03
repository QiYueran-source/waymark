//! Shared fixtures for the Web integration tests: a temporary project, a built
//! asset directory, a live loopback listener, and a raw HTTP/1.1 client.
//!
//! The client is deliberately hand-written over `std::net::TcpStream`: these
//! tests must be able to send a missing or hostile `Host`/`Origin` header, which
//! a standards-respecting HTTP client would not let them do. No TLS, no
//! redirects, no connection reuse — one request, one connection.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use camino::Utf8PathBuf;
use tempfile::TempDir;

use waymark_core::config::{Category, Config};
use waymark_core::model::WorkItem;
use waymark_core::store::{InitOptions, Project};
use waymark_core::testing;
use waymark_core::WorkItemId;

/// A temporary project with the `bug` and `task` categories.
pub struct Fixture {
    pub dir: TempDir,
    pub project: Project,
}

impl Fixture {
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// A project with two categories: `bug` (Bugs, shape 0) and `task` (Tasks,
/// shape 1) in configured key order.
pub fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(dir.path());
    let mut config = Config::fresh();
    config.categories.clear();
    for (key, name, path) in [
        ("bug", "Bugs", "work/bugs"),
        ("task", "Tasks", "work/tasks"),
    ] {
        config.categories.insert(
            waymark_core::CategoryKey::parse(key).expect("a valid category key"),
            Category {
                path: path.to_owned(),
                name: name.to_owned(),
            },
        );
    }
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .expect("the fixture project initializes");
    Fixture { dir, project }
}

/// A work item with the fixture's deterministic instant.
pub fn item(id: &str, category: &str, title: &str) -> WorkItem {
    testing::work_item(id, category, title).build()
}

/// A parsed stable ID.
pub fn id(value: &str) -> WorkItemId {
    WorkItemId::parse(value).expect("a valid stable ID")
}

/// Write one document exactly where the store would write it.
pub fn seed(project: &Project, item: &WorkItem) {
    let path = project
        .path_for(&item.id, &item.category)
        .expect("the fixture category is configured");
    std::fs::create_dir_all(path.parent().expect("a category directory"))
        .expect("the category directory exists");
    std::fs::write(path, waymark_core::serialize::to_document(item))
        .expect("the fixture document is writable");
}

/// A built frontend fixture: `index.html` plus one hashed-looking asset.
pub fn assets() -> TempDir {
    let dir = tempfile::tempdir().expect("a temporary assets directory");
    std::fs::write(
        dir.path().join("index.html"),
        "<!doctype html><html><head><title>Waymark</title></head><body><div id=\"app\"></div></body></html>",
    )
    .expect("the fixture index is writable");
    std::fs::create_dir_all(dir.path().join("_app")).expect("the asset directory exists");
    std::fs::write(dir.path().join("_app/app.js"), "console.log('waymark');")
        .expect("the fixture asset is writable");
    dir
}

/// Bind a server on port 0 against `fixture`.
pub async fn start(fixture: &Fixture, assets: &Path) -> waymark_web::Running {
    waymark_web::bind(
        fixture.project.clone(),
        waymark_web::WebConfig {
            port: 0,
            assets: assets.to_path_buf(),
        },
    )
    .await
    .expect("the loopback listener binds")
}

/// A parsed HTTP response.
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "response is not JSON ({error}):\nstatus: {}\nbody: {}",
                self.status,
                self.text()
            )
        })
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// `GET path` with a valid `Host` header for `addr`.
pub fn get(addr: SocketAddr, path: &str) -> Response {
    request(addr, "GET", path, &[])
}

/// Any method/path, with a default valid `Host` unless the extras override it.
pub fn request(addr: SocketAddr, method: &str, path: &str, extras: &[(&str, &str)]) -> Response {
    let host = format!("localhost:{}", addr.port());
    let overrides_host = extras
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"));
    let mut head = format!("{method} {path} HTTP/1.1\r\n");
    if !overrides_host {
        head.push_str(&format!("Host: {host}\r\n"));
    }
    for (name, value) in extras {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    raw(addr, &head)
}

/// Send bytes verbatim; the caller owns framing and headers.
pub fn raw(addr: SocketAddr, request: &str) -> Response {
    let mut stream = TcpStream::connect(addr).expect("the test server accepts connections");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout is set");
    stream
        .write_all(request.as_bytes())
        .expect("the request is written");
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .expect("the response is read to the end");
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> Response {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or_else(|| {
            panic!(
                "no header terminator in response: {:?}",
                String::from_utf8_lossy(bytes)
            )
        });
    let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
    let body = bytes[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("a status line");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .expect("a status code")
        .parse()
        .expect("a numeric status code");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Response {
        status,
        headers,
        body,
    }
}

/// Every file under `root`, keyed by its `/`-separated relative path, with the
/// exact bytes. The read-only proof compares two of these maps.
pub fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .expect("a readable fixture directory")
        .map(|entry| entry.expect("a readable directory entry"))
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("inside the fixture root")
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(
                relative,
                std::fs::read(&path).expect("a readable fixture file"),
            );
        }
    }
}

/// Read the launcher's first lines, with a kill-on-drop guard.
pub struct Child {
    pub child: std::process::Child,
    pub stdout: BufReader<std::process::ChildStdout>,
}

impl Child {
    pub fn next_line(&mut self) -> String {
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .expect("the launcher stdout is readable");
        assert!(read > 0, "the launcher exited before printing its address");
        line.trim_end().to_owned()
    }

    pub fn stderr(&mut self) -> String {
        let mut text = String::new();
        if let Some(stderr) = self.child.stderr.as_mut() {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The fixture project root as a UTF-8 path.
fn utf8(path: &Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(path.to_path_buf()).expect("the temporary path is UTF-8")
}
