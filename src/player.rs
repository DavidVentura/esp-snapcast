use anyhow::anyhow;

use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio;
use esp_idf_hal::gpio::{AnyOutputPin, InputPin, OutputPin, PinDriver};
use esp_idf_hal::i2s;
use esp_idf_hal::i2s::config;
use esp_idf_hal::i2s::I2S0;
use esp_idf_hal::peripheral::Peripheral;

use snapcast_client::playback::Player;
use snapcast_client::proto::CodecHeader;

use crate::util;

pub struct I2sPlayerBuilder<
    OP: OutputPin + InputPin,
    OQ: OutputPin + InputPin,
    OR: OutputPin + InputPin,
    P: Peripheral<P = OP> + 'static,
    Q: Peripheral<P = OQ> + 'static,
    R: Peripheral<P = OR> + 'static,
> {
    i2s: Option<I2S0>,
    dout: Option<P>,
    bclk: Option<Q>,
    ws: Option<R>,
    nmute_pin: Option<AnyOutputPin>,
}

impl<
        OP: OutputPin + InputPin,
        OQ: OutputPin + InputPin,
        OR: OutputPin + InputPin,
        P: Peripheral<P = OP> + 'static,
        Q: Peripheral<P = OQ> + 'static,
        R: Peripheral<P = OR> + 'static,
    > I2sPlayerBuilder<OP, OQ, OR, P, Q, R>
{
    pub fn new(
        i2s: I2S0,
        dout: P,
        bclk: Q,
        ws: R,
        nmute_pin: AnyOutputPin,
    ) -> I2sPlayerBuilder<OP, OQ, OR, P, Q, R> {
        I2sPlayerBuilder {
            i2s: Some(i2s),
            dout: Some(dout),
            bclk: Some(bclk),
            ws: Some(ws),
            nmute_pin: Some(nmute_pin),
        }
    }
    // Heavily inspired from https://github.com/10buttons/awedio_esp32/blob/main/src/lib.rs#L218
    pub fn init(&mut self, ch: &CodecHeader) -> anyhow::Result<I2sPlayer> {
        let mclk: Option<gpio::AnyIOPin> = None;

        let i2s_config = config::StdConfig::new(
            config::Config::default()
                .auto_clear(true)
                .dma_buffer_count(10)
                .frames_per_buffer(511),
            config::StdClkConfig::from_sample_rate_hz(ch.metadata.rate() as u32),
            config::StdSlotConfig::philips_slot_default(
                config::DataBitWidth::Bits16,
                config::SlotMode::Stereo,
            ),
            config::StdGpioConfig::default(),
        );

        let i2s = self.i2s.take().ok_or(anyhow!("Initialized I2s twice"))?;
        let bclk = self.bclk.take().ok_or(anyhow!("Initialized twice"))?;
        let dout = self.dout.take().ok_or(anyhow!("Initialized twice"))?;
        let ws = self.ws.take().ok_or(anyhow!("Initialized twice"))?;
        let mut driver = i2s::I2sDriver::new_std_tx(i2s, &i2s_config, bclk, dout, mclk, ws)?;

        let nmute_pin = self.nmute_pin.take().ok_or(anyhow!("Initialized twice"))?;
        let mut nmute = PinDriver::output(nmute_pin)?;
        nmute.set_low()?;

        // Clear TX buffers
        let data: Vec<u8> = vec![0; 128];
        while driver.preload_data(&data)? > 0 {}

        let mut ret = I2sPlayer {
            d: driver,
            is_playing: false,
            volume: 0,
            nmute: nmute,
            sample_rate: ch.metadata.rate() as u16,
        };
        ret.set_volume(20)?;
        Ok(ret)
    }
}
pub struct I2sPlayer {
    d: i2s::I2sDriver<'static, i2s::I2sTx>,
    is_playing: bool,
    volume: i16,
    nmute: PinDriver<'static, AnyOutputPin, gpio::Output>,
    sample_rate: u16,
}

// Powers of two _may_ make the division faster
const VOL_STEP_COUNT: i16 = 128;

impl I2sPlayer {
    const BLOCK_TIME: TickType = TickType::new(100_000_000);
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
        // do not apply soft-volume when playing at 100%
        if self.volume < VOL_STEP_COUNT {
            for s in buf.iter_mut() {
                *s = ((*s as i32 * self.volume as i32) / VOL_STEP_COUNT as i32) as i16;
            }
        }

        // SAFETY: it's always safe to align i16 to u8
        let (_, converted, _) = unsafe { buf[0..buf.len()].align_to::<u8>() };

        util::measure_exec(
            "write to i2s player",
            || {
                self.d
                    .write_all(converted, Self::BLOCK_TIME.into())
                    .unwrap();
            },
            std::time::Duration::from_millis(1),
        );
        Ok(())
    }

    fn latency_ms(&self) -> anyhow::Result<u16> {
        Ok(0)
    }

    fn set_volume(&mut self, val: u8) -> anyhow::Result<()> {
        // convert the 0-100 input range to n/VOL_STEP_COUNT
        if val == 0 {
            self.volume = 0;
            self.nmute.set_low()?;
            return Ok(());
        }
        self.nmute.set_high()?;
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
