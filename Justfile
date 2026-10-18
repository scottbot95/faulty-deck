#!/usr/bin/env just --justfile

mod host
mod firmware 'firmware/firmware.just'

[private]
default:
  @just --list --list-submodules

# Build the host crates and firmware image
build: host::build firmware::build

# Run host tests (firmware must be run separately)
test: host::test

# Clean-up build all artifacts
clean: host::clean firmware::clean
