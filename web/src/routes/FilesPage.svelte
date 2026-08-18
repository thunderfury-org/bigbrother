<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
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
  <header class="page-header">
    <h1 class="page-title">文件索引</h1>
    {#if hasSearched && !loading && !errorMessage && lastQuery}
      <span class="page-count">{items.length} 条结果</span>
    {/if}
  </header>

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); run(); }}>
    <div class="search-wrap">
      <Search class="search-icon" size={16} />
      <input
        type="text"
        bind:value={keyword}
        placeholder="输入文件名、路径或描述片段…"
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

  {#if items.length > 0}
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

  {#if importResults}
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

  {#if errorMessage}
    <div class="banner banner-error">{errorMessage}</div>
  {/if}

  {#if loading}
    <div class="loading">
      <div class="loading-bar"></div>
      <p>正在搜索…</p>
    </div>
  {:else if !hasSearched || !lastQuery}
    <div class="empty">输入关键字开始搜索</div>
  {:else if items.length === 0}
    <div class="empty">没有匹配的文件</div>
  {:else}
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
  {/if}
</section>
