# Independent H.264 decoder fixtures

Generated locally from FFmpeg's synthetic red color source; no captured media.
These fixture files are distributed under this repository's MIT OR Apache-2.0
terms. They are encoded output, not linked FFmpeg/libx264 code.

Reproduce each baseline/main/high file with:

```sh
ffmpeg -nostdin -hide_banner -loglevel error -f lavfi \
  -i color=c=red:s=64x48:r=15 -frames:v 1 -c:v libx264 \
  -profile:v PROFILE -pix_fmt yuv420p -threads 1 -f h264 PROFILE.h264
```

The tests read checked-in files and do not require FFmpeg at runtime. Expected
output: 64x48 opaque red RGBA, allowing small codec/color-conversion differences.
