//! Keeps language servers told about the files open in the editor.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::lsp::Server;
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

pub struct Languages {
    running: HashMap<Key, Server>,
    // servers that wouldn't start, left alone unless the command changes
    refused: HashSet<Key>,
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
            choices: HashMap::new(),
            start,
        }
    }

    // takes in what the servers have said, and starts any that `docs` need
    pub fn update(&mut self, docs: &[OpenDoc], servers: &Servers) {
        self.receive();
        let chosen: Vec<Choice> = docs
            .iter()
            .filter_map(|d| self.choice(d.id, d.path?, servers))
            .collect();
        for choice in &chosen {
            self.run(choice);
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

    // each server is started once, and one that won't start isn't tried again
    fn run(&mut self, choice: &Choice) {
        let key = key(choice);
        if self.running.contains_key(&key) || self.refused.contains(&key) {
            return;
        }
        match (self.start)(&choice.command, &choice.root) {
            Ok(server) => {
                self.running.insert(key, server);
            }
            Err(_) => {
                self.refused.insert(key);
            }
        }
    }
}

fn key(choice: &Choice) -> Key {
    (choice.command.clone(), choice.root.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::testing::{Fake, fake};
    use crate::paths;
    use std::sync::mpsc::{self, Receiver};

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
    }

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
}
