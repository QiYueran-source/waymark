//! `waymark serve` in a Web build: readiness, real HTTP, and failures.
//!
//! These tests drive the real binary over a real loopback socket. The server is
//! started with `--port 0` so the operating system chooses the port, which is
//! then read back from the readiness line the command promises to print before
//! it blocks.

#![cfg(feature = "web")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
// Only the Unix opener test measures a deadline.
#[cfg(unix)]
use std::time::Instant;

use camino::Utf8PathBuf;

const READY: Duration = Duration::from_secs(30);

fn waymark() -> Command {
    Command::new(env!("CARGO_BIN_EXE_waymark"))
}

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    (dir, path)
}

fn init(root: &Utf8PathBuf) {
    let status = waymark()
        .arg("init")
        .arg("--project")
        .arg(root)
        .status()
        .expect("the binary runs");
    assert!(status.success(), "the fixture project initializes");
}

/// A `waymark serve` child that is killed when the test ends.
struct Server {
    child: Child,
}

impl Server {
    fn spawn(root: &Utf8PathBuf, args: &[&str], path_env: Option<&Path>) -> Server {
        let mut command = waymark();
        command
            .arg("serve")
            .arg("--project")
            .arg(root)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = path_env {
            command.env("PATH", path);
        }
        Server {
            child: command.spawn().expect("the server starts"),
        }
    }

    fn stderr(&mut self) -> Lines {
        Lines::new(self.child.stderr.take().expect("stderr is piped"))
    }

    fn stdout(&mut self) -> Lines {
        Lines::new(self.child.stdout.take().expect("stdout is piped"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Lines read from a child stream on a thread, so a missing line fails the test
/// instead of hanging it.
struct Lines {
    receiver: mpsc::Receiver<String>,
}

impl Lines {
    fn new<R: Read + Send + 'static>(stream: R) -> Lines {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Lines { receiver }
    }

    fn next(&self) -> String {
        self.receiver
            .recv_timeout(READY)
            .expect("the server reports before the timeout")
    }
}

/// The port from a `serving http://127.0.0.1:<port>` readiness line.
fn readiness_port(line: &str) -> u16 {
    let url = line
        .strip_prefix("serving ")
        .unwrap_or_else(|| panic!("readiness must start with `serving `: {line:?}"));
    let port = url
        .strip_prefix("http://127.0.0.1:")
        .unwrap_or_else(|| panic!("readiness must name the loopback URL: {line:?}"));
    port.parse()
        .unwrap_or_else(|_| panic!("readiness must end with a port: {line:?}"))
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("the body is JSON ({error}): {}", self.text()))
    }
}

/// One raw HTTP/1.1 request to the loopback server.
fn request(port: u16, method: &str, path: &str) -> Response {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("the server accepts connections");
    stream
        .set_read_timeout(Some(READY))
        .expect("the test can bound the read");
    let host = format!("127.0.0.1:{port}");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("the response is read");
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("the response has a header terminator: {:?}", raw));
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let body = raw[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("the response has a status line");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("the status line names a code: {status_line:?}"));
    let headers = lines
        .filter_map(|line| line.split_once(": "))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned()))
        .collect();
    Response {
        status,
        headers,
        body,
    }
}

#[test]
fn human_mode_prints_readiness_then_serves_the_embedded_tree() {
    let (_dir, root) = tempdir();
    init(&root);
    let mut server = Server::spawn(&root, &["--port", "0"], None);

    let port = readiness_port(&server.stderr().next());

    let index = request(port, "GET", "/");
    assert_eq!(index.status, 200, "GET / must serve the embedded shell");
    assert_eq!(
        index.header("content-encoding"),
        Some("gzip"),
        "the embedded tree is served gzip-encoded"
    );
    assert!(
        index
            .header("content-type")
            .is_some_and(|value| value.starts_with("text/html")),
        "GET / must serve HTML: {:?}",
        index.header("content-type")
    );

    let health = request(port, "GET", "/api/health");
    assert_eq!(health.status, 200);
    assert_eq!(health.json()["data"]["service"], "waymark-web");

    let refused = request(port, "POST", "/api/health");
    assert_eq!(
        refused.status,
        405,
        "the served surface stays read-only: {}",
        refused.text()
    );
}

