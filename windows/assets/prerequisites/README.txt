Offline prerequisite payload for christmas tree ultimate 0.2.2 (Windows x64).

Official Microsoft Visual C++ v14 Redistributable, version 14.52.37029.0.
Downloaded on 12 November 2026 from Microsoft's documented x64 link.
The exact immutable download URL, size and SHA256 are pinned in vc-redist.json.
Build cache: .tools/prerequisites/VC_redist.x64.exe (not committed).
prepare-prerequisites.ps1 checks SHA256, size, version and a valid Microsoft
Authenticode signature. It downloads when absent and never installs anything.

The app is built using MSVC 14.51.36231, so the required runtime is at least
14.51.36231.0. An existing sufficient/newer x64 runtime is retained. The setup
executable uses a static CRT and is checked for external CRT imports, so it can
start on a clean PC before the app's dynamic runtime is installed.

Only the setup embeds the unmodified Microsoft redistributable. When necessary,
it extracts and verifies that package, runs /install /passive /norestart and
checks the installed result. Windows may request UAC permission for that Microsoft
component; the app itself is installed for the current user. Never uninstall or
downgrade an existing runtime, and never automatically reboot Windows.

Windows 10/11 x64 system graphics/audio components are checked, not downloaded
from third-party DLL sites. Missing Windows components require Windows servicing.

Primary references:
https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist
https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files
https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes
