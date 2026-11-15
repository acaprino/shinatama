//! Serves the local HTML documents the user opens in a browser tab (ADR-0056).
//! Moli's protocol server refuses `file:` navigation and its Fetch domain does
//! not pause stylesheets or images, so a document and its folder are served
//! here instead: loopback only, GET/HEAD only, read-only, one random 128-bit
//! token per granted folder, the exact `Host` checked (no DNS rebinding), and
//! every path canonicalised inside its folder. It cannot drive the browser.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::pages::{content_type, inside};

const MAX_HEADER: usize = 8 * 1024;
const MAX_FILE: u64 = 32 << 20;
const MAX_ROOTS: usize = 256;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub struct LocalServer {
    addr: SocketAddr,
    roots: Arc<Mutex<HashMap<String, PathBuf>>>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn encode_path(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The file inside `root` that a request path (after the token) names.
pub fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.split(['?', '#']).next()?;
    let decoded = percent_decode(relative)?;
    if decoded.contains('\\') || decoded.contains(':') || decoded.contains('\0') || decoded.split('/').any(|part| part == "..") {
        return None;
    }
    let candidate = root.join(if decoded.is_empty() { "index.html" } else { decoded.as_str() });
    let path = inside(root, &candidate)?;
    path.is_file().then_some(path)
}

impl LocalServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let roots: Arc<Mutex<HashMap<String, PathBuf>>> = Arc::default();
        let served = Arc::clone(&roots);
        std::thread::Builder::new().name("browser-local-documents".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let roots = Arc::clone(&served);
                let _ = std::thread::Builder::new().name("browser-local-request".into()).spawn(move || {
                    let _ = serve(stream, addr, &roots);
                });
            }
        })?;
        Ok(Self { addr, roots })
    }

    /// Grants `file`'s folder for one tab and returns the file's address.
    pub fn grant(&self, root: &Path, file: &Path) -> Option<String> {
        let file = inside(root, file)?;
        let root = std::fs::canonicalize(root).ok()?;
        let relative = file.strip_prefix(&root).ok()?.to_string_lossy().replace('\\', "/");
        let mut token = [0u8; 16];
        getrandom::fill(&mut token).ok()?;
        let token = hex(&token);
        let mut roots = self.roots.lock().unwrap();
        if roots.len() >= MAX_ROOTS {
            return None;
        }
        roots.insert(token.clone(), root);
        Some(format!("http://{}/{token}/{}", self.addr, encode_path(&relative)))
    }

    /// Withdraws the folder behind a granted address.
    pub fn revoke(&self, url: &str) {
        if let Some(token) = url.strip_prefix(&format!("http://{}/", self.addr)).and_then(|rest| rest.split('/').next()) {
            self.roots.lock().unwrap().remove(token);
        }
    }

    pub fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }
}

fn respond(stream: &mut TcpStream, status: &str, headers: &[(&str, &str)], body: &[u8], head: bool) -> std::io::Result<()> {
    let mut out = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n", body.len());
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    stream.write_all(out.as_bytes())?;
    if !head {
        stream.write_all(body)?;
    }
    stream.flush()
}

fn serve(mut stream: TcpStream, addr: SocketAddr, roots: &Mutex<HashMap<String, PathBuf>>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut reader = BufReader::new((&stream).take(MAX_HEADER as u64));
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut host = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("host")
        {
            host = Some(value.trim().to_owned());
        }
    }
    drop(reader);
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let head = method == "HEAD";
    if method != "GET" && !head {
        return respond(&mut stream, "405 Method Not Allowed", &[("Allow", "GET, HEAD")], b"", false);
    }
    if host.as_deref() != Some(addr.to_string().as_str()) {
        return respond(&mut stream, "421 Misdirected Request", &[], b"", head);
    }
    let Some((token, relative)) = target.strip_prefix('/').map(|rest| rest.split_once('/').unwrap_or((rest, ""))) else {
        return respond(&mut stream, "404 Not Found", &[], b"", head);
    };
    let root = roots.lock().unwrap().get(token).cloned();
    let file = root.as_deref().and_then(|root| resolve(root, relative));
    let body = file.as_deref().and_then(|path| (std::fs::metadata(path).ok()?.len() <= MAX_FILE).then(|| std::fs::read(path).ok()).flatten());
    match (file, body) {
        (Some(path), Some(bytes)) => respond(&mut stream, "200 OK", &[("Content-Type", content_type(&path))], &bytes, head),
        _ => respond(&mut stream, "404 Not Found", &[("Content-Type", "text/plain")], b"", head),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(url: &str, host: Option<&str>) -> (u16, Vec<u8>) {
        let rest = url.strip_prefix("http://").unwrap();
        let (authority, path) = rest.split_once('/').unwrap();
        let mut stream = TcpStream::connect(authority).unwrap();
        write!(stream, "GET /{path} HTTP/1.1\r\nHost: {}\r\n\r\n", host.unwrap_or(authority)).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let text = String::from_utf8_lossy(&response).into_owned();
        let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = text.split_once("\r\n\r\n").map(|(_, b)| b.as_bytes().to_vec()).unwrap_or_default();
        (status, body)
    }

    fn fixture() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("shinatama-local-server-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = base.join("site");
        std::fs::create_dir_all(root.join("css")).unwrap();
        std::fs::write(root.join("index.html"), "<p>hi").unwrap();
        std::fs::write(root.join("css").join("a.css"), "p{}").unwrap();
        std::fs::write(base.join("outside.txt"), "secret").unwrap();
        (base, root)
    }

    #[test]
    fn serves_the_granted_folder_and_nothing_else() {
        let (base, root) = fixture();
        let server = LocalServer::start().unwrap();
        let url = server.grant(&root, &root.join("index.html")).unwrap();
        assert_eq!(get(&url, None), (200, b"<p>hi".to_vec()));
        let css = url.replace("index.html", "css/a.css");
        assert_eq!(get(&css, None).0, 200);
        for bad in ["../outside.txt", "%2E%2E/outside.txt", "css/..%2F..%2Foutside.txt", "..%5Coutside.txt", "C:/Windows/win.ini", "missing.html"] {
            assert_eq!(get(&url.replace("index.html", bad), None).0, 404, "{bad}");
        }
        let wrong_token = format!("{}/{}/index.html", server.origin(), "0".repeat(32));
        assert_eq!(get(&wrong_token, None).0, 404);
        assert_eq!(get(&url, Some("evil.example:80")).0, 421, "a rebound name is refused");
        server.revoke(&url);
        assert_eq!(get(&url, None).0, 404);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn only_get_and_head() {
        let (base, root) = fixture();
        let server = LocalServer::start().unwrap();
        let url = server.grant(&root, &root.join("index.html")).unwrap();
        let rest = url.strip_prefix("http://").unwrap();
        let (authority, path) = rest.split_once('/').unwrap();
        let mut stream = TcpStream::connect(authority).unwrap();
        write!(stream, "POST /{path} HTTP/1.1\r\nHost: {authority}\r\nContent-Length: 0\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 405"), "{response}");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_file_outside_its_folder_is_never_granted() {
        let (base, root) = fixture();
        let server = LocalServer::start().unwrap();
        assert!(server.grant(&root, &base.join("outside.txt")).is_none());
        let _ = std::fs::remove_dir_all(base);
    }
}
