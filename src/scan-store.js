/**
 * In-memory store for scan results. Hooks allow transforming data on ingest.
 */
(function initScanStore(root) {
const { transformStoredResult } = root;

const PAGE_EXTRACT_FIELDS = [
    'title',
    'metaDescription',
    'metaCanonical',
    'headings',
    'ogTitle',
    'ogDescription',
    'ogImage',
    'metaRobots',
    'metaRobotsStatus',
    'metaRobotsLabel',
    'xRobotsTag',
    'xRobotsTagStatus',
    'xRobotsTagLabel',
    'responseHeaders',
];

function isEmptyExtractField(field, value) {
    if (field === 'headings' || field === 'responseHeaders') {
        return !Array.isArray(value) || value.length === 0;
    }
    return value === '' || value === undefined || value === null;
}

function isHttpRedirectStatus(status) {
    return typeof status === 'number' && status >= 300 && status < 400;
}

function mergeFetchedPageFields(existing, incoming) {
    if (!existing || existing.fetched === false || incoming.fetched === false) {
        return incoming;
    }
    const merged = { ...incoming };
    for (const field of PAGE_EXTRACT_FIELDS) {
        if (isEmptyExtractField(field, merged[field]) && !isEmptyExtractField(field, existing[field])) {
            merged[field] = existing[field];
        }
    }
    if ((merged.responseTimeMs === null || merged.responseTimeMs === undefined)
        && existing.responseTimeMs !== null
        && existing.responseTimeMs !== undefined) {
        merged.responseTimeMs = existing.responseTimeMs;
    }

    // Live crawl omits referrers on rows (end sync fills them). Don't wipe
    // any referrers already attached (e.g. after a partial sync).
    if ((!Array.isArray(merged.referrers) || merged.referrers.length === 0)
        && Array.isArray(existing.referrers) && existing.referrers.length > 0) {
        merged.referrers = existing.referrers;
    }

    // Keep crawl redirect summary if a later update (e.g. probe) would wipe it with final 2xx.
    const existingHops = Number(existing.redirectHopCount || 0);
    const incomingHops = Number(incoming.redirectHopCount || 0);
    if (existingHops > 0 && incomingHops === 0) {
        merged.redirectHopCount = existing.redirectHopCount;
        merged.redirectFinalUrl = existing.redirectFinalUrl || '';
        merged.redirectInfinite = Boolean(existing.redirectInfinite);
        merged.redirectChain = Array.isArray(existing.redirectChain) ? existing.redirectChain : [];
        merged.redirectLoopStartUrl = existing.redirectLoopStartUrl || '';
        merged.redirectUrl = existing.redirectUrl || merged.redirectUrl || '';
        if (isHttpRedirectStatus(existing.status) && !isHttpRedirectStatus(incoming.status)) {
            merged.status = existing.status;
            if (existing.responseTimeMs !== null && existing.responseTimeMs !== undefined) {
                merged.responseTimeMs = existing.responseTimeMs;
            }
        }
    } else if (
        existingHops > 0
        && incomingHops > 0
        && isHttpRedirectStatus(existing.status)
        && !isHttpRedirectStatus(incoming.status)
    ) {
        merged.status = existing.status;
        if (existing.responseTimeMs !== null && existing.responseTimeMs !== undefined) {
            merged.responseTimeMs = existing.responseTimeMs;
        }
    }

    return merged;
}

function createScanStore(options = {}) {
    const scanResults = new Map();
    const insertionOrder = [];
    const insertionOrderIndex = new Map();
    let dataRevision = 0;
    let latestReferrersByUrl = new Map();
    let latestRobotsByUrl = new Map();
    let duplicateCountsCache = null;
    /** @type {Map<string, object[]> | null} per-page outlinks (built lazily, one page at a time) */
    let outgoingLinksByPageCache = null;
    /** @type {Map<string, { linkCount: number, internalCount: number, externalCount: number }> | null} */
    let outgoingCountsByPageCache = null;
    let scanHostname = '';

    const getScanHostname = options.getScanHostname || (() => scanHostname);

    function bumpDataRevision() {
        dataRevision += 1;
    }

    function rebuildInsertionOrderIndex() {
        insertionOrderIndex.clear();
        for (let index = 0; index < insertionOrder.length; index += 1) {
            insertionOrderIndex.set(insertionOrder[index], index);
        }
    }

    function getInsertionIndex(url) {
        return insertionOrderIndex.has(url)
            ? insertionOrderIndex.get(url)
            : Number.MAX_SAFE_INTEGER;
    }

    function invalidateOutgoingLinksCache() {
        outgoingLinksByPageCache = null;
        outgoingCountsByPageCache = null;
    }

    function invalidateDuplicateCounts() {
        duplicateCountsCache = null;
    }

    function setScanHostname(hostname) {
        scanHostname = hostname;
    }

    function normalizeIncoming(data) {
        return normalizeLinkEntryImpl(data, getScanHostname());
    }

    function upsertRaw(incoming, { deferUi = false } = {}) {
        const transformed = transformStoredResult(
            { scanResults, getScanHostname },
            incoming
        );
        const incomingUrl = typeof transformed === 'object'
            ? (transformed.url || transformed.href)
            : transformed;
        const existingBeforeMerge = incomingUrl ? scanResults.get(incomingUrl) : undefined;
        const merged = existingBeforeMerge
            ? mergeFetchedPageFields(existingBeforeMerge, transformed)
            : transformed;
        const data = normalizeIncoming(merged);
        const existing = scanResults.get(data.url);
        if (existing && existing.fetched !== false && data.fetched === false) {
            const enrichesResource = isJavascriptResource(data) || isCssResource(data) || isMediaResource(data);
            const enrichesCrawledAsset = existing
                && !isHtmlContentType(existing.contentType || '')
                && (data.kind || data.tag);
            if (existing && (enrichesResource || enrichesCrawledAsset)) {
                scanResults.set(data.url, normalizeIncoming({
                    ...existing,
                    kind: data.kind || existing.kind,
                    tag: data.tag || existing.tag,
                    text: data.text || existing.text,
                    imgAltMissing: data.imgAltMissing || existing.imgAltMissing,
                    ...(data.imgAlt !== undefined
                        ? { imgAlt: data.imgAlt }
                        : (existing.imgAlt !== undefined ? { imgAlt: existing.imgAlt } : {})),
                }));
                bumpDataRevision();
                return { isNew: false, changed: true };
            }
            return { isNew: false, changed: false };
        }
        const isNew = !existing;
        if (isNew) {
            insertionOrder.push(data.url);
            insertionOrderIndex.set(data.url, insertionOrder.length - 1);
        }
        scanResults.set(data.url, data);
        if (isNew || existing) {
            bumpDataRevision();
        }
        return { isNew, changed: true, deferUi };
    }

    function clearData() {
        invalidateDuplicateCounts();
        latestReferrersByUrl = new Map();
        latestRobotsByUrl = new Map();
        scanResults.clear();
        insertionOrder.length = 0;
        insertionOrderIndex.clear();
        dataRevision = 0;
        invalidateOutgoingLinksCache();
    }

    function getRawReferrersForUrl(url) {
        if (latestReferrersByUrl.has(url)) {
            return latestReferrersByUrl.get(url) || [];
        }
        const data = scanResults.get(url);
        return data?.referrers?.length ? data.referrers : [];
    }

    /**
     * Return referrers without allocating a fresh object per edge on every call.
     * Dump/crawl data is already object-shaped; only legacy string refs are normalized once.
     */
    function getReferrersForUrl(url) {
        const raw = getRawReferrersForUrl(url);
        if (!raw.length) {
            return raw;
        }
        if (typeof raw[0] !== 'string') {
            return raw;
        }
        const normalized = raw.map(normalizeReferrerEntry).filter((entry) => entry.href);
        latestReferrersByUrl.set(url, normalized);
        const data = scanResults.get(url);
        if (data) {
            data.referrers = normalized;
        }
        return normalized;
    }

    function rebuildLatestReferrersFromResults() {
        latestReferrersByUrl = new Map();
        latestRobotsByUrl = new Map();
        for (const [url, data] of scanResults.entries()) {
            if (data.referrers?.length) {
                latestReferrersByUrl.set(url, data.referrers);
            }
        }
    }

    function mergeRobotsFieldsIfMissing(data, robotsFields) {
        if (!robotsFields || (robotsFields.robotsAllowed == null && !robotsFields.robotsRule)) {
            return data;
        }
        if (data.robotsAllowed != null || data.robotsRule) {
            return data;
        }
        return {
            ...data,
            robotsAllowed: robotsFields.robotsAllowed,
            robotsRule: robotsFields.robotsRule,
        };
    }

    function materializeDiscoveredFromReferrers() {
        let changed = false;
        for (const [url, refs] of latestReferrersByUrl.entries()) {
            if (scanResults.has(url)) {
                continue;
            }
            const refText = refs[0]?.text || '';
            const robotsFields = latestRobotsByUrl.get(url) || {};
            const upsertResult = upsertRaw({
                url,
                status: '',
                title: '',
                text: refText,
                external: isExternalUrlImpl(url, getScanHostname()),
                fetched: false,
                kind: '',
                tag: '',
                referrers: refs,
                ...robotsFields,
            }, { deferUi: true });
            if (upsertResult.changed) {
                changed = true;
            }
        }
        if (changed) {
            reinferAllLinkKinds();
            invalidateOutgoingLinksCache();
            bumpDataRevision();
        }
        return changed;
    }

    function applyReferrersUpdate(payload) {
        const referrersPayload = payload?.referrers ?? payload;
        const robotsPayload = payload?.robotsByUrl ?? {};
        latestReferrersByUrl = new Map();
        latestRobotsByUrl = new Map(Object.entries(robotsPayload));
        for (const [url, refs] of Object.entries(referrersPayload || {})) {
            const normalized = Array.isArray(refs)
                ? refs.map(normalizeReferrerEntry).filter((entry) => entry.href)
                : [];
            latestReferrersByUrl.set(url, normalized);
        }

        for (const [url, data] of scanResults.entries()) {
            if (latestReferrersByUrl.has(url)) {
                data.referrers = latestReferrersByUrl.get(url);
            }
            const merged = mergeRobotsFieldsIfMissing(data, latestRobotsByUrl.get(url));
            if (merged !== data) {
                scanResults.set(url, normalizeIncoming(merged));
            }
        }
        materializeDiscoveredFromReferrers();
        invalidateOutgoingLinksCache();
        bumpDataRevision();
    }

    /**
     * Lightweight edge stub for outlinks — never clone the full target page.
     */
    function buildOutgoingLink(ref, targetEntry) {
        const edgeHasRelMeta = Boolean(ref.rel)
            || ref.relFollowAllowed !== null
            || ref.relIndexAllowed !== null
            || Boolean(ref.relLabel);
        const stub = {
            url: targetEntry.url,
            status: targetEntry.status,
            external: Boolean(targetEntry.external),
            fetched: targetEntry.fetched ?? (
                targetEntry.status !== ''
                && targetEntry.status !== undefined
                && targetEntry.status !== null
            ),
            contentType: targetEntry.contentType || '',
            kind: ref.kind || targetEntry.kind || '',
            tag: ref.tag || targetEntry.tag || '',
            text: ref.text || targetEntry.text || '',
            rel: edgeHasRelMeta ? (ref.rel || '') : (targetEntry.rel || ''),
            relFollowAllowed: edgeHasRelMeta
                ? (ref.relFollowAllowed ?? null)
                : (targetEntry.relFollowAllowed ?? null),
            relIndexAllowed: edgeHasRelMeta
                ? (ref.relIndexAllowed ?? null)
                : (targetEntry.relIndexAllowed ?? null),
            relLabel: edgeHasRelMeta ? (ref.relLabel || '') : (targetEntry.relLabel || ''),
            imgAltMissing: ref.imgAltMissing === true,
        };
        if (ref.imgAlt !== undefined) {
            stub.imgAlt = ref.imgAlt;
        }
        return stub;
    }

    /**
     * Counts only — used by table columns. Walks raw referrer arrays without
     * normalizing/cloning edges (that clone was the multi-GB WebKit spike).
     */
    function rebuildOutgoingCountsCache() {
        const counts = new Map();
        const host = getScanHostname();
        for (const entry of scanResults.values()) {
            const refs = getRawReferrersForUrl(entry.url);
            const targetIsExternal = entry.external === true
                || (host && isExternalByHost(entry.url, host));
            for (let i = 0; i < refs.length; i += 1) {
                const ref = refs[i];
                const href = typeof ref === 'string' ? ref : ref?.href;
                if (!href) {
                    continue;
                }
                let bucket = counts.get(href);
                if (!bucket) {
                    bucket = { linkCount: 0, internalCount: 0, externalCount: 0 };
                    counts.set(href, bucket);
                }
                bucket.linkCount += 1;
                if (targetIsExternal) {
                    bucket.externalCount += 1;
                } else {
                    bucket.internalCount += 1;
                }
            }
        }
        outgoingCountsByPageCache = counts;
    }

    function isExternalByHost(url, host) {
        try {
            return new URL(url).hostname !== host;
        } catch {
            return Boolean(host);
        }
    }

    function getOutgoingCounts(pageUrl) {
        if (!outgoingCountsByPageCache) {
            rebuildOutgoingCountsCache();
        }
        return outgoingCountsByPageCache.get(pageUrl) || {
            linkCount: 0,
            internalCount: 0,
            externalCount: 0,
        };
    }

    /** Build outlinks for a single page (detail panel / CSV for one row). */
    function getOutgoingLinksFrom(pageUrl) {
        if (!outgoingLinksByPageCache) {
            outgoingLinksByPageCache = new Map();
        }
        if (outgoingLinksByPageCache.has(pageUrl)) {
            return outgoingLinksByPageCache.get(pageUrl);
        }
        const list = [];
        for (const entry of scanResults.values()) {
            const refs = getRawReferrersForUrl(entry.url);
            for (let i = 0; i < refs.length; i += 1) {
                const ref = refs[i];
                const href = typeof ref === 'string' ? ref : ref?.href;
                if (href === pageUrl) {
                    list.push(buildOutgoingLink(
                        typeof ref === 'string' ? { href: ref } : ref,
                        entry,
                    ));
                }
            }
        }
        // Bound cache size so opening many detail panels does not keep all pages forever.
        if (outgoingLinksByPageCache.size > 64) {
            const firstKey = outgoingLinksByPageCache.keys().next().value;
            outgoingLinksByPageCache.delete(firstKey);
        }
        outgoingLinksByPageCache.set(pageUrl, list);
        return list;
    }

    function reinferAllLinkKinds() {
        for (const [url, entry] of scanResults.entries()) {
            scanResults.set(url, normalizeIncoming(entry));
        }
    }

    function getDuplicateCounts() {
        const entries = Array.from(scanResults.values());
        if (!duplicateCountsCache) {
            duplicateCountsCache = {
                h1: buildH1DuplicateCounts(entries),
                title: buildFieldDuplicateCounts((data) => getPageTitle(data), entries),
                description: buildFieldDuplicateCounts((data) => (
                    shouldHavePageTitle(data) ? String(data.metaDescription || '').trim() : ''
                ), entries),
            };
        }
        return duplicateCountsCache;
    }

    return {
        scanResults,
        insertionOrder,
        insertionOrderIndex,
        getDataRevision: () => dataRevision,
        getInsertionIndex,
        rebuildInsertionOrderIndex,
        getScanHostname,
        setScanHostname,
        upsertRaw,
        clearData,
        getReferrersForUrl,
        getOutgoingLinksFrom,
        getOutgoingCounts,
        rebuildLatestReferrersFromResults,
        applyReferrersUpdate,
        materializeDiscoveredFromReferrers,
        reinferAllLinkKinds,
        invalidateOutgoingLinksCache,
        invalidateDuplicateCounts,
        getDuplicateCounts,
        get latestReferrersByUrl() { return latestReferrersByUrl; },
        get latestRobotsByUrl() { return latestRobotsByUrl; },
    };
}

const exported = { createScanStore };

if (typeof module !== 'undefined' && module.exports) {
    module.exports = exported;
}
Object.assign(root, exported);
})(typeof globalThis !== 'undefined' ? globalThis : window);
