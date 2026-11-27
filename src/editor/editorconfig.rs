use super::Editor;

impl Editor {
    // your own indenting back, as it was before a buffer's .editorconfig changed it
    pub(crate) fn own_indenting(&mut self) {
        if let Some((expand, tab_stop, shift_width)) = self.own_indent.take() {
            (self.options.expand_tab, self.options.tab_stop, self.options.shift_width) =
                (expand, tab_stop, shift_width);
        }
    }

    // the buffer's .editorconfig indenting, while it's the one shown, and your own otherwise
    pub(crate) fn apply_editorconfig(&mut self) {
        self.own_indenting();
        let ec = &self.file.editorconfig;
        let (spaces, tab_stop, shift_width) = (ec.spaces, ec.tab_stop(), ec.shift_width());
        if spaces.is_none() && tab_stop.is_none() && shift_width.is_none() {
            return;
        }
        let o = &mut self.options;
        self.own_indent = Some((o.expand_tab, o.tab_stop, o.shift_width));
        o.expand_tab = spaces.unwrap_or(o.expand_tab);
        o.tab_stop = tab_stop.unwrap_or(o.tab_stop);
        o.shift_width = shift_width.unwrap_or(o.shift_width);
    }
}
