use std::io::Write;

use tinsel::app;
use tinsel::{Rgb, mix, palette};

// dark colours; tinsel flips them on light terminals
struct Colors {
    app: Rgb,
    faint: Rgb,
    bar: Rgb,
    ink: Rgb,
    selection: Rgb,
    search: Rgb,
    current_match: Rgb,
    bracket: Rgb,
    guide: Rgb,
    scrollbar: Rgb,
    word: Rgb,
    trailing: Rgb,
    picked: Rgb,
}

fn colors() -> Colors {
    let app = Rgb(23, 18, 27);
    let bar = Rgb(44, 33, 51);
    Colors {
        app,
        faint: Rgb(77, 67, 79),
        bar,
        ink: Rgb(23, 18, 27),
        selection: mix(app, ORANGE, 0.34),
        search: mix(app, palette::AMBER, 0.28),
        current_match: mix(app, palette::AMBER, 0.6),
        bracket: mix(app, palette::ROSE, 0.5),
        guide: mix(app, palette::LINE, 0.4),
        scrollbar: mix(app, palette::ROSE, 0.4),
        word: mix(app, palette::LINE, 0.5),
        trailing: mix(app, palette::RED, 0.3),
        picked: mix(bar, palette::ROSE, 0.3),
    }
}

const ORANGE: Rgb = Rgb(240, 140, 70);
