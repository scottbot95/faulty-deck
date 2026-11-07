#!/usr/bin/env just --justfile

chip := "esp32s3"
elf := f"target/xtensa-{{chip}}-none-elf/release/firmware"
esp_idf := "github:mirrexagon/nixpkgs-esp-dev#esp-idf-full"

# Rebuild boot/bootloader-rollback.bin from boot/sdkconfig.defaults
bootloader:
  rm -f boot/sdkconfig
  IDF_COMPONENT_MANAGER=0 nix develop {{esp_idf}} --command idf.py -C boot -B boot/build bootloader
  cp boot/build/bootloader/bootloader.bin boot/bootloader-rollback.bin

# Build the firmware image
build profile='release':
  cargo build --profile {{profile}}

clean:
  cargo clean

# Flash bootloader, partition table and firmware
flash profile='release': (build profile)
  espflash flash --erase-parts otadata {{elf}}

# Save the app image usedd for OTA updates
image profile='release': (build profile)
  espflash save-image --chip {{chip}} target/firmware.bin

# Erase the otadata so the bootloader fails back to ota_0
reset-slots:
  espflash erase-parts --partition-table partitions.csv otadata

# Erase the whole flash
erase:
  espflash erase-flash

# Dump the USB descriptor info (for debugging)
descriptor:
  sudo usbhid-dump -d 1209:0001 -e descriptor
