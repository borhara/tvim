# tvim

tvim is a text editor for the terminal that uses vim's keys. If you already use vim, most of what your fingers do will just work. This page covers getting it running, what's different from vim, and the extra bits.

## Install

You need Rust. Then:

```sh
cargo install tinsel
```

## Opening files

```sh
tvim notes.md             # edit a file
tvim .                    # browse a folder in the sidebar
tvim                      # reopen whatever was open last time
tvim +42 src/main.rs      # start on line 42
tvim +/TODO src/lib.rs    # start on the first TODO
```

Files reopen at the line you left them on.

## Coming from vim

Normal, insert, replace and visual modes work as you'd expect, along with motions, operators, text objects, counts, registers, marks, macros, `.`, undo and redo, and the usual `:` commands (`:w`, `:q`, `:s`, `:e`, `:normal`, ranges and so on). A few things behave differently:

- **Search** uses regular expressions the way most other tools write them, so `(a|b)+` and `\d{3}` work without extra backslashes. Search ignores case unless you type a capital. Use `\c` or `\C` in the pattern to force it either way.
- **`Tab`** in normal mode moves between the file and the sidebar. Terminals send the same key for `Ctrl-i`, so straight after `Ctrl-o` it goes forward through jumps instead.
- **Brackets and quotes** close themselves as you type. Turn this off with `:set noap`.
- **Opening another file** never asks you to save first. Unsaved files stay open with a dot next to their name, and `:q` won't quit while any are left.
- **`gcc`** comments or uncomments a line, and `gc` works with a motion or in visual mode.

There is no line wrapping and no block visual mode (`Ctrl-v`) yet.

## Extras

`Space` is the leader key. Pause after pressing it, or after `d`, `c`, `y`, `g`, `z`, `"`, `@` or `'`, and a strip along the bottom shows what can come next.

| Keys | What it does |
| --- | --- |
| `Space f` or `Ctrl-p` | Find a file by name |
| `Space r` | Files you opened recently |
| `Space g` or `Space /` | Search every file in the project |
| `Space b` | Open files |
| `Space G` | Git menu |
| `Space m` | Markdown preview |
| `Space z` | Zen mode: just the text, centred |

**Sidebar.** `tvim .`, `Tab` or `:Lex` opens a folder tree beside the text. Use `j`/`k` to move, `Enter` to open, and `l`/`h` to open and close folders. `-` goes up a folder, `.` shows hidden files, `/` filters, and `q` hides the sidebar.

**Git.** The git menu lists staged, changed and untracked files with a diff beside them. `s` stages, `c` commits, `d` discards, `l` shows the log, `b` lists branches, and `p`/`P` pull and push. Lines you've changed since the last commit are marked in the gutter. The line under the cursor shows who last changed it and when, which `:set nogl` turns off.

**Several files.** Open files are listed along the top. Switch with `gt`/`gT`, `:b name` or `:bn`/`:bp`, and close one with `:bd`. `:wa` saves them all.

**Splits.** `Ctrl-w s` and `Ctrl-w v` (or `:sp` and `:vs`) split the window. `Ctrl-w` with `h`/`j`/`k`/`l` moves between windows and `Ctrl-w q` closes one.

**Colours.** Syntax colouring covers Rust, Python, JavaScript and TypeScript, Go, C and C++, Java, shell, Lua, TOML, YAML, JSON and Markdown. tvim uses its own colour scheme rather than your terminal's, and switches to a light one on a light background.

## Settings

Put `:set` lines in `~/.config/tvim/config`, one per line:

```
set tabstop=2 noexpandtab
set relativenumber
```

`:set` on its own lists every option with its current value. The ones you're most likely to want:

| Option | Short | Default | |
| --- | --- | --- | --- |
| `number` | `nu` | on | line numbers |
| `relativenumber` | `rnu` | off | count lines from the cursor |
| `zeropad` | `zp` | off | line numbers start with zeros, as in `01` |
| `tabstop` | `ts` | 4 | how wide a tab shows |
| `shiftwidth` | `sw` | 4 | how far `>>` and `<<` move |
| `expandtab` | `et` | on | `Tab` types spaces |
| `ignorecase` | `ic` | on | search ignores case |
| `smartcase` | `scs` | on | unless the search has capitals |
| `scrolloff` | `so` | 3 | lines kept above and below the cursor |
| `autopairs` | `ap` | on | brackets and quotes close themselves |
| `smooth` | `sm` | on | scroll smoothly on big jumps |
| `gitlens` | `gl` | on | show who last changed the cursor's line |
| `roomy` | `ro` | on | a little space above and below the text |
| `dim` | | on | fade the sidebar while you're in the file, and the other way round |

## Contributing

```sh
cargo test
cargo clippy --all-targets
```

The editor doesn't need a terminal to run, so tests type keys in vim's notation and check the result:

```rust
let mut e = Editor::with_text("one two");
e.keys(&tvim::keys::parse("cwhello<Esc>"));
assert_eq!(e.text(), "hello two");
```
