use std::io::Write;
use std::process::{Command, Stdio};

pub trait Clipboard {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str) -> bool;
}

pub struct System;

const COPY: &[&[&str]] = &[
    &["pbcopy"],
    &["wl-copy"],
    &["xclip", "-selection", "clipboard"],
    &["xsel", "--clipboard", "--input"],
    &["clip.exe"],
];

const PASTE: &[&[&str]] = &[
    &["pbpaste"],
    &["wl-paste", "--no-newline"],
    &["xclip", "-selection", "clipboard", "-o"],
    &["xsel", "--clipboard", "--output"],
    &["powershell.exe", "-NoProfile", "-Command", "Get-Clipboard"],
];

impl Clipboard for System {
    fn get(&mut self) -> Option<String> {
        PASTE.iter().find_map(|cmd| {
            let out = Command::new(cmd[0])
                .args(&cmd[1..])
                .stderr(Stdio::null())
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
        })
    }

    fn set(&mut self, text: &str) -> bool {
        COPY.iter().any(|cmd| {
            let Ok(mut child) = Command::new(cmd[0])
                .args(&cmd[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                return false;
            };
            let wrote = child
                .stdin
                .take()
                .is_some_and(|mut i| i.write_all(text.as_bytes()).is_ok());
            child.wait().is_ok_and(|s| s.success()) && wrote
        })
    }
}

#[derive(Default)]
pub struct Memory(pub Option<String>);

impl Clipboard for Memory {
    fn get(&mut self) -> Option<String> {
        self.0.clone()
    }

    fn set(&mut self, text: &str) -> bool {
        self.0 = Some(text.to_string());
        true
    }
}
