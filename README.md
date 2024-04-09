[Snapcast](https://github.com/badaix/snapcast) client for the ESP32. Works on the standard ESP32, does not need versions with extra memory / PSRAM.

Supported codecs:
- PCM (stereo, 48KHz ~= 1.5Mbit/s)
- OPUS (instant-crash)

Supported backends:
- I2S


To build with the `opus` feature you need to:
```bash
export CC=xtensa-esp32-elf-gcc
export CXX=xtensa-esp32-elf-g++
```

## Known issues

- On startup, for ~1s there will be some glitched audio until the buffer fills up
- When the audio server (snapserver) stops transmitting data, the last sample plays over and over.
- There still are some scenarios which cause a missed frame, and there's no stretching/cutting, so it's a jarring transition.
