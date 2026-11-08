#![no_std]
#![forbid(unsafe_code)]
//! Offline-tested LCD stream profile for the related RT880 1.16 image.
//!
//! This is not a qualified UV98 hardware driver. A board adapter must implement
//! reset, command/data selection, bus synchronization, and accurate delays.
//! The default profile preserves stock MADCTL=0xA8 (including BGR). The explicit
//! portrait profile uses MADCTL=0x08 and a 240x320 logical window. Colors are raw
//! 565 words sent most-significant byte first. Physical color order is unverified.
//! Initialization preserves the observed control prefix, then uses a complete
//! address window and one black clear before DisplayOn. It deliberately does
//! not reproduce the stock's partial cursor writes and redundant second clear.

pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 240;
pub const MADCTL: u8 = 0xA8;
pub const PORTRAIT_WIDTH: u16 = 240;
pub const PORTRAIT_HEIGHT: u16 = 320;
pub const PORTRAIT_MADCTL: u8 = 0x08;
/// Maximum temporary pixel staging, independent of rectangle dimensions.
pub const PIXEL_BUFFER_BYTES: usize = 64;

/// Synchronous logical LCD bus. Successful return means bytes have completed.
///
/// The adapter owns CS/DC and must preserve the command/data distinction.
/// Implementations must not return success while DMA still borrows `bytes`.
/// `reset(true)` means the physical active-low RESET line is driven high.
pub trait DisplayBus {
    type Error;
    fn reset(&mut self, high: bool) -> Result<(), Self::Error>;
    fn command(&mut self, command: u8) -> Result<(), Self::Error>;
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
    /// Synchronous RGB565 pixel stream, high byte first with no reinterpretation.
    /// A transport may override this to keep CS asserted for the entire slice.
    /// The fallback preserves the historical bounded 64-byte staging protocol.
    /// Success must mean every serial bit completed; no slice may be retained.
    fn pixels_rgb565(&mut self, pixels: &[u16]) -> Result<(), Self::Error> {
        let mut buffer = [0u8; PIXEL_BUFFER_BYTES];
        for chunk in pixels.chunks(PIXEL_BUFFER_BYTES / 2) {
            for (index, pixel) in chunk.iter().enumerate() {
                let bytes = pixel.to_be_bytes();
                buffer[2 * index] = bytes[0];
                buffer[2 * index + 1] = bytes[1];
            }
            self.data(&buffer[..chunk.len() * 2])?;
        }
        Ok(())
    }
}

pub trait Delay {
    fn delay_ms(&mut self, ms: u32);
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    Bus(E),
    NotInitialized,
    InvalidRectangle,
    WindowInProgress,
    NoWindow,
    TooManyPixels,
    IncompletePixels { remaining: u32 },
}

#[derive(Clone, Copy)]
enum Profile {
    Landscape,
    Portrait,
}

pub struct Display<B, D> {
    bus: B,
    delay: D,
    profile: Profile,
    initialized: bool,
    window_active: bool,
    remaining: u32,
}

impl<B: DisplayBus, D: Delay> Display<B, D> {
    /// Constructs an inert driver. No bus access occurs until initialization.
    /// The historical landscape profile remains 320x240, MADCTL=0xA8.
    pub const fn new(bus: B, delay: D) -> Self {
        Self::with_profile(bus, delay, Profile::Landscape)
    }

    /// Constructs an inert 240x320 portrait driver, MADCTL=0x08. This selects a
    /// logical orientation, not a claim that a physical panel was inspected.
    /// Timing, SPI configuration and color-byte order remain unchanged.
    pub const fn new_portrait(bus: B, delay: D) -> Self {
        Self::with_profile(bus, delay, Profile::Portrait)
    }

    const fn with_profile(bus: B, delay: D, profile: Profile) -> Self {
        Self {
            bus,
            delay,
            profile,
            initialized: false,
            window_active: false,
            remaining: 0,
        }
    }

    pub const fn width(&self) -> u16 {
        match self.profile {
            Profile::Landscape => WIDTH,
            Profile::Portrait => PORTRAIT_WIDTH,
        }
    }

    pub const fn height(&self) -> u16 {
        match self.profile {
            Profile::Landscape => HEIGHT,
            Profile::Portrait => PORTRAIT_HEIGHT,
        }
    }

    pub const fn madctl(&self) -> u8 {
        match self.profile {
            Profile::Landscape => MADCTL,
            Profile::Portrait => PORTRAIT_MADCTL,
        }
    }

    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub const fn remaining_pixels(&self) -> u32 {
        self.remaining
    }
    pub fn into_parts(self) -> (B, D) {
        (self.bus, self.delay)
    }

    fn invalidate(&mut self) {
        self.initialized = false;
        self.window_active = false;
        self.remaining = 0;
    }

    fn bus_result(&mut self, result: Result<(), B::Error>) -> Result<(), Error<B::Error>> {
        result.map_err(|error| {
            self.invalidate();
            Error::Bus(error)
        })
    }

    fn command(&mut self, command: u8) -> Result<(), Error<B::Error>> {
        let result = self.bus.command(command);
        self.bus_result(result)
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), Error<B::Error>> {
        let result = self.bus.data(bytes);
        self.bus_result(result)
    }

    fn reset(&mut self, high: bool) -> Result<(), Error<B::Error>> {
        let result = self.bus.reset(high);
        self.bus_result(result)
    }

    fn register(&mut self, command: u8, data: &[u8]) -> Result<(), Error<B::Error>> {
        self.command(command)?;
        if !data.is_empty() {
            self.data(data)?;
        }
        Ok(())
    }

