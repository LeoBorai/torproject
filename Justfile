default:
    @echo "No default task specified."
    just --list

# Dependency check, as CI runs it: warnings are errors (needs cargo-deny)
deny:
    cargo deny check -D warnings

# Runs the Development Kit Container
dkc:
    docker pull ghcr.io/leoborai/dkc:latest
    docker run -it --rm \
        -v $(pwd):/app \
        -w /app \
        ghcr.io/leoborai/dkc:latest


fmt:
    cargo clippy --fix --workspace --allow-dirty --allow-staged
    cargo fmt

# Unit and integration tests
test:
    cargo nextest run --workspace --no-tests=pass
