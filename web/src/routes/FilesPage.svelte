<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
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
  import {
    failedEpisodes,
    formatEpisodes,
    formatSeasonLabel,
    succeededEpisodes,
  } from '../lib/importDisplay';

  let keyword = $state('');
  let limit = $state(50);
  let lastQuery = $state('');
  let items: FileSearchItem[] = $state([]);
  let threads: CommunityThread[] = $state([]);
  let loading = $state(false);
  let fileError = $state('');
  let communityError = $state('');
  let hasSearched = $state(false);
  let activeTab: 'files' | 'community' = $state('files');

  let selectedIds: Set<number> = $state(new Set());
  let selectedTids: Set<number> = $state(new Set());
  let importing = $state(false);
  let importResults: ImportFileResult[] | null = $state(null);
  let communityImportResults: CommunityImportResult[] | null = $state(null);

  async function run() {
    const q = keyword.trim();
    lastQuery = q;
    hasSearched = true;
    loading = true;
    fileError = '';
    communityError = '';
    selectedIds = new Set();
    selectedTids = new Set();
    importResults = null;
    communityImportResults = null;
    try {
      const [fileResult, communityResult] = await Promise.allSettled([
        searchFiles(q, limit),
        searchCommunityThreads(q, limit),
      ]);
      if (fileResult.status === 'fulfilled') {
        items = fileResult.value.items;
      } else {
        const err = fileResult.reason;
        fileError = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
        items = [];
      }
      if (communityResult.status === 'fulfilled') {
        threads = communityResult.value.items;
      } else {
        const err = communityResult.reason;
        communityError = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
        threads = [];
      }
    } finally {
      loading = false;
    }
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
    selectedIds = new Set();
    selectedTids = new Set();
    importResults = null;
    communityImportResults = null;
    activeTab = 'files';
  }

  function toggleSelect(id: number) {
    if (selectedIds.has(id)) {
      selectedIds.delete(id);
    } else {
      selectedIds.add(id);
    }
    selectedIds = new Set(selectedIds);
  }

  function toggleThread(tid: number) {
    if (selectedTids.has(tid)) {
      selectedTids.delete(tid);
    } else {
      selectedTids.add(tid);
    }
    selectedTids = new Set(selectedTids);
  }

  function selectAll() {
    selectedIds = new Set(items.map(item => item.id));
  }

  function deselectAll() {
    selectedIds = new Set();
  }

  function selectAllThreads() {
    selectedTids = new Set(threads.map(thread => thread.tid));
  }

  function deselectAllThreads() {
    selectedTids = new Set();
  }

  async function runImport() {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    importing = true;
    importResults = null;
    fileError = '';
    try {
      const resp = await importFiles(ids);
      importResults = resp.results;
    } catch (err) {
      fileError = err instanceof ApiError ? `导入失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      importing = false;
      selectedIds = new Set();
    }
  }

  async function runCommunityImport() {
    const tids = Array.from(selectedTids);
    if (tids.length === 0) return;
    importing = true;
    communityImportResults = null;
    communityError = '';
    try {
      const resp = await importCommunityThreads(tids);
      communityImportResults = resp.results;
    } catch (err) {
      communityError = err instanceof ApiError ? `导入失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      importing = false;
      selectedTids = new Set();
    }
  }

  function formatCost(ms: number): string {
    if (!ms) return '—';
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
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
      case 'partially_failed': return '部分失败';
      default: return status;
    }
  }

  const currentError = $derived(activeTab === 'files' ? fileError : communityError);
  const currentCount = $derived(activeTab === 'files' ? items.length : threads.length);
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">搜索</h1>
    {#if hasSearched && !loading && !currentError && lastQuery}
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
      />
    </div>
    <label class="field">
      <span class="field-label">条数</span>
      <select bind:value={limit} class="select">
        <option value={20}>20</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={200}>200</option>
      </select>
    </label>
    <button type="submit" class="btn btn-primary">搜索</button>
    <button type="button" onclick={reset} class="btn btn-ghost">重置</button>
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
        文件索引 {#if hasSearched && !loading}<span class="tab-count">{items.length}</span>{/if}
      </button>
      <button
        type="button"
        role="tab"
        class="tab"
        class:is-active={activeTab === 'community'}
        aria-selected={activeTab === 'community'}
        onclick={() => { activeTab = 'community'; }}
      >
        123分享社区 {#if hasSearched && !loading}<span class="tab-count">{threads.length}</span>{/if}
      </button>
    </div>
  {/if}

  {#if activeTab === 'files' && items.length > 0}
    <div class="work-band">
      <div class="work-band-left">
        <button type="button" onclick={selectAll} class="btn btn-ghost btn-sm">全选</button>
        <button type="button" onclick={deselectAll} class="btn btn-ghost btn-sm">取消</button>
        <span class="work-count">{selectedIds.size} 已选</span>
      </div>
      <button
        type="button"
        onclick={runImport}
        disabled={selectedIds.size === 0 || importing}
        class="btn btn-primary btn-sm"
      >
        {importing ? '导入中…' : `导入选中 (${selectedIds.size})`}
      </button>
    </div>
  {/if}

  {#if activeTab === 'community' && threads.length > 0}
    <div class="work-band">
      <div class="work-band-left">
        <button type="button" onclick={selectAllThreads} class="btn btn-ghost btn-sm">全选</button>
        <button type="button" onclick={deselectAllThreads} class="btn btn-ghost btn-sm">取消</button>
        <span class="work-count">{selectedTids.size} 已选</span>
      </div>
      <button
        type="button"
        onclick={runCommunityImport}
        disabled={selectedTids.size === 0 || importing}
        class="btn btn-primary btn-sm"
      >
        {importing ? '解锁导入中…' : `导入选中 (${selectedTids.size})`}
      </button>
    </div>
  {/if}

  {#if activeTab === 'files' && importResults}
    <div class="panel">
      <div class="panel-header">
        <h3 class="panel-title">导入结果</h3>
        <button type="button" onclick={() => { importResults = null; }} class="btn btn-ghost btn-sm">关闭</button>
      </div>
      {#each importResults as result}
        <div class="result-row" data-status={result.status}>
          <span class="status status-{result.status}">{statusLabel(result.status)}</span>
          {#if result.title}
            <span class="cell-title">{result.title}</span>
          {/if}
          {#if result.year}
            <span class="cell-sub">{result.year}</span>
          {/if}
          {#if result.size}
            <span class="mono cell-sub">{formatSize(result.size)}</span>
          {/if}
          {#if result.error}
            <span class="banner-error">{result.error}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if activeTab === 'community' && communityImportResults}
    <div class="panel">
      <div class="panel-header">
        <h3 class="panel-title">导入总结</h3>
        <button type="button" onclick={() => { communityImportResults = null; }} class="btn btn-ghost btn-sm">关闭</button>
      </div>
      {#each communityImportResults as result}
        <div class="import-summary" data-status={result.status}>
          <div class="result-row">
            <span class="status status-{result.status}">{statusLabel(result.status)}</span>
            <span class="cell-title">{result.thread_title}</span>
          </div>
          {#if result.summary}
            <div class="detail-grid">
              {#each result.summary.items as item, index (index)}
                {#if item.type === 'movie'}
                  <div>
                    <span class="detail-label">电影</span>
                    <span class="detail-value">{item.year ? `${item.title} (${item.year})` : item.title}</span>
                  </div>
                  <div>
                    <span class="detail-label">本次结果</span>
                    <span class="detail-value">{item.succeeded ? '入库成功' : '入库失败'}</span>
                  </div>
                  {#if item.size}
                    <div>
                      <span class="detail-label">大小</span>
                      <span class="detail-value">{formatSize(item.size)}</span>
                    </div>
                  {/if}
                  {#if item.cost_ms}
                    <div>
                      <span class="detail-label">耗时</span>
                      <span class="detail-value">{formatCost(item.cost_ms)}</span>
                    </div>
                  {/if}
                {:else if item.type === 'tv'}
                  <div>
                    <span class="detail-label">剧集</span>
                    <span class="detail-value">{item.year ? `${item.name} (${item.year})` : item.name} {formatSeasonLabel(item.season)}</span>
                  </div>
                  {#if succeededEpisodes(item).length > 0}
                    <div>
                      <span class="detail-label">本次入库</span>
                      <span class="detail-value">{formatEpisodes(succeededEpisodes(item))}</span>
                    </div>
                  {/if}
                  {#if failedEpisodes(item).length > 0}
                    <div>
                      <span class="detail-label">本次失败</span>
                      <span class="detail-value">{formatEpisodes(failedEpisodes(item))}</span>
                    </div>
                  {/if}
                  {#if item.missing_episodes.length > 0}
                    <div>
                      <span class="detail-label">库内缺失</span>
                      <span class="detail-value">相对整季还缺 {formatEpisodes(item.missing_episodes)}</span>
                    </div>
                  {/if}
                  {#if item.total_size}
                    <div>
                      <span class="detail-label">大小</span>
                      <span class="detail-value">{formatSize(item.total_size)}</span>
                    </div>
                  {/if}
                  {#if item.cost_ms}
                    <div>
                      <span class="detail-label">耗时</span>
                      <span class="detail-value">{formatCost(item.cost_ms)}</span>
                    </div>
                  {/if}
                {:else if item.type === 'skipped' && result.summary.skipped_files.length === 0 && item.files.length > 0}
                  <div>
                    <span class="detail-label">跳过文件</span>
                    <div class="detail-value">
                      {#each item.files as file}
                        <div class="mono">{file}</div>
                      {/each}
                    </div>
                  </div>
                {/if}
              {/each}
              {#if result.summary.skipped_files.length > 0}
                <div>
                  <span class="detail-label">跳过文件</span>
                  <div class="detail-value">
                    {#each result.summary.skipped_files as file}
                      <div class="mono">{file}</div>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          {:else if result.title}
            <div class="cell-sub">{result.title}{result.year ? ` (${result.year})` : ''}</div>
          {/if}
          {#if result.share_url}
            <div class="mono cell-sub">{result.share_url}</div>
          {/if}
          {#if result.error}
            <div class="banner-error">{result.error}</div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if currentError}
    <div class="banner banner-error">{currentError}</div>
  {/if}

  {#if loading}
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
            <th class="col-check"></th>
            <th>帖子</th>
            <th>作者</th>
            <th>时间</th>
            <th>评论</th>
          </tr>
        </thead>
        <tbody>
          {#each threads as thread (thread.tid)}
            <tr class:is-selected={selectedTids.has(thread.tid)}>
              <td class="col-check">
                <label>
                  <input
                    type="checkbox"
                    checked={selectedTids.has(thread.tid)}
                    onchange={() => toggleThread(thread.tid)}
                  />
                </label>
              </td>
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
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>
