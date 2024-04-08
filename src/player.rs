use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio;
use esp_idf_hal::i2s;
use esp_idf_hal::i2s::config;
use esp_idf_hal::peripherals::Peripherals;

use snapcast_client::playback::Player;
use snapcast_client::proto::CodecHeader;

// Heavily inspired from https://github.com/10buttons/awedio_esp32/blob/main/src/lib.rs#L218
pub struct I2sPlayer {
    d: i2s::I2sDriver<'static, i2s::I2sTx>,
    channel_count: u8,
    sample_rate: usize,
}

impl I2sPlayer {
    const SAMPLE_SIZE: usize = std::mem::size_of::<i16>();
    const BLOCK_TIME: TickType = TickType::new(100_000_000);

    pub fn new(ch: &CodecHeader) -> I2sPlayer {
        let i2s_config = config::StdConfig::new(
            config::Config::default(),
            config::StdClkConfig::from_sample_rate_hz(ch.metadata.rate() as u32),
            config::StdSlotConfig::philips_slot_default(
                config::DataBitWidth::Bits16,
                config::SlotMode::Mono,
            ),
            config::StdGpioConfig::default(),
        );

        let peripherals = Peripherals::take().unwrap();
        let i2s = peripherals.i2s0;
        let bclk = peripherals.pins.gpio2;
        let dout = peripherals.pins.gpio4;
        let mclk: Option<gpio::AnyIOPin> = None;
        let ws = peripherals.pins.gpio5;
        let driver = i2s::I2sDriver::new_std_tx(i2s, &i2s_config, bclk, dout, mclk, ws).unwrap();

        I2sPlayer {
            d: driver,
            channel_count: ch.metadata.channels() as u8,
            sample_rate: ch.metadata.sample_rate(),
        }
    }
}
impl Player for I2sPlayer {
    fn play(&mut self) -> anyhow::Result<()> {
        self.d.tx_enable().expect("Failed to tx_enable");
        Ok(())
    }

    fn write(&mut self, buf: &[i16]) -> anyhow::Result<()> {
        let byte_slice = unsafe {
            core::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len() * Self::SAMPLE_SIZE)
        };
        self.d.write_all(&byte_slice, Self::BLOCK_TIME.into())?;
        Ok(())
    }
    fn latency_ms(&self) -> anyhow::Result<u16> {
        Ok(0)
    }
    fn set_volume(&mut self, val: u8) -> anyhow::Result<()> {
        Ok(())
    }
}
