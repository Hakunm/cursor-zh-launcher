<h1 align="center">Cursor Chinese Launcher</h1>

<p align="center">
  Broader Simplified Chinese coverage for Cursor on Windows, without modifying official files.
</p>

<p align="center">
  <a href="README.md">简体中文</a> · <strong>English</strong>
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-v1.0.1-0969da" />
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20x64-0078d4" />
  <img alt="License" src="https://img.shields.io/badge/license-GPL--3.0--only-2da44e" />
  <img alt="No telemetry" src="https://img.shields.io/badge/telemetry-none-8250df" />
  <a href="https://github.com/Hakunm/cursor-zh-launcher/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/Hakunm/cursor-zh-launcher/actions/workflows/ci.yml/badge.svg" /></a>
</p>

> [!IMPORTANT]
> This is an unofficial community project and is not affiliated with or endorsed by Anysphere,
> Cursor, Microsoft, or OpenAI. The current release is unsigned and may trigger Windows
> SmartScreen.

<p align="center">
  <img src="docs/images/cursor-agents.png" alt="Cursor Agents in Simplified Chinese" width="100%" />
</p>

## Download and install

1. Open the latest [GitHub Release](https://github.com/Hakunm/cursor-zh-launcher/releases/latest)
   and download `CursorZhLauncher-1.0.1-Setup.exe`.
2. Fully exit every running Cursor window.
3. Run the installer, then open **Cursor 中文启动器 (Cursor Chinese Launcher)** from the
   desktop.

The installer keeps the original Cursor entry intact and adds a separate launcher that uses
the locally installed Cursor icon.

> [!TIP]
> If SmartScreen appears, verify the SHA-256 published with the GitHub Release before choosing
> **More info** to continue.

## Features

- Uses the official Simplified Chinese language pack matched to Cursor's embedded VS Code build.
- Adds coverage for Cursor Agents, IDE, settings, Automations, customization pages, and menus.
- Checks language state on every launch and repairs configuration reset by Cursor updates.
- Does not modify `Cursor.exe` or any official file in the Cursor installation directory.
- Collects no telemetry, accounts, chats, source code, or local file contents.

## Screenshots

<p align="center">
  <img src="docs/images/cursor-ide.png" alt="Cursor IDE home screen in Simplified Chinese" width="100%" />
  <br />
  <sub>The IDE menubar, titlebar, and welcome actions are localized.</sub>
</p>

<p align="center">
  <img src="docs/images/cursor-settings.png" alt="Cursor settings in Simplified Chinese" width="100%" />
  <br />
  <sub>Navigation, descriptions, options, and actions in Cursor settings are localized.</sub>
</p>

## Compatibility

The launcher detects the installed Cursor build and aims to support most Windows x64 versions.
A different version does not disable localization: safe, narrowly scoped translations remain
active. Major future UI changes may temporarily leave some new pages in English until the next
compatibility update.

## Safety and privacy

- The official Cursor installation remains read-only and its core file checksums stay intact.
- Only the launcher's own directories and recoverable user-language settings are written.
- Existing language files are backed up and restored conditionally during uninstall.
- Local communication uses random loopback-only ports and is never exposed to the network.
- Update checks only query this repository's GitHub Releases and never install silently.

## FAQ

**Will a Cursor update completely break localization?**

Usually not. The launcher prepares the language environment again on the next start, and safe
exact translations remain active. A major redesign can temporarily leave some text in English.

**Why must Cursor be closed first?**

A running Cursor process does not reload its startup language state. The launcher will not
force-close windows that may contain unsaved work.

**Can I return to the original interface?**

Yes. Uninstalling Cursor Chinese Launcher restores language settings that are still managed by
the tool. The official Cursor application and its original shortcut are always preserved.

**Why does SmartScreen appear?**

The current release does not have a commercial code-signing certificate. Download only from
this repository and verify the SHA-256 supplied with the release.

## License

Licensed under [GNU GPL v3.0 only](LICENSE). See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for dependency notices. Cursor, Microsoft
language-pack VSIX files, and third-party localization packages are not redistributed.
