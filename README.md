[Snapcast](https://github.com/badaix/snapcast) client for the ESP32. Works on the standard ESP32, does not need versions with extra memory / PSRAM.

Supported codecs:
- PCM
- Flac
- OPUS (builds, but crashes instantly)

Supported backends:
- I2S

To build with the `opus` feature you need to:
```bash
export CC=xtensa-esp32-elf-gcc
export CXX=xtensa-esp32-elf-g++
```

## Bandwidth

On stereo at 48KHz:

- PCM requires ~1.6Mbit/s
- Flac requires ~1Mbit/s

Both of which seem perfectly fine on the ESP32.

## Hardware

I use an [UDA1334A](https://nl.aliexpress.com/item/1005006140641304.html) module with an [ESP32-WROOM-32](https://nl.aliexpress.com/item/1005006500507950.html).

## Known issues

- On startup, for ~1s there will be some glitched audio until the buffer fills up
- When the audio server (snapserver) stops transmitting data, the last sample plays over and over.
- There still are some scenarios which cause a missed frame, and there's no stretching/cutting, so it's a jarring transition.

## Recommended snapserver settings

```
chunk_ms = 30
buffer = 500
```
