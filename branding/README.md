# Branding

Every logo and icon Bolum shows comes from this folder. Replace a file here, keep its
name, and run:

```sh
npm run icons
```

| File | Used for | Format |
| --- | --- | --- |
| `app-icon.svg` | Dock, Finder, Launchpad, the `.dmg`, Linux packages | Square SVG, or `app-icon.png` at 1024×1024. On macOS, keep the artwork inside the centred 824×824 area. |
| `logo.svg` | In-app logo: sidebar, About page, welcome banner | Square SVG. It is loaded directly, so it needs no build step. |
| `mark.svg` | The mark on its own, for other uses | SVG |
| `logo-full.svg` | Mark and wordmark, for the README, a website or release notes | SVG |
| `tray/idle-template.svg` | macOS menu bar while idle | Black on a transparent background. macOS tints it for light and dark menu bars. |
| `tray/idle.svg` | Linux tray while idle | A mid grey that reads on light and dark panels |
| `tray/recording.svg` | Menu bar while listening | Any colour |
| `tray/transcribing.svg` | Menu bar while transcribing | Any colour |

`npm run icons` does two things:

- It turns `app-icon.svg` (or `.png`) into every size in `src-tauri/icons/`.
- It renders each `tray/*.svg` to the 44×44 `tray/*.png` the app embeds. 44×44 is 22 pt at 2×,
  the macOS menu-bar size.

If you would rather draw the tray icons as PNGs, delete the matching `.svg` and save a 44×44
PNG under the same name.

Restart `npm run tauri dev` after changing the app or tray icons. The tray icons are compiled
into the binary, and macOS caches app icons.
