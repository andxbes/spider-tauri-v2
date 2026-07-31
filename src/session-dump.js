const SESSION_DUMP_VERSION = 1;
const WORKSPACE_STORAGE_KEY = 'spider-tauri.workspace.v1';
const WORKSPACE_SELECTED_URL_KEY = 'spider-tauri.workspace.selectedUrl.v1';
const WORKSPACE_VERSION = 1;

function cloneResultEntry(data) {
    const entry = {
        url: data.url,
        status: data.status,
        title: data.title ?? '',
        metaDescription: data.metaDescription ?? '',
        metaCanonical: data.metaCanonical ?? '',
        contentType: data.contentType ?? '',
        metaRobots: data.metaRobots ?? '',
        metaRobotsStatus: data.metaRobotsStatus ?? 'none',
        metaRobotsLabel: data.metaRobotsLabel ?? '',
        xRobotsTag: data.xRobotsTag ?? '',
        xRobotsTagStatus: data.xRobotsTagStatus ?? 'none',
        xRobotsTagLabel: data.xRobotsTagLabel ?? '',
        responseHeaders: Array.isArray(data.responseHeaders)
            ? data.responseHeaders.map((header) => ({ ...header }))
            : [],
        robotsAllowed: data.robotsAllowed ?? null,
        robotsRule: data.robotsRule ?? '',
        responseTimeMs: data.responseTimeMs ?? null,
        redirectUrl: data.redirectUrl ?? '',
        redirectHopCount: data.redirectHopCount ?? 0,
        redirectFinalUrl: data.redirectFinalUrl ?? '',
        redirectInfinite: Boolean(data.redirectInfinite),
        redirectChain: Array.isArray(data.redirectChain) ? [...data.redirectChain] : [],
        redirectLoopStartUrl: data.redirectLoopStartUrl ?? '',
        redirectHopOnly: Boolean(data.redirectHopOnly),
        external: Boolean(data.external),
        fetched: data.fetched ?? (data.status !== '' && data.status !== undefined && data.status !== null),
        kind: data.kind ?? '',
        tag: data.tag ?? '',
        text: data.text ?? data.linkText ?? data.title ?? '',
        imgAltMissing: data.imgAltMissing === true,
        referrers: Array.isArray(data.referrers)
            ? data.referrers.map((ref) => (
                typeof ref === 'string'
                    ? { href: ref, text: '' }
                    : {
                        href: ref.href ?? '',
                        text: ref.text ?? '',
                        rel: ref.rel ?? '',
                        tag: ref.tag ?? '',
                        kind: ref.kind ?? '',
                        relFollowAllowed: ref.relFollowAllowed ?? null,
                        relIndexAllowed: ref.relIndexAllowed ?? null,
                        relLabel: ref.relLabel ?? '',
                        imgAltMissing: ref.imgAltMissing === true,
                        ...(ref.imgAlt !== undefined ? { imgAlt: ref.imgAlt } : {}),
                        ...(Array.isArray(ref.imgAltStates) && ref.imgAltStates.length
                            ? {
                                imgAltStates: ref.imgAltStates.map((state) => ({
                                    tag: state.tag ?? '',
                                    imgAltMissing: state.imgAltMissing === true,
                                    ...(state.imgAlt !== undefined ? { imgAlt: state.imgAlt } : {}),
                                })),
                            }
                            : {}),
                    }
            ))
            : [],
        headings: Array.isArray(data.headings) ? data.headings.map((heading) => ({ ...heading })) : [],
        ogTitle: data.ogTitle ?? '',
        ogDescription: data.ogDescription ?? '',
        ogImage: data.ogImage ?? '',
    };
    if (data.imgAlt !== undefined) {
        entry.imgAlt = data.imgAlt;
    }
    return entry;
}

/**
 * Compact dump for IPC transfer: skips responseHeaders and redirectChain
 * (large arrays rarely needed after scan; saves ~60-80% of payload size).
 */
