#!/usr/bin/env just --justfile

# Avoid repeating needing to add `--chip` for all probe-rs commands
export PROBE_RS_CHIP := "esp32s3"

elf := "target/xtensa-esp32s3-none-elf/release/firmware"

# Build the firmware image
build profile='release':
  cargo build --profile {{profile}}

clean:
  cargo clean

# Flash the firmware and run it
flash profile='release': (build profile)
  probe-rs run {{elf}}

# Download new firmware and reset the chip
download profile='release': (build profile)
  probe-rs download {{elf}} --preverify --reset
  probe-rs reset

# Attach to RTT logging
attach:
  probe-rs attach {{elf}}

# Dump the USB descriptor info (for debugging)
descriptor:
  sudo usbhid-dump -d 1209:0001 -e descriptor