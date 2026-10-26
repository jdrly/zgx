//! What the system asks the app to open, as Terminal opens it: a folder
//! (dropped on the Dock icon, Open With) gets a tab there; a script (a
//! `.command`, a shell script) a tab in its folder running it; an `ssh://`
//! link a tab connecting; an `x-man-page://` link a tab reading the page.
//!
//! The interface takes them (`opened`, app.luau) as tabs to make: where each
//! starts and what is typed into its shell, quoted for it.
use std::{cell::RefCell, os::unix::fs::PermissionsExt, path::Path, rc::Rc};
use zgx_script::{Shared, Value};

/// Tabs asked for, until the interface takes them.
#[derive(Clone, Default)]
pub struct Opened(Rc<RefCell<(Vec<serde_json::Value>, Option<Shared>)>>);

impl Opened {
    /// The interface's view of them: what came before it started too.
    pub fn publish(&self, shared: Shared) {
        self.0.borrow_mut().1 = Some(shared);
        self.show();
    }
    /// Tabs for `urls`.
    pub fn push(&self, urls: &[String]) {
        self.0
            .borrow_mut()
            .0
            .extend(urls.iter().filter_map(|url| tab(url)));
        self.show();
    }
    /// The interface made them.
    pub fn take(&self) {
        self.0.borrow_mut().0.clear();
        self.show();
    }
    /// Shows the interface what is asked for, borrowing nothing meanwhile:
    /// the interface runs as it is set, and takes them.
    fn show(&self) {
        let (tabs, shared) = {
            let inner = self.0.borrow();
            (list(&inner.0), inner.1.clone())
        };
        if let Some(shared) = shared {
            shared.set(tabs);
        }
    }
}

fn list(tabs: &[serde_json::Value]) -> Value {
    Value::from_json(&serde_json::Value::from(tabs.to_vec()).to_string()).unwrap_or(Value::Nil)
}

/// The tab for `url`: `{ dir, input? }`.
fn tab(url: &str) -> Option<serde_json::Value> {
    let (scheme, rest) = url.split_once("://")?;
    match scheme.to_ascii_lowercase().as_str() {
        "file" => {
            // `file:///path` or `file://localhost/path`.
            let path = decode(rest.strip_prefix("localhost").unwrap_or(rest))?;
            let path = Path::new(&path);
            if path.is_dir() {
                return Some(serde_json::json!({ "dir": path }));
            }
            let meta = path.metadata().ok()?;
            let dir = path.parent()?;
            // An executable runs itself (its #! picks the interpreter); a
            // script without the bit, as Terminal would refuse, runs in sh.
            let run = if meta.permissions().mode() & 0o111 != 0 {
                quote(&path.to_string_lossy())
            } else {
                format!("sh {}", quote(&path.to_string_lossy()))
            };
            Some(serde_json::json!({ "dir": dir, "input": format!("{run}\r") }))
        }
        "ssh" => {
            // `ssh://[user@]host[:port][/]`
            let authority = decode(rest.split('/').next()?)?;
            let (login, host) = match authority.rsplit_once('@') {
                Some((user, host)) => (Some(user), host),
                None => (None, authority.as_str()),
            };
            // `[v6]:port` or `host:port`.
            let (host, port) = match host.strip_prefix('[') {
                Some(v6) => {
                    let (host, after) = v6.split_once(']')?;
                    (host, after.strip_prefix(':'))
                }
                None => match host.split_once(':') {
                    Some((host, port)) => (host, Some(port)),
                    None => (host, None),
                },
            };
            if host.is_empty() || host.starts_with('-') {
                return None;
            }
            let mut command = String::from("ssh");
            if let Some(port) = port.filter(|port| !port.is_empty()) {
                if !port.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                command.push_str(&format!(" -p {port}"));
            }
            if let Some(user) = login {
                if user.is_empty() || user.starts_with('-') {
                    return None;
                }
                command.push_str(&format!(" -l {}", quote(user)));
            }
            command.push_str(&format!(" {}\r", quote(host)));
            Some(serde_json::json!({ "dir": "", "input": command }))
        }
        "x-man-page" => {
            // `x-man-page://[section/]page`
            let path = decode(rest.trim_matches('/'))?;
            let (section, page) = match path.split_once('/') {
                Some((section, page)) => (Some(section), page),
                None => (None, path.as_str()),
            };
            if page.is_empty() || page.starts_with('-') {
                return None;
            }
            let section = section
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric()))
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            Some(
                serde_json::json!({ "dir": "", "input": format!("man{section} {}\r", quote(page)) }),
            )
        }
        _ => None,
    }
}

/// A word for the shell: single-quoted, so nothing in it is special.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `%XX` escapes decoded; none for a control character (it would be typed).
fn decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(out).ok()?;
    (!text.chars().any(char::is_control)).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let input = |url: &str| tab(url).and_then(|t| t["input"].as_str().map(str::to_owned));
        assert_eq!(
            input("ssh://me@example.com:2222/").as_deref(),
            Some("ssh -p 2222 -l 'me' 'example.com'\r")
        );
        assert_eq!(
            input("ssh://[::1]:22").as_deref(),
            Some("ssh -p 22 '::1'\r")
        );
        assert_eq!(input("ssh://-oProxyCommand=x"), None);
        assert_eq!(input("ssh://h:1;rm"), None);
        assert_eq!(input("x-man-page://1/ls").as_deref(), Some("man 1 'ls'\r"));
        assert_eq!(
            input("x-man-page://a%27b").as_deref(),
            Some("man 'a'\\''b'\r")
        );
        assert_eq!(input("file:///tmp/a%0Ab"), None);
        assert_eq!(tab("https://example.com"), None);
    }
}
