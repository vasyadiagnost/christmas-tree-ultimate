# christmas tree ultimate

[Русский README](README.md)

A Christmas desktop overlay for Windows, based on Snowhush 0.13.1. Animated
lights line three edges of each screen. A hand-painted Christmas tree can be
resized, dragged into place, or moved to another monitor. Santa's sleigh passes
behind the tree, and a snowplow clears a path through the snow as it builds up.

- Choose steady lights, twinkling lights, or a moving wave.
- Keep the tree and lights visible even with snowfall turned off. You can hide
  the decorations over full-screen games and videos.
- Set the snow depth that triggers the plow and the interval between Santa's
  rides. You can also start either ride from the tray menu.
- Adjust Santa's bells and the plow's engine sound separately, or mute both.
  Version 0.2.2 smooths out the gap at the end of the bell loop.

## Install

Download the installer for Windows 10 or 11 on Intel or AMD x64. Setup works
offline and includes the Microsoft Visual C++ x64 runtime. Windows may ask for
administrator permission if that component needs to be installed.
[Download from Releases](https://github.com/vasyadiagnost/christmas-tree-ultimate/releases/latest).

The installer adds a Start menu shortcut and enables launch at sign-in. Use
**Settings → Open at Login** in the system tray menu to turn it off. You can
launch the portable build directly from its extracted archive on a PC with the
runtime installed.

## Run in English

The app opens in Russian by default. If it's already running, quit it from the
tray menu first. For the portable version, open PowerShell in the folder with
the executable and run:

```powershell
.\christmas-tree-ultimate.exe --english
```

For a standard per-user installation:

```powershell
& "$env:LOCALAPPDATA\Programs\ChristmasTreeUltimate\christmas-tree-ultimate.exe" --english
```

The argument applies to that launch. To start in English from a shortcut every
time, append `--english` to its Target field.

## Build and credits

The [build guide](RELEASING.md) covers Windows builds and local audio
preparation. The Christmas features are available in the Windows build.

Snowhush was created by Mykhailo Bezruchko. This project uses the
[PolyForm Noncommercial license](LICENSE). See
[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) for asset and dependency credits.
