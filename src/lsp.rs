//! Talking to a language server: a child process exchanging JSON-RPC messages over its
//! stdin and stdout, each one a Content-Length header and a JSON body.

use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tinsel::app::Waker;

use crate::json::Json;
use crate::sync::lock;

// bigger than any real message, small enough that a garbled header can't exhaust memory
const MAX_MESSAGE: usize = 64 << 20;
const METHOD_NOT_FOUND: i64 = -32601;

// how columns are counted in positions sent either way
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

impl Encoding {
    // how far into the line its first `chars` characters reach
    pub fn units(self, line: &str, chars: usize) -> usize {
        line.chars().take(chars).map(|c| self.width(c)).sum()
    }

    // which character `units` into the line falls in, or the end if that's past it
    pub fn chars(self, line: &str, units: usize) -> usize {
        let mut at = 0;
        for (i, c) in line.chars().enumerate() {
            at += self.width(c);
            if at > units {
                return i;
            }
        }
        line.chars().count()
    }

    fn width(self, c: char) -> usize {
        match self {
            Encoding::Utf8 => c.len_utf8(),
            Encoding::Utf16 => c.len_utf16(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, PartialEq)]
pub enum Incoming {
    Reply {
        id: i64,
        result: Result<Json, ResponseError>,
    },
    Notice {
        method: String,
        params: Json,
    },
}

enum State {
    // nothing but replies may go out until the server answers initialize
    Starting { init: i64, queued: Vec<Json> },
    Ready,
    Failed(String),
}

pub struct Server {
    child: Option<Child>,
    outgoing: Sender<Json>,
    incoming: Receiver<Json>,
    next_id: i64,
    state: State,
    encoding: Encoding,
    capabilities: Json,
    // the last line the server wrote to stderr, to say why it stopped
    last_words: Arc<Mutex<String>>,
}

impl Server {
    pub fn spawn(program: &str, args: &[String], root: &Path) -> io::Result<Server> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let input = child.stdin.take().expect("stdin is piped");
        let output = child.stdout.take().expect("stdout is piped");
        let errors = child.stderr.take().expect("stderr is piped");
        let mut server = Server::connect(output, input, errors, root);
        server.child = Some(child);
        Ok(server)
    }

    fn connect(
        from: impl Read + Send + 'static,
        to: impl Write + Send + 'static,
        errors: impl Read + Send + 'static,
        root: &Path,
    ) -> Server {
        let (outgoing, to_server) = mpsc::channel();
        let (from_server, incoming) = mpsc::channel();
        let (errors_open, errors_closed) = mpsc::channel::<()>();
        let last_words = Arc::new(Mutex::new(String::new()));
        let words = last_words.clone();
        // writing gets its own thread too, so a server slow to read never stalls the editor
        std::thread::spawn(move || write_all(to, to_server));
        std::thread::spawn(move || read_all(from, from_server, errors_closed, Waker::new()));
        std::thread::spawn(move || {
            keep_last_line(errors, &words);
            drop(errors_open);
        });
        let server = Server {
            child: None,
            outgoing,
            incoming,
            next_id: 2,
            state: State::Starting {
                init: 1,
                queued: Vec::new(),
            },
            encoding: Encoding::Utf16,
            capabilities: Json::Null,
            last_words,
        };
        server.write(message(Some(1.into()), "initialize", initialize_params(root)));
        server
    }

    pub fn ready(&self) -> bool {
        matches!(self.state, State::Ready)
    }

    pub fn failure(&self) -> Option<&str> {
        match &self.state {
            State::Failed(why) => Some(why),
            _ => None,
        }
    }

    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    pub fn capabilities(&self) -> &Json {
        &self.capabilities
    }

    // the id comes back on the reply, to tell which request it answers
    pub fn request(&mut self, method: &str, params: Json) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(message(Some(id.into()), method, params));
        id
    }

    pub fn notify(&mut self, method: &str, params: Json) {
        self.send(message(None, method, params));
    }

