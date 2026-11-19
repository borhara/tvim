//! Keeps language servers told about the files open in the editor, and holds the problems
//! they find in them.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::buffer::Pos;
use crate::json::Json;
use crate::lsp::{self, Encoding, Incoming, Server};
use crate::paths;
use crate::servers::{Choice, Servers};

// a file open in the editor, as a server needs to know it
pub struct OpenDoc<'a> {
    pub id: usize,
    pub path: Option<&'a Path>,
    // changes with every edit, and goes back with undo
    pub version: u64,
    // the version last written to disk
    pub saved: u64,
    pub lines: &'a [String],
    pub final_newline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub start: Pos,
    pub end: Pos,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Update {
    // problems came or went, so there's something new to show
    pub changed: bool,
}

// a server is one command run for one project
type Key = (Vec<String>, PathBuf);
type Start = Box<dyn FnMut(&[String], &Path) -> io::Result<Server>>;

struct Synced {
    key: Key,
    path: PathBuf,
    uri: String,
    version: u64,
    saved: u64,
    // the text as the server has it, a line each, with an empty last line for a final newline
    lines: Vec<String>,
    // the version number the server was last given
    sent: i64,
    problems: Vec<Problem>,
}

// how a server wants to hear about files, from its capabilities
struct Sync {
    open_close: bool,
    change: Change,
    // Some when saves are wanted, saying whether with the text
    save: Option<bool>,
}

#[derive(PartialEq, Eq)]
enum Change {
    Never,
    Whole,
    Edits,
}

pub struct Languages {
    running: HashMap<Key, Server>,
    // servers that wouldn't start, left alone unless the command changes
    refused: HashSet<Key>,
    synced: HashMap<usize, Synced>,
    // each file's server, worked out again only when its path or the choices change
    choices: HashMap<usize, (PathBuf, u64, Option<Choice>)>,
    start: Start,
}

impl Default for Languages {
    fn default() -> Self {
        Languages::with_start(Box::new(|command, root| {
            let (program, args) = command
                .split_first()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no command given"))?;
            Server::spawn(program, args, root)
        }))
    }
}

impl Languages {
    fn with_start(start: Start) -> Languages {
        Languages {
            running: HashMap::new(),
            refused: HashSet::new(),
            synced: HashMap::new(),
            choices: HashMap::new(),
            start,
        }
    }

    pub fn problems(&self, id: usize) -> &[Problem] {
        self.synced.get(&id).map_or(&[], |s| &s.problems)
    }

    // takes in what the servers have said, then tells them what changed in `docs`
    pub fn update(&mut self, docs: &[OpenDoc], servers: &Servers) -> Update {
        let mut update = Update::default();
        self.receive(&mut update);
        let chosen: Vec<(&OpenDoc, &Path, Choice)> = docs
            .iter()
            .filter_map(|d| {
                let path = d.path?;
                Some((d, path, self.choice(d.id, path, servers)?))
            })
            .collect();
        let wanted: HashMap<usize, (Key, &Path)> = chosen.iter().map(|(d, p, c)| (d.id, (key(c), *p))).collect();

        // closed, renamed, or given another server: told it's closed, to be opened afresh
        let stale: Vec<usize> = self
            .synced
            .iter()
            .filter(|(id, s)| wanted.get(id).is_none_or(|(k, p)| *k != s.key || *p != s.path))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            let s = self.synced.remove(&id).expect("listed above");
            update.changed |= !s.problems.is_empty();
            if let Some(server) = self.running.get_mut(&s.key).filter(|server| server.ready()) {
                if sync_of(server.capabilities()).open_close {
                    server.notify(
                        "textDocument/didClose",
                        Json::object([("textDocument", document(&s.uri))]),
                    );
                }
            }
        }

