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
    volume: i16,
    vol_adj_buf: Vec<i16>,
}

// Larger values allow for more fine-grained step, but there's loss of precision
const VOL_STEP_COUNT: i16 = 32;

impl I2sPlayer {
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

        let mut ret = I2sPlayer {
            d: driver,
            is_playing: false,
            volume: 0,
            vol_adj_buf: vec![0; 4096],
        };
        ret.set_volume(40);
        ret
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
        for (i, s) in buf.iter().enumerate() {
            self.vol_adj_buf[i] = (s / VOL_STEP_COUNT) * self.volume;
        }

        let (_, converted, _) = unsafe { self.vol_adj_buf[0..buf.len()].align_to::<u8>() };

        self.d.write_all(converted, Self::BLOCK_TIME.into())?;
        Ok(())
    }

    fn latency_ms(&self) -> anyhow::Result<u16> {
        Ok(0)
    }
    fn set_volume(&mut self, val: u8) -> anyhow::Result<()> {
        // convert the 0-100 input range to n/VOL_STEP_COUNT
        if val == 0 {
            self.volume = 0;
            return Ok(());
        }
        let volume_float = f64::from(val);
        let normalized_volume = (volume_float - 1.0) / 99.0;
        let scaled = normalized_volume.powf(2.0) * f64::from(VOL_STEP_COUNT);
        self.volume = scaled.round() as i16;
        println!("vol is now {}/{}", self.volume, VOL_STEP_COUNT,);
        Ok(())
    }
}
