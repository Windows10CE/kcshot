use std::f64::consts::PI;

use cairo::{Context, Error as CairoError, ImageSurface};
use tracing::{error, info, warn};

mod data;
mod stack;

pub use data::*;
pub use stack::*;

#[derive(Clone, Debug)]
pub enum Operation {
    Finish,
    Crop(Rectangle),
    WindowSelect(Rectangle),
    Blur(Rectangle),
    Pixelate(Rectangle),
    DrawLine {
        start: Point,
        end: Point,
        colour: Colour,
    },
    DrawRectangle {
        rect: Rectangle,
        colour: Colour,
    },
    Text {
        text: String,
        border: Colour,
        fill: Colour,
    },
    DrawArrow {
        start: Point,
        end: Point,
        colour: Colour,
    },
    Highlight {
        rect: Rectangle,
    },
    DrawEllipse {
        ellipse: Ellipse,
        border: Colour,
        fill: Colour,
    },
}

impl Operation {
    #[allow(unused_variables)]
    pub fn execute(&self, surface: &mut ImageSurface, cairo: &Context) -> Result<(), CairoError> {
        match self {
            Operation::Finish => todo!(),
            Operation::Crop(_) => todo!(),
            Operation::WindowSelect(_) => todo!(),
            Operation::Blur(_) => todo!(),
            Operation::Pixelate(_) => todo!(),
            Operation::DrawLine { start, end, colour } => todo!(),
            Operation::DrawRectangle { rect, colour } => todo!(),
            Operation::Text { text, border, fill } => todo!(),
            Operation::DrawArrow { start, end, colour } => todo!(),
            Operation::Highlight { rect } => todo!(),
            Operation::DrawEllipse {
                ellipse,
                border,
                fill,
            } => {
                info!("Ellipse");
                cairo.save()?;

                cairo.save()?;
                // 1. Position our ellipse at (x, y)
                cairo.translate(ellipse.x, ellipse.y);
                // 2. Scale its x coordinates by w, and its y coordinates by h
                cairo.scale(ellipse.w, ellipse.h);
                // 3. Create it by faking a circle on [0,1]x[0,1] centered on (0.5, 0.5)
                cairo.arc(0.5, 0.5, 1.0, 0.0, 2.0 * PI);
                let (r, g, b, a) = fill.to_float_tuple();
                cairo.set_source_rgba(r, g, b, a);
                cairo.fill_preserve()?;
                cairo.restore()?;

                let (r, g, b, a) = border.to_float_tuple();
                cairo.set_source_rgba(r, g, b, a);
                // 4. Draw a border arround it
                cairo.stroke()?;

                cairo.restore()?;
            }
        };

        Ok(())
    }
}
