mod panel;
mod sprite;

pub use panel::{
    BusConfig, Display, DisplayError, DisplayGeometry, DisplayOrientation, DisplayTransaction,
    DisplayTransactionError, DisplayTransferStats, LcdTransactionDevice, LcdTransactionError,
    LcdTransactionWriter, PanelConfig, PixelDataError,
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