        for (doc, path, choice) in &chosen {
            self.sync(doc, path, choice);
        }
        let needed: HashSet<&Key> = wanted.values().map(|(k, _)| k).collect();
        self.running.retain(|k, _| needed.contains(k));
        self.choices.retain(|id, _| docs.iter().any(|d| d.id == *id));
        update
    }

    fn choice(&mut self, id: usize, path: &Path, servers: &Servers) -> Option<Choice> {
        match self.choices.get(&id) {
            Some((p, changes, choice)) if p == path && *changes == servers.changes() => choice.clone(),
            _ => {
                let choice = servers.for_file(path);
                self.choices
                    .insert(id, (path.to_path_buf(), servers.changes(), choice.clone()));
                choice
            }
        }
    }

    fn receive(&mut self, update: &mut Update) {
        let Languages { running, synced, .. } = self;
        for (key, server) in running.iter_mut() {
            while let Some(incoming) = server.poll() {
                let Incoming::Notice { method, params } = incoming else {
                    continue;
                };
                if method != "textDocument/publishDiagnostics" {
                    continue;
                }
                let uri = params["uri"].as_str();
                if let Some(s) = synced
                    .values_mut()
                    .find(|s| s.key == *key && Some(s.uri.as_str()) == uri)
                {
                    s.problems = problems(&params["diagnostics"], &s.lines, server.encoding());
                    update.changed = true;
                }
            }
        }
    }

    fn sync(&mut self, doc: &OpenDoc, path: &Path, choice: &Choice) {
        let key = key(choice);
        if !self.running.contains_key(&key) {
            if self.refused.contains(&key) {
                return;
            }
            match (self.start)(&choice.command, &choice.root) {
                Ok(server) => {
                    self.running.insert(key.clone(), server);
                }
                Err(_) => {
                    self.refused.insert(key);
                    return;
                }
            }
        }
        let server = self.running.get_mut(&key).expect("started above");
        // what a server wants to hear is only known once it has started
        if !server.ready() {
            return;
        }
        let sync = sync_of(server.capabilities());
        let Some(s) = self.synced.get_mut(&doc.id) else {
            let lines = text_of(doc);
            let uri = lsp::file_uri(&paths::absolute(path));
            if sync.open_close {
                let item = Json::object([
                    ("uri", uri.clone().into()),
                    ("languageId", choice.language.into()),
                    ("version", 1.into()),
                    ("text", lines.join("\n").into()),
                ]);
                server.notify("textDocument/didOpen", Json::object([("textDocument", item)]));
            }
            let synced = Synced {
                key,
                path: path.to_path_buf(),
                uri,
                version: doc.version,
                saved: doc.saved,
                lines,
                sent: 1,
                problems: Vec::new(),
            };
            self.synced.insert(doc.id, synced);
            return;
        };
        if s.version != doc.version {
            let lines = text_of(doc);
            if sync.open_close && sync.change != Change::Never {
                let change = match sync.change {
                    Change::Edits => edit(&s.lines, &lines, server.encoding()),
                    _ => Json::object([("text", lines.join("\n").into())]),
                };
                s.sent += 1;
                let item = Json::object([("uri", s.uri.clone().into()), ("version", s.sent.into())]);
                let params = Json::object([("textDocument", item), ("contentChanges", vec![change].into())]);
                server.notify("textDocument/didChange", params);
            }
            s.lines = lines;
            s.version = doc.version;
        }
        if s.saved != doc.saved {
            s.saved = doc.saved;
            // the saved version moves on a write, and then the text is what was written
            if let Some(with_text) = sync.save.filter(|_| sync.open_close && doc.saved == doc.version) {
                let mut params = vec![("textDocument".to_string(), document(&s.uri))];
                if with_text {
                    params.push(("text".into(), s.lines.join("\n").into()));
                }
                server.notify("textDocument/didSave", Json::Object(params));
            }
        }
    }
}

fn key(choice: &Choice) -> Key {
    (choice.command.clone(), choice.root.clone())
}

fn document(uri: &str) -> Json {
    Json::object([("uri", uri.into())])
}

