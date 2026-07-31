# Підтримка документації Spider-Tauri

Інструкція для розробників і AI-агентів: коли і як оновлювати внутрішню документацію після змін у коді.

## Файли документації

| Файл | Призначення |
|------|-------------|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | Архітектура, commands/events, моделі даних, константи, команди, розміри |
| [DOC_MAINTENANCE.md](./DOC_MAINTENANCE.md) | Цей файл — правила оновлення |
| `.cursor/rules/spider-tauri.mdc` | Cursor rule: нагадування оновлювати docs при правках |

## Коли оновлювати ARCHITECTURE.md

Оновлення **обов'язкове** в тому ж PR/коміті, що й код, якщо змінилось хоча б одне з нижче:

### Crawl (`src-tauri/src/crawl/`)

- [ ] `maxPages`, `concurrency`, timeout, User-Agent, правила домену
- [ ] Алгоритм обходу, черга, ліміти
- [ ] Обробка robots.txt, redirects, nofollow
- [ ] Поля в об'єкті `spider-result`
- [ ] Нові/видалені Tauri commands або events

### Frontend bridge (`src/api.js`)

- [ ] Методи `window.api`
- [ ] Імена events

### Renderer (`src/`)

- [ ] Нові UI-елементи або потоки даних
- [ ] Колонки / формат CSV export
- [ ] Зміни в `scanResults` або відображенні
- [ ] Памʼять / dump load / IPC payload (див. секцію «Памʼять» в ARCHITECTURE.md)

### Інфраструктура

- [ ] `package.json` / `Cargo.toml` deps
- [ ] Нова структура каталогів
- [ ] `tauri.conf.json`, capabilities, CSP
- [ ] Size comparison після релізної збірки

### Не потребує оновлення ARCHITECTURE.md

- Косметичні зміни UI без зміни поведінки
- Рефакторинг без зміни публічної поведінки
- Коментарі в коді

## Чеклист після правки

1. Прочитати [ARCHITECTURE.md](./ARCHITECTURE.md) і знайти зачеплені секції.
2. Оновити відповідні таблиці, діаграми, константи.
3. Змінити рядок **«Останнє оновлення»** на початку ARCHITECTURE.md на поточну дату.
4. Якщо додано новий тип змін — дописати пункт у DOC_MAINTENANCE.md.

## Для AI-агентів (Cursor)

Правило `.cursor/rules/spider-tauri.mdc` з `alwaysApply: true` нагадує:

- Перед правками — прочитати `docs/ARCHITECTURE.md`.
- Після правок, що змінюють поведінку — оновити docs у тому ж сеансі.
- Не створювати дублікати (README vs ARCHITECTURE) без запиту користувача.
