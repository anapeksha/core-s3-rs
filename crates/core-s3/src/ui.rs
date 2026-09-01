//! Lightweight embedded-graphics widgets for CoreS3 examples and applications.

use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

use crate::{
    power::BatteryStatus,
    touch::{TouchEvent, TouchPhase},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub foreground: Rgb565,
    pub background: Rgb565,
    pub accent: Rgb565,
    pub disabled: Rgb565,
    pub warning: Rgb565,
}

impl Theme {
    pub const DARK: Self = Self {
        foreground: Rgb565::WHITE,
        background: Rgb565::BLACK,
        accent: Rgb565::CYAN,
        disabled: Rgb565::new(8, 16, 8),
        warning: Rgb565::YELLOW,
    };
}

pub trait Widget {
    fn bounds(&self) -> Rectangle;
    fn hit_test(&self, point: Point) -> bool {
        self.bounds().contains(point)
    }
}

pub trait InteractiveWidget: Widget {
    fn handle_touch(&mut self, event: TouchEvent) -> bool;
}

pub struct Label<'a> {
    pub text: &'a str,
    pub top_left: Point,
    pub color: Rgb565,
}

impl Label<'_> {
    pub fn draw<T>(&self, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        Text::new(
            self.text,
            self.top_left,
            MonoTextStyle::new(&FONT_6X10, self.color),
        )
        .draw(target)
        .map(|_| ())
    }
}

impl Widget for Label<'_> {
    fn bounds(&self) -> Rectangle {
        Rectangle::new(self.top_left, Size::new((self.text.len() as u32) * 6, 10))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Button<'a> {
    pub bounds: Rectangle,
    pub label: &'a str,
    pub pressed: bool,
}

impl Button<'_> {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        let fill = if self.pressed {
            theme.accent
        } else {
            theme.background
        };
        self.bounds
            .into_styled(PrimitiveStyle::with_fill(fill))
            .draw(target)?;
        self.bounds
            .into_styled(PrimitiveStyle::with_stroke(theme.foreground, 1))
            .draw(target)?;
        Text::with_alignment(
            self.label,
            Point::new(self.bounds.center().x, self.bounds.center().y + 4),
            MonoTextStyle::new(&FONT_6X10, theme.foreground),
            Alignment::Center,
        )
        .draw(target)
        .map(|_| ())
    }
}
impl Widget for Button<'_> {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}
impl InteractiveWidget for Button<'_> {
    fn handle_touch(&mut self, event: TouchEvent) -> bool {
        if !self.hit_test(event.point) {
            return false;
        }
        self.pressed = !matches!(event.phase, TouchPhase::Up);
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Toggle<'a> {
    pub bounds: Rectangle,
    pub label: &'a str,
    pub on: bool,
}

impl Toggle<'_> {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        Button {
            bounds: self.bounds,
            label: self.label,
            pressed: self.on,
        }
        .draw(target, theme)
    }
}
impl Widget for Toggle<'_> {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}
impl InteractiveWidget for Toggle<'_> {
    fn handle_touch(&mut self, event: TouchEvent) -> bool {
        if self.hit_test(event.point) && matches!(event.phase, TouchPhase::Up) {
            self.on = !self.on;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Slider {
    pub bounds: Rectangle,
    pub value: u8,
}

impl Slider {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        ProgressBar {
            bounds: self.bounds,
            value: self.value,
        }
        .draw(target, theme)
    }

    fn value_from_x(&self, x: i32) -> u8 {
        let rel = (x - self.bounds.top_left.x).clamp(0, self.bounds.size.width as i32);
        ((rel as u32 * 100) / self.bounds.size.width.max(1)) as u8
    }
}
impl Widget for Slider {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}
impl InteractiveWidget for Slider {
    fn handle_touch(&mut self, event: TouchEvent) -> bool {
        if self.hit_test(event.point) {
            self.value = self.value_from_x(event.point.x);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressBar {
    pub bounds: Rectangle,
    pub value: u8,
}

impl ProgressBar {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        self.bounds
            .into_styled(PrimitiveStyle::with_stroke(theme.foreground, 1))
            .draw(target)?;
        let inner = Rectangle::new(
            self.bounds.top_left + Point::new(2, 2),
            Size::new(
                self.bounds.size.width.saturating_sub(4),
                self.bounds.size.height.saturating_sub(4),
            ),
        );
        let fill_width = inner
            .size
            .width
            .saturating_mul(u32::from(self.value.min(100)))
            / 100;
        Rectangle::new(inner.top_left, Size::new(fill_width, inner.size.height))
            .into_styled(PrimitiveStyle::with_fill(theme.accent))
            .draw(target)
    }
}
impl Widget for ProgressBar {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryIndicator {
    pub bounds: Rectangle,
    pub status: BatteryStatus,
}

impl BatteryIndicator {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        let color = if self.status.low_battery {
            theme.warning
        } else {
            theme.accent
        };
        ProgressBar {
            bounds: self.bounds,
            value: self.status.percentage,
        }
        .draw(
            target,
            Theme {
                accent: color,
                ..theme
            },
        )
    }
}
impl Widget for BatteryIndicator {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}

pub struct StatusBar<'a> {
    pub bounds: Rectangle,
    pub text: &'a str,
}

impl StatusBar<'_> {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        self.bounds
            .into_styled(PrimitiveStyle::with_fill(theme.accent))
            .draw(target)?;
        Text::new(
            self.text,
            self.bounds.top_left + Point::new(4, 12),
            MonoTextStyle::new(&FONT_6X10, theme.background),
        )
        .draw(target)
        .map(|_| ())
    }
}
impl Widget for StatusBar<'_> {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}

pub struct Menu<'a, const N: usize> {
    pub bounds: Rectangle,
    pub items: [&'a str; N],
    pub selected: usize,
    pub row_height: u32,
}

impl<const N: usize> Menu<'_, N> {
    pub fn draw<T>(&self, target: &mut T, theme: Theme) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        for (index, item) in self.items.iter().enumerate() {
            let y = self.bounds.top_left.y + (index as i32 * self.row_height as i32);
            let row = Rectangle::new(
                Point::new(self.bounds.top_left.x, y),
                Size::new(self.bounds.size.width, self.row_height),
            );
            if index == self.selected {
                row.into_styled(PrimitiveStyle::with_fill(theme.accent))
                    .draw(target)?;
            }
            Text::new(
                item,
                row.top_left + Point::new(4, 12),
                MonoTextStyle::new(&FONT_6X10, theme.foreground),
            )
            .draw(target)?;
        }
        Ok(())
    }
}
impl<const N: usize> Widget for Menu<'_, N> {
    fn bounds(&self) -> Rectangle {
        self.bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_maps_touch_to_value() {
        let mut slider = Slider {
            bounds: Rectangle::new(Point::new(10, 0), Size::new(100, 20)),
            value: 0,
        };
        assert!(slider.handle_touch(TouchEvent {
            id: 0,
            point: Point::new(60, 10),
            phase: TouchPhase::Move
        }));
        assert_eq!(slider.value, 50);
    }

    #[test]
    fn toggle_flips_on_touch_up() {
        let mut toggle = Toggle {
            bounds: Rectangle::new(Point::zero(), Size::new(50, 20)),
            label: "x",
            on: false,
        };
        assert!(toggle.handle_touch(TouchEvent {
            id: 0,
            point: Point::new(2, 2),
            phase: TouchPhase::Up
        }));
        assert!(toggle.on);
    }
}