// the text a line each, as the file on disk would have it
fn text_of(doc: &OpenDoc) -> Vec<String> {
    let mut lines = doc.lines.to_vec();
    if doc.final_newline {
        lines.push(String::new());
    }
    lines
}

fn sync_of(capabilities: &Json) -> Sync {
    let sync = &capabilities["textDocumentSync"];
    let change = |kind: Option<i64>| match kind {
        Some(1) => Change::Whole,
        Some(2) => Change::Edits,
        _ => Change::Never,
    };
    // a bare number is the kind of change, with opens and closes but no saves
    if let Some(kind) = sync.as_i64() {
        return Sync {
            open_close: true,
            change: change(Some(kind)),
            save: None,
        };
    }
    let save = match &sync["save"] {
        Json::Bool(true) => Some(false),
        save @ Json::Object(_) => Some(save["includeText"].as_bool().unwrap_or(false)),
        _ => None,
    };
    Sync {
        open_close: sync["openClose"].as_bool().unwrap_or(false),
        change: change(sync["change"].as_i64()),
        save,
    }
}

// the one edit that turns `old` into `new`: the lines between those the two share at the
// start and at the end, replaced
fn edit(old: &[String], new: &[String], encoding: Encoding) -> Json {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - head;
    let tail = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let end_of = |row: usize| (row, encoding.units(&old[row], usize::MAX));
    let last = old.len() - 1;
    let (from, to, text) = if tail > 0 {
        // whole lines, up to the start of the first line after that's the same
        let text: String = new[head..new.len() - tail].iter().map(|l| format!("{l}\n")).collect();
        ((head, 0), (old.len() - tail, 0), text)
    } else if head > 0 {
        // the same to the end of a line, and different from there to the end of the text
        let text: String = new[head..].iter().map(|l| format!("\n{l}")).collect();
        (end_of(head - 1), end_of(last), text)
    } else {
        ((0, 0), end_of(last), new.join("\n"))
    };
    let position =
        |(line, character): (usize, usize)| Json::object([("line", line.into()), ("character", character.into())]);
    let range = Json::object([("start", position(from)), ("end", position(to))]);
    Json::object([("range", range), ("text", text.into())])
}

