//! Canvas drawings: the progress donut of the panel and the segments of the bar.
//! tiny-skia antialiases every path.

use std::f32::consts::{FRAC_PI_2, TAU};

use iced::widget::canvas::{self, Frame, Geometry, Path, path::Arc};
use iced::{Color, Point, Radians, Rectangle, Renderer, Size, Theme, mouse};

/// Done (accent) vs pending (track) as a donut with a hole of 58% of the radius.
pub struct Donut {
    pub value: f32,
    pub done: Color,
    pub pending: Color,
    pub hole: Color,
}

impl<Message> canvas::Program<Message> for Donut {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let radius = bounds.width.min(bounds.height) / 2.0;
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        frame.fill(&Path::circle(center, radius), self.pending);
        let value = self.value.clamp(0.0, 1.0);
        if value >= 0.9999 {
            frame.fill(&Path::circle(center, radius), self.done);
        } else if value > 0.0 {
            let start = -FRAC_PI_2;
            let wedge = Path::new(|b| {
                b.move_to(center);
                b.arc(Arc {
                    center,
                    radius,
                    start_angle: Radians(start),
                    end_angle: Radians(start + value * TAU),
                });
                b.close();
            });
            frame.fill(&wedge, self.done);
        }
        frame.fill(&Path::circle(center, radius * 0.58), self.hole);
        vec![frame.into_geometry()]
    }
}

/// The bar's segments (`▰▰▱▱`) as rounded pills.
pub struct Segments {
    pub filled: usize,
    pub total: usize,
    pub on: Color,
    pub off: Color,
}

pub const SEGMENT_W: f32 = 9.0;
pub const SEGMENT_GAP: f32 = 3.0;
pub const SEGMENT_H: f32 = 12.0;

impl Segments {
    pub fn width(total: usize) -> f32 {
        total as f32 * SEGMENT_W + total.saturating_sub(1) as f32 * SEGMENT_GAP
    }
}

impl<Message> canvas::Program<Message> for Segments {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let top = (bounds.height - SEGMENT_H) / 2.0;
        for i in 0..self.total {
            let x = i as f32 * (SEGMENT_W + SEGMENT_GAP);
            let pill =
                Path::rounded_rectangle(Point::new(x, top), Size::new(SEGMENT_W, SEGMENT_H), 2.5.into());
            if i < self.filled {
                frame.fill(&pill, self.on);
            } else {
                frame.stroke(
                    &pill,
                    canvas::Stroke::default().with_color(self.off).with_width(1.3),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

/// Segments filled for `done` of `total`, rounded like the Omarchy widget.
pub fn filled_segments(done: usize, total: usize, segments: usize) -> usize {
    let ratio = if total > 0 {
        done as f64 / total as f64
    } else {
        0.0
    };
    ((ratio * segments as f64 + 0.5).floor() as usize).min(segments)
}
