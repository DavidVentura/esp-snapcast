use snapcast_client::client::{Client, Message};
use snapcast_client::decoder::{Decode, Decoder};
use snapcast_client::playback::Player;
use snapcast_client::proto::TimeVal;

use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use std::sync::{mpsc, Arc, Mutex};
use std::time;

mod player;
mod wifi;
use player::I2sPlayer;

use esp_idf_svc::sys::esp_get_free_heap_size;

const SSID: &'static str = env!("SSID");
const PASS: &'static str = env!("PASS");

fn handle_samples<P: Player>(
    mut dec_sample_buf: Vec<i16>,
    sample_rx: mpsc::Receiver<(TimeVal, TimeVal, Vec<u8>)>,
    time_base_c: time::Instant,
    player: Arc<Mutex<Option<P>>>,
    dec: Arc<Mutex<Option<Decoder>>>,
) {
    let mut player_lat_ms: u16 = 1;
    let mut samples_per_ms: u16 = 1; // irrelevant, will be overwritten

    let mut free_heap = unsafe { esp_get_free_heap_size() };

    while let Ok((client_audible_ts, rem_at_queue_time, samples)) = sample_rx.recv() {
        let mut valid = true;
        let mut remaining: TimeVal;

        let mut skip_samples = 0;

        let free = unsafe { esp_get_free_heap_size() };
        if free < free_heap {
            log::debug!("heap low water mark: {free}");
            free_heap = free;
        }
        loop {
            remaining = client_audible_ts - time_base_c.elapsed().into();
            if remaining.sec == -1 && remaining.usec > 0 {
                remaining.sec = 0;
                remaining.usec -= 1_000_000;
            }

            if remaining.sec != 0 {
                log::info!(
                    "rem {remaining:?} too far away! hard cutting - at queue time was {:?}",
                    rem_at_queue_time.abs()
                );
                valid = false;
                break;
            }

            // consider 0.5ms reasonable to just play
            if remaining.usec > -500 {
                skip_samples = 0;
                if (remaining.usec / 1000) as u16 <= player_lat_ms {
                    break;
                } else {
                    std::thread::sleep(time::Duration::from_micros(499));
                }
            } else {
                let ms_to_skip = (remaining.usec / 1000).abs() as u16;
                skip_samples = ms_to_skip * samples_per_ms;
                log::info!(
                    "skipping {skip_samples} samples = {ms_to_skip}ms - at queue time was {:?}",
                    rem_at_queue_time.abs()
                );
                break;
            }
        }

        if !valid {
            continue;
        }

        // Guard against chunks coming before the decoder is initialized
        let Some(ref mut dec) = *dec.lock().unwrap() else {
            continue;
        };
        let Some(ref mut p) = *player.lock().unwrap() else {
            continue;
        };
        // Backends with 0ms of buffer (file, tcp) otherwise behave erratically
        player_lat_ms = std::cmp::max(1, p.latency_ms().unwrap());
        if samples_per_ms == 1 {
            samples_per_ms = p.sample_rate() / 1000;
        }
        let decoded_sample_c = dec.decode_sample(&samples, &mut dec_sample_buf).unwrap();
        if skip_samples as usize > decoded_sample_c {
            log::info!("Tried to skip way too much, skipping the whole sample");
            continue;
        }
        let sample = &mut dec_sample_buf[0..decoded_sample_c];
        p.write(&mut sample[(skip_samples as usize)..]).unwrap();
    }
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    esp_idf_svc::log::EspLogger.set_target_level("target", log::LevelFilter::Info)?;

    let free = unsafe { esp_get_free_heap_size() };
    log::info!("[startup] heap low water mark: {free}");

    let mut peripherals = Peripherals::take().unwrap();

    let i2s = peripherals.i2s0;
    let dout = peripherals.pins.gpio19;
    let bclk = peripherals.pins.gpio21;
    let ws = peripherals.pins.gpio18;
    let player = I2sPlayer::new(i2s, dout, bclk, ws);

    let nvsp = EspDefaultNvsPartition::take().unwrap();
    wifi::configure(SSID, PASS, nvsp, &mut peripherals.modem).expect("Could not configure wifi");

    let client = Client::new("99:22:33:44:55:66".into(), "esp32".into());
    // TODO: discover stream
    let mut client = client.connect("192.168.2.131:1704")?;
    let time_base_c = client.time_base();

    let dec: Arc<Mutex<Option<Decoder>>> = Arc::new(Mutex::new(None));
    let dec_2 = dec.clone();

    let player: Arc<Mutex<Option<I2sPlayer>>> = Arc::new(Mutex::new(Some(player)));
    let player_2 = player.clone();

    // Validated experimentally -- with a queue depth of 4, "once in a while", a packet
    // would only be put onto the queue _after_ it's deadline had passed
    let (sample_tx, sample_rx) = mpsc::sync_channel::<(TimeVal, TimeVal, Vec<u8>)>(24);

    // >= (960 * 2) for OPUS
    // >= 2880 for PCM
    // >= 4700 for flac
    let dec_samples_buf = vec![0; 4700];

    std::thread::spawn(move || {
        handle_samples(dec_samples_buf, sample_rx, time_base_c, player, dec)
    });
    let free = unsafe { esp_get_free_heap_size() };
    log::info!("[setup done] heap low water mark: {free}");

    loop {
        let in_sync = client.synchronized();
        let msg = client.tick()?; // TODO: this will break on server restarts
                                  // and the esp won't auto-reboot
        match msg {
            Message::CodecHeader(ch) => {
                log::info!("Initializing player with: {ch:?}");
                _ = dec_2.lock().unwrap().insert(Decoder::new(&ch)?);
                let mut _a = player_2.lock().unwrap();
                let _p = _a.as_mut().unwrap();
                {
                    _p.init(&ch);
                    _p.play().unwrap();
                }
            }
            Message::WireChunk(wc, audible_at) => {
                // This will sometimes block on send()
                // to to minimize memory usage (number of buffers in mem).
                // Effectively using the network as a buffer
                if in_sync {
                    let remaining_at_queue = audible_at - time_base_c.elapsed().into();
                    sample_tx.send((audible_at, remaining_at_queue, wc.payload.to_vec()))?;
                }
            }

            Message::PlaybackVolume(v) => {
                player_2
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .set_volume(v)
                    .unwrap();
            }
            Message::Nothing => (),
        }
    }
}
