use crate::html::{clean_whitespace, html_to_text, url_decode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
struct SessionState {
    email: String,
    csrf_token: String,
    cookies: HashMap<String, String>,
    #[serde(default)]
    password: String,
}

pub struct FakeMailClient {
    pub email: String,
    pub password: String,
    pub csrf_token: String,
    pub cookies: HashMap<String, String>,
    session_file: PathBuf,
}

#[derive(Debug)]
pub enum ClientError {
    SessionExpired,
    Http(Box<ureq::Error>),
    Json(serde_json::Error),
    Io(std::io::Error),
    Other(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionExpired => write!(f, "Session expired"),
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

impl From<Box<dyn std::error::Error>> for ClientError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        Self::Other(e.to_string())
    }
}

fn get_session_file_path() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".fakemail_cli_session.json")
}

fn get_cookie_string(cookies: &HashMap<String, String>) -> String {
    let mut cookie_str = String::new();
    for (i, (k, v)) in cookies.iter().enumerate() {
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

fn is_redirected_to_home(resp: &ureq::Response) -> bool {
    let url = resp.get_url();
    url == "https://www.fakemail.net" || url == "https://www.fakemail.net/"
}

fn is_html_response(body: &str) -> bool {
    let trimmed = body.trim();
    trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<!doctype")
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
        };
        client.load_session()?;
        Ok(client)
    }

    fn load_session(&mut self) -> Result<(), ClientError> {
        let read_and_parse = || -> Result<SessionState, Box<dyn std::error::Error>> {
            let content = fs::read_to_string(&self.session_file)?;
            let state = serde_json::from_str(&content)?;
            Ok(state)
        };

        if let Ok(state) = read_and_parse() {
            self.email = state.email;
            self.password = state.password;
            self.csrf_token = state.csrf_token;
            self.cookies = state.cookies;
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
        fs::write(&self.session_file, content)?;
        Ok(())
    }

    fn request(&self, method: &str, url: &str) -> ureq::Request {
        let mut req = match method {
            "POST" => ureq::post(url),
            _ => ureq::get(url),
        };

        req = req.set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

        let cookie_str = get_cookie_string(&self.cookies);
        if !cookie_str.is_empty() {
            req = req.set("Cookie", &cookie_str);
        }

        req
    }

    pub fn init_new_session(&mut self) -> Result<(), ClientError> {
        self.cookies.clear();

        // 1. Fetch main page to get PHPSESSID and CSRF token
        let resp = self.request("GET", "https://www.fakemail.net/").call()?;
        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;

        // Find CSRF token (64-char hex string)
        if let Some(idx) = body.find("const CSRF=\"") {
            let token_start = idx + "const CSRF=\"".len();
            if token_start + 64 <= body.len() {
                self.csrf_token = body[token_start..token_start + 64].to_string();
            } else {
                return Err(ClientError::Other(
                    "CSRF token position out of bounds".into(),
                ));
            }
        } else {
            return Err(ClientError::Other(
                "Could not find CSRF token on main page".into(),
            ));
        }

        // 2. Get initial random email
        let index_url = format!(
            "https://www.fakemail.net/index/index?csrf_token={}",
            self.csrf_token
        );
        let resp = self
            .request("GET", &index_url)
            .set("Referer", "https://www.fakemail.net/")
            .set("X-Requested-With", "XMLHttpRequest")
            .call()?;

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix("\u{feff}").unwrap_or(&body);

        let json_data: serde_json::Value = serde_json::from_str(body_clean)?;
        if let Some(email) = json_data
            .get("email")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            self.email = email.to_string();
        } else {
            return Err(ClientError::Other(
                "Email not found in index response".into(),
            ));
        }
        if let Some(heslo) = json_data
            .get("heslo")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            self.password = heslo.to_string();
        } else {
            self.password = String::new();
        }

        self.save_session()?;
        Ok(())
    }

    pub fn set_custom_username(&mut self, prefix: &str) -> Result<String, ClientError> {
        let cleaned_prefix: String = prefix
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect::<String>()
            .to_lowercase();

        if cleaned_prefix.len() < 3 || cleaned_prefix.len() > 18 {
            return Err(ClientError::Other(
                "Username must be between 3 and 18 characters.".into(),
            ));
        }

        let resp = self
            .request("POST", "https://www.fakemail.net/index/new-email/")
            .set("Referer", "https://www.fakemail.net/")
            .set("X-Requested-With", "XMLHttpRequest")
            .send_form(&[("emailInput", cleaned_prefix.as_str()), ("format", "json")])?;

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body
            .strip_prefix("\u{feff}")
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
        let resp = self
            .request("GET", "https://www.fakemail.net/index/refresh")
            .set("Referer", "https://www.fakemail.net/")
            .set("X-Requested-With", "XMLHttpRequest")
            .call()?;

        if is_redirected_to_home(&resp) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix("\u{feff}").unwrap_or(&body);

        if is_html_response(body_clean) {
            return Err(ClientError::SessionExpired);
        }

        let json_val = serde_json::from_str(body_clean)?;
        Ok(json_val)
    }

    fn read_email_internal(&mut self, email_id: &str) -> Result<String, ClientError> {
        let url = format!("https://www.fakemail.net/email/id/{}", email_id);
        let resp = self.request("GET", &url).call()?;

        if is_redirected_to_home(&resp) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix("\u{feff}").unwrap_or(&body);

        if body_clean.contains("const CSRF=\"") || body_clean.contains("id=\"emailAddr\"") {
            return Err(ClientError::SessionExpired);
        }

        let text = html_to_text(body_clean);
        Ok(clean_whitespace(&text))
    }

    fn delete_email_internal(&mut self, email_id: &str) -> Result<(), ClientError> {
        let url = format!("https://www.fakemail.net/delete-email/{}", email_id);
        let resp = self
            .request("POST", &url)
            .set("Referer", "https://www.fakemail.net/")
            .set("X-Requested-With", "XMLHttpRequest")
            .send_form(&[("id", email_id)])?;

        if is_redirected_to_home(&resp) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix("\u{feff}").unwrap_or(&body).trim();

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
        let url = format!("https://www.fakemail.net/expirace/{}", seconds);
        let resp = self.request("GET", &url).call()?;

        if is_redirected_to_home(&resp) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        self.save_session()?;
        Ok(())
    }

    fn get_lifetime_internal(&mut self) -> Result<(String, String), ClientError> {
        let resp = self
            .request("GET", "https://www.fakemail.net/index/zivot")
            .set("Referer", "https://www.fakemail.net/")
            .set("X-Requested-With", "XMLHttpRequest")
            .call()?;

        if is_redirected_to_home(&resp) {
            return Err(ClientError::SessionExpired);
        }

        update_cookies(&mut self.cookies, &resp);
        let body = resp.into_string()?;
        let body_clean = body.strip_prefix("\u{feff}").unwrap_or(&body);

        if is_html_response(body_clean) {
            return Err(ClientError::SessionExpired);
        }

        let json_data: serde_json::Value = serde_json::from_str(body_clean)?;
        let ted = json_data
            .get("ted")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("")
            .to_string();
        let konec = json_data
            .get("konec")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("")
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
        match self.read_email_internal(email_id) {
            Err(ClientError::SessionExpired) => {
                self.init_new_session()?;
                eprintln!(
                    "\x1B[33mSession expired. Generated new mailbox: {}\x1B[0m",
                    self.email
                );
                self.read_email_internal(email_id)
            }
            other => other,
        }
    }

    pub fn delete_email(&mut self, email_id: &str) -> Result<(), ClientError> {
        match self.delete_email_internal(email_id) {
            Err(ClientError::SessionExpired) => {
                self.init_new_session()?;
                eprintln!(
                    "\x1B[33mSession expired. Generated new mailbox: {}\x1B[0m",
                    self.email
                );
                self.delete_email_internal(email_id)
            }
            other => other,
        }
    }

    pub fn extend_lifetime(&mut self, seconds: u64) -> Result<(), ClientError> {
        match self.extend_lifetime_internal(seconds) {
            Err(ClientError::SessionExpired) => {
                self.init_new_session()?;
                eprintln!(
                    "\x1B[33mSession expired. Generated new mailbox: {}\x1B[0m",
                    self.email
                );
                self.extend_lifetime_internal(seconds)
            }
            other => other,
        }
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
