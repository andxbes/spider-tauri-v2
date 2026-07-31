/**
 * Workspace persistence: sessionStorage snapshot, restore, populate results.
 */
(function initWorkspaceController(root) {
const WORKSPACE_PERSIST_DELAY_MS = 200;
/** sessionStorage cannot hold huge scans; skip to avoid multi-minute freezes. */
const WORKSPACE_PERSIST_MAX_RESULTS = 2500;
/** Mirror scan-handlers large-scan threshold; dump kinds are already stored. */
const LARGE_DUMP_REINFER_THRESHOLD = 5000;

function createWorkspaceController(deps) {
    const {
        scanStore,
        tableFilters,
        tableView,
        elements,
        getLastScanProgress,
        setLastScanProgress,
        getSelectedUrl,
        setSelectedUrl,
        getSortState,
        setSortState,
        getUiState,
        normalizeLinkEntry,
        reinferAllLinkKinds,
        clearScanData,
        updateExportButton,
        updateUrlInputProgress,
        scheduleStartupTableRefresh,
        selectRow,
        requestRefreshTable,
        setScanHostnameFromUrl,
    } = deps;

    const {
        urlInput,
        statusText,
        statusScanned,
        statusQueue,
        statusActive,
        statusRate,
        selectedUrlHint,
        selectedUrlBar,
        detailContent,
        resultsTable,
    } = elements;

    let workspacePersistTimer = null;

    function flattenLegacyOutlinks(entry) {
        const extras = [];
        for (const link of entry.outlinks || []) {
            const normalized = normalizeLegacyLink(link);
            if (normalized) {
                extras.push(normalizeLinkEntry(normalized));
            }
        }
        const { outlinks, linkCount, ...rest } = entry;
        return { entry: normalizeLinkEntry(rest), extras };
    }

    function collectWorkspaceSnapshot() {
        return {
            ...buildWorkspaceSnapshot({
                scanResults: scanStore.scanResults,
                insertionOrder: scanStore.insertionOrder,
                startUrl: urlInput.value.trim(),
                lastScanProgress: getLastScanProgress(),
                selectedUrl: getSelectedUrl(),
                statusHint: statusText.textContent,
                filters: tableFilters.getFilterSnapshot(),
            }),
        };
    }

    function persistWorkspaceNow() {
        if (workspacePersistTimer) {
            clearTimeout(workspacePersistTimer);
            workspacePersistTimer = null;
        }
        if (scanStore.scanResults.size === 0) {
            clearWorkspaceSession();
            return;
        }
        if (scanStore.scanResults.size > WORKSPACE_PERSIST_MAX_RESULTS) {
            clearWorkspaceSession();
            return;
        }
        saveWorkspaceToSession(collectWorkspaceSnapshot());
    }

    function scheduleWorkspacePersist() {
        if (getUiState() === 'running') {
            return;
        }
        if (workspacePersistTimer) {
            return;
        }
        workspacePersistTimer = setTimeout(() => {
            workspacePersistTimer = null;
            persistWorkspaceNow();
        }, WORKSPACE_PERSIST_DELAY_MS);
    }

    function cancelWorkspacePersistTimer() {
        if (workspacePersistTimer) {
            clearTimeout(workspacePersistTimer);
            workspacePersistTimer = null;
        }
    }

    function scheduleWorkspacePersistSelectedUrl() {
        if (getUiState() === 'running') {
            return;
        }
        saveSelectedUrlToSession(getSelectedUrl());
    }

    function populateScanResults(normalized) {
        clearScanData();
        urlInput.value = normalized.startUrl;
        setScanHostnameFromUrl(normalized.startUrl);

        const results = normalized.results;
        const isLarge = results.length > LARGE_DUMP_REINFER_THRESHOLD;
        const seen = new Set();

        const adoptEntry = (rawEntry) => {
            if (!rawEntry?.url || seen.has(rawEntry.url)) {
                return;
            }
            // Legacy dumps may still carry outlinks; materialize targets without a second full Map.
            if (Array.isArray(rawEntry.outlinks) && rawEntry.outlinks.length) {
                for (const link of rawEntry.outlinks) {
                    const normalizedLink = normalizeLegacyLink(link);
                    if (!normalizedLink?.url && !normalizedLink?.href) {
                        continue;
                    }
                    const extra = isLarge
                        ? {
                            ...normalizedLink,
                            url: normalizedLink.url || normalizedLink.href,
                            fetched: false,
                            referrers: [],
                        }
                        : normalizeLinkEntry(normalizedLink);
                    if (extra.url && !seen.has(extra.url) && !scanStore.scanResults.has(extra.url)) {
                        scanStore.scanResults.set(extra.url, extra);
                        scanStore.insertionOrder.push(extra.url);
                        seen.add(extra.url);
                    }
                }
            }
            if (Object.prototype.hasOwnProperty.call(rawEntry, 'outlinks')) {
                delete rawEntry.outlinks;
            }
            if (Object.prototype.hasOwnProperty.call(rawEntry, 'linkCount')) {
                delete rawEntry.linkCount;
            }
            if (isLarge) {
                // Adopt dump objects in place — no normalizeLinkEntry spreads (2× heap).
                if (rawEntry.responseHeaders) {
                    rawEntry.responseHeaders = [];
                }
                if (rawEntry.redirectChain) {
                    rawEntry.redirectChain = [];
                }
                scanStore.scanResults.set(rawEntry.url, rawEntry);
            } else {
                scanStore.scanResults.set(rawEntry.url, normalizeLinkEntry(rawEntry));
            }
            scanStore.insertionOrder.push(rawEntry.url);
            seen.add(rawEntry.url);
        };

        if (isLarge) {
            const byUrl = new Map();
            for (let i = 0; i < results.length; i += 1) {
                const item = results[i];
                if (item?.url) {
                    byUrl.set(item.url, item);
                }
            }
            for (const url of normalized.insertionOrder) {
                const item = byUrl.get(url);
                if (item) {
                    adoptEntry(item);
                    byUrl.delete(url);
                }
            }
            for (const item of byUrl.values()) {
                adoptEntry(item);
            }
            byUrl.clear();
        } else {
            const resultMap = new Map();
            for (const rawEntry of results) {
                const { entry, extras } = flattenLegacyOutlinks(rawEntry);
                resultMap.set(entry.url, entry);
                for (const extra of extras) {
                    if (!resultMap.has(extra.url)) {
                        resultMap.set(extra.url, extra);
                    }
                }
            }
            for (const url of normalized.insertionOrder) {
                if (resultMap.has(url) && !seen.has(url)) {
                    scanStore.insertionOrder.push(url);
                    scanStore.scanResults.set(url, resultMap.get(url));
                    seen.add(url);
                }
            }
            for (const [url, entry] of resultMap) {
                if (!seen.has(url)) {
                    scanStore.insertionOrder.push(url);
                    scanStore.scanResults.set(url, entry);
                    seen.add(url);
                }
            }
            resultMap.clear();
        }

        // Drop source array slots so GC can reclaim the dump wrapper sooner.
        results.length = 0;
        if (Array.isArray(normalized.insertionOrder)) {
            normalized.insertionOrder = scanStore.insertionOrder;
        }

        scanStore.rebuildLatestReferrersFromResults();
        scanStore.rebuildInsertionOrderIndex();
        // Large dumps already carry kinds; reinfer All re-clones every entry (WebKit peak).
        if (!isLarge) {
            reinferAllLinkKinds();
        }
    }

    function restoreWorkspaceFromSession() {
        const workspace = loadWorkspaceFromSession();
        if (!workspace?.results?.length) {
            return false;
        }

        const normalized = normalizeLoadedDump({
            version: SESSION_DUMP_VERSION,
            startUrl: workspace.startUrl,
            insertionOrder: workspace.insertionOrder,
            results: workspace.results,
            progressAtSave: workspace.lastScanProgress,
        });

        populateScanResults(normalized);
        if (workspace.filters) {
            tableFilters.applyFilterState(workspace.filters);
        }

        setLastScanProgress(workspace.lastScanProgress || null);
        scheduleStartupTableRefresh();
        updateUrlInputProgress(workspace.lastScanProgress);
        statusScanned.textContent = `Проскановано: ${scanStore.scanResults.size}`;
        statusQueue.textContent = 'У черзі: 0';
        if (statusActive) {
            statusActive.textContent = 'Активних: 0';
        }
        if (statusRate) {
            statusRate.textContent = 'Швидкість: —';
        }
        if (workspace.statusHint) {
            statusText.textContent = workspace.statusHint;
        }

        if (workspace.selectedUrl && scanStore.scanResults.has(workspace.selectedUrl)) {
            selectRow(workspace.selectedUrl);
        }

        updateExportButton();
        return true;
    }

    async function applySessionDump(dump, filePath = '') {
        if (filePath) {
            dump.filePath = filePath;
        }
        const normalized = normalizeLoadedDump(dump);
        tableFilters.resetTableFilters();
        populateScanResults(normalized);
        // Release adopted dump arrays so GC can reclaim after populate cloned into scanResults.
        if (Array.isArray(dump.results)) {
            dump.results.length = 0;
        }
        if (Array.isArray(dump.insertionOrder)) {
            dump.insertionOrder.length = 0;
        }

        if (normalized.settings && typeof applyDumpSettings === 'function') {
            await applyDumpSettings(normalized.settings);
        }

        setSelectedUrl(null);
        setLastScanProgress(normalized.progressAtSave);
        requestRefreshTable({ immediate: true });
        deps.setUIState('idle');
        updateUrlInputProgress(normalized.progressAtSave);
        statusScanned.textContent = `Проскановано: ${scanStore.scanResults.size}`;
        statusQueue.textContent = 'У черзі: 0';
        if (statusActive) {
            statusActive.textContent = 'Активних: 0';
        }
        if (statusRate) {
            statusRate.textContent = 'Швидкість: —';
        }
        statusText.textContent = filePath
            ? `Завантажено дамп (${scanStore.scanResults.size} URL): ${filePath}`
            : `Завантажено дамп: ${scanStore.scanResults.size} URL`;
        persistWorkspaceNow();
    }

    /** Rust-streamed dump import: start → N batches → done (no JS JSON.parse of the file). */
    let pendingImport = null;

    function beginRustDumpImport(meta) {
        clearScanData();
        tableFilters.resetTableFilters();
        setSelectedUrl(null);
        urlInput.value = meta.startUrl || '';
        setScanHostnameFromUrl(meta.startUrl || '');
        setLastScanProgress(meta.progressAtSave || null);
        pendingImport = {
            filePath: meta.filePath || '',
            insertionOrder: Array.isArray(meta.insertionOrder) ? meta.insertionOrder : [],
            settings: meta.settings || null,
            received: 0,
            resultCount: meta.resultCount || 0,
        };
        statusText.textContent = `Завантаження дампу… 0 / ${pendingImport.resultCount || '?'}`;
        statusScanned.textContent = 'Проскановано: 0';
        statusQueue.textContent = 'У черзі: 0';
        if (statusActive) {
            statusActive.textContent = 'Активних: 0';
        }
        if (statusRate) {
            statusRate.textContent = 'Швидкість: —';
        }
        deps.setUIState('idle');
        if (meta.settings && typeof applyDumpSettings === 'function') {
            void applyDumpSettings(meta.settings);
        }
    }

    function appendRustDumpBatch(entries) {
        if (!pendingImport || !Array.isArray(entries)) {
            return;
        }
        for (let i = 0; i < entries.length; i += 1) {
            const entry = entries[i];
            if (!entry?.url || scanStore.scanResults.has(entry.url)) {
                continue;
            }
            if (entry.fetched == null) {
                entry.fetched = entry.status !== ''
                    && entry.status !== undefined
                    && entry.status !== null;
            }
            if (!Array.isArray(entry.referrers)) {
                entry.referrers = [];
            }
            if (!Array.isArray(entry.headings)) {
                entry.headings = [];
            }
            entry.responseHeaders = [];
            entry.redirectChain = [];
            scanStore.scanResults.set(entry.url, entry);
            pendingImport.received += 1;
        }
        statusText.textContent = `Завантаження дампу… ${pendingImport.received} / ${pendingImport.resultCount || '?'}`;
        statusScanned.textContent = `Проскановано: ${scanStore.scanResults.size}`;
    }

    function finishRustDumpImport(donePayload = {}) {
        if (donePayload?.ok === false) {
            pendingImport = null;
            statusText.textContent = donePayload.error || 'Не вдалося завантажити дамп.';
            alert(donePayload.error || 'Не вдалося завантажити дамп.');
            return;
        }
        if (!pendingImport) {
            return;
        }
        const { filePath, insertionOrder } = pendingImport;
        const seen = new Set();
        scanStore.insertionOrder.length = 0;
        for (const url of insertionOrder) {
            if (scanStore.scanResults.has(url) && !seen.has(url)) {
                scanStore.insertionOrder.push(url);
                seen.add(url);
            }
        }
        for (const url of scanStore.scanResults.keys()) {
            if (!seen.has(url)) {
                scanStore.insertionOrder.push(url);
                seen.add(url);
            }
        }
        scanStore.rebuildLatestReferrersFromResults();
        scanStore.rebuildInsertionOrderIndex();
        scanStore.invalidateOutgoingLinksCache();
        scanStore.invalidateDuplicateCounts();

        const count = scanStore.scanResults.size;
        const path = filePath;
        pendingImport = null;

        requestRefreshTable({ immediate: true });
        updateUrlInputProgress(getLastScanProgress());
        statusScanned.textContent = `Проскановано: ${count}`;
        statusText.textContent = path
            ? `Завантажено дамп (${count} URL): ${path}`
            : `Завантажено дамп: ${count} URL`;
        updateExportButton();
        persistWorkspaceNow();
    }

    function clearScanResults() {
        clearScanData();
        tableFilters.resetTableFilters();
        updateExportButton();
        clearWorkspaceSession();
    }

    return {
        persistWorkspaceNow,
        scheduleWorkspacePersist,
        scheduleWorkspacePersistSelectedUrl,
        cancelWorkspacePersistTimer,
        populateScanResults,
        restoreWorkspaceFromSession,
        applySessionDump,
        beginRustDumpImport,
        appendRustDumpBatch,
        finishRustDumpImport,
        clearScanResults,
        collectWorkspaceSnapshot,
    };
}

const exported = { createWorkspaceController };

if (typeof module !== 'undefined' && module.exports) {
    module.exports = exported;
}
Object.assign(root, exported);
})(typeof globalThis !== 'undefined' ? globalThis : window);
