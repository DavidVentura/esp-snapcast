use snapcast_client::client::Client;
use snapcast_client::decoder::{Decode, Decoder};
use snapcast_client::playback::Player;
use snapcast_client::proto::{ServerMessage, TimeVal};

use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use std::sync::{mpsc, Arc, Mutex};
use std::time;

mod player;
mod wifi;
use player::I2sPlayer;

const SSID: &'static str = env!("SSID");
const PASS: &'static str = env!("PASS");

fn handle_samples<P: Player>(
    sample_rx: mpsc::Receiver<(TimeVal, TimeVal, Vec<u8>)>,
    time_base_c: time::Instant,
    player: Arc<Mutex<Option<P>>>,
    dec: Arc<Mutex<Option<Decoder>>>,
) {
    // >= (960 * 2) for OPUS
    // >= 2880 for PCM
    // >= 4700 for flac
    let mut samples_out = vec![0; 4700];

    let mut player_lat_ms: u16 = 1;
    let mut samples_per_ms: u16 = 1; // irrelevant, will be overwritten

    while let Ok((client_audible_ts, rem_at_queue_time, samples)) = sample_rx.recv() {
        let mut valid = true;
        let mut remaining: TimeVal;

        let mut skip_samples = 0;
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
        let decoded_sample_c = dec.decode_sample(&samples, &mut samples_out).unwrap();
        if skip_samples as usize > decoded_sample_c {
            log::info!("Tried to skip way too much, skipping the whole sample");
            continue;
        }
        let sample = &mut samples_out[0..decoded_sample_c];
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

    let mut peripherals = Peripherals::take().unwrap();

    let i2s = peripherals.i2s0;
    let dout = peripherals.pins.gpio19;
    let bclk = peripherals.pins.gpio21;
    let ws = peripherals.pins.gpio18;
    let player = I2sPlayer::new(i2s, dout, bclk, ws);

    let nvsp = EspDefaultNvsPartition::take().unwrap();
    wifi::configure(SSID, PASS, nvsp, &mut peripherals.modem).expect("Could not configure wifi");

    let client = Client::new("99:22:33:44:55:66".into(), "framework".into());
    // TODO: discover stream
    let mut client = client.connect("192.168.2.131:1704")?;
    let time_base_c = client.time_base();

    let dec: Arc<Mutex<Option<Decoder>>> = Arc::new(Mutex::new(None));
    let dec_2 = dec.clone();

    let player: Arc<Mutex<Option<I2sPlayer>>> = Arc::new(Mutex::new(Some(player)));
    let player_2 = player.clone();

    let mut buffer_ms = TimeVal {
        sec: 0,
        usec: 999_999,
    };
    let mut local_latency = TimeVal { sec: 0, usec: 0 };

    // Experimentally, with a queue depth of 4, 50% of the packets block for ~30ms.
    // With a queue depth of 16, 36% of the packets block, and it requires most of the
    // system's memory.
    let (sample_tx, sample_rx) = mpsc::sync_channel::<(TimeVal, TimeVal, Vec<u8>)>(4);
    std::thread::spawn(move || handle_samples(sample_rx, time_base_c, player, dec));

    loop {
        let median_tbase = client.latency_to_server();
        let in_sync = client.synchronized();
        let msg = client.tick()?;
        match msg {
            // TODO: Need to mute player / play a set of 0's if it's been a while without packets
            // (buflen + 100ms?)
            ServerMessage::CodecHeader(ch) => {
                log::info!("Initializing player with: {ch:?}");
                _ = dec_2.lock().unwrap().insert(Decoder::new(&ch)?);
                let mut _a = player_2.lock().unwrap();
                let _p = _a.as_mut().unwrap();
                {
                    _p.init(&ch);
                    _p.play().unwrap();
                }
            }
            ServerMessage::WireChunk(wc) => {
                let t_s = wc.timestamp;
                let t_c = t_s - median_tbase;
                let audible_at = t_c + buffer_ms - local_latency;
                // This will sometimes block on send()
                // to to minimize memory usage (number of buffers in mem).
                // Effectively using the network as a buffer
                if in_sync {
                    let remaining_at_queue = audible_at - time_base_c.elapsed().into();
                    sample_tx.send((audible_at, remaining_at_queue, wc.payload.to_vec()))?;
                }
            }

            ServerMessage::ServerSettings(s) => {
                buffer_ms = TimeVal::from_millis(s.bufferMs as i32);
                local_latency = TimeVal::from_millis(s.latency as i32);
                log::info!("local lat now {local_latency:?}, vol at {}", s.volume);
                player_2
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .set_volume(s.volume)
                    .unwrap();
            }
            _ => (),
        }
    }
}
