.PHONY: build flash flashm monitor
build:
	cargo build --release
monitor:
	cargo espflash monitor -p /dev/ttyUSB0
flash:
	cargo espflash flash --release -p /dev/ttyUSB0 -f 80mhz -b 921600 --partition-table partitions.csv
flashm:
	cargo espflash flash --release -p /dev/ttyUSB0 -f 80mhz -b 921600 --flash-mode dio -M --partition-table partitions.csv
