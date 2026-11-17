pub trait Clipboard {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str) -> bool;
}

// the system's clipboard, as tinsel reaches it: its tools, or the terminal over ssh
pub struct System;

impl Clipboard for System {
    fn get(&mut self) -> Option<String> {
        tinsel::clipboard::paste()
    }

    fn set(&mut self, text: &str) -> bool {
        tinsel::clipboard::copy(text)
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
