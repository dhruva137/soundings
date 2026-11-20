# Soundings

**Sound out your Windows PC.** One small, native-feeling app that shows what is on your machine and what has been going wrong with it:

- **Storage:** a DaisyDisk-style map of the whole drive, large and long-unused files, true duplicates (compared by content), project build folders (`node_modules`, `.venv`, `target`, …), and where your downloads came from.
- **Apps:** every installed program, labelled by origin: *you installed it*, *came with the PC*, *manufacturer*, or *Windows*. Also everything that starts with Windows.
- **Developer:** every copy of Python, Node, Java, Git, Rust, Go, .NET, conda, uv… with versions, sizes and **which one actually runs** on your PATH. Caches and model stores (npm, pip, Hugging Face, Ollama, Docker…) with the tool's own command to clear each one.
- **Games:** Steam, Epic, GOG, Xbox, Ubisoft, Rockstar, Minecraft and Roblox libraries, plus engine fingerprints (Unity, Unreal, Godot…) for standalone games. Leftover Steam folders too.
- **Stability:** blue screens, hard restarts, GPU resets (`nvlddmkm`, TDR 4101, LiveKernelEvent), WHEA hardware errors, low-memory events and app crashes on one timeline, lined up against graphics-driver installs and Windows updates.
- **Crash recorder (opt-in):** a Task Scheduler task with *event triggers* runs `soundings.exe --record-events` only when a matching event is written, and at sign-in. **Zero idle CPU or battery.** It keeps history longer than Windows does and shows a notification for each new incident.

## Safety model

- Reads names, sizes and timestamps. Opens file contents only to compare duplicate candidates, when you ask.
- Never follows junctions or symlinks, and never hydrates OneDrive placeholders.
- **The only removal is "Move to Recycle Bin"** from the Review list, for items the app itself listed, with Undo. Paths under `C:\Windows` and `C:\Program Files` are refused.
- Folders can be excluded entirely in Settings (they are never read or listed).
- Nothing leaves your PC. No telemetry.

## Build

Requirements: Windows 10/11, [Rust](https://rustup.rs) (MSVC toolchain), Visual Studio Build Tools with the C++ workload, Node.js 18+, and the WebView2 runtime (built into Windows 11).

```powershell
npm install
npx tauri build          # installer in src-tauri/target/release/bundle/nsis
npx tauri dev            # run while developing
```

> If your checkout path contains `&`, npm's shims break; run `node node_modules/@tauri-apps/cli/tauri.js build` instead.

### Preview the UI in a browser

```powershell
src-tauri\target\release\soundings.exe --dump mock C:\
python -m http.server 5180    # then open http://127.0.0.1:5180/src/index.html?mock
```
`mock/` contains your own file names and is git-ignored.

## Architecture

| Part | Where |
|---|---|
| Parallel scanner (rayon, sizes only) and duplicate finder (BLAKE3) | `src-tauri/src/scan.rs` |
| Installed programs, origin heuristics, startup items | `src-tauri/src/apps.rs` |
| Runtimes, PATH resolution, caches | `src-tauri/src/dev.rs` |
| Game discovery (manifests + engine fingerprints) | `src-tauri/src/games.rs` |
| Event-log reader (wevtapi), classifier, recorder, task registration | `src-tauri/src/stability.rs` |
| UI: plain ES modules, no framework | `src/` |

## Licence

MIT
