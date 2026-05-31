<script lang="ts">
  import { ApiError, searchFiles, importFiles, type FileSearchItem, type ImportFileResult } from '../lib/api';

  let keyword = $state('');
  let limit = $state(50);
  let lastQuery = $state('');
  let items: FileSearchItem[] = $state([]);
  let loading = $state(false);
  let errorMessage = $state('');
  let hasSearched = $state(false);

  let selectedIds: Set<number> = $state(new Set());
  let importing = $state(false);
  let importResults: ImportFileResult[] | null = $state(null);

  async function run() {
    const q = keyword.trim();
    lastQuery = q;
    hasSearched = true;
    loading = true;
    errorMessage = '';
    selectedIds = new Set();
    importResults = null;
    try {
      const page = await searchFiles(q, limit);
      items = page.items;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      items = [];
    } finally {
      loading = false;
    }
  }

  function reset() {
    keyword = '';
    limit = 50;
    lastQuery = '';
    items = [];
    hasSearched = false;
    errorMessage = '';
    selectedIds = new Set();
    importResults = null;
  }

  function toggleSelect(id: number) {
    if (selectedIds.has(id)) {
      selectedIds.delete(id);
    } else {
      selectedIds.add(id);
    }
    selectedIds = new Set(selectedIds);
  }

  function selectAll() {
    selectedIds = new Set(items.map(item => item.id));
  }

  function deselectAll() {
    selectedIds = new Set();
  }

  async function runImport() {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    importing = true;
    importResults = null;
    errorMessage = '';
    try {
      const resp = await importFiles(ids);
      importResults = resp.results;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `导入失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      importing = false;
      selectedIds = new Set();
    }
  }

  function formatSize(bytes: number): string {
    if (bytes == null) return '—';
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    let value = Number(bytes);
    let unitIndex = 0;
    while (value >= 1024 && unitIndex < units.length - 1) {
      value /= 1024;
      unitIndex += 1;
    }
    return `${unitIndex === 0 ? value.toFixed(0) : value.toFixed(2)} ${units[unitIndex]}`;
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'succeeded': return '成功';
      case 'failed': return '失败';
      case 'skipped': return '跳过';
      default: return status;
    }
  }
</script>

<section>
  <!-- Page header -->
  <header class="page-header">
    <div>
      <h1 class="page-title">文件索引</h1>
      <p class="page-subtitle">FILE INDEX</p>
    </div>
    {#if hasSearched && !loading && !errorMessage && lastQuery}
      <span class="page-count">{items.length} 条结果</span>
    {/if}
  </header>

  <!-- Search bar -->
  <form
    class="search-bar"
    onsubmit={(e) => { e.preventDefault(); run(); }}
  >
    <div class="search-field">
      <svg class="search-icon" viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
        <path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
      </svg>
      <input
        type="text"
        bind:value={keyword}
        placeholder="输入文件名、路径或描述片段…"
        class="search-input"
      />
    </div>
    <label class="search-limit">
      <span class="filter-label">条数</span>
      <select bind:value={limit} class="filter-select">
        <option value={20}>20</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={200}>200</option>
      </select>
    </label>
    <button type="submit" class="btn-gold">搜索</button>
    <button type="button" onclick={reset} class="btn-ghost">重置</button>
  </form>

  <!-- Import bar -->
  {#if items.length > 0}
    <div class="import-bar">
      <div class="import-bar-left">
        <button type="button" onclick={selectAll} class="btn-ghost btn-sm">全选</button>
        <button type="button" onclick={deselectAll} class="btn-ghost btn-sm">取消</button>
        <span class="import-count">{selectedIds.size} 已选</span>
      </div>
      <button
        type="button"
        onclick={runImport}
        disabled={selectedIds.size === 0 || importing}
        class="btn-gold btn-sm"
      >
        {importing ? '导入中…' : `导入选中 (${selectedIds.size})`}
      </button>
    </div>
  {/if}

  <!-- Results panel -->
  {#if importResults}
    <div class="results-panel">
      <div class="results-header">
        <h3>导入结果</h3>
        <button type="button" onclick={() => { importResults = null; }} class="btn-ghost btn-sm">关闭</button>
      </div>
      <div class="results-list">
        {#each importResults as result}
          <div class="result-item result-{result.status}">
            <span class="result-status">{statusLabel(result.status)}</span>
            {#if result.title}
              <span class="result-title">{result.title}</span>
            {/if}
            {#if result.year}
              <span class="result-year">{result.year}</span>
            {/if}
            {#if result.size}
              <span class="result-size">{formatSize(result.size)}</span>
            {/if}
            {#if result.error}
              <span class="result-error">{result.error}</span>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Error -->
  {#if errorMessage}
    <div class="error-banner">
      <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
        <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
      </svg>
      <span>{errorMessage}</span>
    </div>
  {/if}

  <!-- Results -->
  {#if loading}
    <div class="loading-state">
      <div class="loading-bar"></div>
      <p>正在搜索…</p>
    </div>
  {:else if !hasSearched || !lastQuery}
    <div class="empty-state">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
          <circle cx="11" cy="11" r="8"/>
          <path d="M21 21l-4.35-4.35" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="empty-text">输入关键字开始搜索</p>
    </div>
  {:else if items.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
          <path d="M3 3h18v18H3zM8 12h8" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="empty-text">没有匹配的文件</p>
    </div>
  {:else}
    <div class="file-list">
      {#each items as item, i}
        <article class="file-card" class:selected={selectedIds.has(item.id)} style="animation-delay: {Math.min(i * 40, 400)}ms">
          <div class="file-card-header">
            <label class="file-checkbox">
              <input
                type="checkbox"
                checked={selectedIds.has(item.id)}
                onchange={() => toggleSelect(item.id)}
              />
            </label>
            <span class="hash-badge">
              <span class="hash-type">{item.hash_type}</span>
              <span class="hash-value" title={item.hash_value}>{item.hash_value.slice(0, 16)}…</span>
            </span>
            <span class="file-size">{formatSize(item.size)}</span>
          </div>

          <div class="file-card-body">
            {#each item.locations as loc}
              <div class="location">
                <div class="location-name">
                  <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                    <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/>
                  </svg>
                  <span>{loc.file_name}</span>
                </div>
                {#if loc.file_path}
                  <span class="location-path">{loc.file_path}</span>
                {/if}
                {#if loc.descriptions.length}
                  <div class="location-tags">
                    {#each loc.descriptions as desc}
                      <span class="tag" title={desc}>{desc}</span>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>

          <!-- Full hash on hover -->
          <div class="file-card-footer">
            <span class="full-hash" title="{item.hash_type}:{item.hash_value}">
              {item.hash_value}
            </span>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  /* ── Page header ── */
  .page-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 24px;
  }
  .page-title {
    font-family: var(--font-display);
    font-size: 32px;
    letter-spacing: 0.08em;
    color: var(--color-bb-cream);
    line-height: 1;
  }
  .page-subtitle {
    font-family: var(--font-display);
    font-size: 13px;
    letter-spacing: 0.25em;
    color: var(--color-bb-gold-dim);
    margin-top: 4px;
  }
  .page-count {
    font-size: 13px;
    color: var(--color-bb-text-muted);
  }

  /* ── Search bar ── */
  .search-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 12px;
    padding: 16px 20px;
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
    border-radius: 6px;
    margin-bottom: 24px;
  }
  .search-field {
    flex: 1;
    min-width: 240px;
    position: relative;
  }
  .search-icon {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--color-bb-text-muted);
    pointer-events: none;
  }
  .search-input {
    width: 100%;
    padding: 9px 12px 9px 38px;
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 15%, transparent);
    border-radius: 4px;
    color: var(--color-bb-cream);
    font-size: 14px;
    font-family: var(--font-body);
    outline: none;
    transition: border-color 0.2s ease;
  }
  .search-input::placeholder {
    color: var(--color-bb-text-muted);
    opacity: 0.6;
  }
  .search-input:focus {
    border-color: var(--color-bb-gold);
  }
  .search-limit {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* ── Shared filter styles ── */
  .filter-label {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-bb-text-muted);
  }
  .filter-select {
    padding: 7px 12px;
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 15%, transparent);
    border-radius: 4px;
    color: var(--color-bb-text);
    font-size: 13px;
    font-family: var(--font-body);
    outline: none;
    transition: border-color 0.2s ease;
  }
  .filter-select:focus {
    border-color: var(--color-bb-gold);
  }

  /* ── Buttons ── */
  .btn-gold {
    padding: 9px 24px;
    background: linear-gradient(135deg, var(--color-bb-gold-dim), var(--color-bb-gold));
    color: var(--color-bb-void);
    font-family: var(--font-display);
    font-size: 16px;
    letter-spacing: 0.1em;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.25s ease;
  }
  .btn-gold:hover {
    background: linear-gradient(135deg, var(--color-bb-gold), var(--color-bb-gold-light));
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
  }
  .btn-ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 9px 16px;
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
    border-radius: 4px;
    color: var(--color-bb-text-muted);
    font-size: 13px;
    font-family: var(--font-body);
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .btn-ghost:hover:not(:disabled) {
    color: var(--color-bb-gold-light);
    border-color: color-mix(in srgb, var(--color-bb-gold) 40%, transparent);
    background: color-mix(in srgb, var(--color-bb-gold) 6%, transparent);
  }

  /* ── States ── */
  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 48px 0;
  }
  .loading-state p {
    font-size: 13px;
    color: var(--color-bb-text-muted);
  }
  .loading-bar {
    width: 120px;
    height: 2px;
    background: var(--color-bb-muted);
    border-radius: 1px;
    overflow: hidden;
    position: relative;
  }
  .loading-bar::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, transparent, var(--color-bb-gold), transparent);
    animation: loading-sweep 1.5s ease-in-out infinite;
  }
  @keyframes loading-sweep {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 64px 0;
  }
  .empty-icon {
    color: var(--color-bb-muted);
    opacity: 0.5;
  }
  .empty-text {
    font-size: 14px;
    color: var(--color-bb-text-muted);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    margin-bottom: 16px;
    background: color-mix(in srgb, var(--color-bb-red) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bb-red) 25%, transparent);
    border-radius: 6px;
    font-size: 13px;
    color: #f08080;
  }

  /* ── File list ── */
  .file-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .file-card {
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
    border-radius: 6px;
    overflow: hidden;
    transition: all 0.25s ease;
    animation: file-enter 0.4s ease both;
  }
  .file-card:hover {
    border-color: color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
  }
  @keyframes file-enter {
    from { opacity: 0; transform: translateX(-8px); }
    to { opacity: 1; transform: translateX(0); }
  }

  .file-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 18px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-bb-gold) 6%, transparent);
    background: color-mix(in srgb, var(--color-bb-gold) 3%, transparent);
  }

  .hash-badge {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .hash-type {
    display: inline-block;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
    border-radius: 3px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-bb-gold);
  }
  .hash-value {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-bb-text-muted);
  }

  .file-size {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 500;
    color: var(--color-bb-cream);
  }

  .file-card-body {
    padding: 14px 18px;
  }

  .location {
    padding: 10px 14px;
    background: var(--color-bb-deep);
    border-radius: 4px;
  }
  .location + .location {
    margin-top: 8px;
  }

  .location-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-bb-cream);
  }
  .location-name svg {
    flex-shrink: 0;
    color: var(--color-bb-gold-dim);
  }

  .location-path {
    display: block;
    margin-top: 4px;
    padding-left: 20px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-bb-text-muted);
    word-break: break-all;
  }

  .location-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
    padding-left: 20px;
  }
  .tag {
    display: inline-block;
    max-width: 200px;
    padding: 2px 8px;
    background: color-mix(in srgb, var(--color-bb-blue) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bb-blue) 20%, transparent);
    border-radius: 20px;
    font-size: 11px;
    color: color-mix(in srgb, var(--color-bb-blue) 80%, white);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-card-footer {
    padding: 8px 18px;
    border-top: 1px solid color-mix(in srgb, var(--color-bb-gold) 5%, transparent);
  }
  .full-hash {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-bb-text-muted);
    opacity: 0.5;
    word-break: break-all;
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Responsive ── */
  @media (max-width: 640px) {
    .search-bar {
      flex-direction: column;
      align-items: stretch;
    }
  }

  /* ── Import bar ── */
  .import-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
    border-radius: 6px;
    margin-bottom: 16px;
  }
  .import-bar-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .import-count {
    font-size: 13px;
    color: var(--color-bb-text-muted);
  }
  .btn-sm {
    padding: 6px 12px;
    font-size: 12px;
  }
  .btn-sm:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* ── Results panel ── */
  .results-panel {
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
    border-radius: 6px;
    margin-bottom: 16px;
    overflow: hidden;
  }
  .results-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
    background: color-mix(in srgb, var(--color-bb-gold) 5%, transparent);
  }
  .results-header h3 {
    font-family: var(--font-display);
    font-size: 16px;
    color: var(--color-bb-cream);
    letter-spacing: 0.05em;
    margin: 0;
  }
  .results-list {
    padding: 8px;
  }
  .result-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: 4px;
    font-size: 13px;
  }
  .result-item + .result-item {
    margin-top: 4px;
  }
  .result-succeeded {
    background: color-mix(in srgb, #22c55e 8%, transparent);
  }
  .result-failed {
    background: color-mix(in srgb, #ef4444 8%, transparent);
  }
  .result-skipped {
    background: color-mix(in srgb, #f59e0b 8%, transparent);
  }
  .result-status {
    display: inline-block;
    min-width: 40px;
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 600;
    text-align: center;
  }
  .result-succeeded .result-status {
    background: color-mix(in srgb, #22c55e 20%, transparent);
    color: #4ade80;
  }
  .result-failed .result-status {
    background: color-mix(in srgb, #ef4444 20%, transparent);
    color: #f87171;
  }
  .result-skipped .result-status {
    background: color-mix(in srgb, #f59e0b 20%, transparent);
    color: #fbbf24;
  }
  .result-title {
    color: var(--color-bb-cream);
    font-weight: 500;
  }
  .result-year {
    color: var(--color-bb-text-muted);
  }
  .result-size {
    color: var(--color-bb-text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    margin-left: auto;
  }
  .result-error {
    color: #f87171;
    font-size: 12px;
    margin-left: auto;
  }

  /* ── Checkbox ── */
  .file-checkbox {
    display: flex;
    align-items: center;
    cursor: pointer;
  }
  .file-checkbox input {
    width: 16px;
    height: 16px;
    accent-color: var(--color-bb-gold);
    cursor: pointer;
  }
  .file-card.selected {
    border-color: color-mix(in srgb, var(--color-bb-gold) 40%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
  }
</style>
