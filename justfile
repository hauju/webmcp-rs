# Format check (matches CI)
[group("lint")]
fmt:
    cargo fmt --all --check

# Clippy on both halves: host and wasm32 (matches CI)
[group("lint")]
clippy:
    cargo clippy --all-targets --all-features -- -D warnings
    cargo clippy --all-targets -- -D warnings
    cargo clippy --all-targets --no-default-features -- -D warnings
    cargo clippy --target wasm32-unknown-unknown --all-features -- -D warnings
    cargo clippy --target wasm32-unknown-unknown --no-default-features -- -D warnings

# Tests + docs (matches CI)
[group("dev")]
test:
    cargo test --all-features
    cargo test --no-default-features
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Everything CI runs
[group("lint")]
check: fmt clippy test
    cargo machete

# WASM execution tests in headless Chrome (requires wasm-pack)
[group("dev")]
test-browser:
    wasm-pack test --headless --chrome --all-features
    wasm-pack test --headless --chrome --no-default-features
    wasm-pack test --headless --chrome site

# What `cargo publish` would upload
[group("release")]
publish-dry-run:
    cargo publish --dry-run

# Serve the demo site at http://localhost:8080/
[group("site")]
site-serve:
    cd site && bun install --frozen-lockfile && ./node_modules/.bin/tailwindcss -i tailwind.css -o assets/tailwind.css --minify && dx serve

# Build exactly what CI publishes to GitHub Pages
[group("site")]
site-build:
    cd site && bun install --frozen-lockfile && ./node_modules/.bin/tailwindcss -i tailwind.css -o assets/tailwind.css --minify && dx bundle --web --release
