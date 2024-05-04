.PHONY: build flash flashm monitor only_build
ELF = target/xtensa-esp32-espidf/release/esp-snapcast
build:
	cargo build --release
	python3 replacer.py ${ELF}
only_build:
	cargo build --release
monitor:
	cargo espflash monitor -p /dev/ttyUSB0
flash: build
	espflash flash -p /dev/ttyUSB0 -f 80mhz -B 921600 --partition-table partitions.csv ${ELF}
flashm: build
	espflash flash -p /dev/ttyUSB0 -f 80mhz -B 921600 --flash-mode dio -M --partition-table partitions.csv ${ELF}
