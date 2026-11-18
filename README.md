# HebnixMac

Native macOS release of Hebnix — a Rocket League stats overlay, Workshop map
manager, item swapper and plugin host, ported from the Windows/Linux app.

## Install

Download `releases/HebnixMac.dmg`, open it and drag `HebnixMac.app` to
Applications. It is ad-hoc signed; if macOS blocks it, right-click → Open, or:

```sh
xattr -dr com.apple.quarantine /Applications/HebnixMac.app
```

## First run

1. On the **Launch** page, set your Rocket League install folder.
2. **Home** shows live match stats once the game is running.
3. **Maps / Items / UPK Studio / Game Mods** manage game files.
4. **Player** changes in-game name, friends and titles. Install the local
   certificate in Settings → System first.
5. **Plugins** and **Settings** are for extensions and appearance.

Rocket League runs under Obelisk on macOS; Hebnix swaps files and reads the
game session, it does not launch the game itself. Screen capture (`screen.*`
plugin API) needs Screen Recording permission.

## Build from source

Requires Rust and the Xcode command-line tools:

```sh
./scripts/build-hebnix-app.sh release   # -> build/HebnixMac.app
```

To regenerate the app icon (needs Pillow): `python3 scripts/make-app-icon.py`.
All sources needed to build are committed here — no machine-specific setup.

See `macos/README.md` for platform details. Licensed under LICENSE.md.
