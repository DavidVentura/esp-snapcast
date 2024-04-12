use snapcast_client::client::{Client, ConnectedClient, Message};
use snapcast_client::decoder::{Decode, Decoder};
use snapcast_client::playback::Player;
use snapcast_client::proto::TimeVal;

use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp;
use esp_idf_svc::sys::esp_get_free_heap_size;

use std::sync::{mpsc, mpsc::SyncSender, Arc, Mutex};
use std::time;

mod player;
mod wifi;
use player::{I2sPlayer, I2sPlayerBuilder};

const SSID: &str = env!("SSID");
const PASS: &str = env!("PASS");

fn handle_samples<P: Player>(
    dec_sample_buf: &mut [i16],
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
            log::info!("heap low water mark: {free}");
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
                    "rem {remaining:?} too far away! hard cutting - at queue time was {:?} - client base {:?}",
                    rem_at_queue_time.abs(), time_base_c.elapsed(),
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
                let ms_to_skip = (remaining.usec / 1000).unsigned_abs() as u16;
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
        let decoded_sample_c = dec.decode_sample(&samples, dec_sample_buf).unwrap();
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
    let mut player_builder = I2sPlayerBuilder::new(i2s, dout, bclk, ws);

    let nvsp = EspDefaultNvsPartition::take().unwrap();
    let mac = wifi::configure(SSID, PASS, nvsp, &mut peripherals.modem)
        .expect("Could not configure wifi");

    let _sntp = sntp::EspSntp::new_default()?;
    log::info!("SNTP initialized");

    let mac = format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    let name = "esp32";

    let dec: Arc<Mutex<Option<Decoder>>> = Arc::new(Mutex::new(None));

    // >= (960 * 2) for OPUS
    // >= 2880 for PCM
    // >= 4700 for flac
    let mut dec_samples_buf = vec![0; 4700];

    let player: Arc<Mutex<Option<I2sPlayer>>> = Arc::new(Mutex::new(None));
    loop {
        // Validated experimentally -- with a queue depth of 4, "once in a while", a packet
        // would only be put onto the queue _after_ it's deadline had passed
        let (sample_tx, sample_rx) = mpsc::sync_channel::<(TimeVal, TimeVal, Vec<u8>)>(24);
        let client = Client::new(mac.clone(), name.into());
        // TODO: discover stream
        let client = client.connect("192.168.2.131:1704")?;
        let time_base_c = client.time_base();

        let player_2 = player.clone();
        let player_3 = player.clone();

        let dec2 = dec.clone();
        let dec3 = dec.clone();
        let decref = &mut dec_samples_buf;

        std::thread::scope(|s| {
            s.spawn(move || handle_samples(decref, sample_rx, time_base_c, player_2, dec2));

            let r = connection_main(client, &mut player_builder, player_3, sample_tx, dec3);
            log::error!("Connection dropped: {r:?}");
            // sample_tx is dropped here - sample_rx dies -> thread expires -> scope finishes
        });
        // reset decoder
        dec.lock().unwrap().take();
    }
}

use esp_idf_hal::gpio::{InputPin, OutputPin};
use esp_idf_hal::peripheral::Peripheral;
fn connection_main<
    OP: OutputPin + InputPin,
    OQ: OutputPin + InputPin,
    OR: OutputPin + InputPin,
    P: Peripheral<P = OP> + 'static,
    Q: Peripheral<P = OQ> + 'static,
    R: Peripheral<P = OR> + 'static,
>(
    mut client: ConnectedClient,
    pb: &mut I2sPlayerBuilder<OP, OQ, OR, P, Q, R>,
    player: Arc<Mutex<Option<I2sPlayer>>>,
    sample_tx: SyncSender<(TimeVal, TimeVal, Vec<u8>)>,
    decoder: Arc<Mutex<Option<Decoder>>>,
) -> anyhow::Result<()> {
    log::info!("Starting a new connection");

    let free = unsafe { esp_get_free_heap_size() };
    log::info!("[setup done] heap low water mark: {free}");

    let time_base_c = client.time_base();
    let mut start_vol = 20;
    loop {
        let in_sync = client.synchronized();
        let msg = client.tick()?;
        match msg {
            Message::CodecHeader(ch) => {
                log::info!("Initializing player with: {ch:?}");
                _ = decoder.lock().unwrap().insert(Decoder::new(&ch)?);
                let mut _a = player.lock().unwrap();
                let mut _p = _a.insert(pb.init(&ch).unwrap());
                // right now we can't re-init the player; this `unwrap()` forces the ESP32 to reboot
                _p.set_volume(start_vol)?;
                _p.play()?;
            }
            Message::WireChunk(wc, audible_at) => {
                // This will sometimes block on send()
                // to minimize memory usage (number of buffers in mem).
                // Effectively using the network as a buffer
                if in_sync {
                    let remaining_at_queue = audible_at - time_base_c.elapsed().into();
                    sample_tx.send((audible_at, remaining_at_queue, wc.payload.to_vec()))?;
                }
            }

            Message::PlaybackVolume(v) => {
                let mut p = player.lock().unwrap();
                start_vol = v;
                if let Some(p) = p.as_mut() {
                    p.set_volume(v)?;
                } else {
                    log::info!("Delaying volume config");
                }
            }
            Message::Nothing => (),
        }
    }
}
