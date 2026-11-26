//! `cargo run -p taskline-assets --bin social-preview` renders `assets/social-preview.png`
//! (1280x640, the GitHub social card) from `assets/logo.png`, headlessly with iced's
//! software renderer and the JetBrains Mono embedded in the app. It replaces the old
//! Pillow script, so the repository needs no Python.

use std::path::{Path, PathBuf};

use iced::font::{Family, Weight};
use iced::widget::canvas::{self, Frame, Geometry, Path as Shape, Stroke};
use iced::widget::{canvas as canvas_widget, container, image, pin, stack, text};
use iced::{Color, Element, Fill, Font, Point, Rectangle, Renderer, Size, Theme, mouse};
use iced_test::simulator::Simulator;

const MONO: Font = Font {
    family: Family::Name("JetBrains Mono"),
    ..Font::DEFAULT
};
const MONO_BOLD: Font = Font {
    weight: Weight::Bold,
    ..MONO
};
const BG: Color = Color::from_rgb8(0x1a, 0x1b, 0x26);
const FG: Color = Color::from_rgb8(0xc0, 0xca, 0xf5);
const ACCENT: Color = Color::from_rgb8(0x7a, 0xa2, 0xf7);
const TRACK: Color = Color::from_rgb8(0x56, 0x5f, 0x89);

/// The snapshot renders at twice the logical size: 640x320 logical -> 1280x640 pixels.
const SIZE: Size = Size::new(640.0, 320.0);

struct Segments;

impl canvas::Program<()> for Segments {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        for i in 0..10 {
            let pill = Shape::rounded_rectangle(
                Point::new(1.0 + i as f32 * 19.0, 1.0),
                Size::new(15.0, 11.0),
                2.5.into(),
            );
            if i < 3 {
                frame.fill(&pill, ACCENT);
            } else {
                frame.stroke(&pill, Stroke::default().with_color(TRACK).with_width(1.0));
            }
        }
        vec![frame.into_geometry()]
    }
}

fn logo(root: &Path) -> image::Handle {
    let decoder =
        png::Decoder::new(std::fs::File::open(root.join("assets/logo.png")).expect("assets/logo.png"));
    let mut reader = decoder.read_info().expect("logo.png");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("logo.png");
    buffer.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba, "logo.png is RGBA");
    image::Handle::from_rgba(info.width, info.height, buffer)
}

fn card(logo: image::Handle) -> Element<'static, ()> {
    let layers: Vec<Element<'static, ()>> = vec![
        container(text(""))
            .width(Fill)
            .height(Fill)
            .style(|_| container::Style {
                background: Some(BG.into()),
                ..Default::default()
            })
            .into(),
        pin(image(logo).width(150).height(150)).x(55.0).y(85.0).into(),
        pin(text("Taskline").size(48).font(MONO_BOLD).color(FG))
            .x(235.0)
            .y(92.0)
            .into(),
        pin(text("Your day, as one line.").size(20).font(MONO).color(ACCENT))
            .x(237.0)
            .y(162.0)
            .into(),
        pin(canvas_widget(Segments).width(192).height(13))
            .x(237.0)
            .y(205.0)
            .into(),
        pin(text("1/4").size(20).font(MONO).color(FG))
            .x(437.0)
            .y(198.0)
            .into(),
        pin(text("Omarchy · Windows · MIT").size(15).font(MONO).color(TRACK))
            .x(237.0)
            .y(234.0)
            .into(),
    ];
    stack(layers).width(Fill).height(Fill).into()
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("assets/social-preview.png"));
    let settings = iced::Settings {
        fonts: vec![
            include_bytes!("../../taskline-app/assets/fonts/JetBrainsMono-Regular.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../taskline-app/assets/fonts/JetBrainsMono-Bold.ttf")
                .as_slice()
                .into(),
        ],
        default_font: MONO,
        ..Default::default()
    };
    let mut ui = Simulator::with_size(settings, SIZE, card(logo(&root)));
    let snapshot = ui.snapshot(&Theme::Dark).expect("render");
    // iced_test writes `<name>-<renderer>.png`; render into a scratch folder, then
    // store it as an opaque RGB PNG at the destination.
    let scratch = std::env::temp_dir().join(format!("taskline-social-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    snapshot
        .matches_image(scratch.join("card"))
        .expect("write snapshot");
    let rendered = std::fs::read_dir(&scratch)
        .expect("snapshot")
        .flatten()
        .next()
        .expect("snapshot file")
        .path();
    let decoder = png::Decoder::new(std::fs::File::open(&rendered).expect("snapshot"));
    let mut reader = decoder.read_info().expect("snapshot");
    let mut rgba = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut rgba).expect("snapshot");
    let rgb: Vec<u8> = rgba[..info.buffer_size()]
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let file = std::io::BufWriter::new(std::fs::File::create(&out).expect("output file"));
    let mut encoder = png::Encoder::new(file, info.width, info.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_compression(png::Compression::Best);
    encoder
        .write_header()
        .expect("png")
        .write_image_data(&rgb)
        .expect("png");
    let _ = std::fs::remove_dir_all(&scratch);
    println!("{} ({}x{})", out.display(), info.width, info.height);
}
