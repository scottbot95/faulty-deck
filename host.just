# Build all host crates
build profile='dev':
  cargo build --workspace --profile {{profile}}

# Run all host tests
test:
  cargo test --workspace

clean:
  cargo clean

# Regenerate the Wireshark dissector from the protocol types
wireshark:
  #!/usr/bin/env bash
  set -euo pipefail
  tmp=$(mktemp tools/wireshark/faulty-deck.lua.XXXXXX)
  trap 'rm -f "$tmp"' EXIT
  cargo run -q -p protocol --example wireshark > "$tmp"
  chmod 644 "$tmp"
  mv "$tmp" tools/wireshark/faulty-deck.lua

