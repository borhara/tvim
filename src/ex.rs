use crate::buffer::Pos;
use crate::editor::Editor;

impl Editor {
    fn show_registers(&mut self) {
        if all.is_empty() {
            return self.info("No registers set");
        }
        let shown: Vec<String> = all
            .iter()
            .map(|(k, r)| {
                let text: String = r.text.replace('\n', "⏎").chars().take(24).collect();
                format!("\"{k} {text}")
            })
            .collect();
        self.info(shown.join("  "));
    }

    fn show_marks(&mut self) {
        let mut marks: Vec<(char, Pos)> = self.marks().iter().map(|(k, v)| (*k, *v)).collect();
        if marks.is_empty() {
            return self.info("No marks set");
        }
        marks.sort();
        let shown: Vec<String> = marks
            .iter()
            .map(|(k, p)| format!("{k} {}:{}", p.row + 1, p.col + 1))
            .collect();
        self.info(shown.join("  "));
    }
}
