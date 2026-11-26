mod common;
use common::temp;
use tinsel::app::Frame;
use tinsel::image::{Protocol, Support, with_support};
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::draw;

// a PNG's header, which is all tvim reads: the terminal decodes the rest
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend([8, 6, 0, 0, 0]);
    b
}

fn open(name: &str, bytes: &[u8]) -> Editor {
    let path = temp(name);
    std::fs::write(&path, bytes).unwrap();
    let mut e = Editor::open(&path).unwrap();
    e.set_clipboard(Box::new(tvim::clipboard::Memory::default()));
    e
}

fn drawn(e: &Editor, support: Support) -> Frame {
    with_support(support, || {
        tinsel::with_color_mode(tinsel::ColorMode::None, || {
            let mut frame = Frame::new(80, 24);
            draw(e, &mut frame);
            frame
        })
    })
}

#[test]
fn a_picture_opens_as_one() {
    let e = open("pic.png", &png(1228, 160));
    let image = e.image().expect("an image");
    assert_eq!((image.width(), image.height()), (1228, 160));
    assert_eq!(e.text(), "");
    assert!(e.message().unwrap().text.ends_with("pic.png\" 1228×160 PNG"));
    assert!(!e.is_modified());
}

#[test]
fn other_files_that_arent_text_say_so_once() {
    let path = temp("blob.bin");
    std::fs::write(&path, [0xff, 0xfe, 0x00, 0x9f]).unwrap();
    let mut e = common::ed("");
    e.ex(&format!("e {}", path.display()));
    assert_eq!(
        e.message().unwrap().text,
        format!("\"{}\" isn't UTF-8 text", path.display())
    );
}

#[test]
fn a_picture_cant_be_changed() {
    let bytes = png(4, 4);
    let mut e = open("still.png", &bytes);
    for keys in [
        "dd",
        "ihello<Esc>",
        "otext<Esc>",
        "p",
        ":s/^/x/<CR>",
        ":normal ihi<CR>",
        ":r !echo hi<CR>",
    ] {
        e.keys(&parse(keys));
        assert_eq!(e.text(), "", "{keys}");
        assert!(!e.is_modified(), "{keys}");
    }
    e.paste("pasted");
    assert_eq!(e.text(), "");
    e.keys(&parse(":w<CR>"));
    assert!(
        e.message()
            .unwrap()
            .text
            .contains("is an image, which tvim shows but doesn't write")
    );
    e.keys(&parse(":w!<CR>"));
    assert_eq!(
        std::fs::read(temp("still.png")).unwrap(),
        bytes,
        "the file is as it was"
    );
}

#[test]
fn moving_around_a_picture_still_works() {
    let mut e = open("look.png", &png(4, 4));
    e.keys(&parse("jjGgg:set nu<CR>"));
    assert!(e.message().is_none_or(|m| !m.error));
}

#[test]
fn kitty_and_iterm_get_the_picture() {
    let e = open("shown.png", &png(400, 300));
    for protocol in [Protocol::Kitty, Protocol::Iterm] {
        let frame = drawn(&e, Support::Yes(protocol));
        assert_eq!(frame.images().len(), 1, "{protocol:?}");
        let (area, image) = &frame.images()[0];
        assert_eq!((image.width(), image.height()), (400, 300));
        assert!(area.width > 0 && area.height > 0);
        assert!(
            frame.snapshot().plain().contains("shown.png · 400×300 PNG"),
            "with a line saying what it is"
        );
    }
}

#[test]
fn other_terminals_say_they_cant() {
    let e = open("hidden.png", &png(400, 300));
    let frame = drawn(&e, Support::No);
    assert!(frame.images().is_empty());
    let screen = frame.snapshot().plain();
    assert!(screen.contains("hidden.png · 400×300 PNG"), "{screen}");
    assert!(screen.contains("this terminal can't show images"), "{screen}");
}

#[test]
fn kitty_shows_jpegs_too() {
    let e = open("photo.jpg", include_bytes!("data/small.jpg"));
    for protocol in [Protocol::Kitty, Protocol::Iterm] {
        let frame = drawn(&e, Support::Yes(protocol));
        assert_eq!(frame.images().len(), 1, "{protocol:?}");
        assert!(frame.snapshot().plain().contains("photo.jpg · 37×23 JPEG"));
    }
}

#[test]
fn a_broken_jpeg_says_why_in_kitty() {
    // a JPEG's header up to the size in its frame header, and nothing after
    let e = open("broken.jpg", b"\xff\xd8\xff\xc0\x00\x11\x08\x00\x10\x00\x20");
    let frame = drawn(&e, Support::Yes(Protocol::Kitty));
    assert!(frame.images().is_empty());
    let screen = frame.snapshot().plain();
    assert!(screen.contains("can't read it"), "{screen}");
}

#[test]
fn a_picture_waits_while_something_covers_it() {
    let mut e = open("covered.png", &png(400, 300));
    let kitty = Support::Yes(Protocol::Kitty);
    e.keys(&parse(":"));
    assert!(drawn(&e, kitty).images().is_empty(), "under the command palette");
    e.keys(&parse("<Esc>"));
    assert_eq!(drawn(&e, kitty).images().len(), 1, "and back once it's gone");
}

#[test]
fn a_picture_in_another_window_shows_too() {
    let mut e = open("split.png", &png(400, 300));
    e.keys(&parse(":vnew<CR>"));
    assert_eq!(drawn(&e, Support::Yes(Protocol::Kitty)).images().len(), 1);
}
