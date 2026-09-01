mod panel;
mod sprite;

pub use panel::{
    BusConfig, Display, DisplayError, DisplayGeometry, DisplayOrientation, PanelConfig,
};
pub use sprite::{DirtySprite, DirtySpriteError, RegionSet};

/// CoreS3 native panel dimensions in landscape orientation.
pub const WIDTH: u16 = crate::devices::display::WIDTH;
pub const HEIGHT: u16 = crate::devices::display::HEIGHT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Orientation {
    Landscape,
    Portrait,
    LandscapeInverted,
    PortraitInverted,
}
