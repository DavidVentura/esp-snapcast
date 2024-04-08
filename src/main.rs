use snapcast_client::client::Client;
use snapcast_client::playback::Player;
use snapcast_client::proto::{ServerMessage, TimeVal};

use snapcast_client::decoder::{Decode, Decoder};

use std::sync::{mpsc, Arc, Mutex};
use std::time;

mod player;
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
    while let Ok((client_audible_ts, samples)) = sample_rx.recv() {
        let mut valid = true;
        loop {
            let remaining = client_audible_ts - time_base_c.elapsed().into();
            if remaining.sec < 0 {
                valid = false;
                break;
            }

            if remaining.millis().unwrap() <= player_lat_ms {
                break;
            }
            std::thread::sleep(time::Duration::from_millis(1));
        }

        if !valid {
            println!("aaa in the past");
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
        p.play().unwrap();
        p.write(sample).unwrap();
    }
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let client = Client::new("99:22:33:44:55:66".into(), "framework".into());
    let mut client = client.connect("192.168.2.131:1704")?;
    let time_base_c = client.time_base();

    let dec: Arc<Mutex<Option<Decoder>>> = Arc::new(Mutex::new(None));
    let dec_2 = dec.clone();

    let player: Arc<Mutex<Option<I2sPlayer>>> = Arc::new(Mutex::new(None));
    let player_2 = player.clone();

    let mut buffer_ms = TimeVal {
        sec: 0,
        usec: 999_999,
    };
    let mut local_latency = TimeVal { sec: 0, usec: 0 };

    let (sample_tx, sample_rx) = mpsc::channel::<(TimeVal, Vec<u8>)>();
    std::thread::spawn(move || handle_samples(sample_rx, time_base_c, player, dec));

    loop {
        let median_tbase = client.latency_to_server();
        let msg = client.tick()?;
        match msg {
            ServerMessage::CodecHeader(ch) => {
                _ = dec_2.lock().unwrap().insert(Decoder::new(&ch)?);
                let p = I2sPlayer::new(&ch);
                _ = player_2.lock().unwrap().insert(p);
            }
            ServerMessage::WireChunk(wc) => {
                let t_s = wc.timestamp;
                let t_c = t_s - median_tbase;
                let audible_at = t_c + buffer_ms - local_latency;
                sample_tx.send((audible_at, wc.payload.to_vec()))?;
            }

            ServerMessage::ServerSettings(s) => {
                buffer_ms = TimeVal::from_millis(s.bufferMs as i32);
                local_latency = TimeVal::from_millis(s.latency as i32);
                println!("local lat now {local_latency:?}");
                // TODO volume
            }
            _ => (),
        }
    }
}