fn problems(diagnostics: &Json, lines: &[String], encoding: Encoding) -> Vec<Problem> {
    let pos = |p: &Json| {
        let row = p["line"].as_u64().unwrap_or(0) as usize;
        let line = lines.get(row).map_or("", String::as_str);
        let units = p["character"].as_u64().unwrap_or(0) as usize;
        Pos::new(row, encoding.chars(line, units))
    };
    let mut out: Vec<Problem> = diagnostics
        .as_array()
        .unwrap_or(&[])
        .iter()
        .map(|d| Problem {
            start: pos(&d["range"]["start"]),
            end: pos(&d["range"]["end"]),
            // one without a severity is taken as an error
            severity: match d["severity"].as_i64() {
                Some(2) => Severity::Warning,
                Some(3) => Severity::Info,
                Some(4) => Severity::Hint,
                _ => Severity::Error,
            },
            message: d["message"].as_str().unwrap_or("").trim().to_string(),
        })
        .collect();
    out.sort_by_key(|p| (p.start, p.severity));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::testing::{Fake, fake};
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    struct Rig {
        langs: Languages,
        servers: Servers,
        // each server started, with the command and folder it was started with
        started: Receiver<(Vec<String>, PathBuf, Fake)>,
    }

    fn rig() -> Rig {
        let (tx, started) = mpsc::channel();
        let langs = Languages::with_start(Box::new(move |command, root| {
            let (server, fake) = fake(root, "");
            tx.send((command.to_vec(), root.to_path_buf(), fake)).unwrap();
            Ok(server)
        }));
        Rig {
            langs,
            servers: Servers::default(),
            started,
        }
    }

    impl Rig {
        fn update(&mut self, docs: &[OpenDoc]) -> Update {
            self.langs.update(docs, &self.servers)
        }

        // the next server started, once it has been told it's initialized
        fn start(&mut self, docs: &[OpenDoc], capabilities: &str) -> Fake {
            self.update(docs);
            let (_, _, mut fake) = self.started.try_recv().expect("a server was started");
            fake.start(capabilities);
            self.until(docs, |r| r.langs.running.values().all(Server::ready));
            assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("initialized"));
            fake
        }

        fn until(&mut self, docs: &[OpenDoc], done: impl Fn(&Rig) -> bool) -> Update {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let update = self.update(docs);
                if done(self) || update.changed {
                    return update;
                }
                assert!(Instant::now() < deadline, "nothing happened");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    // as the editor's paths are sent, which on Windows means a drive in front
    fn uri(path: &str) -> String {
        lsp::file_uri(&paths::absolute(Path::new(path)))
    }

    // the problems a server reports for /work/a.rs
    fn diagnostics(list: &str) -> String {
        let params = format!(r#"{{"uri":{},"diagnostics":[{list}]}}"#, Json::from(uri("/work/a.rs")));
        format!(r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{params}}}"#)
    }

    const EDITS: &str = r#"{"textDocumentSync":{"openClose":true,"change":2,"save":{"includeText":false}}}"#;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    fn doc<'a>(path: &'a str, version: u64, saved: u64, lines: &'a [String]) -> OpenDoc<'a> {
        OpenDoc {
            id: 1,
            path: Some(Path::new(path)),
            version,
            saved,
            lines,
            final_newline: true,
        }
    }

    #[test]
    fn each_project_gets_one_server_for_each_command() {
        let mut r = rig();
        let v1 = lines("a");
        let docs = [
            doc("/work/a.ts", 1, 1, &v1),
            OpenDoc {
                id: 2,
                ..doc("/work/b.jsx", 1, 1, &v1)
            },
            OpenDoc {
                id: 3,
                ..doc("/work/notes.txt", 1, 1, &v1)
            },
            OpenDoc {
                id: 4,
                ..doc("/work/c.rs", 1, 1, &v1)
            },
        ];
        r.update(&docs);
        let mut started: Vec<(Vec<String>, PathBuf)> = r.started.try_iter().map(|(c, root, _)| (c, root)).collect();
        started.sort();
        let work = paths::absolute(Path::new("/work"));
        let typescript = vec!["typescript-language-server".to_string(), "--stdio".into()];
        assert_eq!(
            started,
            [(vec!["rust-analyzer".to_string()], work.clone()), (typescript, work)]
        );
        r.update(&docs);
        assert!(r.started.try_recv().is_err(), "and only once");
    }

    #[test]
    fn a_file_is_opened_changed_saved_and_closed() {
        let mut r = rig();
        let v1 = lines("fn main() {\n}");
        let mut fake = r.start(&[doc("/work/a.rs", 1, 1, &v1)], EDITS);
        let open = fake.recv().unwrap();
        assert_eq!(open["method"].as_str(), Some("textDocument/didOpen"));
        let item = &open["params"]["textDocument"];
        assert_eq!(item["uri"].as_str(), Some(uri("/work/a.rs").as_str()));
        assert_eq!(item["languageId"].as_str(), Some("rust"));
        assert_eq!(
            item["text"].as_str(),
            Some("fn main() {\n}\n"),
            "with its final newline"
        );

        let v2 = lines("fn main() {\n    go();\n}");
        r.update(&[doc("/work/a.rs", 2, 1, &v2)]);
        let change = fake.recv().unwrap();
        assert_eq!(change["method"].as_str(), Some("textDocument/didChange"));
        assert_eq!(change["params"]["textDocument"]["version"].as_i64(), Some(2));
        assert_eq!(
            change["params"]["contentChanges"].to_string(),
            r#"[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":0}},"text":"    go();\n"}]"#
        );

        r.update(&[doc("/work/a.rs", 2, 2, &v2)]);
        let save = fake.recv().unwrap();
        assert_eq!(save["method"].as_str(), Some("textDocument/didSave"));
        assert_eq!(save["params"].get("text"), None, "not asked for");

        r.update(&[]);
        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("textDocument/didClose"));
        assert_eq!(
            fake.recv().unwrap()["method"].as_str(),
            Some("shutdown"),
            "nothing needs it now"
        );
    }

    #[test]
    fn undo_back_to_the_saved_text_is_a_change_but_not_a_save() {
        let mut r = rig();
        let (v1, v2) = (lines("a"), lines("b"));
        let mut fake = r.start(&[doc("/work/a.rs", 1, 1, &v1)], EDITS);
        fake.recv().unwrap();
        r.update(&[doc("/work/a.rs", 2, 1, &v2)]);
        assert_eq!(
            fake.recv().unwrap()["params"]["contentChanges"][0]["text"].as_str(),
            Some("b\n")
        );
        r.update(&[doc("/work/a.rs", 1, 1, &v1)]);
        let back = fake.recv().unwrap();
        assert_eq!(
            back["params"]["textDocument"]["version"].as_i64(),
            Some(3),
            "versions only go up"
        );
        r.update(&[]);
        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("textDocument/didClose"));
    }

    #[test]
    fn a_server_without_edits_gets_the_whole_text() {
        let mut r = rig();
        let (v1, v2) = (lines("a"), lines("a\nb"));
        let mut fake = r.start(&[doc("/work/a.py", 1, 1, &v1)], "{\"textDocumentSync\":1}");
        assert_eq!(
            fake.recv().unwrap()["params"]["textDocument"]["languageId"].as_str(),
            Some("python")
        );
        r.update(&[doc("/work/a.py", 2, 2, &v2)]);
        let change = fake.recv().unwrap();
        assert_eq!(change["params"]["contentChanges"].to_string(), r#"[{"text":"a\nb\n"}]"#);
        r.update(&[]);
        assert_eq!(
            fake.recv().unwrap()["method"].as_str(),
            Some("textDocument/didClose"),
            "a bare number asks for no saves"
        );
    }

    #[test]
    fn a_server_that_wants_no_files_is_told_of_none() {
        let mut r = rig();
        let v1 = lines("a");
        let mut fake = r.start(&[doc("/work/a.rs", 1, 1, &v1)], "{}");
        r.update(&[doc("/work/a.rs", 2, 2, &v1)]);
        r.update(&[]);
        assert_eq!(fake.recv().unwrap()["method"].as_str(), Some("shutdown"));
    }

    #[test]
    fn problems_arrive_in_the_editors_columns() {
        let mut r = rig();
        let v1 = lines("let 😀 = é;\nok");
        let docs = [doc("/work/a.rs", 1, 1, &v1)];
        let mut fake = r.start(&docs, EDITS);
        fake.recv().unwrap();
        fake.send(&diagnostics(
            r#"{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":2}},"severity":2,"message":"unused\n"},
               {"range":{"start":{"line":0,"character":9},"end":{"line":0,"character":10}},"message":"what's é?"}"#,
        ));
        assert!(r.until(&docs, |_| false).changed);
        let found = r.langs.problems(1);
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].start, found[0].end),
            (Pos::new(0, 8), Pos::new(0, 9)),
            "the emoji is two"
        );
        assert_eq!(found[0].severity, Severity::Error, "unless it says otherwise");
        assert_eq!(found[1].severity, Severity::Warning);
        assert_eq!(found[1].message, "unused");

        r.update(&[]);
        assert!(r.langs.problems(1).is_empty(), "gone with the file");
    }

    #[test]
    fn a_renamed_file_is_opened_afresh() {
        let mut r = rig();
        let v1 = lines("a");
        let mut fake = r.start(&[doc("/work/a.rs", 1, 1, &v1)], EDITS);
        fake.recv().unwrap();
        r.update(&[doc("/work/b.rs", 1, 1, &v1)]);
        let close = fake.recv().unwrap();
        assert_eq!(
            close["params"]["textDocument"]["uri"].as_str(),
            Some(uri("/work/a.rs").as_str())
        );
        let open = fake.recv().unwrap();
        assert_eq!(
            open["params"]["textDocument"]["uri"].as_str(),
            Some(uri("/work/b.rs").as_str())
        );
    }

    #[test]
    fn files_in_one_project_share_a_server() {
        let mut r = rig();
        let v1 = lines("a");
        let one = doc("/work/a.ts", 1, 1, &v1);
        let two = OpenDoc {
            id: 2,
            path: Some(Path::new("/work/b.jsx")),
            ..doc("", 1, 1, &v1)
        };
        let mut fake = r.start(&[one, two], EDITS);
        let mut languages: Vec<String> = (0..2)
            .map(|_| {
                fake.recv().unwrap()["params"]["textDocument"]["languageId"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        languages.sort();
        assert_eq!(languages, ["javascriptreact", "typescript"]);
        assert!(r.started.try_recv().is_err(), "only the one");
    }

    #[test]
    fn another_command_means_another_server() {
        let mut r = rig();
        let v1 = lines("a");
        let docs = [doc("/work/a.rs", 1, 1, &v1)];
        let mut first = r.start(&docs, EDITS);
        first.recv().unwrap();
        r.servers.set("rust", "my-server --fast").unwrap();
        r.update(&docs);
        assert_eq!(first.recv().unwrap()["method"].as_str(), Some("textDocument/didClose"));
        assert_eq!(first.recv().unwrap()["method"].as_str(), Some("shutdown"));
        let (command, root, _) = r.started.try_recv().unwrap();
        assert_eq!(
            (command, root),
            (
                vec!["my-server".into(), "--fast".into()],
                paths::absolute(Path::new("/work"))
            )
        );
    }

    fn edited(old: &str, new: &str) -> String {
        edit(&lines(old), &lines(new), Encoding::Utf16).to_string()
    }

    fn range(a: (usize, usize), b: (usize, usize), text: &str) -> String {
        format!(
            r#"{{"range":{{"start":{{"line":{},"character":{}}},"end":{{"line":{},"character":{}}}}},"text":{}}}"#,
            a.0,
            a.1,
            b.0,
            b.1,
            Json::from(text)
        )
    }

    #[test]
    fn an_edit_covers_only_the_lines_that_differ() {
        assert_eq!(edited("a\nb\nc", "a\nx\nc"), range((1, 0), (2, 0), "x\n"));
        assert_eq!(edited("a\nc", "a\nb\nc"), range((1, 0), (1, 0), "b\n"), "a line put in");
        assert_eq!(edited("a\nb\nc", "a\nc"), range((1, 0), (2, 0), ""), "a line taken out");
        assert_eq!(
            edited("a\nb", "a\nb\nc"),
            range((1, 1), (1, 1), "\nc"),
            "added at the end"
        );
        assert_eq!(
            edited("a\nb\nc", "a\nb"),
            range((1, 1), (2, 1), ""),
            "taken off the end"
        );
        assert_eq!(edited("a\nb", "a\nB"), range((0, 1), (1, 1), "\nB"));
        assert_eq!(edited("a", "b"), range((0, 0), (0, 1), "b"));
        assert_eq!(edited("x\na", "y\na"), range((0, 0), (1, 0), "y\n"));
        assert_eq!(
            edited("a\na", "a\na\na"),
            range((1, 1), (1, 1), "\na"),
            "repeats share what they can"
        );
        assert_eq!(edited("é😀\n", "é😀x\n"), range((0, 0), (1, 0), "é😀x\n"));
        assert_eq!(
            edited("é😀", "é😀x"),
            range((0, 0), (0, 3), "é😀x"),
            "columns in UTF-16"
        );
    }
}
