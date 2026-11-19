//! Talking to a language server: a child process exchanging JSON-RPC messages over its
//! stdin and stdout, each one a Content-Length header and a JSON body.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::json::Json;

// bigger than any real message, small enough that a garbled header can't exhaust memory
const MAX_MESSAGE: usize = 64 << 20;

fn write_message(to: &mut impl Write, msg: &Json) -> io::Result<()> {
    let body = msg.to_string();
    write!(to, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    to.flush()
}

// the next message's body, or None if the stream ended cleanly between messages
fn read_message(from: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let invalid = |why: &str| io::Error::new(io::ErrorKind::InvalidData, why.to_string());
    let mut length = None;
    let mut line = String::new();
    let mut first = true;
    loop {
        line.clear();
        if from.read_line(&mut line)? == 0 {
            return if first {
                Ok(None)
            } else {
                Err(io::ErrorKind::UnexpectedEof.into())
            };
        }
        first = false;
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':').ok_or_else(|| invalid("malformed header"))?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            let n = value.trim().parse().map_err(|_| invalid("bad Content-Length"))?;
            length = Some(n);
        }
    }
    let length = length.ok_or_else(|| invalid("no Content-Length"))?;
    if length > MAX_MESSAGE {
        return Err(invalid("message too big"));
    }
    let mut body = vec![0; length];
    from.read_exact(&mut body)?;
    Ok(Some(body))
}

pub fn file_uri(path: &Path) -> String {
    uri_of(&path.to_string_lossy(), cfg!(windows))
}

pub fn uri_path(uri: &str) -> Option<PathBuf> {
    path_of(uri, cfg!(windows)).map(PathBuf::from)
}

// file:///home/me/a%20b.rs for /home/me/a b.rs, file:///C:/a.rs for C:\a.rs,
// and file://server/share/a.rs for \\server\share\a.rs
fn uri_of(path: &str, windows: bool) -> String {
    let path = if windows {
        crate::paths::plain(path).as_deref().unwrap_or(path).replace('\\', "/")
    } else {
        path.to_string()
    };
    let mut uri = String::from(if path.starts_with("//") {
        "file:"
    } else if path.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

fn path_of(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let (host, path) = rest.split_at(rest.find('/')?);
    let path = decode(path)?;
    let local = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    if !windows {
        return local.then_some(path);
    }
    if !local {
        return Some(format!("//{host}{path}").replace('/', "\\"));
    }
    // /C:/a.rs is C:\a.rs
    let path = match path.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => &path[1..],
        _ => &path,
    };
    Some(path.replace('/', "\\"))
}

fn decode(s: &str) -> Option<String> {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            out.push((hex(bytes.next()?)? << 4) | hex(bytes.next()?)?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_framed_by_their_length_in_bytes() {
        let mut out = Vec::new();
        write_message(&mut out, &Json::from("é")).unwrap();
        assert_eq!(out, b"Content-Length: 4\r\n\r\n\"\xc3\xa9\"");
        let mut from = io::Cursor::new([out.clone(), out].concat());
        assert_eq!(read_message(&mut from).unwrap().unwrap(), "\"é\"".as_bytes());
        assert_eq!(read_message(&mut from).unwrap().unwrap(), "\"é\"".as_bytes());
        assert_eq!(read_message(&mut from).unwrap(), None, "a clean end");
    }

    #[test]
    fn other_headers_are_ignored_and_the_name_is_any_case() {
        let text = "content-length: 2\r\nContent-Type: application/json; charset=utf-8\r\n\r\n{}";
        let mut from = io::Cursor::new(text);
        assert_eq!(read_message(&mut from).unwrap().unwrap(), b"{}");
    }

    #[test]
    fn broken_frames_are_errors() {
        for text in [
            "Content-Length: 5\r\n\r\n{}",
            "Content-Length: 2\r\n",
            "Content-Type: x\r\n\r\n{}",
            "Content-Length: two\r\n\r\n{}",
            "no colon\r\n\r\n",
            "Content-Length: 99999999999\r\n\r\n",
        ] {
            assert!(read_message(&mut io::Cursor::new(text)).is_err(), "{text:?}");
        }
    }

    #[test]
    fn uris_for_unix_paths() {
        assert_eq!(uri_of("/home/me/a b#1%.rs", false), "file:///home/me/a%20b%231%25.rs");
        assert_eq!(uri_of("/tmp/é", false), "file:///tmp/%C3%A9");
        assert_eq!(
            path_of("file:///home/me/a%20b%231%25.rs", false).unwrap(),
            "/home/me/a b#1%.rs"
        );
        assert_eq!(path_of("file://localhost/etc/hosts", false).unwrap(), "/etc/hosts");
        assert_eq!(
            path_of("file:///tmp/%c3%a9", false).unwrap(),
            "/tmp/é",
            "lower-case hex too"
        );
        assert_eq!(
            path_of("file://elsewhere/etc/hosts", false),
            None,
            "not on this machine"
        );
    }

    #[test]
    fn uris_for_windows_paths() {
        assert_eq!(uri_of(r"C:\Users\me\a b.rs", true), "file:///C:/Users/me/a%20b.rs");
        assert_eq!(uri_of(r"\\?\C:\a.rs", true), "file:///C:/a.rs");
        assert_eq!(uri_of(r"\\server\share\a.rs", true), "file://server/share/a.rs");
        assert_eq!(
            path_of("file:///C:/Users/me/a%20b.rs", true).unwrap(),
            r"C:\Users\me\a b.rs"
        );
        assert_eq!(path_of("file:///c%3A/a.rs", true).unwrap(), r"c:\a.rs");
        assert_eq!(
            path_of("file://server/share/a.rs", true).unwrap(),
            r"\\server\share\a.rs"
        );
    }

    #[test]
    fn uris_that_arent_local_files() {
        for uri in [
            "untitled:1",
            "http://x/a",
            "file:relative",
            "file://host",
            "file:///%zz",
            "file:///%ff",
        ] {
            assert_eq!(path_of(uri, false), None, "{uri}");
        }
    }
}
