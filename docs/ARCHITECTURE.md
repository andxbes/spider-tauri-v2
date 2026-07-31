# Spider-Tauri — внутрішня документація

> Останнє оновлення: 2026-07-31 (Linux menubar: force Adwaita light GTK theme)  
> Короткий довідник для розробки та правок. Детальніше про підтримку — [DOC_MAINTENANCE.md](./DOC_MAINTENANCE.md).

## Що це

Desktop-краулер на **Tauri v2**: Rust (`src-tauri`) обходить сайт по HTTP і парсить HTML (`reqwest` + `scraper`), frontend (ванільний JS) показує результати та експортує CSV. Контракт UI (`window.api`) збережено з Electron-версії.

## Структура файлів

```
assets/                    # Іконки (джерело)
src/                       # Frontend (webview)
├── api.js                 # Bridge: invoke/listen → window.api
├── index.html
├── renderer.js            # Оркестрація UI
├── shared/                # url-utils, user-agents, hooks, redirect-chain (UI)
├── plugins/               # og-meta, redirect-chain (UI)
└── *.js                   # scan-store, table-*, settings, export…
src-tauri/
├── tauri.conf.json
├── capabilities/default.json
└── src/
    ├── lib.rs             # Tauri builder, plugins, menu
    ├── commands.rs        # settings, session, about, spider control
    ├── settings.rs        # settings.json у AppData
    └── crawl/             # Rust crawl engine
        ├── orchestrator.rs
        ├── network.rs
        ├── html.rs
        ├── queue.rs / state.rs / referrers.rs / results.rs
        ├── probe.rs / sitemap.rs / emit.rs
        └── …
docs/
├── ARCHITECTURE.md
└── DOC_MAINTENANCE.md
```

## Архітектура

```
Renderer (renderer.js)
    ↓ window.api.startSpider(url)
api.js (Tauri invoke)
    ↓ start_spider
Rust orchestrator
    ↓ reqwest + scraper
    ↑ app.emit("spider-*")
api.js listen → Renderer
```

- Мережеві запити — **тільки в Rust**.
- Frontend не має Node; доступ лише через `window.api`.
- `withGlobalTauri: true` — `@tauri-apps/api` через `window.__TAURI__`.

## IPC → Tauri mapping

| Electron (preload) | Tauri |
|--------------------|-------|
| `start-spider` (send) | command `start_spider` |
| `spider-pause` / `resume` / `stop` | `spider_pause` / `spider_resume` / `spider_stop` |
| `settings:get` / `save` | `settings_get` / `settings_save` |
| `shell:open-external` | `open_external` (plugin-opener) |
| `app:getAbout` | `get_about` |
| `session:save` / `save-json` / `load` | `session_save` / `session_save_json` / `session_load` |
| `spider-result` etc. (receive) | events з тими ж іменами |

## Модель даних `spider-result`

Сумісна з Electron / `.spider.json` dump `version: 1` (поле `app`: `spider-tauri`). Поля — див. історичну Electron-документацію; Rust серіалізує camelCase через serde.

## Алгоритм краулера

Семантика як у spider-electron: BFS FIFO, optional sitemap seed, robots.txt, manual redirects (max 20), probe для медіа/зовнішніх, IPC/event batching, pause/resume/stop.

Константи: HTTP timeout 20s (sitemap 60s), delay default 500ms ±20% jitter, concurrency 1–50 (default 3), HTML parse via `spawn_blocking`.

## Налаштування

`settings.json` у AppData. Поля: `useSitemap`, `respectRobotsTxt`, `userAgentPreset` / `userAgentCustom`, `requestDelayMs`, `maxPages`, `concurrency`, `authType` / `authUsername` / `authPassword` / `authToken`.

## Залежності

| Пакет | Використання |
|-------|--------------|
| `tauri` 2 | Desktop shell |
| `reqwest` | HTTP (rustls) |
| `scraper` | HTML parsing |
| `tokio` | async crawl |
| `tailwindcss` | стилі frontend |

## Команди

```bash
npm install
npm run build:css
npm run dev          # tauri dev
npm run build        # tauri build → .deb + release binary
npm run install:linux  # user-local install (Manjaro/Arch): ~/.local/opt + PATH + .desktop
npm run deploy:linux   # build + install:linux
```

`scripts/install-linux.sh` шукає бінарник у `dist/spider-tauri` або `src-tauri/target/release/spider-tauri`, ставить у `~/.local/opt/spider-tauri`, symlink у `~/.local/bin`, іконку та `.desktop` у XDG. Root не потрібен.
## Size comparison (Linux amd64, виміряно 2026-07-31)

| Артефакт | Electron 1.0.1 | Tauri 1.0.0 | Співвідношення |
|----------|----------------|------------|----------------|
| Дистрибутив | zip **~121 MB** | `.deb` **~3.7 MB** | ≈ **33×** менше |
| Unpacked / binary | dir **~317 MB** (бінарник Electron ~207 MB) | release binary **~8.7 MB** | ≈ **36×** менше (dir) / ≈ **24×** (binary) |

Таuri використовує системний WebView (WebKitGTK на Linux) замість вбудованого Chromium — основна економія місця.

**Linux menubar:** нативне меню бере кольори з GTK. Якщо системна тема dark/mixed — текст меню може бути білим на світлій смузі. У `main.rs` (і в `.desktop` після `install:linux`) форсується `GTK_THEME=Adwaita` + `GTK_APPLICATION_PREFER_DARK_THEME=0`, якщо змінні ще не задані користувачем.

## Типові місця для правок

| Задача | Де шукати |
|--------|-----------|
| Краулер, timeout, UA | `src-tauri/src/crawl/` |
| Нові поля HTML | `crawl/html.rs` + UI hooks |
| UI / CSV | `src/ui-*.js`, `export-csv.js` |
| Нова команда/подія | `commands.rs` + `api.js` |
| Стилі | `src/input.css` |

## Тести

```bash
cd src-tauri && cargo test
```

Покриття (unit): url_utils, robots.txt, auth hostname-scope, redirect loops, meta robots, user-agent resolve.

## Команди Tauri (Rust)

| Command | Призначення |
|---------|-------------|
| `start_spider` | Старт скану |
| `spider_pause` / `spider_resume` / `spider_stop` | Керування |
| `settings_get` / `settings_save` | `settings.json` у AppData |
| `open_external` | Відкрити URL у браузері |
| `get_about` | Метадані «Про програму» |
| `session_save` / `session_save_json` / `session_load` | Дамп `.spider.json` |

Events (імена як в Electron): `spider-result`, `spider-results-batch`, `spider-progress`, `spider-referrers-update`, `spider-end`, `session-dump-request-save`, `session-dump-loaded`, `about-show`.
