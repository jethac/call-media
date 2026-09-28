# Source attribution

Audio processing, device capture, H.264 helpers, screen capture, diagnostics, and
Linux live decoding are adapted from [Serein](https://github.com/ViceVerse-cz/Serein)
at commit `1ecf8d1695728e42ec4f47174fa1dcbd160f859e`.
Copyright (c) 2026 Serein contributors. Licensed MIT OR Apache-2.0;
the original license texts are retained in `LICENSES/`.

The extraction replaces application settings imports with local media types,
separates device features from always-available transport data types, and removes
the desktop platform dependency. Upstream live-device claims remain unverified
until exercised on actual hardware. No UI code or credential handling is included.
