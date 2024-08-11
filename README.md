[Snapcast](https://github.com/badaix/snapcast) client for the ESP32. Works on the standard ESP32, does not need versions with extra memory / PSRAM.

Supported codecs:
- PCM
- Flac
- OPUS (builds, but crashes instantly)

## Building

_Note that you can download the pre-built firmware [from CI](https://github.com/DavidVentura/esp-snapcast/actions)._

### Requirements

* Install [esp-rust](https://docs.esp-rs.org/book/installation/rust.html)
* `cargo install ldproxy espflash`


If you want to build the `opus` backend, you also need:
```bash
export CC=xtensa-esp32-elf-gcc
export CXX=xtensa-esp32-elf-g++
```

Create a `cfg.toml` file, using `cfg.toml.example` as a template. Update the settings with your WiFi and Snapcast server details.

To build the project, run `make build`.

Note that mDNS (automatic server discovery) planned, but is not yet implemented.

### Flashing

Requires [espflash](https://github.com/esp-rs/espflash/tree/main/espflash).

To flash the project into an ESP32 you can run `make flashm`

## Hardware

I use an [UDA1334A](https://nl.aliexpress.com/item/1005006140641304.html) module with an [ESP32-WROOM-32](https://nl.aliexpress.com/item/1005006500507950.html) (a 320KiB RAM model).

Wire the pins according to this table:

|ESP | I2s board|
|----|----|
D21 | WSEL
D19 | DIN
D18| BCLK
GND | GND
3v3 | VIN

The specific pinout is not required, you only need pins that can output, are not bootstrap pins, and do not output garbage on boot.
If you want to change the wiring, you also need to modify the `i2s`, `dout`, `ws` and `bclk` variables in `main()`.

A pull-down resistor on WSEL makes for quiet reboots; without this, there's a lot of garbled noise until playback starts.


## Recommended snapserver settings

```
chunk_ms = 30
buffer = 690
codec = flac
```

## Bandwidth

On stereo at 48KHz:

- PCM requires ~1.6Mbit/s
- Flac requires ~1Mbit/s
- Opus requires ~250Kbit/s

All of which seem perfectly fine on the ESP32.


## Memory usage

Basic heap analysis:

* On startup, heap low water mark 273KiB
* After setup, heap low water mark 188KiB

Free heap space:

|Buffer Duration|PCM    |FLAC   |OPUS   |
|---------------|-------|-------|-------|
|150ms          |167KiB |173KiB |146KiB |
|500ms          |93KiB  |117KiB |?      |
|700ms          |31KiB\*| 53KiB |?      |

\* Got a random OOM a few times, investigating

## Known issues

- OPUS does not work 

## TODO

[ ] Host a page with [esp tools](https://esphome.github.io/esp-web-tools/) to provide easy flashing/firmware building
