# call-media

Headless Rust audio/video devices and codecs for calling applications. Public
OSS under **MIT OR Apache-2.0**, with optional native adapters and no UI toolkit.
[discord-core](https://github.com/jethac/discord-core) is the first consumer;
service signaling and encryption live there.

## Status

Initial implementation, unstable API, not published to crates.io. Linux is the
initial target. Camera and screen code includes macOS/Windows backends, but those
platforms have not been validated in this extraction. Live Discord and physical
Hub interoperability have not been validated.

| Feature | Implementation |
|---|---|
| default | PCM/frame types, processing settings, screen-sharing interfaces, bounded Annex B parsing |
| `audio` | CPAL microphone/speaker selection, 48 kHz mono PCM, bounded queues, echo cancellation, noise suppression, mute/deafen, gain and processing controls |
| `codecs` | Software H.264 encoder/decoder for caller-owned RGB24/Annex B data |
| `video` | Camera capture, H.264 encoding, desktop/portal capture, Linux V4L2 sharing input, optional hardware encode/decode |

Video is required alongside audio: camera sending, incoming decoded video, and
HDMI sharing are supported by distinct interfaces. Protocol-specific transport
is the responsibility of the calling service library.

## Media ownership

- No device is opened by importing the crate. Start capture explicitly and retain
  the worker for its lifetime. Stop/drop it to release the device.
- `Frame` is 960 mono `f32` samples: 20 ms at 48 kHz. Audio capture/playback use
  bounded standard-library queues. Use nonblocking sends from real-time code.
- `audio::Audio` manages selected devices and processing. Coordinate camera
  preview and active calls: they must not compete for the same hardware.
- `camera::Camera` supplies RGB preview and independently decodable H.264 at
  640x480/15 fps. `codec::Encoder` accepts caller-owned RGB24 at other supported
  sizes, up to 1080p pixel count. Codec work belongs off the UI thread.
- `screen::Video` carries bounded encoded frames, readiness/keyframe/bitrate
  feedback, and optional 48 kHz interleaved stereo audio. Honor readiness and
  audio epochs so buffered media does not cross an encryption transition.
- On Linux, `screen::SourceId::VideoDevice(n)` selects `/dev/videoN`, including
  HDMI capture cards. V4L2 source negotiation handles raw/encoded video through
  GStreamer. Hardware compatibility remains unverified.
- HDMI audio must be supplied separately through `Video.audio`; requesting
  desktop loopback for a V4L2 input is rejected. Automatic HDMI audio-device
  pairing is not implemented.

## Native dependencies

Default builds need no audio/video system libraries. OpenH264 source compilation
(`codecs`) needs a C/C++ build toolchain. Linux `audio` needs ALSA and PulseAudio
development packages; `video` needs GStreamer base development libraries and
runtime plugins for the selected capture/encoder/decoder pipeline. On Ubuntu:

```sh
sudo apt-get install build-essential pkg-config libasound2-dev libpulse-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav
```

Wayland desktop capture also requires a running desktop portal/PipeWire service
and user approval through its chooser. V4L2 requires access to the selected node.

## Development and evidence

```sh
cargo check --workspace --all-features --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-features --locked -- -D warnings
cargo doc --workspace --all-features --no-deps --locked
```

Local automated checks exercised PCM processing, source validation and synthetic
H.264 encode/decode. They do not establish camera/HDMI compatibility, acoustic
echo performance, latency or device release on a physical Hub. See
[THIRD_PARTY.md](THIRD_PARTY.md) for source attribution. Meet integration is future
work; this crate contains no Meet protocol implementation.

## License

Licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your
option. Contributions are dual licensed on the same terms unless stated otherwise.
