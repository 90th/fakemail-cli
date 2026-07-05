use crate::html::{clean_whitespace, html_to_text, url_decode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const BASE_URL: &str = "https://www.fakemail.net";
const HOME_URL: &str = "https://www.fakemail.net/";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Serialize, Deserialize, Debug)]
struct SessionState {
    email: String,
    csrf_token: String,
    cookies: HashMap<String, String>,
    #[serde(default)]
    password: String,
}

pub struct FakeMailClient {
    email: String,
    password: String,
    csrf_token: String,
    cookies: HashMap<String, String>,
    session_file: PathBuf,
    agent: ureq::Agent,
    base_url: String,
    home_url: String,
}

#[derive(Debug)]
pub enum ClientError {
    SessionExpired,
    InvalidEmailId(String),
    Http(Box<ureq::Error>),
    Json(serde_json::Error),
    Io(std::io::Error),
    Other(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionExpired => write!(f, "Session expired"),
            Self::InvalidEmailId(id) => write!(f, "Invalid email ID: {}", id),
            Self::Http(e) => write!(f, "HTTP error: {}", e),
            Self::Json(e) => write!(f, "JSON error: {}", e),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Other(s) => write!(f, "Error: {}", s),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<ureq::Error> for ClientError {
    fn from(e: ureq::Error) -> Self {
        Self::Http(Box::new(e))
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn get_session_file_path() -> PathBuf {
    let state_home = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("state"));
    state_home.join("fakemail-cli").join("session.json")
}

fn get_legacy_session_file_path() -> PathBuf {
    home_dir().join(".fakemail_cli_session.json")
}

fn path_exists(path: &Path) -> Result<bool, ClientError> {
    match fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(ClientError::Other(format!(
            "Could not inspect session file {}: {}",
            path.display(),
            e
        ))),
    }
}

fn get_cookie_string(cookies: &HashMap<String, String>) -> String {
    let mut pairs: Vec<_> = cookies.iter().collect();
    pairs.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut cookie_str = String::new();
    for (i, (k, v)) in pairs.into_iter().enumerate() {
        if i > 0 {
            cookie_str.push_str("; ");
        }
        cookie_str.push_str(k);
        cookie_str.push('=');
        cookie_str.push_str(v);
    }
    cookie_str
}

fn update_cookies(cookies: &mut HashMap<String, String>, resp: &ureq::Response) {
    for header in resp.all("Set-Cookie") {
        if let Some((key, val)) = header
            .split(';')
            .next()
            .and_then(|part| part.split_once('='))
        {
            cookies.insert(key.trim().to_string(), val.trim().to_string());
        }
    }
}

fn is_redirected_to_home(resp: &ureq::Response, base_url: &str) -> bool {
    resp.get_url().trim_end_matches('/') == base_url
}

fn is_html_response(body: &str) -> bool {
    let prefix = body
        .trim_start()
        .chars()
        .take(128)
        .flat_map(char::to_lowercase)
        .collect::<String>();

    prefix.starts_with("<!doctype")
        || prefix.starts_with("<html")
        || prefix.starts_with("<?xml")
        || prefix.starts_with("<!--") && prefix.contains("<html")
}

fn build_agent() -> ureq::Agent {
    ureq::builder()
        .timeout_connect(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .timeout_read(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .timeout_write(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{base_url}{path}")
}

fn parse_email_id(email_id: &str) -> Result<u64, ClientError> {
    let trimmed = email_id.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(ClientError::InvalidEmailId(email_id.to_string()));
    }
    trimmed
        .parse::<u64>()
        .map_err(|_| ClientError::InvalidEmailId(email_id.to_string()))
}

fn extract_csrf_token(body: &str) -> Option<String> {
    let token = body.split_once("const CSRF=\"")?.1.get(..64)?;
    token
        .chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| token.to_string())
}

fn validate_username(prefix: &str) -> Result<String, ClientError> {
    let normalized = prefix.trim().to_lowercase();
    let valid_chars = normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let valid_edges = normalized
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && normalized
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric());

    if normalized.len() < 3 || normalized.len() > 18 || !valid_chars || !valid_edges {
        return Err(ClientError::Other(
            "Username must be 3-18 ASCII letters, digits, dots, or hyphens and must start and end with a letter or digit.".into(),
        ));
    }

    Ok(normalized)
}

impl FakeMailClient {
    pub fn new() -> Result<Self, ClientError> {
        let session_file = get_session_file_path();
        let mut client = Self {
            email: String::new(),
            password: String::new(),
            csrf_token: String::new(),
            cookies: HashMap::new(),
            session_file,
            agent: build_agent(),
            base_url: BASE_URL.to_string(),
            home_url: HOME_URL.to_string(),
        };
        client.load_session()?;
        Ok(client)
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    fn load_session_from(&mut self, path: &Path) -> Result<(), ClientError> {
        let content = fs::read_to_string(path).map_err(|e| {
            ClientError::Other(format!(
                "Could not read session file {}: {}",
                path.display(),
                e
            ))
        })?;
        let state: SessionState = serde_json::from_str(&content).map_err(|e| {
            ClientError::Other(format!(
                "Could not parse session file {}: {}",
                path.display(),
                e
            ))
        })?;

        self.email = state.email;
        self.password = state.password;
        self.csrf_token = state.csrf_token;
        self.cookies = state.cookies;
        Ok(())
    }

    fn load_session(&mut self) -> Result<(), ClientError> {
        if path_exists(&self.session_file)? {
            return self.load_session_from(&self.session_file.clone());
        }

        let legacy_session_file = get_legacy_session_file_path();
        if legacy_session_file != self.session_file && path_exists(&legacy_session_file)? {
            self.load_session_from(&legacy_session_file)?;
            self.save_session()?;
            return Ok(());
        }

        self.init_new_session()
    }

    fn save_session(&self) -> Result<(), ClientError> {
        let state = SessionState {
            email: self.email.clone(),
            password: self.password.clone(),
            csrf_token: self.csrf_token.clone(),
            cookies: self.cookies.clone(),
        };
        let content = serde_json::to_string_pretty(&state)?;

        if let Some(parent) = self.session_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.session_file.with_extension("json.tmp");
        if path_exists(&tmp_path)? {
            fs::remove_file(&tmp_path)?;
        }

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut tmp_file = options.open(&tmp_path)?;
        tmp_file.write_all(content.as_bytes())?;
        tmp_file.write_all(b"\n")?;
        tmp_file.sync_all()?;
        drop(tmp_file);

        fs::rename(&tmp_path, &self.session_file)?;
        Ok(())
    }

    fn request(&self, method: &str, url: &str) -> ureq::Request {
        let mut req = match method {
            "POST" => self.agent.post(url),
            _ => self.agent.get(url),
        };

        req = req.set("User-Agent", USER_AGENT);

        let cookie_str = get_cookie_string(&self.cookies);
        if !cookie_str.is_empty() {
            req = req.set("Cookie", &cookie_str);
        }

        req
    }

    fn ajax_request(&self, method: &str, url: &str) -> ureq::Request {
        self.request(method, url)
            .set("Referer", &self.home_url)
            .set("X-Requested-With", "XMLHttpRequest")
    }

    pub fn init_new_session(&mut self) -> Result<(), ClientError> {
        self.cookies.clear();

        let resp = self.request("GET", &self.home_url).call()?;
        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;

        self.csrf_token = extract_csrf_token(&body)
            .ok_or_else(|| ClientError::Other("Could not find CSRF token on main page".into()))?;

        let index_url = endpoint(
            &self.base_url,
            &format!("/index/index?csrf_token={}", self.csrf_token),
        );
        let resp = self.ajax_request("GET", &index_url).call()?;

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix('\u{feff}').unwrap_or(&body);

        let json_data: serde_json::Value = serde_json::from_str(body_clean)?;
        self.email = json_data
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ClientError::Other("Email not found in index response".into()))?
            .to_string();
        self.password = json_data
            .get("heslo")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        self.save_session()?;
        Ok(())
    }

    pub fn set_custom_username(&mut self, prefix: &str) -> Result<String, ClientError> {
        let cleaned_prefix = validate_username(prefix)?;
        let url = endpoint(&self.base_url, "/index/new-email/");
        let resp = self
            .ajax_request("POST", &url)
            .send_form(&[("emailInput", cleaned_prefix.as_str()), ("format", "json")])?;

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body
            .strip_prefix('\u{feff}')
            .unwrap_or(&body)
            .trim()
            .trim_matches('"');

        if body_clean != "ok" {
            return Err(ClientError::Other(format!(
                "Server rejected custom username: {}",
                body_clean
            )));
        }

        if let Some(val) = self.cookies.get("TMA") {
            self.email = url_decode(val);
        } else {
            self.email = format!("{}@forliion.com", cleaned_prefix);
        }

        self.save_session()?;
        Ok(self.email.clone())
    }

    fn get_inbox_internal(&mut self) -> Result<serde_json::Value, ClientError> {
        let url = endpoint(&self.base_url, "/index/refresh");
        let resp = self.ajax_request("GET", &url).call()?;

        if is_redirected_to_home(&resp, &self.base_url) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix('\u{feff}').unwrap_or(&body);

        if is_html_response(body_clean) {
            return Err(ClientError::SessionExpired);
        }

        Ok(serde_json::from_str(body_clean)?)
    }

    fn read_email_internal(&mut self, email_id: u64) -> Result<String, ClientError> {
        let url = endpoint(&self.base_url, &format!("/email/id/{email_id}"));
        let resp = self.request("GET", &url).call()?;

        if is_redirected_to_home(&resp, &self.base_url) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix('\u{feff}').unwrap_or(&body);

        if body_clean.contains("const CSRF=\"") || body_clean.contains("id=\"emailAddr\"") {
            return Err(ClientError::SessionExpired);
        }

        let text = html_to_text(body_clean);
        Ok(clean_whitespace(&text))
    }

    fn delete_email_internal(&mut self, email_id: u64) -> Result<(), ClientError> {
        let url = endpoint(&self.base_url, &format!("/delete-email/{email_id}"));
        let id = email_id.to_string();
        let resp = self
            .ajax_request("POST", &url)
            .send_form(&[("id", id.as_str())])?;

        if is_redirected_to_home(&resp, &self.base_url) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix('\u{feff}').unwrap_or(&body).trim();

        if is_html_response(body_clean) {
            return Err(ClientError::SessionExpired);
        }

        if body_clean != "ok" {
            return Err(ClientError::Other(format!(
                "Server rejected delete request: {}",
                body_clean
            )));
        }
        Ok(())
    }

    fn extend_lifetime_internal(&mut self, seconds: u64) -> Result<(), ClientError> {
        let url = endpoint(&self.base_url, &format!("/expirace/{seconds}"));
        let resp = self.request("GET", &url).call()?;

        update_cookies(&mut self.cookies, &resp);
        if is_redirected_to_home(&resp, &self.base_url) {
            self.get_lifetime_internal()?;
        }

        self.save_session()?;
        Ok(())
    }

    fn get_lifetime_internal(&mut self) -> Result<(String, String), ClientError> {
        let url = endpoint(&self.base_url, "/index/zivot");
        let resp = self.ajax_request("GET", &url).call()?;

        if is_redirected_to_home(&resp, &self.base_url) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix('\u{feff}').unwrap_or(&body);

        if is_html_response(body_clean) {
            return Err(ClientError::SessionExpired);
        }

        let json_data: serde_json::Value = serde_json::from_str(body_clean)?;
        let ted = json_data
            .get("ted")
            .and_then(|v| v.as_str())
            .ok_or(ClientError::SessionExpired)?
            .to_string();
        let konec = json_data
            .get("konec")
            .and_then(|v| v.as_str())
            .ok_or(ClientError::SessionExpired)?
            .to_string();

        Ok((ted, konec))
    }

    pub fn get_inbox(&mut self) -> Result<serde_json::Value, ClientError> {
        match self.get_inbox_internal() {
            Err(ClientError::SessionExpired) => {
                self.init_new_session()?;
                eprintln!(
                    "\x1B[33mSession expired. Generated new mailbox: {}\x1B[0m",
                    self.email
                );
                self.get_inbox_internal()
            }
            other => other,
        }
    }

    pub fn read_email(&mut self, email_id: &str) -> Result<String, ClientError> {
        let email_id = parse_email_id(email_id)?;
        match self.read_email_internal(email_id) {
            Err(ClientError::SessionExpired) => Err(ClientError::Other(
                "Session expired; message-specific read was not retried against a new mailbox. Run list to refresh the session.".into(),
            )),
            other => other,
        }
    }

    pub fn delete_email(&mut self, email_id: &str) -> Result<(), ClientError> {
        let email_id = parse_email_id(email_id)?;
        match self.delete_email_internal(email_id) {
            Err(ClientError::SessionExpired) => Err(ClientError::Other(
                "Session expired; message-specific delete was not retried against a new mailbox. Run list to refresh the session.".into(),
            )),
            other => other,
        }
    }

    pub fn extend_lifetime(&mut self, seconds: u64) -> Result<(), ClientError> {
        self.extend_lifetime_internal(seconds)
    }

    pub fn get_lifetime(&mut self) -> Result<(String, String), ClientError> {
        match self.get_lifetime_internal() {
            Err(ClientError::SessionExpired) => {
                self.init_new_session()?;
                eprintln!(
                    "\x1B[33mSession expired. Generated new mailbox: {}\x1B[0m",
                    self.email
                );
                self.get_lifetime_internal()
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
    use std::thread;
    use std::time::Duration;

    /// Minimal HTTP test server built on std primitives.
    ///
    /// The handler closure receives the raw request path and returns
    /// `(status, headers, body)`.  Redirect Location headers should use
    /// relative or absolute paths — the test never needs the port.
    struct TestServer {
        port: u16,
        stop: Arc<AtomicBool>,
        _handle: thread::JoinHandle<()>,
    }

    type Response = (u16, Vec<(String, String)>, Vec<u8>);

    impl TestServer {
        fn new<F>(handler: F) -> Self
        where
            F: Fn(&str) -> Response + Send + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind test server");
            let port = listener.local_addr().unwrap().port();
            let stop = Arc::new(AtomicBool::new(false));
            let stop_clone = stop.clone();

            let handle = thread::spawn(move || {
                listener.set_nonblocking(true).ok();
                loop {
                    if stop_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    // Use &mut so match ergonomics gives &mut TcpStream (needed for
                    // read/write).
                    match &mut listener.accept() {
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        Err(_) => break,
                        Ok((stream, _)) => {
                            let mut buf = [0; 8192];
                            let n = match stream.read(&mut buf) {
                                Ok(0) | Err(_) => continue,
                                Ok(n) => n,
                            };
                            let request = String::from_utf8_lossy(&buf[..n]);
                            let path = request
                                .lines()
                                .next()
                                .and_then(|l| l.split_whitespace().nth(1))
                                .unwrap_or("/");

                            let (status, headers, body) = handler(path);
                            let status_text = match status {
                                200 => "OK",
                                302 => "Found",
                                404 => "Not Found",
                                _ => "Unknown",
                            };

                            let mut response = format!(
                                "HTTP/1.1 {status} {status_text}\r\n\
                                 Connection: close\r\n"
                            );
                            for (k, v) in &headers {
                                response.push_str(&format!("{k}: {v}\r\n"));
                            }
                            response.push_str(&format!("Content-Length: {}\r\n", body.len()));
                            response.push_str("\r\n");

                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.write_all(&body);
                            let _ = stream.flush();
                        }
                    }
                }
            });

            TestServer {
                port,
                stop,
                _handle: handle,
            }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            // Connect to unblock the accept loop's sleep/poll.
            let _ = TcpStream::connect(format!("127.0.0.1:{}", self.port));
        }
    }

    static SESSION_COUNTER: AtomicU16 = AtomicU16::new(0);

    fn unique_session_path() -> PathBuf {
        let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("fakemail_test_session_{n}.json"))
    }

    /// Construct a FakeMailClient pointed at a local test server with a
    /// session file in a temp directory and the given email pre-populated.
    fn test_client(base_url: &str, email: &str) -> FakeMailClient {
        let session_file = unique_session_path();
        // Remove any leftover from a previous run.
        let _ = std::fs::remove_file(&session_file);

        FakeMailClient {
            email: email.to_string(),
            password: String::new(),
            csrf_token: "00000000000000000000000000000000\
                         00000000000000000000000000000000"
                .to_string(),
            cookies: HashMap::new(),
            session_file,
            agent: ureq::builder().build(),
            base_url: base_url.to_string(),
            home_url: format!("{base_url}/"),
        }
    }

    // ------------------------------------------------------------------
    // Tests
    // ------------------------------------------------------------------

    /// Success path: /expirace/3600 redirects → /, then /index/zivot
    /// returns valid JSON.  Extend returns Ok and preserves the existing
    /// email/session.
    #[test]
    fn extend_lifetime_success() {
        let server = TestServer::new(|path| {
            if path.starts_with("/expirace/") {
                // Successful extension redirects to home; ureq follows.
                (302, vec![("Location".into(), "/".into())], Vec::new())
            } else if path == "/index/zivot" {
                (
                    200,
                    vec![("Content-Type".into(), "application/json".into())],
                    br#"{"ted":"1 day","konec":"2026-07-06"}"#.to_vec(),
                )
            } else {
                // Root page (redirect target) — just a valid HTML body.
                (200, vec![], b"<html><body>ok</body></html>".to_vec())
            }
        });

        let mut client = test_client(&server.url(), "regression@forliion.com");
        let result = client.extend_lifetime(3600);

        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(client.email(), "regression@forliion.com");
    }

    /// Expired-session path: /expirace/3600 redirects → /, then
    /// /index/zivot also redirects → home (signalling session expiry).
    /// Extend returns SessionExpired and preserves the existing email
    /// without calling init_new_session.
    #[test]
    fn extend_lifetime_expired_session() {
        let server = TestServer::new(|path| {
            if path.starts_with("/expirace/") {
                (302, vec![("Location".into(), "/".into())], Vec::new())
            } else if path == "/index/zivot" {
                // Session-check endpoint redirects → expired.
                (302, vec![("Location".into(), "/".into())], Vec::new())
            } else {
                (200, vec![], b"<html><body>ok</body></html>".to_vec())
            }
        });

        let mut client = test_client(&server.url(), "expired@forliion.com");
        let result = client.extend_lifetime(3600);

        assert!(matches!(result, Err(ClientError::SessionExpired)));
        assert_eq!(client.email(), "expired@forliion.com");
    }
}