    /// Reset, observed stock control prefix, canonical full-window clear, on.
    /// Any bus failure invalidates the driver; initialize again before drawing.
    /// Calling initialize explicitly abandons any unfinished pixel stream.
    pub fn initialize(&mut self) -> Result<(), Error<B::Error>> {
        self.invalidate();
        self.reset(true)?;
        self.delay.delay_ms(1);
        self.reset(false)?;
        self.delay.delay_ms(1);
        self.reset(true)?;
        self.delay.delay_ms(120);
        self.command(0x11)?;
        self.delay.delay_ms(120);
        self.register(0x36, &[self.madctl()])?;
        self.register(0x3A, &[0x05])?;
        self.register(0xB2, &[0x0C, 0x0C, 0x00, 0x33, 0x33])?;
        self.register(0xB7, &[0x35])?;
        self.register(0xBB, &[0x2B])?;
        self.register(0xC0, &[0x2C])?;
        self.register(0xC2, &[0x01])?;
        self.register(0xC3, &[0x23])?;
        self.register(0xC4, &[0x20])?;
        self.register(0xC6, &[0x0F])?;
        self.register(0xD0, &[0xA4, 0xA1])?;
        self.register(
            0xE0,
            &[
                0xD0, 0x00, 0x05, 0x0E, 0x15, 0x0D, 0x37, 0x43, 0x47, 0x09, 0x15, 0x12, 0x16, 0x19,
            ],
        )?;
        self.register(
            0xE1,
            &[
                0xD0, 0x00, 0x05, 0x0D, 0x0C, 0x06, 0x2D, 0x44, 0x40, 0x0E, 0x1C, 0x18, 0x16, 0x19,
            ],
        )?;
        // Stock preserves this bit despite its unresolved physical SPI2EN use.
        self.register(0xE7, &[0x10])?;
        self.window(0, 0, self.width() - 1, self.height() - 1)?;
        self.repeat(0, u32::from(self.width()) * u32::from(self.height()))?;
        self.command(0x29)?;
        self.initialized = true;
        Ok(())
    }

    fn window(&mut self, x: u16, y: u16, end_x: u16, end_y: u16) -> Result<(), Error<B::Error>> {
        let xs = x.to_be_bytes();
        let xe = end_x.to_be_bytes();
        let ys = y.to_be_bytes();
        let ye = end_y.to_be_bytes();
        self.register(0x2A, &[xs[0], xs[1], xe[0], xe[1]])?;
        self.register(0x2B, &[ys[0], ys[1], ye[0], ye[1]])?;
        self.command(0x2C)
    }

    /// Start a bounded logical window. Ends are derived inclusively; sizes must
    /// be nonzero. Every validation failure occurs before any bus access.
    pub fn begin_window(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<(), Error<B::Error>> {
        if !self.initialized {
            return Err(Error::NotInitialized);
        }
        if self.window_active {
            return Err(Error::WindowInProgress);
        }
        if width == 0
            || height == 0
            || u32::from(x) + u32::from(width) > u32::from(self.width())
            || u32::from(y) + u32::from(height) > u32::from(self.height())
        {
            return Err(Error::InvalidRectangle);
        }
        self.window(x, y, x + width - 1, y + height - 1)?;
        self.window_active = true;
        self.remaining = u32::from(width) * u32::from(height);
        Ok(())
    }

    /// Write raw 565 words into the current window through the synchronous
    /// pixel transport. Its fallback uses bounded stack chunks; an optimized
    /// transport may stream directly from the caller's existing row buffer.
    /// Empty writes are allowed only while a window is active. Overrun rejects
    /// the entire slice before sending bytes. A failed transfer's partial
    /// progress is unknowable and invalidates the whole driver.
    pub fn write_pixels(&mut self, pixels: &[u16]) -> Result<(), Error<B::Error>> {
        if !self.initialized {
            return Err(Error::NotInitialized);
        }
        if !self.window_active {
            return Err(Error::NoWindow);
        }
        if pixels.len() > self.remaining as usize {
            return Err(Error::TooManyPixels);
        }
        if pixels.is_empty() {
            return Ok(());
        }
        let result = self.bus.pixels_rgb565(pixels);
        self.bus_result(result)?;
        self.remaining -= pixels.len() as u32;
        Ok(())
    }

    /// Finish only after exactly the declared pixel count. An incomplete
    /// window stays active so the caller may supply the missing pixels.
    pub fn finish_pixels(&mut self) -> Result<(), Error<B::Error>> {
        if !self.initialized {
            return Err(Error::NotInitialized);
        }
        if !self.window_active {
            return Err(Error::NoWindow);
        }
        if self.remaining != 0 {
            return Err(Error::IncompletePixels {
                remaining: self.remaining,
            });
        }
        self.window_active = false;
        Ok(())
    }

    fn repeat(&mut self, color: u16, mut count: u32) -> Result<(), Error<B::Error>> {
        let word = color.to_be_bytes();
        let mut buffer = [0u8; PIXEL_BUFFER_BYTES];
        for pair in buffer.chunks_exact_mut(2) {
            pair.copy_from_slice(&word);
        }
        while count > 0 {
            let pixels = count.min((PIXEL_BUFFER_BYTES / 2) as u32);
            self.data(&buffer[..pixels as usize * 2])?;
            count -= pixels;
        }
        Ok(())
    }

    pub fn fill_rect(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        color: u16,
    ) -> Result<(), Error<B::Error>> {
        self.begin_window(x, y, width, height)?;
        self.repeat(color, self.remaining)?;
        self.remaining = 0;
        self.finish_pixels()
    }
}