function cloneResultEntryCompact(data) {
    const entry = {
        url: data.url,
        status: data.status,
        title: data.title ?? '',
        metaDescription: data.metaDescription ?? '',
        metaCanonical: data.metaCanonical ?? '',
        contentType: data.contentType ?? '',
        metaRobots: data.metaRobots ?? '',
        metaRobotsStatus: data.metaRobotsStatus ?? 'none',
        metaRobotsLabel: data.metaRobotsLabel ?? '',
        xRobotsTag: data.xRobotsTag ?? '',
        xRobotsTagStatus: data.xRobotsTagStatus ?? 'none',
        xRobotsTagLabel: data.xRobotsTagLabel ?? '',
        robotsAllowed: data.robotsAllowed ?? null,
        robotsRule: data.robotsRule ?? '',
        responseTimeMs: data.responseTimeMs ?? null,
        redirectUrl: data.redirectUrl ?? '',
        redirectHopCount: data.redirectHopCount ?? 0,
        redirectFinalUrl: data.redirectFinalUrl ?? '',
        redirectInfinite: Boolean(data.redirectInfinite),
        redirectLoopStartUrl: data.redirectLoopStartUrl ?? '',
        redirectHopOnly: Boolean(data.redirectHopOnly),
        external: Boolean(data.external),
        fetched: data.fetched ?? (data.status !== '' && data.status !== undefined && data.status !== null),
        kind: data.kind ?? '',
        tag: data.tag ?? '',
        text: data.text ?? data.linkText ?? data.title ?? '',
        imgAltMissing: data.imgAltMissing === true,
        referrers: Array.isArray(data.referrers)
            ? data.referrers.map((ref) => (
                typeof ref === 'string'
                    ? { href: ref, text: '' }
                    : {
                        href: ref.href ?? '',
                        text: ref.text ?? '',
                        rel: ref.rel ?? '',
                        tag: ref.tag ?? '',
                        kind: ref.kind ?? '',
                        relFollowAllowed: ref.relFollowAllowed ?? null,
                        relIndexAllowed: ref.relIndexAllowed ?? null,
                        relLabel: ref.relLabel ?? '',
                        imgAltMissing: ref.imgAltMissing === true,
                        ...(ref.imgAlt !== undefined ? { imgAlt: ref.imgAlt } : {}),
                        ...(Array.isArray(ref.imgAltStates) && ref.imgAltStates.length
                            ? {
                                imgAltStates: ref.imgAltStates.map((state) => ({
                                    tag: state.tag ?? '',
                                    imgAltMissing: state.imgAltMissing === true,
                                    ...(state.imgAlt !== undefined ? { imgAlt: state.imgAlt } : {}),
                                })),
                            }
                            : {}),
                    }
            ))
            : [],
        headings: Array.isArray(data.headings) ? data.headings.map((heading) => ({ ...heading })) : [],
        ogTitle: data.ogTitle ?? '',
        ogDescription: data.ogDescription ?? '',
        ogImage: data.ogImage ?? '',
    };
    if (data.imgAlt !== undefined) {
        entry.imgAlt = data.imgAlt;
    }
    return entry;
}

/**
 * Build the dump as a JSON string to avoid Structured Clone overhead when
 * passing a large JS object over IPC. The string is written directly to disk
 * by the main process without any re-serialisation.
 * Uses the compact entry format (no responseHeaders / redirectChain).
 *
 * @deprecated Prefer streamSessionDumpSave — full stringify OOMs WebKit on large scans.
 */
function buildSessionDumpJson({
    scanResults,
    insertionOrder,
    startUrl,
    uiState,
    lastScanProgress,
    settings,
}) {
    const results = insertionOrder
        .map((url) => scanResults.get(url))
        .filter(Boolean)
        .map(cloneResultEntryCompact);

    const payload = {
        version: SESSION_DUMP_VERSION,
        app: 'spider-tauri',
        savedAt: new Date().toISOString(),
        startUrl: startUrl || '',
        uiStateAtSave: uiState,
        progressAtSave: lastScanProgress ? { ...lastScanProgress } : null,
        // Put results before insertionOrder so lightweight header peeks still work.
        results,
        resultCount: results.length,
        insertionOrder: [...insertionOrder],
    };
    if (settings && typeof settings === 'object') {
        payload.settings = { ...settings };
    }
    return JSON.stringify(payload);
}

const DUMP_WRITE_BATCH = 80;

