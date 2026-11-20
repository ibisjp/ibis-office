# ibis inc.'s own sounds

Five of the 18 transition sounds are made by `scripts/transition-sounds/build.py` from noise and tones (numpy, fixed seeds), so that they are ibis inc.'s and the same each time it is run:

| file | how |
|---|---|
| `arrow.ogg` | a low-passed noise whoosh that swells and fades, then a 95 Hz thud (0.9 s) |
| `breeze.ogg` | soft band-limited noise (300 to 2 200 Hz) with a slow swell (2 s) |
| `cash-register.ogg` | a drawer click (a short noise burst), then a bell of three partials (2 093, 3 135 and 4 186 Hz) decaying (1.6 s) |
| `drum-roll.ogg` | 2.2 s of short noise-and-tone hits (190 Hz), getting quicker and louder |
| `wind.ogg` | low-passed noise (900 Hz) with a slow swell (2.6 s) |

They are normalised to a peak of -3 dBFS, 44.1 kHz and encoded as Ogg Vorbis (ffmpeg's native `vorbis` encoder). Copyright (c) ibis inc.; the licence is the project's (see `LICENSE`).
