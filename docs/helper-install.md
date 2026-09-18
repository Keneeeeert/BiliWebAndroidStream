# Helper installation

The Firefox extension cannot execute a downloaded binary or write a native
messaging registration. Download the asset for your platform from the latest
[GitHub Release](https://github.com/Ujhhgtg/BiliWebAndroidStream/releases/latest),
extract it, and run the installer from that directory.

- Linux and macOS: `./install-native-host.sh`
- Windows PowerShell: `./install-native-host.ps1`

The installer copies the helper to a per-user location and registers the
Firefox Native Messaging host. The uninstall script removes that registration
and the installed helper. The extension's Helper section can open the release
page and check the installed helper after installation.

Verify the downloaded archive against `SHA256SUMS` before running it.

## Uninstall

- Linux and macOS: `./uninstall-native-host.sh`
- Windows PowerShell: `./uninstall-native-host.ps1`

Restart Firefox after installing or uninstalling the native host.
