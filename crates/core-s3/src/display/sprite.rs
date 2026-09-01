use core::convert::Infallible;

use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    pixelcolor::PixelColor,
    primitives::Rectangle,
};
use heapless::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtySpriteError {
    BufferTooSmall,
    TooManyRegions,
}

/// Small fixed-capacity dirty rectangle set.
pub struct RegionSet<const MAX_REGIONS: usize> {
    regions: Vec<Rectangle, MAX_REGIONS>,
}

impl<const MAX_REGIONS: usize> RegionSet<MAX_REGIONS> {
    pub const fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.regions.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Rectangle> + '_ {
        self.regions.iter().copied()
    }

    pub fn add(&mut self, rect: Rectangle) -> Result<(), DirtySpriteError> {
        if rect.is_zero_sized() {
            return Ok(());
        }

        if let Some(existing) = self
            .regions
            .iter_mut()
            .find(|existing| intersects_or_touches(**existing, rect))
        {
            *existing = bounding_rect(*existing, rect);
            self.compact();
            return Ok(());
        }

        self.regions
            .push(rect)
            .map_err(|_| DirtySpriteError::TooManyRegions)
    }

    fn compact(&mut self) {
        let mut i = 0;
        while i < self.regions.len() {
            let mut j = i + 1;
            while j < self.regions.len() {
                if intersects_or_touches(self.regions[i], self.regions[j]) {
                    let merged = bounding_rect(self.regions[i], self.regions[j]);
                    self.regions[i] = merged;
                    self.regions.swap_remove(j);
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
}

impl<const MAX_REGIONS: usize> Default for RegionSet<MAX_REGIONS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Off-screen framebuffer that tracks the rectangles touched by draw calls.
///
/// Use a full-screen sprite (`W=320`, `H=240`) when RAM is available, or create
/// smaller sprites per widget. `N` must be at least `W * H`; it is separate from
/// `W`/`H` to stay on stable Rust without generic-const arithmetic.
pub struct DirtySprite<C, const W: u16, const H: u16, const N: usize, const MAX_REGIONS: usize>
where
    C: PixelColor + Copy + Default,
{
    pixels: [C; N],
    dirty: RegionSet<MAX_REGIONS>,
}

impl<C, const W: u16, const H: u16, const N: usize, const MAX_REGIONS: usize>
    DirtySprite<C, W, H, N, MAX_REGIONS>
where
    C: PixelColor + Copy + Default,
{
    pub fn new(clear: C) -> Result<Self, DirtySpriteError> {
        if N < usize::from(W) * usize::from(H) {
            return Err(DirtySpriteError::BufferTooSmall);
        }

        Ok(Self {
            pixels: [clear; N],
            dirty: RegionSet::new(),
        })
    }

    pub fn dirty_regions(&self) -> impl Iterator<Item = Rectangle> + '_ {
        self.dirty.iter()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    pub fn pixel(&self, point: Point) -> Option<C> {
        self.index(point).map(|idx| self.pixels[idx])
    }

    /// Returns pixels from a clipped sprite-local region in row-major order.
    pub fn region_pixels(&self, area: Rectangle) -> SpriteRegionPixels<'_, C> {
        let clipped = clip_to_bounds(area, W, H);
        SpriteRegionPixels {
            pixels: &self.pixels[..usize::from(W) * usize::from(H)],
            stride: usize::from(W),
            x: clipped.top_left.x.max(0) as usize,
            y: clipped.top_left.y.max(0) as usize,
            width: clipped.size.width as usize,
            height: clipped.size.height as usize,
            current_x: 0,
            current_y: 0,
        }
    }

    /// Draws a clipped sprite-local region into another draw target.
    pub fn draw_region_at<T>(
        &self,
        target: &mut T,
        source_area: Rectangle,
        dest_top_left: Point,
    ) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = C>,
    {
        let clipped = clip_to_bounds(source_area, W, H);
        if clipped.is_zero_sized() {
            return Ok(());
        }

        let offset = clipped.top_left - source_area.top_left;
        let target_area = Rectangle::new(dest_top_left + offset, clipped.size);
        target.fill_contiguous(&target_area, self.region_pixels(clipped))
    }

    pub fn set_pixel(&mut self, point: Point, color: C) -> Result<(), DirtySpriteError> {
        if let Some(idx) = self.index(point)
            && self.pixels[idx] != color
        {
            self.pixels[idx] = color;
            self.dirty.add(Rectangle::new(point, Size::new(1, 1)))?;
        }
        Ok(())
    }

    /// Repaint only dirty rectangles into a concrete display draw target.
    pub fn flush_dirty<T>(&mut self, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = C>,
    {
        self.flush_dirty_at(target, Point::zero())
    }

    /// Repaint only dirty rectangles into a concrete display draw target at `origin`.
    pub fn flush_dirty_at<T>(&mut self, target: &mut T, origin: Point) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = C>,
    {
        let mut regions = [None; MAX_REGIONS];
        for (slot, region) in regions.iter_mut().zip(self.dirty.iter()) {
            *slot = Some(region);
        }

        for region in regions.into_iter().flatten() {
            self.draw_region_at(target, region, origin + region.top_left)?;
        }
        self.clear_dirty();
        Ok(())
    }

    fn index(&self, point: Point) -> Option<usize> {
        if point.x < 0 || point.y < 0 || point.x >= i32::from(W) || point.y >= i32::from(H) {
            return None;
        }

        Some(point.y as usize * usize::from(W) + point.x as usize)
    }
}

impl<C, const W: u16, const H: u16, const N: usize, const MAX_REGIONS: usize> OriginDimensions
    for DirtySprite<C, W, H, N, MAX_REGIONS>
where
    C: PixelColor + Copy + Default,
{
    fn size(&self) -> Size {
        Size::new(u32::from(W), u32::from(H))
    }
}

impl<C, const W: u16, const H: u16, const N: usize, const MAX_REGIONS: usize> DrawTarget
    for DirtySprite<C, W, H, N, MAX_REGIONS>
where
    C: PixelColor + Copy + Default,
{
    type Color = C;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            let _ = self.set_pixel(point, color);
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.pixels[..usize::from(W) * usize::from(H)].fill(color);
        let _ = self.dirty.add(Rectangle::new(Point::zero(), self.size()));
        Ok(())
    }
}

/// Iterator over a dirty sprite region in row-major order.
pub struct SpriteRegionPixels<'a, C>
where
    C: PixelColor + Copy + Default,
{
    pixels: &'a [C],
    stride: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    current_x: usize,
    current_y: usize,
}

impl<C> Iterator for SpriteRegionPixels<'_, C>
where
    C: PixelColor + Copy + Default,
{
    type Item = C;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_y >= self.height {
            return None;
        }

        let index = (self.y + self.current_y) * self.stride + self.x + self.current_x;
        let color = self.pixels.get(index).copied();
        self.current_x += 1;
        if self.current_x >= self.width {
            self.current_x = 0;
            self.current_y += 1;
        }
        color
    }
}

fn clip_to_bounds(rect: Rectangle, width: u16, height: u16) -> Rectangle {
    let bounds = Rectangle::new(
        Point::zero(),
        Size::new(u32::from(width), u32::from(height)),
    );
    rect.intersection(&bounds)
}

fn intersects_or_touches(a: Rectangle, b: Rectangle) -> bool {
    let a_br = a.bottom_right().unwrap_or(a.top_left);
    let b_br = b.bottom_right().unwrap_or(b.top_left);

    a.top_left.x <= b_br.x + 1
        && a_br.x + 1 >= b.top_left.x
        && a.top_left.y <= b_br.y + 1
        && a_br.y + 1 >= b.top_left.y
}

fn bounding_rect(a: Rectangle, b: Rectangle) -> Rectangle {
    let a_br = a.bottom_right().unwrap_or(a.top_left);
    let b_br = b.bottom_right().unwrap_or(b.top_left);
    let min_x = a.top_left.x.min(b.top_left.x);
    let min_y = a.top_left.y.min(b.top_left.y);
    let max_x = a_br.x.max(b_br.x);
    let max_y = a_br.y.max(b_br.y);

    Rectangle::new(
        Point::new(min_x, min_y),
        Size::new((max_x - min_x + 1) as u32, (max_y - min_y + 1) as u32),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::PrimitiveStyle};

    #[test]
    fn tracks_dirty_regions_for_drawn_shapes() {
        let mut sprite = DirtySprite::<Rgb565, 8, 8, 64, 8>::new(Rgb565::BLACK).unwrap();
        Rectangle::new(Point::new(1, 2), Size::new(3, 4))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(&mut sprite)
            .unwrap();

        let regions: std::vec::Vec<_> = sprite.dirty_regions().collect();
        assert_eq!(
            regions,
            std::vec![Rectangle::new(Point::new(1, 2), Size::new(3, 4))]
        );
    }

    #[test]
    fn rejects_too_small_buffer() {
        let err = match DirtySprite::<Rgb565, 8, 8, 63, 8>::new(Rgb565::BLACK) {
            Ok(_) => panic!("expected buffer validation to fail"),
            Err(err) => err,
        };
        assert_eq!(err, DirtySpriteError::BufferTooSmall);
    }
}
