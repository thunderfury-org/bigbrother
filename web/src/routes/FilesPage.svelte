<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import X from '@lucide/svelte/icons/x';
  import {
    ApiError,
    searchFiles,
    importFiles,
    searchCommunityThreads,
    importCommunityThreads,
    type FileSearchItem,
    type ImportFileResult,
    type CommunityThread,
    type CommunityImportResult,
  } from '../lib/api';
  import ImportSummaryItems from '../lib/ImportSummaryItems.svelte';
  import {
    formatSize,
    statusLabel,
  } from '../lib/importDisplay';

  let keyword = $state('');
  let limit = $state(50);
  let lastQuery = $state('');
  let items: FileSearchItem[] = $state([]);
  let threads: CommunityThread[] = $state([]);
  let fileLoading = $state(false);
  let communityLoading = $state(false);
  let fileError = $state('');
  let communityError = $state('');
  let hasSearched = $state(false);
  let activeTab: 'files' | 'community' = $state('files');
  let searchSeq = 0;

  let selectedIds: Set<number> = $state(new Set());
  let importing = $state(false);
  let importOpen = $state(false);
  let importLabel = $state('');
  let importError = $state('');
  let importFileResults: ImportFileResult[] | null = $state(null);
  let importCommunityResults: CommunityImportResult[] | null = $state(null);

  function run() {
    const q = keyword.trim();
    const seq = ++searchSeq;
    lastQuery = q;
    hasSearched = true;
    fileLoading = true;
    communityLoading = true;
    fileError = '';
    communityError = '';
    selectedIds = new Set();
    items = [];
    threads = [];

    void searchFiles(q, limit)
      .then((page) => {
        if (seq !== searchSeq) return;
        items = page.items;
      })
      .catch((err) => {
        if (seq !== searchSeq) return;
        items = [];
        fileError = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      })
      .finally(() => {
        if (seq !== searchSeq) return;
        fileLoading = false;
      });

    void searchCommunityThreads(q, limit)
      .then((page) => {
        if (seq !== searchSeq) return;
        threads = page.items;
      })
      .catch((err) => {
        if (seq !== searchSeq) return;
        threads = [];
        communityError = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      })
      .finally(() => {
        if (seq !== searchSeq) return;
        communityLoading = false;
      });
  }

  function reset() {
    keyword = '';
    limit = 50;
    lastQuery = '';
    items = [];
    threads = [];
    hasSearched = false;
    fileError = '';
    communityError = '';
    fileLoading = false;
    communityLoading = false;
    searchSeq += 1;
    selectedIds = new Set();
    activeTab = 'files';
    closeImport();
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

  function fileLabel(item: FileSearchItem): string {
    return item.locations[0]?.file_name || `${item.hash_type}:${item.hash_value.slice(0, 8)}`;
  }

  function fileLabelById(id: number): string {
    const item = items.find((entry) => entry.id === id);
    return item ? fileLabel(item) : `文件 #${id}`;
  }

  function openImport(label: string) {
    importOpen = true;
    importing = true;
    importLabel = label;
    importError = '';
    importFileResults = null;
    importCommunityResults = null;
  }

  function closeImport() {
    if (importing) return;
    importOpen = false;
    importLabel = '';
    importError = '';
    importFileResults = null;
    importCommunityResults = null;
  }

  async function runFileImport(ids: number[], label: string) {
    if (ids.length === 0 || importing) return;
    openImport(label);
    try {
      const resp = await importFiles(ids);
      importFileResults = resp.results;
      selectedIds = new Set();
    } catch (err) {
      importError = err instanceof ApiError ? `导入失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      importing = false;
    }
  }

  async function runCommunityImport(thread: CommunityThread) {
    if (importing) return;
    openImport(thread.title);
    try {
      const resp = await importCommunityThreads([thread.tid]);
      importCommunityResults = resp.results;
    } catch (err) {
      importError = err instanceof ApiError ? `导入失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      importing = false;
    }
  }

  function importSelectedFiles() {
    const ids = Array.from(selectedIds);
    const label = ids.length === 1
      ? fileLabelById(ids[0])
      : `${ids.length} 个文件`;
    return runFileImport(ids, label);
  }

  $effect(() => {
    if (!importOpen) return;
    void importing;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeImport();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const currentError = $derived(activeTab === 'files' ? fileError : communityError);
  const currentCount = $derived(activeTab === 'files' ? items.length : threads.length);
  const currentLoading = $derived(activeTab === 'files' ? fileLoading : communityLoading);
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">搜索</h1>
    {#if hasSearched && !currentLoading && !currentError && lastQuery}
      <span class="page-count">{currentCount} 条结果</span>
    {/if}
  </header>

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); run(); }}>
    <div class="search-wrap">
      <Search class="search-icon" size={16} />
      <input
        type="text"
        bind:value={keyword}
        placeholder="输入文件名、路径、描述或片名…"
        class="input"
        disabled={importing}
      />
    </div>
    <label class="field">
      <span class="field-label">条数</span>
      <select bind:value={limit} class="select" disabled={importing}>
        <option value={20}>20</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={200}>200</option>
      </select>
    </label>
    <button type="submit" class="btn btn-primary" disabled={importing}>搜索</button>
    <button type="button" onclick={reset} class="btn btn-ghost" disabled={importing}>重置</button>
  </form>

  {#if hasSearched}
    <div class="tabs" role="tablist" aria-label="搜索来源">
      <button
        type="button"
        role="tab"
        class="tab"
        class:is-active={activeTab === 'files'}
        aria-selected={activeTab === 'files'}
        onclick={() => { activeTab = 'files'; }}
      >
        文件索引 {#if hasSearched && !fileLoading}<span class="tab-count">{items.length}</span>{/if}
      </button>
      <button
        type="button"
        role="tab"
        class="tab"
        class:is-active={activeTab === 'community'}
        aria-selected={activeTab === 'community'}
        onclick={() => { activeTab = 'community'; }}
      >
        123分享社区 {#if hasSearched && !communityLoading}<span class="tab-count">{threads.length}</span>{/if}
      </button>
    </div>
  {/if}

  {#if activeTab === 'files' && items.length > 0}
    <div class="work-band">
      <div class="work-band-left">
        <button type="button" onclick={selectAll} class="btn btn-ghost btn-sm" disabled={importing}>全选</button>
        <button type="button" onclick={deselectAll} class="btn btn-ghost btn-sm" disabled={importing}>取消</button>
        <span class="work-count">{selectedIds.size} 已选</span>
      </div>
      <button
        type="button"
        onclick={importSelectedFiles}
        disabled={selectedIds.size === 0 || importing}
        class="btn btn-primary btn-sm"
      >
        导入选中 ({selectedIds.size})
      </button>
    </div>
  {/if}

  {#if currentError}
    <div class="banner banner-error">{currentError}</div>
  {/if}

  {#if currentLoading}
    <div class="loading">
      <div class="loading-bar"></div>
      <p>正在搜索…</p>
    </div>
  {:else if !hasSearched || !lastQuery}
    <div class="empty">输入关键字开始搜索</div>
  {:else if activeTab === 'files' && items.length === 0}
    <div class="empty">没有匹配的文件</div>
  {:else if activeTab === 'community' && threads.length === 0}
    <div class="empty">没有匹配的帖子</div>
  {:else if activeTab === 'files'}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th class="col-check"></th>
            <th>哈希</th>
            <th>大小</th>
            <th>位置</th>
            <th class="col-action"></th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr class:is-selected={selectedIds.has(item.id)}>
              <td class="col-check">
                <label>
                  <input
                    type="checkbox"
                    checked={selectedIds.has(item.id)}
                    disabled={importing}
                    onchange={() => toggleSelect(item.id)}
                  />
                </label>
              </td>
              <td>
                <div class="cell-stack">
                  <span class="mono">
                    <span class="hash-type">{item.hash_type}</span>
                    {item.hash_value.slice(0, 16)}…
                  </span>
                  <span class="mono cell-sub" title="{item.hash_type}:{item.hash_value}">{item.hash_value}</span>
                </div>
              </td>
              <td class="mono">{formatSize(item.size)}</td>
              <td>
                {#each item.locations as loc}
                  <div class="cell-stack" style="margin-bottom: 8px;">
                    <span class="cell-title">{loc.file_name}</span>
                    {#if loc.file_path}
                      <span class="mono cell-sub">{loc.file_path}</span>
                    {/if}
                    {#if loc.descriptions.length}
                      <div>
                        {#each loc.descriptions as desc}
                          <span class="tag" title={desc}>{desc}</span>
                        {/each}
                      </div>
                    {/if}
                  </div>
                {/each}
              </td>
              <td class="col-action">
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  disabled={importing}
                  onclick={() => runFileImport([item.id], fileLabel(item))}
                >
                  导入
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>帖子</th>
            <th>作者</th>
            <th>时间</th>
            <th>评论</th>
            <th class="col-action"></th>
          </tr>
        </thead>
        <tbody>
          {#each threads as thread (thread.tid)}
            <tr>
              <td>
                <div class="cell-stack">
                  <a class="cell-title" href={thread.url} target="_blank" rel="noreferrer">{thread.title}</a>
                  {#if thread.tags.length}
                    <div>
                      {#each thread.tags as tag}
                        <span class="tag">{tag}</span>
                      {/each}
                    </div>
                  {/if}
                </div>
              </td>
              <td>{thread.author}</td>
              <td class="cell-sub">{thread.posted_at}</td>
              <td class="mono">{thread.comments}</td>
              <td class="col-action">
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  disabled={importing}
                  onclick={() => runCommunityImport(thread)}
                >
                  导入
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#if importOpen}
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) closeImport();
      }}
    >
      <div
        class="modal"
        role="dialog"
        tabindex="-1"
        aria-modal="true"
        aria-labelledby="import-dialog-title"
      >
        <header class="modal-header">
          <h2 id="import-dialog-title" class="modal-title">{importing ? '导入中' : '导入结果'}</h2>
          <button
            type="button"
            class="drawer-close"
            aria-label="关闭"
            disabled={importing}
            onclick={closeImport}
          >
            <X size={16} />
          </button>
        </header>
        <div class="modal-body">
          {#if importing}
            <div class="loading">
              <div class="loading-bar"></div>
              <p>正在导入 {importLabel}，请保持此页打开</p>
            </div>
          {:else if importError}
            <div class="banner banner-error">{importError}</div>
          {:else if importFileResults && importFileResults.length > 0}
            {#each importFileResults as result (result.id)}
              <div class="import-summary" data-status={result.status}>
                <div class="result-row">
                  <span class="status status-{result.status}">{statusLabel(result.status)}</span>
                  {#if !result.summary}
                    <span class="cell-title">{fileLabelById(result.id)}</span>
                  {/if}
                </div>
                {#if result.summary}
                  <ImportSummaryItems summary={result.summary} />
                {/if}
                {#if result.summary}
                  <div class="mono cell-sub">{fileLabelById(result.id)}</div>
                {/if}
                {#if result.error}
                  <div class="banner-error">{result.error}</div>
                {/if}
              </div>
            {/each}
          {:else if importCommunityResults && importCommunityResults.length > 0}
            {#each importCommunityResults as result, index (`${result.tid}-${index}`)}
              <div class="import-summary" data-status={result.status}>
                <div class="result-row">
                  <span class="status status-{result.status}">{statusLabel(result.status)}</span>
                  {#if !result.summary}
                    <span class="cell-title">{result.thread_title}</span>
                  {/if}
                </div>
                {#if result.summary}
                  <ImportSummaryItems summary={result.summary} />
                {/if}
                {#if result.share_url}
                  <div class="mono cell-sub">{result.share_url}</div>
                {/if}
                {#if result.error}
                  <div class="banner-error">{result.error}</div>
                {/if}
              </div>
            {/each}
          {:else}
            <div class="empty">没有返回导入结果</div>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</section>
