# HebnixMac

A native macOS build of Hebnix — a Rocket League stats overlay, Workshop map
manager, item swapper and plugin host, ported from the Windows/Linux app.

## Install

Open `HebnixMac.dmg` and drag `HebnixMac.app` to Applications. It is ad-hoc
signed; if macOS blocks it, right-click → Open, or run:

```sh
xattr -dr com.apple.quarantine /Applications/HebnixMac.app
```

## First run

1. Launch HebnixMac. On the **Launch** page, set your Rocket League install
   folder (the one containing `RocketLeague.exe`).
2. The **Home** page shows live match stats once the game is running.
3. **Maps**, **Items**, **UPK Studio** and **Game Mods** manage game files.
4. **Player** (Spoofer) changes in-game name, friends and titles. Install the
   local certificate in Settings → System first; spoofing routes the game's
   HTTPS through a local proxy set in Obelisk's Wine prefix.
5. **Plugins** and **Settings** are for extensions and appearance.

## Notes

- Rocket League runs under Obelisk on macOS; Hebnix swaps files and reads the
  game session, it does not launch the game itself.
- Screen capture (the `screen.*` plugin API) needs Screen Recording permission.
- The app uses the HW icon everywhere (Dock, Launchpad, Finder).

## Build from source

```sh
./scripts/build-hebnix-app.sh release     # builds build/HebnixMac.app
```

Requires Rust, Xcode command-line tools and, only for regenerating the icon,
Pillow (`python3 scripts/make-app-icon.py`). All required source files are
tracked in git, so a fresh clone builds without machine-specific setup.