    // what has arrived since last time, one at a time; None once there's nothing waiting
    pub fn poll(&mut self) -> Option<Incoming> {
        loop {
            match self.incoming.try_recv() {
                Ok(msg) => {
                    if let Some(incoming) = self.receive(msg) {
                        return Some(incoming);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    if self.failure().is_none() {
                        self.state = State::Failed(self.why_stopped());
                    }
                    return None;
                }
            }
        }
    }

    fn why_stopped(&self) -> String {
        let words = lock(&self.last_words);
        if words.is_empty() {
            "the server stopped".to_string()
        } else {
            format!("the server stopped: {words}")
        }
    }

    fn send(&mut self, msg: Json) {
        match &mut self.state {
            State::Starting { queued, .. } => queued.push(msg),
            State::Ready => self.write(msg),
            State::Failed(_) => {}
        }
    }

    fn write(&self, msg: Json) {
        // a closed channel means the server has gone, which poll reports
        let _ = self.outgoing.send(msg);
    }

    fn receive(&mut self, msg: Json) -> Option<Incoming> {
        match (msg["method"].as_str(), msg.get("id")) {
            // edits it wants made, which are, so it's told they were and the editor told what
            (Some(method @ "workspace/applyEdit"), Some(id)) => {
                self.write(reply(id.clone(), Ok(Json::object([("applied", true.into())]))));
                Some(Incoming::Notice {
                    method: method.to_string(),
                    params: msg["params"].clone(),
                })
            }
            (Some(method), Some(id)) => {
                let answer = answer(method, &msg["params"]);
                self.write(reply(id.clone(), answer));
                None
            }
            (Some(method), None) => Some(Incoming::Notice {
                method: method.to_string(),
                params: msg["params"].clone(),
            }),
            (None, Some(id)) => {
                let id = id.as_i64()?;
                let result = match msg.get("error") {
                    Some(e) => Err(ResponseError {
                        code: e["code"].as_i64().unwrap_or(0),
                        message: e["message"].as_str().unwrap_or("").to_string(),
                    }),
                    None => Ok(msg["result"].clone()),
                };
                if matches!(self.state, State::Starting { init, .. } if init == id) {
                    self.started(result);
                    return None;
                }
                Some(Incoming::Reply { id, result })
            }
            (None, None) => None,
        }
    }

    fn started(&mut self, result: Result<Json, ResponseError>) {
        let State::Starting { queued, .. } = std::mem::replace(&mut self.state, State::Ready) else {
            return;
        };
        match result {
            Ok(result) => {
                self.capabilities = result["capabilities"].clone();
                if self.capabilities["positionEncoding"].as_str() == Some("utf-8") {
                    self.encoding = Encoding::Utf8;
                }
                self.write(message(None, "initialized", Json::Object(Vec::new())));
                for msg in queued {
                    self.write(msg);
                }
            }
            Err(e) => self.state = State::Failed(format!("the server wouldn't start: {}", e.message)),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.ready() {
            let id = self.next_id;
            self.write(message(Some(id.into()), "shutdown", Json::Null));
        }
        self.write(message(None, "exit", Json::Null));
        // the server should leave on its own; one that doesn't is stopped, without the editor waiting
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                for _ in 0..50 {
                    if !matches!(child.try_wait(), Ok(None)) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

fn initialize_params(root: &Path) -> Json {
    let uri = file_uri(root);
    let name = root
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    Json::object([
        ("processId", std::process::id().into()),
        (
            "clientInfo",
            Json::object([("name", "tvim".into()), ("version", env!("CARGO_PKG_VERSION").into())]),
        ),
        ("rootUri", uri.clone().into()),
        (
            "workspaceFolders",
            vec![Json::object([("uri", uri.into()), ("name", name.into())])].into(),
        ),
        (
            "capabilities",
            Json::object([
                (
                    "workspace",
                    Json::object([
                        ("workspaceEdit", Json::object([("documentChanges", true.into())])),
                        ("applyEdit", true.into()),
                        ("executeCommand", Json::Object(Vec::new())),
                    ]),
                ),
                (
                    "general",
                    Json::object([("positionEncodings", vec!["utf-8", "utf-16"].into())]),
                ),
                (
                    "textDocument",
                    Json::object([
                        ("synchronization", Json::object([("didSave", true.into())])),
                        ("publishDiagnostics", Json::Object(Vec::new())),
                        ("definition", Json::object([("linkSupport", true.into())])),
                        ("references", Json::Object(Vec::new())),
                        ("rename", Json::Object(Vec::new())),
                        ("formatting", Json::Object(Vec::new())),
                        (
                            "documentSymbol",
                            Json::object([("hierarchicalDocumentSymbolSupport", true.into())]),
                        ),
                        (
                            "signatureHelp",
                            Json::object([(
                                "signatureInformation",
                                Json::object([
                                    ("documentationFormat", vec!["markdown", "plaintext"].into()),
                                    (
                                        "parameterInformation",
                                        Json::object([("labelOffsetSupport", true.into())]),
                                    ),
                                    ("activeParameterSupport", true.into()),
                                ]),
                            )]),
                        ),
                        // actions as objects with their edits, rather than commands alone
                        (
                            "codeAction",
                            Json::object([
                                (
                                    "codeActionLiteralSupport",
                                    Json::object([(
                                        "codeActionKind",
                                        Json::object([(
                                            "valueSet",
                                            vec![
                                                "",
                                                "quickfix",
                                                "refactor",
                                                "refactor.extract",
                                                "refactor.inline",
                                                "refactor.rewrite",
                                                "source",
                                                "source.organizeImports",
                                            ]
                                            .into(),
                                        )]),
                                    )]),
                                ),
                                ("isPreferredSupport", true.into()),
                                ("disabledSupport", true.into()),
                            ]),
                        ),
                        ("rangeFormatting", Json::Object(Vec::new())),
                        // without snippets, so what comes back is plain text to type
                        (
                            "completion",
                            Json::object([("completionItem", Json::object([("snippetSupport", false.into())]))]),
                        ),
                        (
                            "hover",
                            Json::object([("contentFormat", vec!["markdown", "plaintext"].into())]),
                        ),
                    ]),
                ),
            ]),
        ),
    ])
}

// requests the server makes of the editor, none of which need anything from it yet
fn answer(method: &str, params: &Json) -> Result<Json, ResponseError> {
    match method {
        // no settings of our own, so each one asked for is left to the server's default
        "workspace/configuration" => {
            let asked = params["items"].as_array().map_or(0, <[Json]>::len);
            Ok(Json::Array(vec![Json::Null; asked]))
        }
        "window/workDoneProgress/create" | "client/registerCapability" | "client/unregisterCapability" => {
            Ok(Json::Null)
        }
        _ => Err(ResponseError {
            code: METHOD_NOT_FOUND,
            message: format!("{method} isn't supported"),
        }),
    }
}

fn message(id: Option<Json>, method: &str, params: Json) -> Json {
    let mut fields = vec![("jsonrpc".to_string(), Json::from("2.0"))];
    if let Some(id) = id {
        fields.push(("id".into(), id));
    }
    fields.push(("method".into(), method.into()));
    // params, when given, must be an object or array, so none at all rather than null
    if !params.is_null() {
        fields.push(("params".into(), params));
    }
    Json::Object(fields)
}

fn reply(id: Json, answer: Result<Json, ResponseError>) -> Json {
    let outcome = match answer {
        Ok(result) => ("result", result),
        Err(e) => (
            "error",
            Json::object([("code", e.code.into()), ("message", e.message.into())]),
        ),
    };
    Json::object([("jsonrpc", "2.0".into()), ("id", id), outcome])
}

fn write_all(to: impl Write, outgoing: Receiver<Json>) {
    let mut to = BufWriter::new(to);
    for msg in outgoing {
        if write_message(&mut to, &msg).is_err() {
            return;
        }
    }
}

fn read_all(from: impl Read, incoming: Sender<Json>, errors_closed: Receiver<()>, waker: Waker) {
    let mut from = BufReader::new(from);
    // a body that isn't JSON is skipped, but a broken frame loses our place, so it ends things
    while let Ok(Some(body)) = read_message(&mut from) {
        let Some(msg) = std::str::from_utf8(&body).ok().and_then(|s| Json::parse(s).ok()) else {
            continue;
        };
        if incoming.send(msg).is_err() {
            return;
        }
        waker.wake();
    }
    // a moment for the last of stderr to be read, so the reason is there when the editor looks
    let _ = errors_closed.recv_timeout(Duration::from_secs(1));
    // dropped before waking, so the editor finds the channel closed when it looks
    drop(incoming);
    waker.wake();
}

fn keep_last_line(errors: impl Read, last: &Mutex<String>) {
    for line in BufReader::new(errors).split(b'\n') {
        let Ok(line) = line else { return };
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if !line.is_empty() {
            *lock(last) = line.to_string();
        }
    }
}

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
pub(crate) mod testing {
    use super::*;
    use std::time::Instant;

    // one end of an in-memory pipe each way, standing in for a server process
    pub(crate) struct PipeIn {
        chunks: Receiver<Vec<u8>>,
        chunk: Vec<u8>,
        at: usize,
    }

    pub(crate) struct PipeOut(Sender<Vec<u8>>);

    impl Read for PipeIn {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.at == self.chunk.len() {
                match self.chunks.recv() {
                    Ok(chunk) => (self.chunk, self.at) = (chunk, 0),
                    Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(self.chunk.len() - self.at);
            buf[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
            self.at += n;
            Ok(n)
        }
    }

    impl Write for PipeOut {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.send(buf.to_vec()).map_err(|_| io::ErrorKind::BrokenPipe)?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn pipe() -> (PipeOut, PipeIn) {
        let (tx, rx) = mpsc::channel();
        let read = PipeIn {
            chunks: rx,
            chunk: Vec::new(),
            at: 0,
        };
        (PipeOut(tx), read)
    }

    // the server's side of the conversation
    pub(crate) struct Fake {
        pub(crate) from: BufReader<PipeIn>,
        pub(crate) to: PipeOut,
    }

    impl Fake {
        pub(crate) fn recv(&mut self) -> Option<Json> {
            let body = read_message(&mut self.from).unwrap()?;
            Some(Json::parse(std::str::from_utf8(&body).unwrap()).unwrap())
        }

        pub(crate) fn send(&mut self, text: &str) {
            write_message(&mut self.to, &Json::parse(text).unwrap()).unwrap();
        }

        // answers initialize with these capabilities; the initialized that follows comes
        // once the server has been polled
        pub(crate) fn start(&mut self, capabilities: &str) {
            let id = self.recv().unwrap()["id"].as_i64().unwrap();
            self.send(&format!(
                r#"{{"jsonrpc":"2.0","id":{id},"result":{{"capabilities":{capabilities}}}}}"#
            ));
        }
    }

    // a server for the project at `root`, with `errors` as what it writes to stderr
    pub(crate) fn fake(root: &Path, errors: &'static str) -> (Server, Fake) {
        let (to_server, server_in) = pipe();
        let (server_out, from_server) = pipe();
        let errors = io::Cursor::new(errors);
        let server = Server::connect(from_server, to_server, errors, root);
        let fake = Fake {
            from: BufReader::new(server_in),
            to: server_out,
        };
        (server, fake)
    }

    pub(crate) fn next(server: &mut Server) -> Incoming {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(incoming) = server.poll() {
                return incoming;
            }
            assert!(Instant::now() < deadline, "nothing arrived");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub(crate) fn settle(server: &mut Server, until: impl Fn(&Server) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !until(server) {
            assert_eq!(server.poll(), None);
            assert!(Instant::now() < deadline, "never settled");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    fn connected() -> (Server, Fake) {
        connected_saying("")
    }

    fn connected_saying(errors: &'static str) -> (Server, Fake) {
        fake(Path::new("/work/my project"), errors)
    }

    // messages are handled in order, so once a ping sent after them is seen, they've been dealt with
    fn sync_up(server: &mut Server, fake: &mut Fake) {
        fake.send(r#"{"jsonrpc":"2.0","method":"ping"}"#);
        let ping = Incoming::Notice {
            method: "ping".into(),
            params: Json::Null,
        };
        assert_eq!(next(server), ping);
    }

    fn framed(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    }

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
    fn the_handshake_comes_first_and_requests_wait_for_it() {
        let (mut server, mut fake) = connected();
        let early = server.request("textDocument/hover", Json::object([("x", 1.into())]));
        server.notify("textDocument/didOpen", Json::object([("y", 2.into())]));
        assert!(!server.ready());

        let init = fake.recv().unwrap();
        assert_eq!(init["method"].as_str(), Some("initialize"));
        assert_eq!(
            init["params"]["processId"].as_u64(),
            Some(u64::from(std::process::id()))
        );
        assert_eq!(init["params"]["rootUri"].as_str(), Some("file:///work/my%20project"));
        assert_eq!(
            init["params"]["workspaceFolders"][0]["name"].as_str(),
            Some("my project")
        );

        // the server may ask things before it has answered, and is answered straight away
        fake.send(r#"{"jsonrpc":"2.0","id":"cfg","method":"workspace/configuration","params":{"items":[{},{}]}}"#);
        sync_up(&mut server, &mut fake);
        let answer = fake.recv().unwrap();
        assert_eq!(
            answer.to_string(),
            r#"{"jsonrpc":"2.0","id":"cfg","result":[null,null]}"#
        );

        let id = init["id"].as_i64().unwrap();
        fake.send(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"capabilities":{{"positionEncoding":"utf-8","hoverProvider":true}}}}}}"#
        ));
        settle(&mut server, Server::ready);
        assert_eq!(server.encoding(), Encoding::Utf8);
        assert_eq!(server.capabilities()["hoverProvider"].as_bool(), Some(true));

        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("initialized"));
        let hover = fake.recv().unwrap();
        assert_eq!(hover["method"].as_str(), Some("textDocument/hover"));
        assert_eq!(hover["id"].as_i64(), Some(early));
        assert_eq!(fake.recv().unwrap()["params"]["y"].as_i64(), Some(2));
    }

    fn started() -> (Server, Fake) {
        let (mut server, mut fake) = connected();
        fake.start("{}");
        settle(&mut server, Server::ready);
        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("initialized"));
        (server, fake)
    }

    #[test]
    fn requests_are_numbered_and_sent_in_order() {
        let (mut server, mut fake) = started();
        let a = server.request("a", Json::object([("x", 1.into())]));
        server.notify("n", Json::Null);
        let b = server.request("b", Json::Null);
        assert_eq!(b, a + 1);
        let sent = fake.recv().unwrap().to_string();
        assert_eq!(
            sent,
            format!(r#"{{"jsonrpc":"2.0","id":{a},"method":"a","params":{{"x":1}}}}"#)
        );
        let sent = fake.recv().unwrap().to_string();
        assert_eq!(sent, r#"{"jsonrpc":"2.0","method":"n"}"#, "no params rather than null");
        assert_eq!(fake.recv().unwrap()["id"].as_i64(), Some(b));
    }

    #[test]
    fn replies_and_notices_arrive_in_order() {
        let (mut server, mut fake) = started();
        assert_eq!(server.encoding(), Encoding::Utf16, "unless the server picks otherwise");
        let a = server.request("a", Json::Null);
        let b = server.request("b", Json::Null);
        fake.recv().unwrap();
        fake.recv().unwrap();

        fake.send(r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"message":"hi"}}"#);
        fake.send(&format!(r#"{{"jsonrpc":"2.0","id":{a},"result":null}}"#));
        fake.send(&format!(
            r#"{{"jsonrpc":"2.0","id":{b},"error":{{"code":-32800,"message":"cancelled"}}}}"#
        ));
        let params = Json::object([("message", "hi".into())]);
        assert_eq!(
            next(&mut server),
            Incoming::Notice {
                method: "window/logMessage".into(),
                params
            }
        );
        assert_eq!(
            next(&mut server),
            Incoming::Reply {
                id: a,
                result: Ok(Json::Null)
            }
        );
        let error = ResponseError {
            code: -32800,
            message: "cancelled".into(),
        };
        assert_eq!(
            next(&mut server),
            Incoming::Reply {
                id: b,
                result: Err(error)
            }
        );
    }

    #[test]
    fn edits_the_server_asks_for_are_said_to_be_made_and_passed_on() {
        let (mut server, mut fake) = started();
        fake.send(r#"{"jsonrpc":"2.0","id":5,"method":"workspace/applyEdit","params":{"edit":{"changes":{}}}}"#);
        let passed = Incoming::Notice {
            method: "workspace/applyEdit".into(),
            params: Json::parse(r#"{"edit":{"changes":{}}}"#).unwrap(),
        };
        assert_eq!(next(&mut server), passed);
        assert_eq!(
            fake.recv().unwrap().to_string(),
            r#"{"jsonrpc":"2.0","id":5,"result":{"applied":true}}"#
        );
    }

    #[test]
    fn requests_the_editor_cant_answer_get_an_error_back() {
        let (mut server, mut fake) = started();
        fake.send(r#"{"jsonrpc":"2.0","id":4,"method":"window/showDocument","params":{}}"#);
        sync_up(&mut server, &mut fake);
        let answer = fake.recv().unwrap();
        assert_eq!(answer["id"].as_i64(), Some(4));
        assert_eq!(answer["error"]["code"].as_i64(), Some(METHOD_NOT_FOUND));
    }

    #[test]
    fn garbage_between_messages_is_skipped() {
        let (mut server, mut fake) = started();
        fake.to.write_all(framed("not json").as_bytes()).unwrap();
        fake.send(r#"{"jsonrpc":"2.0","method":"ping"}"#);
        assert_eq!(
            next(&mut server),
            Incoming::Notice {
                method: "ping".into(),
                params: Json::Null
            }
        );
    }

    #[test]
    fn a_refused_start_is_a_failure() {
        let (mut server, mut fake) = connected();
        let id = fake.recv().unwrap()["id"].as_i64().unwrap();
        fake.send(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":1,"message":"no workspace"}}}}"#
        ));
        settle(&mut server, |s| s.failure().is_some());
        assert_eq!(server.failure(), Some("the server wouldn't start: no workspace"));
    }

    #[test]
    fn a_server_that_goes_away_is_a_failure() {
        let (mut server, fake) = started();
        drop(fake);
        settle(&mut server, |s| s.failure().is_some());
        assert_eq!(server.failure(), Some("the server stopped"));
        server.request("after", Json::Null);
    }

    #[test]
    fn a_server_that_goes_away_says_why_if_it_can() {
        let (mut server, fake) = connected_saying("starting\nerror: no toolchain\n\n");
        drop(fake);
        settle(&mut server, |s| s.failure().is_some());
        assert_eq!(server.failure(), Some("the server stopped: error: no toolchain"));
    }

    #[test]
    fn dropping_shuts_the_server_down() {
        let (server, mut fake) = started();
        drop(server);
        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("shutdown"));
        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("exit"));
        assert_eq!(fake.recv(), None, "and its input closes");
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let err = Server::spawn("tvim-no-such-server", &[], &std::env::temp_dir())
            .err()
            .unwrap();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
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

    #[test]
    fn columns_counted_each_way() {
        let line = "aé😀b";
        assert_eq!(Encoding::Utf16.units(line, 4), 5, "the emoji takes two");
        assert_eq!(Encoding::Utf8.units(line, 4), 8);
        assert_eq!(Encoding::Utf16.units(line, 99), 5, "past the end is the end");
        assert_eq!(Encoding::Utf16.chars(line, 2), 2);
        assert_eq!(Encoding::Utf16.chars(line, 3), 2, "inside the emoji is the emoji");
        assert_eq!(Encoding::Utf16.chars(line, 4), 3);
        assert_eq!(Encoding::Utf8.chars(line, 2), 1, "inside é is é");
        assert_eq!(Encoding::Utf8.chars(line, 99), 4);
    }
}
