[Snapcast](https://github.com/badaix/snapcast) client for the ESP32. Works on the standard ESP32, does not need versions with extra memory / PSRAM.

Supported codecs:
- PCM
- Flac (popping/craclking on some boots??)
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
- Opus requires ~250Kbit/s

All of which seem perfectly fine on the ESP32.

## Hardware

I use an [UDA1334A](https://nl.aliexpress.com/item/1005006140641304.html) module with an [ESP32-WROOM-32](https://nl.aliexpress.com/item/1005006500507950.html) (a 320KiB RAM model).

### Memory usage

Basic heap analysis:

* On startup, heap low water mark 273KiB
* After setup, heap low water mark 188KiB

Free heap space:

|Buffer Duration|PCM    |FLAC   |OPUS   |
|---------------|-------|-------|-------|
|150ms          |167KiB |173KiB |146KiB |
|500ms          |93KiB  |117KiB |?      |
|750ms          |31KiB\*| 97KiB |?      |

\* Got a random OOM a few times

## Known issues

- There still _may be_ scenarios which cause a missed frame, and there's no stretching/cutting, so it's a jarring transition.
	- Have not seen this since increasing the buffer sizes
- The ESP32's clock is very inaccurate -- I've measured ~100ms drift per hour with the standard configuration
	- The SNTP client _should_ correct the drift accumulation

## Recommended snapserver settings

```
chunk_ms = 30
buffer = 500
codec = pcm
```
