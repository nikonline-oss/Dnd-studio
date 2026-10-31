# Midnight

A safe theme plugin for DndStudio (ADR-0030): cool daylight surfaces and deep navy
night surfaces, with periwinkle accents, rose errors, and mint success colors.
Both modes override all 16 color variables in the current whitelist. Values are
opaque six-digit HEX colors; there is no CSS, code, font, image, or external asset.

Primary and muted text use contrasting colors on the background and surface
variants. Accent controls have a separate contrasting foreground. Light, dark,
and system mode remain device preferences; installing this plugin does not
change another participant's selected theme.

## Package and try locally

`plugin-tool` is currently a placeholder; it does not implement a pack command.
Create a ZIP containing **only** these two files at its root, without a containing
`theme-midnight/` directory:

```text
manifest.json
theme.json
```

Name the resulting archive `theme-midnight-0.0.1.dndplugin` (rename the `.zip`
extension). Do not include this README. For example, from this directory in
PowerShell, using an output filename that does not already exist:

```powershell
Compress-Archive -LiteralPath manifest.json, theme.json -DestinationPath theme-midnight-0.0.1.zip
Rename-Item -LiteralPath theme-midnight-0.0.1.zip -NewName theme-midnight-0.0.1.dndplugin
```

Open a campaign as GM or Co-GM, use **Plugins → Install from file**, and select
the archive. A locally packaged plugin is unsigned. In the appearance settings,
select Midnight and switch between light and dark modes; system mode follows
the operating system. Check text, menus, buttons, keyboard focus, and error
states in both modes. Selecting the default palette or uninstalling Midnight
should restore the built-in colors.

The backend pack/load integration can use this directory as its sample input:
`pack_directory` must include root-level `theme.json`, and `read_package` should
preserve both palettes in the plugin DTO. No generated archive is checked in.

## License

MIT OR Apache-2.0. Author: DndStudio.
