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

pub fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tvim-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}
