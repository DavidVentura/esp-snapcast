use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio;
use esp_idf_hal::gpio::{Gpio15, Gpio2, Gpio4};
use esp_idf_hal::i2s;
use esp_idf_hal::i2s::config;
use esp_idf_hal::i2s::I2S0;

use snapcast_client::playback::Player;
use snapcast_client::proto::CodecHeader;

// Heavily inspired from https://github.com/10buttons/awedio_esp32/blob/main/src/lib.rs#L218
pub struct I2sPlayer {
    d: i2s::I2sDriver<'static, i2s::I2sTx>,
    is_playing: bool,
}

impl I2sPlayer {
    const SAMPLE_SIZE: usize = std::mem::size_of::<i16>();
    const BLOCK_TIME: TickType = TickType::new(100_000_000);

    pub fn init(&mut self, _ch: &CodecHeader) {
        //
    }

    pub fn new(i2s: I2S0, dout: Gpio2, bclk: Gpio4, ws: Gpio15) -> I2sPlayer {
        let mclk: Option<gpio::AnyIOPin> = None;
        let i2s_config = config::StdConfig::new(
            config::Config::default(),
            config::StdClkConfig::from_sample_rate_hz(48000), // FIXME: how to init in the calling loop?
            config::StdSlotConfig::philips_slot_default(
                config::DataBitWidth::Bits16,
                config::SlotMode::Stereo,
            ),
            config::StdGpioConfig::default(),
        );
        let driver = i2s::I2sDriver::new_std_tx(i2s, &i2s_config, bclk, dout, mclk, ws).unwrap();

        I2sPlayer {
            d: driver,
            is_playing: false,
        }
    }
}
impl Player for I2sPlayer {
    fn play(&mut self) -> anyhow::Result<()> {
        if !self.is_playing {
            println!("Enabling TX!");
            self.d.tx_enable().expect("Failed to tx_enable");
            println!("Enabled TX!");
            self.is_playing = true;
        }
        Ok(())
    }

    fn write(&mut self, buf: &[i16]) -> anyhow::Result<()> {
        // SAFETY: it's always safe to align i16 to u8
        let (_, converted, _) = unsafe { buf.align_to::<u8>() };
        self.d.write_all(converted, Self::BLOCK_TIME.into())?;
        Ok(())
    }
    fn latency_ms(&self) -> anyhow::Result<u16> {
        Ok(0)
    }
    fn set_volume(&mut self, val: u8) -> anyhow::Result<()> {
        Ok(())
    }
}
