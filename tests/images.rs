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

#[test]
fn a_big_photo_shows_the_night_sky_until_its_decoded() {
    // the small JPEG with a megabyte of comment segments before its picture
    let small = include_bytes!("data/small.jpg");
    let mut big = small[..2].to_vec();
    for _ in 0..17 {
        big.extend([0xff, 0xfe, 0xff, 0xff]);
        big.extend([b'x'; 0xfffd]);
    }
    big.extend(&small[2..]);
    let e = open("big.jpg", &big);
    let kitty = Support::Yes(Protocol::Kitty);
    // decoding goes on another thread, so it may be done by the first frame or a few later
    let started = std::time::Instant::now();
    loop {
        let frame = drawn(&e, kitty);
        if !frame.images().is_empty() {
            break;
        }
        let screen = frame.snapshot().plain();
        assert!(screen.contains("loading big.jpg · 37×23 JPEG"), "{screen}");
        assert!(started.elapsed().as_secs() < 10, "never decoded");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_photo_from_a_phone_stands_the_way_it_was_taken() {
    let e = open("portrait.jpg", include_bytes!("data/rotated.jpg"));
    let image = e.image().unwrap();
    assert_eq!((image.width(), image.height()), (23, 37));
    let frame = drawn(&e, Support::Yes(Protocol::Kitty));
    assert!(frame.snapshot().plain().contains("portrait.jpg · 23×37 JPEG"));
    // tinsel fits it to the area by this size, so it goes up portrait
    let (_, shown) = &frame.images()[0];
    assert_eq!((shown.width(), shown.height()), (23, 37));
}

#[test]
fn the_newer_ways_of_changing_text_leave_a_picture_alone_too() {
    let mut e = open("untouched.png", &png(4, 4));
    for keys in [
        ":g/^/normal ihi<CR>",
        ":%!echo hi<CR>",
        ":r !echo hi<CR>",
        ":sort<CR>",
        ":t.<CR>",
        "gqq",
        "<A-j>",
        "yss)",
    ] {
        e.keys(&parse(keys));
        assert_eq!(e.text(), "", "{keys}");
        assert!(!e.is_modified(), "{keys}");
    }
}

#[test]
fn visual_mode_leaves_a_picture_and_the_registers_alone() {
    let mut e = open("chosen.png", &png(4, 4));
    let text = temp("yanked.txt");
    std::fs::write(&text, "kept\n").unwrap();
    e.keys(&parse(&format!(":e {}<CR>yy:bp<CR>", text.display())));
    assert!(e.image().is_some());
    for keys in [
        "vx",
        "Vd",
        "vD",
        "vs",
        "vI",
        "vA",
        "v<C-a>",
        "vrx",
        "vJ",
        "vp",
        "vgc",
        "V<A-j>",
        "<C-v>Ihi<Esc>",
    ] {
        e.keys(&parse(keys));
        assert_eq!(e.mode(), tvim::editor::Mode::Normal, "{keys}");
        assert_eq!(e.text(), "", "{keys}");
        assert!(!e.is_modified(), "{keys}");
        assert_eq!(e.register('"').unwrap().text, "kept", "{keys}");
    }
}

#[test]
fn ex_commands_that_would_change_a_picture_leave_the_registers_alone() {
    let mut e = open("ex.png", &png(4, 4));
    let text = temp("ex-yanked.txt");
    std::fs::write(&text, "kept\n").unwrap();
    e.keys(&parse(&format!(":e {}<CR>yy:bp<CR>", text.display())));
    for keys in [":d<CR>", ":g/^/d<CR>", ":m0<CR>", ":><CR>", ":1!sort<CR>"] {
        e.keys(&parse(keys));
        assert_eq!(e.text(), "", "{keys}");
        assert_eq!(e.register('"').unwrap().text, "kept", "{keys}");
        assert_eq!(e.register('1').map(|r| r.text.as_str()), None, "{keys}");
    }
}

#[test]
fn wq_closes_a_picture() {
    for keys in [":wq<CR>", ":wq!<CR>", ":x<CR>"] {
        let mut e = open("closed.png", &png(4, 4));
        e.keys(&parse(keys));
        assert!(e.should_quit(), "{keys}: {:?}", e.message());
    }
    let mut e = open("named.png", &png(4, 4));
    e.keys(&parse(&format!(":wq {}<CR>", temp("copy.png").display())));
    assert!(!e.should_quit(), "nor written under another name");
}

#[test]
fn text_that_starts_like_a_picture_is_still_text() {
    let e = open("notes.txt", b"GIF8 notes on the formats\n");
    assert!(e.image().is_none());
    assert_eq!(e.text(), "GIF8 notes on the formats");
}

#[test]
fn a_picture_waits_for_the_palette_only_where_its_drawn() {
    let mut e = open("small.png", &png(400, 300));
    e.keys(&parse(":"));
    let kitty = Support::Yes(Protocol::Kitty);
    let frame = with_support(kitty, || {
        tinsel::with_color_mode(tinsel::ColorMode::None, || {
            // too short for the palette, so : is typed in the status line, clear of it
            let mut frame = Frame::new(80, 7);
            draw(&e, &mut frame);
            frame
        })
    });
    assert_eq!(frame.images().len(), 1);
}
