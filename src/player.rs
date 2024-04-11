use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio;
use esp_idf_hal::gpio::{InputPin, OutputPin};
use esp_idf_hal::i2s;
use esp_idf_hal::i2s::config;
use esp_idf_hal::i2s::I2S0;
use esp_idf_hal::peripheral::Peripheral;

use snapcast_client::playback::Player;
use snapcast_client::proto::CodecHeader;

// Heavily inspired from https://github.com/10buttons/awedio_esp32/blob/main/src/lib.rs#L218
pub struct I2sPlayer {
    d: i2s::I2sDriver<'static, i2s::I2sTx>,
    is_playing: bool,
    volume: i16,
    sample_rate: u16,
}

// Powers of two _may_ make the division faster
const VOL_STEP_COUNT: i16 = 128;

impl I2sPlayer {
    const BLOCK_TIME: TickType = TickType::new(100_000_000);

    pub fn init(&mut self, _ch: &CodecHeader) {
        //
    }

    pub fn new(
        i2s: I2S0,
        dout: impl Peripheral<P = impl OutputPin + InputPin> + 'static,
        bclk: impl Peripheral<P = impl OutputPin + InputPin> + 'static,
        ws: impl Peripheral<P = impl OutputPin + InputPin> + 'static,
    ) -> I2sPlayer {
        let mclk: Option<gpio::AnyIOPin> = None;

        let i2s_config = config::StdConfig::new(
            config::Config::default().auto_clear(true),
            config::StdClkConfig::from_sample_rate_hz(48000), // FIXME: how to init in the calling loop?
            config::StdSlotConfig::philips_slot_default(
                config::DataBitWidth::Bits16,
                config::SlotMode::Stereo,
            ),
            config::StdGpioConfig::default(),
        );
        let mut driver =
            i2s::I2sDriver::new_std_tx(i2s, &i2s_config, bclk, dout, mclk, ws).unwrap();

        // Clear TX buffers
        let data: Vec<u8> = vec![0; 128];
        while driver.preload_data(&data).unwrap() > 0 {}

        let mut ret = I2sPlayer {
            d: driver,
            is_playing: false,
            volume: 0,
            sample_rate: 48000, // FIXME same as the other 48000
        };
        ret.set_volume(40).unwrap();
        ret
    }
}
impl Player for I2sPlayer {
    fn play(&mut self) -> anyhow::Result<()> {
        if !self.is_playing {
            self.d.tx_enable().expect("Failed to tx_enable");
            self.is_playing = true;
        }
        Ok(())
    }

    fn write(&mut self, buf: &mut [i16]) -> anyhow::Result<()> {
        for s in buf.iter_mut() {
            *s = ((*s as i32 * self.volume as i32) / VOL_STEP_COUNT as i32) as i16;
        }

        // SAFETY: it's always safe to align i16 to u8
        let (_, converted, _) = unsafe { buf[0..buf.len()].align_to::<u8>() };

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
        log::info!("vol is now {}/{}", self.volume, VOL_STEP_COUNT);
        Ok(())
    }

    fn sample_rate(&self) -> u16 {
        self.sample_rate
    }
}
