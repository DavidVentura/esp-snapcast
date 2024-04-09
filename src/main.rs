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

fn handle_samples<P: Player>(
    sample_rx: mpsc::Receiver<(TimeVal, Vec<u8>)>,
    time_base_c: time::Instant,
    player: Arc<Mutex<Option<P>>>,
    dec: Arc<Mutex<Option<Decoder>>>,
) {
    // >= (960 * 2) for OPUS
    // >= 2880 for PCM
    let mut samples_out = vec![0; 4096];

    let mut player_lat_ms: u16 = 1;
    //let mut hs = HeapStats_t::default();
    while let Ok((client_audible_ts, samples)) = sample_rx.recv() {
        let mut valid = true;
        let mut remaining: TimeVal;
        loop {
            remaining = client_audible_ts - time_base_c.elapsed().into();
            let abs = remaining.abs();
            if remaining.sec < 0 && (abs.sec > 0 || abs.usec > 10_000) {
                // maybe 10ms is ok? probably not
                valid = false;
                break;
            }

            // FIXME, fails often on startup
            match remaining.millis() {
                Ok(v) => {
                    if v <= player_lat_ms {
                        break;
                    }
                }
                Err(e) => {
                    println!("Failed to call millis: {}", e);
                    break;
                }
            }
            std::thread::sleep(time::Duration::from_micros(499));
        }

        if !valid {
            println!(
                "aaa in the past {:?} - abs = {:?}",
                remaining,
                remaining.abs()
            );
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
        let decoded_sample_c = dec.decode_sample(&samples, &mut samples_out).unwrap();
        let sample = &samples_out[0..decoded_sample_c];
        p.write(sample).unwrap();
    }
}

const SSID: &'static str = env!("SSID");
const PASS: &'static str = env!("PASS");

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");
    let mut peripherals = Peripherals::take().unwrap();

    let i2s = peripherals.i2s0;
    let dout = peripherals.pins.gpio2;
    let bclk = peripherals.pins.gpio4;
    let ws = peripherals.pins.gpio15;
    let player = I2sPlayer::new(i2s, dout, bclk, ws);

    let nvsp = EspDefaultNvsPartition::take().unwrap();
    wifi::configure(SSID, PASS, nvsp, &mut peripherals.modem).expect("Could not configure wifi");

    let client = Client::new("99:22:33:44:55:66".into(), "framework".into());
    println!("Connecting!");
    let mut client = client.connect("192.168.2.131:1704")?;
    println!("Connected!");
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

    // 5.6KB buffs
    // #  = bad/good
    // 4  = 403/502 - 418/500
    // 8  = 308/501
    // 16 = 286/500
    let (sample_tx, sample_rx) = mpsc::sync_channel::<(TimeVal, Vec<u8>)>(4);
    std::thread::spawn(move || handle_samples(sample_rx, time_base_c, player, dec));

    loop {
        let median_tbase = client.latency_to_server();
        let msg = client.tick()?;
        match msg {
            // TODO: Need to mute player / play a set of 0's if it's been a while without packets
            // (buflen + 100ms?)
            ServerMessage::CodecHeader(ch) => {
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
                // This will sometimes block on send(), to minimize memory usage (number of buffers
                // in mem).
                sample_tx.send((audible_at, wc.payload.to_vec()))?;
            }

            ServerMessage::ServerSettings(s) => {
                buffer_ms = TimeVal::from_millis(s.bufferMs as i32);
                local_latency = TimeVal::from_millis(s.latency as i32);
                println!("local lat now {local_latency:?}, vol at {}", s.volume);
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
