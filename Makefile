.PHONY: build flash flashm build dump_section_size dump_flash_rodata dump_flash_text
flash: build
	cargo espflash flash --release -p /dev/ttyUSB0 -f 80mhz -b 921600 --partition-table partitions.csv
flashm: build
	cargo espflash flash --release -p /dev/ttyUSB0 -f 80mhz -b 921600 --flash-mode dio -M --partition-table partitions.csv
build:
	cargo build --release
