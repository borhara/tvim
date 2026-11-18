use tinsel::app::Key;

/// The modes a mapping can be made for, as vim's n, x, i and c.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapMode {
    Normal,
    Visual,
    Insert,
    Command,
}

impl MapMode {
    fn letter(self) -> char {
        match self {
            MapMode::Normal => 'n',
            MapMode::Visual => 'x',
            MapMode::Insert => 'i',
            MapMode::Command => 'c',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub mode: MapMode,
    pub lhs: Vec<Key>,
    pub rhs: Vec<Key>,
    pub noremap: bool,
}

/// What a run of typed keys comes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// no mapping starts with these keys
    None,
    /// a longer mapping might still match, and `exact` is what to take if none does
    Prefix { exact: Option<Mapping> },
    /// a mapping matches and nothing longer could
    Exact(Mapping),
}

#[derive(Clone, Debug, Default)]
pub struct Maps {
    all: Vec<Mapping>,
}

impl Maps {
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    pub fn add(&mut self, m: Mapping) {
        self.all.retain(|o| !(o.mode == m.mode && o.lhs == m.lhs));
        self.all.push(m);
    }

    pub fn remove(&mut self, mode: MapMode, lhs: &[Key]) -> bool {
        let before = self.all.len();
        self.all.retain(|o| !(o.mode == mode && o.lhs == lhs));
        self.all.len() < before
    }

    pub fn clear(&mut self, mode: MapMode) {
        self.all.retain(|o| o.mode != mode);
    }

    pub fn has_mode(&self, mode: MapMode) -> bool {
        self.all.iter().any(|m| m.mode == mode)
    }

    pub fn lookup(&self, mode: MapMode, keys: &[Key]) -> Lookup {
        let mut exact = None;
        let mut longer = false;
        for m in self.all.iter().filter(|m| m.mode == mode && m.lhs.starts_with(keys)) {
            if m.lhs.len() == keys.len() {
                exact = Some(m.clone());
            } else {
                longer = true;
            }
        }
        match (exact, longer) {
            (None, false) => Lookup::None,
            (exact, true) => Lookup::Prefix { exact },
            (Some(m), false) => Lookup::Exact(m),
        }
    }

    /// mappings for any of `modes` whose keys start with `prefix`, sorted for listing
    pub fn list(&self, modes: &[MapMode], prefix: &[Key]) -> Vec<&Mapping> {
        let mut found: Vec<&Mapping> = self
            .all
            .iter()
            .filter(|m| modes.contains(&m.mode) && m.lhs.starts_with(prefix))
            .collect();
        found.sort_by_key(|m| (show_keys(&m.lhs), m.mode.letter()));
        found
    }
}

/// Keys in vim's notation, with `<leader>` as space and `<Nop>` as nothing.
pub fn parse_keys(spec: &str) -> Vec<Key> {
    let mut out = String::new();
    let mut rest = spec;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let lower = tail.to_ascii_lowercase();
        if lower.starts_with("<leader>") {
            out.push_str("<Space>");
            rest = &tail["<leader>".len()..];
        } else if lower.starts_with("<nop>") {
            rest = &tail["<nop>".len()..];
        } else {
            out.push('<');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    crate::keys::parse(&out)
}

pub fn show_keys(keys: &[Key]) -> String {
    keys.iter()
        .map(|k| match k {
            Key::Char(' ') => "<Space>".to_string(),
            k => k.to_string(),
        })
        .collect()
}

/// One line of `:map`'s listing, as vim shows it: the mode, the keys, `*` when not remapped,
/// then what they do.
pub fn describe(m: &Mapping) -> String {
    let star = if m.noremap { "*" } else { " " };
    let rhs = if m.rhs.is_empty() {
        "<Nop>".to_string()
    } else {
        show_keys(&m.rhs)
    };
    format!("{}  {} {star}{rhs}", m.mode.letter(), show_keys(&m.lhs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(mode: MapMode, lhs: &str, rhs: &str) -> Mapping {
        Mapping {
            mode,
            lhs: parse_keys(lhs),
            rhs: parse_keys(rhs),
            noremap: false,
        }
    }

    #[test]
    fn leader_is_space_and_nop_is_nothing() {
        assert_eq!(parse_keys("<leader>w"), [Key::Char(' '), Key::Char('w')]);
        assert_eq!(parse_keys("<Leader>"), [Key::Char(' ')]);
        assert_eq!(parse_keys("<Nop>"), []);
        assert_eq!(parse_keys("a<b"), [Key::Char('a'), Key::Char('<'), Key::Char('b')]);
        assert_eq!(parse_keys("<C-s>"), [Key::Ctrl('s')]);
    }

    #[test]
    fn lookups_wait_while_a_longer_mapping_could_match() {
        let mut maps = Maps::default();
        maps.add(map(MapMode::Insert, "jk", "<Esc>"));
        maps.add(map(MapMode::Normal, "g", "x"));
        maps.add(map(MapMode::Normal, "gx", "y"));
        let k = parse_keys;
        assert_eq!(maps.lookup(MapMode::Insert, &k("j")), Lookup::Prefix { exact: None });
        assert!(matches!(maps.lookup(MapMode::Insert, &k("jk")), Lookup::Exact(_)));
        assert_eq!(maps.lookup(MapMode::Insert, &k("jj")), Lookup::None);
        assert_eq!(
            maps.lookup(MapMode::Normal, &k("j")),
            Lookup::None,
            "modes are kept apart"
        );
        assert!(matches!(
            maps.lookup(MapMode::Normal, &k("g")),
            Lookup::Prefix { exact: Some(m) } if m.rhs == k("x")
        ));
    }

    #[test]
    fn mapping_the_same_keys_again_replaces_the_old_one() {
        let mut maps = Maps::default();
        maps.add(map(MapMode::Normal, "Q", "a"));
        maps.add(map(MapMode::Normal, "Q", "b"));
        maps.add(map(MapMode::Visual, "Q", "c"));
        let both = [MapMode::Normal, MapMode::Visual];
        assert_eq!(maps.list(&both, &[]).len(), 2);
        assert!(maps.remove(MapMode::Normal, &parse_keys("Q")));
        assert!(!maps.remove(MapMode::Normal, &parse_keys("Q")));
        assert_eq!(describe(maps.list(&both, &[])[0]), "x  Q  c");
    }
}
