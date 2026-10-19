# Third-party software and data

Glass Cannon itself is MIT licensed (see `LICENSE`). It builds on, or works alongside, the
following, none of which is covered by that licence.

## Game data (downloaded, not shipped)

Skill names and icons, class icons and mob names are **not included** in this repository
or in the release zip. The meter downloads them on first start (and from settings →
update game data) into a `gamedata` folder next to the exe, from:

- [NOIA2](https://github.com/ZDYoung0519/NOIA2) by ZDYoung0519, GPL-3.0: skill names,
  skill and class icons, boss lists.
- [RATmeter (AIon2-Dps-Meter)](https://github.com/Kuroukihime/AIon2-Dps-Meter) by
  Kuroukihime, GPL-3.0: English mob names.

Those files stay under their projects' licences. The underlying names and artwork are
from Aion 2 and belong to their owners.

## Npcap (installed separately)

Packet capture uses [Npcap](https://npcap.com), loaded at runtime from its install folder.
Npcap's free licence doesn't allow bundling it, so it isn't included; install it once
from npcap.com.

## Microsoft Edge WebView2

The windows are drawn by the WebView2 runtime, which ships with Windows 11.

## Rust crates

The programs are built with [Tauri](https://tauri.app) (MIT/Apache-2.0) and other Rust
crates, all under permissive licences (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0,
MPL-2.0, CDLA-Permissive-2.0 and similar). The release zip lists every crate with its
licence in `THIRD-PARTY-CRATES.txt`; `Cargo.lock` pins the exact versions.

## Trademarks

Aion and Aion 2 are trademarks of NCSOFT Corporation. This is an unofficial fan project,
not affiliated with or endorsed by NCSOFT. It only reads the game's network traffic on
your own PC; it doesn't modify the game or its memory.
