[Snapcast](https://github.com/badaix/snapcast) client for the ESP32. Works on the standard ESP32, does not need versions with extra memory / PSRAM.

Supported codecs:
- PCM
- Flac (popping/craclking though)
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

I use an [UDA1334A](https://nl.aliexpress.com/item/1005006140641304.html) module with an [ESP32-WROOM-32](https://nl.aliexpress.com/item/1005006500507950.html) (a 320KiB RAM model).

### Memory usage

Basic heap analysis:

* On startup, heap low water mark 273KiB
* After setup, heap low water mark 188KiB

Free heap space:

|Buffer Duration|PCM    |FLAC   |OPUS   |
|---------------|-------|-------|-------|
|150ms          |167KiB |173KiB |?      |
|500ms          |98KiB  |117KiB |?      |
|750ms          |51KiB\*| 97KiB |?      |

\* Got a random OOM a few times

## Known issues

- There still _may be_ scenarios which cause a missed frame, and there's no stretching/cutting, so it's a jarring transition.
	- Have not seen this since increasing the buffer sizes
- After ~a day of not playing, the time bases get out of sync (by ~2s) and no audio will be played anymore; debugging is a pain
	- This is likely due to the ESP32's inaccurate clock -

## Recommended snapserver settings

```
chunk_ms = 30
buffer = 500
codec = pcm
```
