<script lang="ts">
  import {
    ApiError,
    deleteMediaDirs,
    listMediaDirs,
    searchMediaDirs,
    type MediaDirItem,
  } from '../lib/api';

  type Crumb = { dir_id: number | null; name: string };

  let keyword = $state('');
  let mode: 'browse' | 'search' = $state('browse');
  let crumbs: Crumb[] = $state([{ dir_id: null, name: '库根' }]);
  let items: MediaDirItem[] = $state([]);
  let loading = $state(false);
  let errorMessage = $state('');
  let successMessage = $state('');

  let selected = $state(new Map<number, string>());
  let confirming = $state(false);
  let deleting = $state(false);

  const currentParentId = $derived(crumbs[crumbs.length - 1]?.dir_id ?? null);
  const deletableItems = $derived(items.filter((item) => item.deletable));
  const selectedItems = $derived(
    Array.from(selected.entries()).map(([dir_id, relative_path]) => ({ dir_id, relative_path })),
  );

  function itemPath(item: MediaDirItem): string {
    if (item.relative_path) return item.relative_path;
    const parent = crumbs.slice(1).map((crumb) => crumb.name).join('/');
    return parent ? `${parent}/${item.display_name}` : item.display_name;
  }

  function clearSelection() {
    selected = new Map();
    confirming = false;
  }

  async function loadBrowse(parentId: number | null) {
    loading = true;
    errorMessage = '';
    try {
      const page = await listMediaDirs(parentId);
      items = page.items;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      items = [];
    } finally {
      loading = false;
    }
  }

  async function loadSearch(query: string) {
    loading = true;
    errorMessage = '';
    try {
      const page = await searchMediaDirs(query);
      items = page.items;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `搜索失败 ${err.status}: ${err.body}` : String(err);
      items = [];
    } finally {
      loading = false;
    }
  }

  async function refresh() {
    if (mode === 'search') {
      await loadSearch(keyword.trim());
    } else {
      await loadBrowse(currentParentId);
    }
  }

  function search() {
    const query = keyword.trim();
    if (!query) return;
    mode = 'search';
    successMessage = '';
    clearSelection();
    loadSearch(query);
  }

  function reset() {
    keyword = '';
    mode = 'browse';
    crumbs = [{ dir_id: null, name: '库根' }];
    successMessage = '';
    errorMessage = '';
    clearSelection();
    loadBrowse(null);
  }

  function openFolder(item: MediaDirItem) {
    if (item.deletable) return;
    crumbs = [...crumbs, { dir_id: item.dir_id, name: item.display_name }];
    successMessage = '';
    clearSelection();
    loadBrowse(item.dir_id);
  }

  function jumpTo(index: number) {
    if (index >= crumbs.length - 1) return;
    crumbs = crumbs.slice(0, index + 1);
    mode = 'browse';
    successMessage = '';
    clearSelection();
    loadBrowse(crumbs[index].dir_id);
  }

  function toggleSelect(item: MediaDirItem) {
    if (!item.deletable) return;
    const next = new Map(selected);
    if (next.has(item.dir_id)) {
      next.delete(item.dir_id);
    } else {
      next.set(item.dir_id, itemPath(item));
    }
    selected = next;
    confirming = false;
  }

  function selectAll() {
    selected = new Map(deletableItems.map((item) => [item.dir_id, itemPath(item)]));
    confirming = false;
  }

  function deselectAll() {
    clearSelection();
  }

  async function confirmDelete() {
    if (selectedItems.length === 0) return;
    deleting = true;
    errorMessage = '';
    successMessage = '';
    try {
      await deleteMediaDirs(selectedItems);
      const count = selectedItems.length;
      clearSelection();
      successMessage = `已删除 ${count} 个媒体目录`;
      await refresh();
    } catch (err) {
      errorMessage = err instanceof ApiError ? `删除失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      deleting = false;
    }
  }

  loadBrowse(null);
</script>

<section>
  <header class="page-header">
    <div>
      <h1 class="page-title">媒体目录</h1>
      <p class="page-subtitle">LIBRARY</p>
    </div>
    {#if !loading && !errorMessage}
      <span class="page-count">{items.length} 项</span>
    {/if}
  </header>

  <form
    class="search-bar"
    onsubmit={(e) => { e.preventDefault(); search(); }}
  >
    <div class="search-field">
      <svg class="search-icon" viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
        <path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
      </svg>
      <input
        type="text"
        bind:value={keyword}
        placeholder="输入媒体名…"
        class="search-input"
      />
    </div>
    <button type="submit" class="btn-gold" disabled={!keyword.trim()}>搜索</button>
    <button type="button" onclick={reset} class="btn-ghost">重置</button>
  </form>

  {#if mode === 'browse'}
    <nav class="breadcrumbs" aria-label="目录路径">
      {#each crumbs as crumb, index}
        {#if index > 0}
          <span class="crumb-sep">/</span>
        {/if}
        {#if index < crumbs.length - 1}
          <button type="button" class="crumb-link" onclick={() => jumpTo(index)}>
            {crumb.name}
          </button>
        {:else}
          <span class="crumb-current">{crumb.name}</span>
        {/if}
      {/each}
    </nav>
  {/if}

  {#if deletableItems.length > 0}
    <div class="import-bar">
      <div class="import-bar-left">
        <button type="button" onclick={selectAll} class="btn-ghost btn-sm">全选</button>
        <button type="button" onclick={deselectAll} class="btn-ghost btn-sm">取消</button>
        <span class="import-count">{selected.size} 已选</span>
      </div>
      <button
        type="button"
        onclick={() => { confirming = true; }}
        disabled={selected.size === 0 || deleting}
        class="btn-danger btn-sm"
      >
        删除选中 ({selected.size})
      </button>
    </div>
  {/if}

  {#if confirming && selectedItems.length > 0}
    <div class="results-panel">
      <div class="results-header">
        <h3>确认删除</h3>
      </div>
      <div class="confirm-body">
        <p class="confirm-copy">将把远程目录移入回收站，并删除本地 strm</p>
        <ul class="confirm-list">
          {#each selectedItems as item (item.dir_id)}
            <li>{item.relative_path}</li>
          {/each}
        </ul>
        <div class="confirm-actions">
          <button type="button" class="btn-danger btn-sm" onclick={confirmDelete} disabled={deleting}>
            {deleting ? '删除中…' : '确认删除'}
          </button>
          <button type="button" class="btn-ghost btn-sm" onclick={() => { confirming = false; }} disabled={deleting}>
            取消
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if successMessage}
    <div class="success-banner">{successMessage}</div>
  {/if}

  {#if errorMessage}
    <div class="error-banner">
      <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
        <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
      </svg>
      <span>{errorMessage}</span>
    </div>
  {/if}

  {#if loading}
    <div class="loading-state">
      <div class="loading-bar"></div>
      <p>正在加载…</p>
    </div>
  {:else if items.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
          <path d="M3 7h6l2 2h10v10H3z" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>
      <p class="empty-text">{mode === 'search' ? '没有匹配的媒体目录' : '这个目录是空的'}</p>
    </div>
  {:else}
    <div class="dir-list">
      {#each items as item, i (item.dir_id)}
        {#if item.deletable}
          <article
            class="dir-card media-card"
            class:selected={selected.has(item.dir_id)}
            style="animation-delay: {Math.min(i * 40, 400)}ms"
          >
            <div class="media-row">
              <label class="file-checkbox">
                <input
                  type="checkbox"
                  checked={selected.has(item.dir_id)}
                  onchange={() => toggleSelect(item)}
                />
              </label>
              <button type="button" class="media-main" onclick={() => toggleSelect(item)}>
                <svg class="row-icon" viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
                  <path d="M4 3a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V7.414A2 2 0 0017.414 6L14 2.586A2 2 0 0012.586 2H4z"/>
                </svg>
                <div class="row-text">
                  <span class="row-title">{item.display_name}</span>
                  <span class="row-path">{itemPath(item)}</span>
                </div>
              </button>
            </div>
          </article>
        {:else}
          <article class="dir-card folder-card" style="animation-delay: {Math.min(i * 40, 400)}ms">
            <button type="button" class="folder-row" onclick={() => openFolder(item)}>
              <svg class="row-icon" viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/>
              </svg>
              <span class="row-title">{item.display_name}</span>
              <svg class="row-chevron" viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
                <path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
              </svg>
            </button>
          </article>
        {/if}
      {/each}
    </div>
  {/if}
</section>

<style>
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

  .search-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 12px;
    padding: 16px 20px;
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
    border-radius: 6px;
    margin-bottom: 16px;
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
  .btn-gold:hover:not(:disabled) {
    background: linear-gradient(135deg, var(--color-bb-gold), var(--color-bb-gold-light));
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
  }
  .btn-gold:disabled {
    opacity: 0.4;
    cursor: not-allowed;
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
  .btn-danger {
    padding: 6px 12px;
    background: color-mix(in srgb, var(--color-bb-red) 85%, black);
    color: #fff;
    font-family: var(--font-display);
    font-size: 14px;
    letter-spacing: 0.08em;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }
  .btn-danger:hover:not(:disabled) {
    background: var(--color-bb-red);
  }
  .btn-sm {
    padding: 6px 12px;
    font-size: 12px;
  }
  .btn-sm:disabled,
  .btn-ghost:disabled,
  .btn-danger:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .breadcrumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 16px;
    font-size: 13px;
  }
  .crumb-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--color-bb-gold);
    cursor: pointer;
    font: inherit;
  }
  .crumb-link:hover {
    color: var(--color-bb-gold-light);
  }
  .crumb-current {
    color: var(--color-bb-cream);
  }
  .crumb-sep {
    color: var(--color-bb-text-muted);
  }

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

  .results-panel {
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-red) 30%, transparent);
    border-radius: 6px;
    margin-bottom: 16px;
    overflow: hidden;
  }
  .results-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-bb-red) 15%, transparent);
    background: color-mix(in srgb, var(--color-bb-red) 8%, transparent);
  }
  .results-header h3 {
    font-family: var(--font-display);
    font-size: 16px;
    color: var(--color-bb-cream);
    letter-spacing: 0.05em;
    margin: 0;
  }
  .confirm-body {
    padding: 16px 20px 18px;
  }
  .confirm-copy {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--color-bb-text);
  }
  .confirm-list {
    margin: 0 0 16px;
    padding-left: 18px;
    color: var(--color-bb-cream);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.7;
    word-break: break-all;
  }
  .confirm-actions {
    display: flex;
    gap: 8px;
  }

  .success-banner {
    padding: 12px 16px;
    margin-bottom: 16px;
    background: color-mix(in srgb, var(--color-bb-green) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bb-green) 25%, transparent);
    border-radius: 6px;
    font-size: 13px;
    color: #8fd48f;
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

  .loading-state,
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 64px 0;
  }
  .loading-state p,
  .empty-text {
    font-size: 14px;
    color: var(--color-bb-text-muted);
  }
  .empty-icon {
    color: var(--color-bb-muted);
    opacity: 0.5;
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

  .dir-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .dir-card {
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
    border-radius: 6px;
    overflow: hidden;
    animation: file-enter 0.4s ease both;
  }
  .dir-card:hover {
    border-color: color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
  }
  .media-card.selected {
    border-color: color-mix(in srgb, var(--color-bb-gold) 40%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
  }
  @keyframes file-enter {
    from { opacity: 0; transform: translateX(-8px); }
    to { opacity: 1; transform: translateX(0); }
  }

  .folder-row,
  .media-row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 14px 18px;
    background: none;
    border: none;
    color: inherit;
    text-align: left;
    font: inherit;
  }
  .folder-row,
  .media-main {
    cursor: pointer;
  }
  .media-main {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1;
    padding: 0;
    background: none;
    border: none;
    color: inherit;
    text-align: left;
    font: inherit;
  }
  .row-icon {
    flex-shrink: 0;
    color: var(--color-bb-gold-dim);
  }
  .row-chevron {
    margin-left: auto;
    color: var(--color-bb-text-muted);
  }
  .row-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row-title {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-bb-cream);
  }
  .row-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-bb-text-muted);
    word-break: break-all;
  }
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

  @media (max-width: 640px) {
    .search-bar,
    .import-bar {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
