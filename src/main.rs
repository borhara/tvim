use tvim::editor::Editor;

fn go_to(editor: &mut Editor, start: &str) {
    let last = editor.buffer().last_row();
    if let Some(text) = start.strip_prefix('/') {
        let mut keys = tvim::keys::parse("G$/");
        editor.keys(&keys);
        return;
    }
    let row = if start.is_empty() {
        last
    } else {
        start.parse::<usize>().map_or(0, |n| n.saturating_sub(1)).min(last)
    };
    editor.keys(&tvim::keys::parse(&format!("{}G", row + 1)));
}