#[test]
fn json_mode_emits_the_envelope_before_it_serves() {
    let (_dir, root) = tempdir();
    init(&root);
    let mut server = Server::spawn(&root, &["--json", "--port", "0"], None);

    let line = server.stdout().next();
    let envelope: serde_json::Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("stdout is JSON ({error}): {line}"));
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["ok"], true);
    assert!(envelope.get("_meta").is_some(), "the envelope keeps _meta");
    assert!(
        envelope["data"].get("port").is_none(),
        "the data carries only the URL: {}",
        envelope["data"]
    );
    let url = envelope["data"]["url"]
        .as_str()
        .expect("the data names the serving URL");
    let port = url
        .strip_prefix("http://127.0.0.1:")
        .unwrap_or_else(|| panic!("the URL names the loopback address: {url}"));
    let port: u16 = port.parse().expect("the URL ends with a port");
    assert_ne!(port, 0, "--port 0 asks the OS for a real port");

    // The envelope is emitted before the command blocks: the server answers
    // only after it was read.
    let health = request(port, "GET", "/api/health");
    assert_eq!(health.status, 200);
    assert_eq!(health.json()["data"]["status"], "ok");
}

#[test]
fn a_busy_port_fails_with_port_in_use_and_never_falls_back() {
    let (_dir, root) = tempdir();
    init(&root);
    // Hold a real loopback port for the duration of the command.
    let occupied = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("a loopback port is available");
    let port = occupied
        .local_addr()
        .expect("the listener has an address")
        .port();

    let result = waymark()
        .arg("--json")
        .arg("serve")
        .arg("--project")
        .arg(&root)
        .arg("--port")
        .arg(port.to_string())
        .output()
        .expect("the binary runs");
    let code = result
        .status
        .code()
        .expect("the command exits instead of falling back");

    let stdout = String::from_utf8_lossy(&result.stdout);
    let envelope: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is one envelope ({error}): {stdout}"));
    assert_eq!(code, 5, "a busy port is an environment failure");
    assert_eq!(envelope["error"]["code"], "PORT_IN_USE");
    assert_eq!(envelope["error"]["details"]["port"], port);
    let message = envelope["error"]["message"]
        .as_str()
        .expect("the failure carries a message");
    assert!(
        message.contains(&port.to_string()),
        "the port is named: {message}"
    );
    assert!(
        message.contains("--port 0"),
        "the message suggests the free-port flag: {message}"
    );
    drop(occupied);
}

#[test]
fn a_browser_launch_failure_is_a_warning_not_a_serve_failure() {
    let (_dir, root) = tempdir();
    init(&root);
    // With no executable on PATH, the opener cannot even be launched.
    let (_empty, empty) = tempdir();
    let mut server = Server::spawn(&root, &["--port", "0", "--open"], Some(empty.as_std_path()));

    let stderr = server.stderr();
    let port = readiness_port(&stderr.next());
    let warning = stderr.next();
    let expected_opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    assert!(
        warning.starts_with("warning:") && warning.contains(expected_opener),
        "the launch failure is reported as a warning naming the opener: {warning}"
    );
    assert_eq!(
        request(port, "GET", "/api/health").status,
        200,
        "the server keeps serving after a failed browser launch"
    );
}

#[cfg(unix)]
#[test]
fn the_opener_runs_only_with_open() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, root) = tempdir();
    init(&root);
    let (_bin, bin) = tempdir();
    let marker = bin.join("opened.txt");
    let opener = bin.join(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    std::fs::write(
        &opener,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", marker.as_str()),
    )
    .expect("the fake opener is writable");
    std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755))
        .expect("the fake opener is executable");

    let mut server = Server::spawn(
        &root,
        &["--json", "--port", "0", "--open"],
        Some(bin.as_std_path()),
    );
    let envelope: serde_json::Value =
        serde_json::from_str(&server.stdout().next()).expect("the readiness envelope is JSON");
    let url = envelope["data"]["url"]
        .as_str()
        .expect("the envelope names the URL")
        .to_owned();

    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&marker).expect("--open launched the opener"),
        url,
        "the opener receives the serving URL"
    );
    drop(server);

    // The same setup without `--open` must not launch anything.
    let marker = bin.join("not-opened.txt");
    std::fs::write(
        &opener,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", marker.as_str()),
    )
    .expect("the fake opener is writable");
    let mut server = Server::spawn(&root, &["--json", "--port", "0"], Some(bin.as_std_path()));
    let _ = server.stdout().next();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "no browser is opened without --open");
}
