# soundings-app

Installs [Soundings](https://github.com/dhruva137/soundings), a free, read-only Windows app that shows what is on your PC: a disk map, large and duplicate files, installed apps, developer tools, games and crash history. It never deletes anything.

```powershell
npx soundings-app            # download the latest installer, check its SHA-256, run it
npx soundings-app open       # open Soundings
npx soundings-app mcp        # run it as a read-only MCP server for Claude
```

To let Claude use it, turn on **Settings > Connect Claude** in the app first; it is off by default.
