//! FT6336U capacitive touch support for the CoreS3.

use embedded_graphics::{geometry::Point, primitives::Rectangle};
use embedded_hal::i2c::I2c;

use crate::{devices, display::DisplayOrientation};

const REG_DEVICE_MODE: u8 = 0x00;
const REG_GESTURE_ID: u8 = 0x01;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gesture {
    None,
    MoveUp,
    MoveRight,
    MoveDown,
    MoveLeft,
    ZoomIn,
    ZoomOut,
    Unknown(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchEvent {
    pub id: u8,
    pub point: Point,
    pub phase: TouchPhase,
}

impl TouchEvent {
    pub fn hits(self, area: Rectangle) -> bool {
        area.contains(self.point)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchReport {
    pub gesture: Gesture,
    pub events: [Option<TouchEvent>; 2],
}

impl TouchReport {
    pub const fn empty() -> Self {
        Self {
            gesture: Gesture::None,
            events: [None, None],
        }
    }
}

pub struct Ft6336u<I2C> {
    i2c: I2C,
    address: u8,
    orientation: DisplayOrientation,
    width: u16,
    height: u16,
}

impl<I2C> Ft6336u<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::FT6336U_TOUCH,
            orientation: DisplayOrientation::Landscape,
            width: devices::display::WIDTH,
            height: devices::display::HEIGHT,
        }
    }

    pub fn set_orientation(&mut self, orientation: DisplayOrientation) {
        self.orientation = orientation;
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Ft6336u<I2C>
where
    I2C: I2c<Error = Error>,
{
    pub fn init(&mut self) -> Result<(), Error> {
        self.write_register(REG_DEVICE_MODE, 0x00)
    }

    pub fn read_report(&mut self) -> Result<TouchReport, Error> {
        let mut data = [0u8; 12];
        self.i2c
            .write_read(self.address, &[REG_GESTURE_ID], &mut data)?;
        let gesture = decode_gesture(data[0]);
        let count = (data[1] & 0x0F).min(2);
        let mut report = TouchReport {
            gesture,
            events: [None, None],
        };
        if count >= 1 {
            report.events[0] = decode_touch(&data[2..8], self.orientation, self.width, self.height);
        }
        if count >= 2 {
            report.events[1] =
                decode_touch(&data[8..12], self.orientation, self.width, self.height);
        }
        Ok(report)
    }

    pub fn read_register(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0u8];
        self.i2c.write_read(self.address, &[register], &mut value)?;
        Ok(value[0])
    }

    pub fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }
}

fn decode_touch(
    data: &[u8],
    orientation: DisplayOrientation,
    width: u16,
    height: u16,
) -> Option<TouchEvent> {
    if data.len() < 4 {
        return None;
    }
    let event = (data[0] >> 6) & 0x03;
    let x = (u16::from(data[0] & 0x0F) << 8) | u16::from(data[1]);
    let y = (u16::from(data[2] & 0x0F) << 8) | u16::from(data[3]);
    let id = data.get(2).copied().unwrap_or(0) >> 4;
    let phase = match event {
        0 => TouchPhase::Down,
        1 => TouchPhase::Up,
        2 => TouchPhase::Move,
        _ => TouchPhase::Move,
    };
    Some(TouchEvent {
        id,
        point: map_touch_point(
            Point::new(i32::from(x), i32::from(y)),
            orientation,
            width,
            height,
        ),
        phase,
    })
}

const fn decode_gesture(value: u8) -> Gesture {
    match value {
        0x00 => Gesture::None,
        0x10 => Gesture::MoveUp,
        0x14 => Gesture::MoveRight,
        0x18 => Gesture::MoveDown,
        0x1C => Gesture::MoveLeft,
        0x48 => Gesture::ZoomIn,
        0x49 => Gesture::ZoomOut,
        other => Gesture::Unknown(other),
    }
}

pub fn map_touch_point(
    point: Point,
    orientation: DisplayOrientation,
    width: u16,
    height: u16,
) -> Point {
    let w = i32::from(width);
    let h = i32::from(height);
    match orientation {
        DisplayOrientation::Landscape => point,
        DisplayOrientation::LandscapeInverted => Point::new(w - 1 - point.x, h - 1 - point.y),
        DisplayOrientation::Portrait => Point::new(point.y, w - 1 - point.x),
        DisplayOrientation::PortraitInverted => Point::new(h - 1 - point.y, point.x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::prelude::Size;

    #[test]
    fn maps_touch_for_all_rotations() {
        let p = Point::new(10, 20);
        assert_eq!(
            map_touch_point(p, DisplayOrientation::Landscape, 320, 240),
            p
        );
        assert_eq!(
            map_touch_point(p, DisplayOrientation::LandscapeInverted, 320, 240),
            Point::new(309, 219)
        );
        assert_eq!(
            map_touch_point(p, DisplayOrientation::Portrait, 320, 240),
            Point::new(20, 309)
        );
        assert_eq!(
            map_touch_point(p, DisplayOrientation::PortraitInverted, 320, 240),
            Point::new(219, 10)
        );
    }

    #[test]
    fn hit_tests_event() {
        let event = TouchEvent {
            id: 0,
            point: Point::new(12, 14),
            phase: TouchPhase::Down,
        };
        assert!(event.hits(Rectangle::new(Point::new(10, 10), Size::new(20, 20))));
    }
}
