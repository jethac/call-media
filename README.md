# call-media

Reusable Rust audio/video device and codec adapters for calling applications.

**Status: initial scaffold. No device or codec implementation is provided yet.**
The crate is not published to crates.io and its public API is not established.

## Planned scope

- Selected microphone and speaker capture/playback with bounded buffers.
- Echo cancellation using the playback reference, and noise suppression.
- Camera or HDMI capture inputs and explicit device ownership.
- Encoding and decoding adapters, with software fallbacks where practical.
- Explicit cancellation and prompt release of devices.

Start with Linux. Public interfaces should allow caller-provided media and keep
platform dependencies optional. Device controls must coordinate with the host
application so previews and calls do not compete for the same camera.

[discord-core](https://github.com/jethac/discord-core) is the first intended
consumer. Discord signaling and encryption belong there. Future calling
integrations, including Google Meet, may reuse these adapters once their
requirements are understood; no Meet support is implemented or promised here.

The initial crate establishes ownership only. Choose device/codec dependencies
and settle frame formats while extracting the first working media path.

## Development

```sh
cargo check --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
```

## License

Licensed under either the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
or the MIT license ([LICENSE-MIT](LICENSE-MIT)), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
