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
    InvalidRegionCapacity,
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
        if MAX_REGIONS == 0 {
            return Err(DirtySpriteError::InvalidRegionCapacity);
        }

        let mut merged = rect;
        let mut index = 0;
        while index < self.regions.len() {
            if intersects_or_touches(self.regions[index], merged) {
                merged = bounding_rect(self.regions.remove(index), merged);
                // The enlarged rectangle can now touch an earlier region.
                index = 0;
            } else {
                index += 1;
            }
        }

        if self.regions.push(merged).is_err() {
            let mut all = merged;
            for region in self.regions.iter().copied() {
                all = bounding_rect(all, region);
            }
            self.regions.clear();
            // MAX_REGIONS was checked above, so this cannot fail.
            let _ = self.regions.push(all);
        }
        self.regions.as_mut_slice().sort_unstable_by_key(|region| {
            (
                region.top_left.y,
                region.top_left.x,
                region.size.height,
                region.size.width,
            )
        });
        Ok(())
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
        if MAX_REGIONS == 0 {
            return Err(DirtySpriteError::InvalidRegionCapacity);
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

    /// Marks the portion of `area` inside the sprite as dirty.
    pub fn invalidate(&mut self, area: Rectangle) -> Result<(), DirtySpriteError> {
        self.dirty.add(clip_to_bounds(area, W, H))
    }

    /// Marks the entire sprite as dirty.
    pub fn invalidate_all(&mut self) -> Result<(), DirtySpriteError> {
        self.invalidate(Rectangle::new(Point::zero(), self.size()))
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
            self.dirty.add(Rectangle::new(point, Size::new(1, 1)))?;
            self.pixels[idx] = color;
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
        // Construction guarantees a nonzero region capacity, so invalidation is infallible.
        let _ = self.invalidate_all();
        self.pixels[..usize::from(W) * usize::from(H)].fill(color);
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

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(width, height))
    }

    fn regions<const N: usize>(set: &RegionSet<N>) -> std::vec::Vec<Rectangle> {
        set.iter().collect()
    }

    #[test]
    fn keeps_non_overlapping_regions_separate() {
        let mut set = RegionSet::<4>::new();
        set.add(rect(0, 0, 1, 1)).unwrap();
        set.add(rect(3, 3, 1, 1)).unwrap();
        assert_eq!(regions(&set), std::vec![rect(0, 0, 1, 1), rect(3, 3, 1, 1)]);
    }

    #[test]
    fn merges_touching_intersecting_and_transitively_connected_regions() {
        let mut set = RegionSet::<5>::new();
        set.add(rect(0, 0, 2, 2)).unwrap();
        set.add(rect(5, 0, 2, 2)).unwrap();
        set.add(rect(1, 1, 2, 2)).unwrap();
        assert_eq!(regions(&set), std::vec![rect(0, 0, 3, 3), rect(5, 0, 2, 2)]);

        set.add(rect(3, 1, 2, 1)).unwrap();
        assert_eq!(regions(&set), std::vec![rect(0, 0, 7, 3)]);
    }

    #[test]
    fn overflow_collapses_every_region_and_new_rectangle_to_a_bounding_box() {
        let mut set = RegionSet::<2>::new();
        set.add(rect(0, 1, 1, 1)).unwrap();
        set.add(rect(4, 4, 1, 1)).unwrap();
        set.add(rect(8, 0, 2, 1)).unwrap();
        assert_eq!(regions(&set), std::vec![rect(0, 0, 10, 5)]);
    }

    #[test]
    fn rejects_zero_region_capacity() {
        let err = match DirtySprite::<Rgb565, 2, 2, 4, 0>::new(Rgb565::BLACK) {
            Ok(_) => panic!("expected region capacity validation to fail"),
            Err(err) => err,
        };
        assert_eq!(err, DirtySpriteError::InvalidRegionCapacity);
        assert_eq!(
            RegionSet::<0>::new().add(rect(0, 0, 1, 1)),
            Err(DirtySpriteError::InvalidRegionCapacity)
        );
    }

    #[test]
    fn invalidate_clips_every_edge_and_ignores_outside_and_empty_areas() {
        let cases = [
            (rect(-2, 1, 4, 2), Some(rect(0, 1, 2, 2))),
            (rect(3, 1, 4, 2), Some(rect(3, 1, 1, 2))),
            (rect(1, -2, 2, 4), Some(rect(1, 0, 2, 2))),
            (rect(1, 3, 2, 4), Some(rect(1, 3, 2, 1))),
            (rect(-5, -5, 2, 2), None),
            (rect(1, 1, 0, 3), None),
        ];

        for (area, expected) in cases {
            let mut sprite = DirtySprite::<Rgb565, 4, 4, 16, 4>::new(Rgb565::BLACK).unwrap();
            sprite.invalidate(area).unwrap();
            assert_eq!(
                sprite.dirty_regions().collect::<std::vec::Vec<_>>(),
                expected.into_iter().collect::<std::vec::Vec<_>>()
            );
        }

        let mut sprite = DirtySprite::<Rgb565, 4, 4, 16, 1>::new(Rgb565::BLACK).unwrap();
        sprite.invalidate_all().unwrap();
        assert_eq!(
            sprite.dirty_regions().collect::<std::vec::Vec<_>>(),
            std::vec![rect(0, 0, 4, 4)]
        );
    }

    #[test]
    fn merging_is_deterministic_across_insertion_orders() {
        let inputs = [rect(0, 0, 2, 2), rect(5, 0, 2, 2), rect(2, 0, 3, 2)];
        let mut forward = RegionSet::<4>::new();
        let mut reverse = RegionSet::<4>::new();
        for area in inputs {
            forward.add(area).unwrap();
        }
        for area in inputs.into_iter().rev() {
            reverse.add(area).unwrap();
        }
        assert_eq!(regions(&forward), std::vec![rect(0, 0, 7, 2)]);
        assert_eq!(regions(&forward), regions(&reverse));
    }

    #[test]
    fn tracks_dirty_regions_for_drawn_shapes() {
        let mut sprite = DirtySprite::<Rgb565, 8, 8, 64, 8>::new(Rgb565::BLACK).unwrap();
        Rectangle::new(Point::new(1, 2), Size::new(3, 4))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(&mut sprite)
            .unwrap();

        assert_eq!(
            sprite.dirty_regions().collect::<std::vec::Vec<_>>(),
            std::vec![rect(1, 2, 3, 4)]
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

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TargetError {
        WriteFailed,
    }

    struct TestTarget {
        fail_on_call: Option<usize>,
        calls: usize,
        pixels: std::vec::Vec<Pixel<Rgb565>>,
    }

    impl TestTarget {
        fn successful() -> Self {
            Self {
                fail_on_call: None,
                calls: 0,
                pixels: std::vec::Vec::new(),
            }
        }
    }

    impl OriginDimensions for TestTarget {
        fn size(&self) -> Size {
            Size::new(8, 8)
        }
    }

    impl DrawTarget for TestTarget {
        type Color = Rgb565;
        type Error = TargetError;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            self.calls += 1;
            if self.fail_on_call == Some(self.calls) {
                return Err(TargetError::WriteFailed);
            }
            self.pixels.extend(pixels);
            Ok(())
        }
    }

    #[test]
    fn successful_flush_clears_dirty_state() {
        let mut sprite = DirtySprite::<Rgb565, 4, 4, 16, 4>::new(Rgb565::BLACK).unwrap();
        sprite.set_pixel(Point::new(1, 1), Rgb565::WHITE).unwrap();
        let mut target = TestTarget::successful();

        sprite.flush_dirty(&mut target).unwrap();

        assert!(sprite.dirty_regions().next().is_none());
        assert_eq!(
            target.pixels,
            std::vec![Pixel(Point::new(1, 1), Rgb565::WHITE)]
        );
    }

    #[test]
    fn failed_flush_preserves_all_dirty_state_for_retry() {
        let mut sprite = DirtySprite::<Rgb565, 4, 4, 16, 4>::new(Rgb565::BLACK).unwrap();
        sprite.set_pixel(Point::new(0, 0), Rgb565::WHITE).unwrap();
        sprite.set_pixel(Point::new(3, 3), Rgb565::WHITE).unwrap();
        let expected = sprite.dirty_regions().collect::<std::vec::Vec<_>>();
        let mut failing = TestTarget {
            fail_on_call: Some(2),
            calls: 0,
            pixels: std::vec::Vec::new(),
        };

        assert_eq!(
            sprite.flush_dirty(&mut failing),
            Err(TargetError::WriteFailed)
        );
        assert_eq!(
            sprite.dirty_regions().collect::<std::vec::Vec<_>>(),
            expected
        );

        let mut retry = TestTarget::successful();
        sprite.flush_dirty(&mut retry).unwrap();
        assert!(sprite.dirty_regions().next().is_none());
        assert_eq!(retry.pixels.len(), 2);
    }
}
