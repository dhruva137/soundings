# Soundings

**Sound out your Windows PC.** One small, native-feeling app that shows what is on your machine and what has been going wrong with it. It only looks: it never deletes, moves or changes your files.

## Install

| Way | Command |
|---|---|
| Installer | Download `Soundings_x.y.z_x64-setup.exe` from [Releases](https://github.com/dhruva137/soundings/releases/latest) and run it. No admin rights needed. |
| npx | `npx soundings-app` downloads the latest installer, checks its SHA-256 and runs it. |
| winget | `winget install dhruva137.Soundings` (once the manifest is accepted into winget-pkgs). |

Windows 10 or 11, 64-bit. The installer is about 1.6 MB and puts Soundings in your Start menu and on your Desktop.

## What it shows

- **Storage:** a DaisyDisk-style map of the whole drive, large and long-unused files, true duplicates (compared by content), project build folders (`node_modules`, `.venv`, `target`...), and where your downloads came from.
- **Apps:** every installed program, labelled by origin: *you installed it*, *came with the PC*, *manufacturer*, or *Windows*. What you actually use most, and what starts with Windows.
- **Developer:** every copy of Python, Node, Java, Git, Rust, Go, .NET, conda, uv... with versions, sizes and **which one actually runs** on your PATH. Caches and model stores (npm, pip, Hugging Face, Ollama, Docker...) with each tool's own command to clear it.
- **Games:** Steam, Epic, GOG, Xbox, Ubisoft, Rockstar, Minecraft and Roblox libraries, plus engine fingerprints (Unity, Unreal, Godot...) for standalone games, and leftover Steam folders.
- **Stability:** blue screens, hard restarts, GPU resets (`nvlddmkm`, TDR 4101, LiveKernelEvent), WHEA hardware errors, low-memory events and app crashes on one timeline, lined up against driver installs and Windows updates.
- **Crash recorder (opt-in):** a Task Scheduler task with *event triggers* runs `soundings.exe --record-events` only when a matching event is written. Zero idle CPU or battery.

## Fast

- A full drive map runs in parallel using Windows' bulk directory reads (about 33 s for 4 million files on a laptop SSD).
- The last map and duplicate check are saved, so the app reopens with them in well under a second.
- Slow reads (apps, games, dev tools, download sources) are cached and refreshed only when they change or on Refresh. Background work uses a small 3-thread pool, and the app idles at 0% CPU.

## Privacy and safety

- **Never deletes.** There is no delete, recycle or move command anywhere in the app. Mark things on your **Shortlist**, press **Show** to open them in File Explorer, and delete them there yourself if you are sure (they go to the Recycle Bin as usual). The shortlist notices what you removed and tells you how much space you freed.
- Reads names, sizes and dates. Opens file contents only to compare duplicate candidates, when you open the Duplicates tab.
- Never follows junctions or symlinks, and never downloads OneDrive placeholders.
- Folders you skip in Settings are never read, listed or shown to Claude.
- No network connections, no analytics, no telemetry. Everything it keeps (settings, shortlist, last map, caches) stays in `%LOCALAPPDATA%\Soundings`.

## Connect Claude (optional)

Soundings can answer Claude's questions about your PC ("what's filling my drive?", "why does it keep crashing?") through a read-only [MCP](https://modelcontextprotocol.io) server built into the app: `soundings.exe --mcp`. It is **off until you turn it on** in *Settings > Connect Claude*, because whatever Claude reads becomes part of your chat.

- **Claude Desktop:** press *Connect* in that Settings section (it adds Soundings to Claude Desktop's config and keeps a backup), then restart Claude Desktop.
- **Claude Code:** `claude mcp add soundings --scope user -- "%LOCALAPPDATA%\Soundings\soundings.exe" --mcp`, or install the plugin from this repo:
  ```
  /plugin marketplace add dhruva137/soundings
  /plugin install soundings@soundings
  ```
- **Anything else that speaks MCP:** command `npx -y soundings-app mcp`.

Tools: `pc_overview`, `storage_map`, `large_files`, `old_files`, `duplicate_files`, `project_build_folders`, `installed_apps`, `app_usage`, `startup_items`, `developer_tools`, `developer_caches`, `games`, `crash_history`, `downloads_summary`. All are read-only.

## Build

Requirements: Windows 10/11, [Rust](https://rustup.rs) (MSVC toolchain), Visual Studio Build Tools with the C++ workload, Node.js 18+, and the WebView2 runtime (built into Windows 11).

```powershell
npm install
npx tauri build          # installer in src-tauri/target/release/bundle/nsis
npx tauri dev            # run while developing
```

> If your checkout path contains `&`, npm's shims break; run `node node_modules/@tauri-apps/cli/tauri.js build` instead.

Other developer flags: `soundings.exe --bench-scan C:\` times a scan; `soundings.exe --dump mock C:\` writes JSON snapshots so the UI can be previewed in a browser at `src/index.html?mock` (`mock/` holds your own file names and is git-ignored).

## Architecture

| Part | Where |
|---|---|
| Parallel scanner (FindFirstFileEx large fetch, rayon), saved map, duplicate finder (BLAKE3) | `src-tauri/src/scan.rs` |
| Read-only MCP server for Claude | `src-tauri/src/mcp.rs` |
| Installed programs, origin heuristics, usage, startup items | `src-tauri/src/apps.rs` |
| Runtimes, PATH resolution, caches | `src-tauri/src/dev.rs` |
| Game discovery (manifests + engine fingerprints) | `src-tauri/src/games.rs` |
| Event-log reader (wevtapi), classifier, recorder, task registration | `src-tauri/src/stability.rs` |
| npm helper, winget manifests, Claude Code plugin | `packaging/`, `.claude-plugin/` |
| UI: plain ES modules, no framework | `src/` |

## Licence

MIT
