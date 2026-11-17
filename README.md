# tvim

tvim is a text editor for the terminal that uses vim's keys. If you already use vim, most of what your fingers do will just work. This page covers getting it running, what's different from vim, and the extra bits.

## Install

You need Rust. From this folder:

```sh
cargo install --path .
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

