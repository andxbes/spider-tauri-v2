# Spider Tauri

Desktop web crawler — Tauri v2 + Rust port of spider-electron.

**Size (Linux):** `.deb` ~3.7 MB / binary ~8.7 MB vs Electron zip ~121 MB (~33× smaller package).

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for internals. Keep docs in sync — [docs/DOC_MAINTENANCE.md](docs/DOC_MAINTENANCE.md).

```bash
npm install
npm run dev           # development
npm run build         # production .deb + binary
npm run install:linux # Manjaro/user-local: ~/.local/opt + PATH + меню
npm run deploy:linux  # build + install
cd src-tauri && cargo test
```