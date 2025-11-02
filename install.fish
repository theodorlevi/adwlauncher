#!/usr/bin/env fish

cargo build --release
cp target/release/adwlauncher ~/.local/bin/