function yieldToUi() {
    return new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * Stream a compact dump to disk via small IPC chunks (pick path → append batches).
 * Avoids one giant JSON.stringify + IPC transfer that crashes WebKit/JSC.
 *
 * @param {object} opts
 * @param {(startUrl: string) => Promise<{ok?: boolean, canceled?: boolean, filePath?: string}>} opts.pickPath
 * @param {(args: {path: string, data: string, truncate: boolean}) => Promise<unknown>} opts.writeChunk
 * @param {(msg: string) => void} [opts.onProgress]
 */
async function streamSessionDumpSave({
    scanResults,
    insertionOrder,
    startUrl,
    uiState,
    lastScanProgress,
    settings,
    pickPath,
    writeChunk,
    onProgress,
}) {
    const pick = await pickPath(startUrl || '');
    if (pick?.canceled || !pick?.ok || !pick?.filePath) {
        return { ok: false, canceled: true };
    }
    const filePath = pick.filePath;
    const urls = insertionOrder.filter((url) => scanResults.has(url));
    const resultCount = urls.length;

    const header = {
        version: SESSION_DUMP_VERSION,
        app: 'spider-tauri',
        savedAt: new Date().toISOString(),
        startUrl: startUrl || '',
        uiStateAtSave: uiState,
        progressAtSave: lastScanProgress ? { ...lastScanProgress } : null,
    };
    if (settings && typeof settings === 'object') {
        header.settings = { ...settings };
    }

    // Open object + results array; entries appended one batch at a time.
    let headerJson = JSON.stringify(header);
    headerJson = `${headerJson.slice(0, -1)},"results":[`;
    await writeChunk({ path: filePath, data: headerJson, truncate: true });

    let wroteResult = false;
    for (let i = 0; i < urls.length; i += DUMP_WRITE_BATCH) {
        const slice = urls.slice(i, i + DUMP_WRITE_BATCH);
        const parts = [];
        for (const url of slice) {
            const data = scanResults.get(url);
            if (!data) {
                continue;
            }
            parts.push(JSON.stringify(cloneResultEntryCompact(data)));
        }
        if (parts.length === 0) {
            continue;
        }
        const prefix = wroteResult ? ',' : '';
        wroteResult = true;
        await writeChunk({
            path: filePath,
            data: prefix + parts.join(','),
            truncate: false,
        });
        if (typeof onProgress === 'function') {
            onProgress(`Зберігаю дамп… ${Math.min(i + DUMP_WRITE_BATCH, resultCount)}/${resultCount}`);
        }
        await yieldToUi();
    }

    // Close results, then stream insertionOrder in batches too (can be large).
    await writeChunk({
        path: filePath,
        data: `],"resultCount":${resultCount},"insertionOrder":[`,
        truncate: false,
    });

    let wroteOrder = false;
    for (let i = 0; i < urls.length; i += DUMP_WRITE_BATCH) {
        const slice = urls.slice(i, i + DUMP_WRITE_BATCH);
        if (slice.length === 0) {
            continue;
        }
        const parts = slice.map((url) => JSON.stringify(url));
        const prefix = wroteOrder ? ',' : '';
        wroteOrder = true;
        await writeChunk({
            path: filePath,
            data: prefix + parts.join(','),
            truncate: false,
        });
        await yieldToUi();
    }

    await writeChunk({ path: filePath, data: ']}', truncate: false });
    return { ok: true, filePath, resultCount };
}

/** @deprecated Use buildSessionDumpJson for new saves; kept for tests / other callers. */
function buildSessionDumpPayload({
    scanResults,
    insertionOrder,
    startUrl,
    uiState,
    lastScanProgress,
    settings,
}) {
    const results = insertionOrder
        .map((url) => scanResults.get(url))
        .filter(Boolean)
        .map(cloneResultEntry);

    const payload = {
        version: SESSION_DUMP_VERSION,
        startUrl: startUrl || '',
        uiStateAtSave: uiState,
        progressAtSave: lastScanProgress ? { ...lastScanProgress } : null,
        insertionOrder: [...insertionOrder],
        results,
        resultCount: results.length,
    };
    if (settings && typeof settings === 'object') {
        payload.settings = { ...settings };
    }
    return payload;
}

/**
 * Normalize a loaded dump. Adopts `dump.results` in place (no deep clone) —
 * callers own the dump and will normalize entries once in populateScanResults.
 */
function normalizeLoadedDump(dump) {
    if (!dump || dump.version !== SESSION_DUMP_VERSION || !Array.isArray(dump.results)) {
        throw new Error('Невірний формат файлу дампу.');
    }

    const insertionOrder = Array.isArray(dump.insertionOrder) && dump.insertionOrder.length > 0
        ? dump.insertionOrder
        : dump.results.map((item) => item.url).filter(Boolean);

    return {
        startUrl: dump.startUrl || '',
        savedAt: dump.savedAt || '',
        filePath: dump.filePath || '',
        progressAtSave: dump.progressAtSave || null,
        insertionOrder,
        results: dump.results,
        settings: dump.settings && typeof dump.settings === 'object' ? dump.settings : null,
    };
}

function buildWorkspaceSnapshot({
    scanResults,
    insertionOrder,
    startUrl,
    lastScanProgress,
    selectedUrl,
    statusHint,
    filters,
}) {
    const results = insertionOrder
        .map((url) => scanResults.get(url))
        .filter(Boolean)
        .map(cloneResultEntry);

    return {
        version: WORKSPACE_VERSION,
        startUrl: startUrl || '',
        insertionOrder: [...insertionOrder],
        results,
        lastScanProgress: lastScanProgress ? { ...lastScanProgress } : null,
        selectedUrl: selectedUrl || null,
        statusHint: statusHint || '',
        filters: filters ? { ...filters } : null,
    };
}

function saveWorkspaceToSession(snapshot) {
    try {
        sessionStorage.setItem(WORKSPACE_STORAGE_KEY, JSON.stringify(snapshot));
        if (snapshot.selectedUrl) {
            sessionStorage.setItem(WORKSPACE_SELECTED_URL_KEY, snapshot.selectedUrl);
        } else {
            sessionStorage.removeItem(WORKSPACE_SELECTED_URL_KEY);
        }
    } catch (error) {
        console.error('Не вдалося зберегти стан робочої області:', error);
    }
}

function saveSelectedUrlToSession(selectedUrl) {
    try {
        if (!selectedUrl) {
            sessionStorage.removeItem(WORKSPACE_SELECTED_URL_KEY);
            return;
        }
        sessionStorage.setItem(WORKSPACE_SELECTED_URL_KEY, selectedUrl);
    } catch (error) {
        console.error('Не вдалося зберегти вибраний URL:', error);
    }
}

function loadSelectedUrlFromSession() {
    try {
        return sessionStorage.getItem(WORKSPACE_SELECTED_URL_KEY) || '';
    } catch {
        return '';
    }
}

function loadWorkspaceFromSession() {
    try {
        const raw = sessionStorage.getItem(WORKSPACE_STORAGE_KEY);
        if (!raw) {
            return null;
        }
        const parsed = JSON.parse(raw);
        if (!parsed || parsed.version !== WORKSPACE_VERSION || !Array.isArray(parsed.results)) {
            return null;
        }
        if (!parsed.selectedUrl) {
            const selectedUrl = loadSelectedUrlFromSession();
            if (selectedUrl) {
                parsed.selectedUrl = selectedUrl;
            }
        }
        return parsed;
    } catch {
        return null;
    }
}

function clearWorkspaceSession() {
    try {
        sessionStorage.removeItem(WORKSPACE_STORAGE_KEY);
        sessionStorage.removeItem(WORKSPACE_SELECTED_URL_KEY);
    } catch {
        // ignore
    }
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        SESSION_DUMP_VERSION,
        WORKSPACE_STORAGE_KEY,
        WORKSPACE_VERSION,
        cloneResultEntry,
        cloneResultEntryCompact,
        buildSessionDumpJson,
        streamSessionDumpSave,
        buildSessionDumpPayload,
        normalizeLoadedDump,
        buildWorkspaceSnapshot,
        saveWorkspaceToSession,
        saveSelectedUrlToSession,
        loadSelectedUrlFromSession,
        loadWorkspaceFromSession,
        clearWorkspaceSession,
    };
}
