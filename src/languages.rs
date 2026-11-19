//! Keeps language servers told about the files open in the editor.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::json::Json;
use crate::lsp::{self, Server};
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

// a server is one command run for one project
type Key = (Vec<String>, PathBuf);
type Start = Box<dyn FnMut(&[String], &Path) -> io::Result<Server>>;

struct Synced {
    key: Key,
    path: PathBuf,
    uri: String,
}

// how a server wants to hear about files, from its capabilities
struct Sync {
    open_close: bool,
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

    // takes in what the servers have said, then tells them what changed in `docs`
    pub fn update(&mut self, docs: &[OpenDoc], servers: &Servers) {
        self.receive();
        let chosen: Vec<(&OpenDoc, &Path, Choice)> = docs
            .iter()
            .filter_map(|d| {
                let path = d.path?;
                Some((d, path, self.choice(d.id, path, servers)?))
            })
            .collect();
        for (doc, path, choice) in &chosen {
            self.sync(doc, path, choice);
        }
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

    // what a server says is read as it comes, though nothing in it is used yet
    fn receive(&mut self) {
        for server in self.running.values_mut() {
            while server.poll().is_some() {}
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
        if self.synced.contains_key(&doc.id) {
            return;
        }
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
        };
        self.synced.insert(doc.id, synced);
    }
}

fn key(choice: &Choice) -> Key {
    (choice.command.clone(), choice.root.clone())
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
    // a bare number is the kind of change, and means opens and closes are wanted too
    Sync {
        open_close: sync.as_i64().is_some() || sync["openClose"].as_bool().unwrap_or(false),
    }
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
        fn update(&mut self, docs: &[OpenDoc]) {
            self.langs.update(docs, &self.servers);
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

        fn until(&mut self, docs: &[OpenDoc], done: impl Fn(&Rig) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.update(docs);
                if done(self) {
                    return;
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
    fn a_file_is_opened() {
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
}
