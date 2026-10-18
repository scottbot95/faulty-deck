# Build all host crates
build profile='dev':
  cargo build --workspace --profile {{profile}}

# Run all host tests
test:
  cargo test --workspace

clean:
  cargo clean
