//! Talking to a language server: a child process exchanging JSON-RPC messages over its
//! stdin and stdout, each one a Content-Length header and a JSON body.

use std::io::{self, BufRead, Write};

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
}
