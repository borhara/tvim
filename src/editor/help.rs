use super::Editor;
use crate::help;

// the help buffer's name, which also gives it markdown's colours
const NAME: &str = "help.md";

impl Editor {
    // :help, or :help topic: the docs in a window above, at what they say about it
    pub(crate) fn show_help(&mut self, query: &str) {
        let Some(row) = help::find(help::DOCS, query) else {
            return self.error(format!("E149: Sorry, no help for {}", query.trim()));
        };
        let is_help = |scratch: &Option<String>| scratch.as_deref() == Some(NAME);
        if !is_help(&self.file.scratch) {
            let doc = self.docs.iter().position(|d| is_help(&d.file.scratch));
            let shown = doc.and_then(|i| self.wins.iter().find(|w| w.buffer == self.docs[i].id).map(|w| w.id));
            match (doc, shown) {
                (_, Some(win)) => self.focus_window(win),
                _ if !self.split_window(false, false) => return,
                (Some(i), None) => self.switch_to(i),
                (None, None) => self.open_scratch(NAME.into(), &help::lines()),
            }
        }
        self.cursor = self.first_non_blank_pos(row.min(self.buf.last_row()));
        self.top.set(self.cursor.row);
        self.set_want();
    }
}
