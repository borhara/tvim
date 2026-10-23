pub fn after(text: &str, spec: &str) -> (String, (usize, usize)) {
    let mut e = ed(text);
    e.keys(&keys::parse(spec));
    let Pos { row, col } = e.cursor();
    (e.text(), (row, col))
}

pub fn text_after(text: &str, spec: &str) -> String {
    after(text, spec).0
}

pub fn cursor_after(text: &str, spec: &str) -> (usize, usize) {
    after(text, spec).1
}
