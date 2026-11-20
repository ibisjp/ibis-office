# Kenney's sounds (CC0)

Sounds of Kenney Vleugels (<https://www.kenney.nl>), released as **CC0 1.0 Universal** (`LICENSE` is its legal code; the packs' own `License.txt` say the same: free for personal, educational
and commercial use, credit not mandatory). Kenney's own download page for a pack only gives the file to a browser, so these were taken from the packs **Kenney uploaded to OpenGameArt**
(each page shows the licence CC0 and the uploader Kenney, <https://opengameart.org/content/all-cc0-uploader-kenney>); the zips were downloaded on 2026-11-19.

| shipped file | made from | pack (page) |
|---|---|---|
| `bomb.ogg` | `lowFrequency_explosion_000.ogg` | Sci-fi Sounds, <https://opengameart.org/content/sci-fi-sounds> (`sci-fi_sounds.zip`) |
| `explosion.ogg` | `explosionCrunch_000.ogg` | Sci-fi Sounds |
| `hammer.ogg` | `impactMetal_000.ogg` | Sci-fi Sounds |
| `laser.ogg` | `laserSmall_000.ogg` | Sci-fi Sounds |
| `chime.ogg` | `confirmation_001.ogg` | Interface Sounds, <https://opengameart.org/content/interface-sounds> (`kenney_interfaceSounds.zip`) |
| `click.ogg` | `click_001.ogg` | Interface Sounds |
| `push.ogg` | `maximize_003.ogg` | Interface Sounds |
| `camera.ogg` | `metalClick.ogg` | RPG Audio, <https://opengameart.org/content/50-rpg-sound-effects> (`RPGsounds_Kenney.zip`) |
| `coin.ogg` | `handleCoins.ogg` | RPG Audio |
| `suction.ogg` | `phaserDown1.mp3` | Digital SFX Set, <https://opengameart.org/content/63-digital-sound-effects-lasers-phasers-space-etc> (`Digital_SFX_Set.zip`; its readme: "You may use these sounds in personal and commercial projects") |
| `voltage.ogg` | `zap1.mp3` | Digital SFX Set |

What was changed: each was made mono, 44.1 kHz, cut to at most 3 s, faded for 4 ms at the start and 30 ms at the end,
normalised to a peak of -3 dBFS and encoded as Ogg Vorbis (`scripts/transition-sounds/build.py`). The sources are not in the repository (`test/sounds-src/`, ignored).
