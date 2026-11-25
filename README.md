<p align="center">
  <img src="assets/paprika.png" width="110" alt="Paprika Launcher logo">
</p>

<h1 align="center">Paprika Launcher</h1>
<p align="center"><b>A lightweight Minecraft launcher for Linux and Windows, with Modrinth built in.</b></p>
<p align="center">Made by <a href="https://github.com/yeeboein">Yeeboein</a></p>

---

Paprika Launcher is a small, fast Minecraft: Java Edition launcher. It's one native app (about 5–7 MB) with no Electron, no Java needed to run the launcher, and nothing to install first.

What makes it different: **Modrinth is part of the launcher.** Browse mods, modpacks, resource packs and shaders inside the app, click **Install**, and they go straight into your instance with their dependencies. You can add mods whenever you like, before or after you've started playing.

🎬 **[Watch the 1-minute trailer](docs/paprika-trailer.mp4)**

![Browse Modrinth](docs/browse.png)

## Features

- 🌶️ **One-click Modrinth installs**: search, sort and install mods, resource packs and shaders straight into an instance. Results only show what works with that instance's Minecraft version and loader.
- 📦 **Automatic dependencies**: install Iris and Sodium comes with it.
- 🎁 **One-click modpacks**: install any Fabric, Quilt, Forge or NeoForge modpack from Modrinth and it becomes a ready-to-play instance.
- ⟳ **Update all mods** with one button (works for mods you added by hand too, as long as Modrinth knows them).
- 🧵 **Fabric, Quilt, Forge and NeoForge**, plus vanilla. Paprika installs the loader for you.
- ☕ **Java included**: Paprika downloads the exact Java runtime each Minecraft version needs, so you never install Java yourself.
- 👤 **Microsoft accounts** with a built-in step-by-step setup wizard, plus **offline accounts** for singleplayer/LAN.
- 🗂️ **Separate instances**, each with its own mods, worlds, settings and memory.
- ✅ Enable/disable or remove mods, open folders, rename and delete instances.
- 🖥️ Live game console.
- 🪶 Lightweight: a native Rust + egui app, and downloads are verified with SHA-1 and run in parallel.

![Instance view](docs/instance.png)

## Download

Get the latest build from [**Releases**](https://github.com/yeeboein/paprika-launcher/releases):

| Platform | File |
|---|---|
| Windows (x64) | `paprika-windows-x86_64.exe` (just run it) or the `.zip` |
| Linux (x64) | `paprika-linux-x86_64.tar.gz` |

**Linux:** extract it and run the installer. It adds Paprika Launcher, with its icon, to your app menu (no root needed):

```bash
tar -xzf paprika-linux-x86_64.tar.gz && cd paprika && ./install.sh
```

Run `./install.sh uninstall` to remove it. Your worlds are kept.

## Build from source

You need [Rust](https://rustup.rs) (stable).

```bash
git clone https://github.com/yeeboein/paprika-launcher
cd paprika-launcher
cargo build --release
```

The app ends up in `target/release/paprika` (or `paprika.exe`). On Linux, run `./install.sh` to add it to your app menu with its icon.

Low on RAM (4 GB)? Build with lighter settings:

```bash
CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=2 cargo build --release -j 2
```

## Microsoft sign-in setup

Open **Accounts → Set up Microsoft sign-in**. A short wizard walks you through it, with buttons that open each page you need:

1. Create a free Azure app (choose **Personal Microsoft accounts only**).
2. Turn on **Allow public client flows**.
3. Paste the **Application (client) ID**.
4. Send the ID to Mojang with [their review form](https://aka.ms/mce-reviewappid). New apps need approval before they can log into Minecraft.

After that, it's just **Sign in with Microsoft**.

To skip this for everyone who downloads your builds, set the `PAPRIKA_MSA_CLIENT_ID` environment variable at build time. For GitHub builds, add a repository *variable* with that name.

Offline accounts work without any of this.

## Where files live

| OS | Folder |
|---|---|
| Linux | `~/.local/share/paprika` |
| Windows | `%APPDATA%\paprika` |

- **Portable mode:** put an empty `portable.txt` next to the executable and Paprika keeps everything in `paprika-data/` beside it (handy on a USB stick).
- Or set `PAPRIKA_HOME` to any folder.

## Releasing

Publish a release on GitHub (for example tag `v0.1.0`). The **Build** workflow builds Linux and Windows and attaches the files to that release.

## Roadmap

- Import instances from other launchers
- Mod changelogs & version picker
- Skin preview

## License

MIT © Yeeboein. See [LICENSE](LICENSE).

Paprika isn't an official Minecraft product and isn't approved by or associated with Mojang or Microsoft. Mod data comes from [Modrinth](https://modrinth.com).
