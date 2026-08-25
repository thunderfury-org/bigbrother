<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import X from '@lucide/svelte/icons/x';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import CheckSquare from '@lucide/svelte/icons/check-square';
  import Square from '@lucide/svelte/icons/square';
  import Download from '@lucide/svelte/icons/download';
  import User from '@lucide/svelte/icons/user';
  import Clock from '@lucide/svelte/icons/clock';
  import MessageSquare from '@lucide/svelte/icons/message-square';
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
  import Skeleton from '../lib/Skeleton.svelte';
  import { toasts } from '../lib/toast.svelte';
  import { formatSize, statusLabel } from '../lib/importDisplay';

  let keyword = $state('');
  let limit = $state(50);
  let items: FileSearchItem[] = $state([]);
  let threads: CommunityThread[] = $state([]);
  let fileLoading = $state(false);
  let communityLoading = $state(false);
  let hasSearched = $state(false);
  let activeTab: 'files' | 'community' = $state('files');
  let searchSeq = 0;

  let selectedIds: Set<number> = $state(new Set());
  let importing = $state(false);
  let importOpen = $state(false);
  let importLabel = $state('');
  let importFileResults: ImportFileResult[] | null = $state(null);
  let importCommunityResults: CommunityImportResult[] | null = $state(null);

  function run() {
    const q = keyword.trim();
    if (!q) return;
    const seq = ++searchSeq;
    hasSearched = true;
    fileLoading = true;
    communityLoading = true;
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
        const msg = err instanceof ApiError ? `文件搜索失败: ${err.body}` : String(err);
        toasts.error(msg);
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
        const msg = err instanceof ApiError ? `社区搜索失败: ${err.body}` : String(err);
        toasts.error(msg);
      })
      .finally(() => {
        if (seq !== searchSeq) return;
        communityLoading = false;
      });
  }

  function reset() {
    keyword = '';
    limit = 50;
    items = [];
    threads = [];
    hasSearched = false;
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
    selectedIds = new Set(items.map((item) => item.id));
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
    importFileResults = null;
    importCommunityResults = null;
  }

  function closeImport() {
    if (importing) return;
    importOpen = false;
    importLabel = '';
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
      toasts.success(`已完成 ${ids.length} 个文件的导入解析`);
    } catch (err) {
      const msg = err instanceof ApiError ? `导入失败: ${err.body}` : String(err);
      toasts.error(msg);
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
      toasts.success(`已完成帖子「${thread.title}」的导入`);
    } catch (err) {
      const msg = err instanceof ApiError ? `导入失败: ${err.body}` : String(err);
      toasts.error(msg);
    } finally {
      importing = false;
    }
  }

  function importSelectedFiles() {
    const ids = Array.from(selectedIds);
    const label = ids.length === 1 ? fileLabelById(ids[0]) : `${ids.length} 个文件`;
    return runFileImport(ids, label);
  }

  $effect(() => {
    if (!importOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeImport();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const currentCount = $derived(activeTab === 'files' ? items.length : threads.length);
  const currentLoading = $derived(activeTab === 'files' ? fileLoading : communityLoading);
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">
      搜索中心
      {#if hasSearched && !currentLoading}
        <span class="page-count">{currentCount} 条结果</span>
      {/if}
    </h1>
  </header>

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); run(); }}>
    <div class="search-wrap">
      <Search class="search-icon" size={16} />
      <input
        type="text"
        bind:value={keyword}
        placeholder="输入文件名、路径、描述或片名关键字…"
        class="input"
        disabled={importing}
      />
    </div>
    <label class="field">
      <span class="field-label">条数限制</span>
      <select bind:value={limit} class="select" disabled={importing}>
        <option value={20}>20 条</option>
        <option value={50}>50 条</option>
        <option value={100}>100 条</option>
        <option value={200}>200 条</option>
      </select>
    </label>
    <button type="submit" class="btn btn-primary" disabled={importing || !keyword.trim()}>
      <Search size={15} />
      <span>搜索</span>
    </button>
    <button type="button" onclick={reset} class="btn btn-ghost" disabled={importing}>
      重置
    </button>
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
        <button type="button" onclick={selectAll} class="btn btn-ghost btn-sm" disabled={importing}>
          <CheckSquare size={13} />
          <span>全选</span>
        </button>
        <button type="button" onclick={deselectAll} class="btn btn-ghost btn-sm" disabled={importing}>
          <Square size={13} />
          <span>取消</span>
        </button>
        <span class="work-count">{selectedIds.size} 项已选择</span>
      </div>
      <button
        type="button"
        onclick={importSelectedFiles}
        disabled={selectedIds.size === 0 || importing}
        class="btn btn-primary btn-sm"
      >
        <Download size={13} />
        <span>导入选中 ({selectedIds.size})</span>
      </button>
    </div>
  {/if}

  {#if currentLoading}
    <div class="data-table-wrap" style="padding: 16px;">
      {#each Array(5) as _, i (i)}
        <div style="display: flex; gap: 16px; padding: 12px 0; border-bottom: 1px solid var(--color-bb-line);">
          <Skeleton width="40px" height="20px" />
          <Skeleton width="180px" height="20px" />
          <Skeleton width="300px" height="20px" />
          <Skeleton width="70px" height="20px" />
        </div>
      {/each}
    </div>
  {:else if !hasSearched}
    <div class="empty">输入关键字回车或点击搜索开始检索文件与社区资源</div>
  {:else if activeTab === 'files' && items.length === 0}
    <div class="empty">文件索引中未找到与「{keyword}」匹配的内容</div>
  {:else if activeTab === 'community' && threads.length === 0}
    <div class="empty">123分享社区中未找到与「{keyword}」匹配的帖子</div>
  {:else if activeTab === 'files'}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th class="col-check"></th>
            <th>哈希 / 标识</th>
            <th>文件大小</th>
            <th>文件名与位置描述</th>
            <th class="col-action"></th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr class:is-selected={selectedIds.has(item.id)}>
              <td class="col-check">
                <input
                  type="checkbox"
                  checked={selectedIds.has(item.id)}
                  disabled={importing}
                  onchange={() => toggleSelect(item.id)}
                  aria-label="选择文件"
                />
              </td>
              <td>
                <div class="cell-stack">
                  <span class="mono">
                    <span class="hash-type">{item.hash_type}</span>
                    {item.hash_value.slice(0, 16)}…
                  </span>
                  <span class="mono cell-sub" title="{item.hash_type}:{item.hash_value}">
                    {item.hash_value}
                  </span>
                </div>
              </td>
              <td class="mono">{formatSize(item.size)}</td>
              <td>
                {#each item.locations as loc}
                  <div class="cell-stack" style="margin-bottom: 8px;">
                    <span class="cell-title">{loc.file_name}</span>
                    {#if loc.file_path}
                      <span class="mono cell-sub text-slate-400">{loc.file_path}</span>
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
                  <Download size={13} />
                  <span>导入</span>
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
            <th>帖子标题与标签</th>
            <th>发布者</th>
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
                  <a class="cell-title" href={thread.url} target="_blank" rel="noreferrer">
                    {thread.title} <ExternalLink size={12} class="inline text-slate-400" />
                  </a>
                  {#if thread.tags.length}
                    <div>
                      {#each thread.tags as tag}
                        <span class="tag">{tag}</span>
                      {/each}
                    </div>
                  {/if}
                </div>
              </td>
              <td>
                <div style="display: flex; align-items: center; gap: 4px; color: var(--color-bb-muted); font-size: 12px;">
                  <User size={13} />
                  <span>{thread.author}</span>
                </div>
              </td>
              <td>
                <div style="display: flex; align-items: center; gap: 4px; color: var(--color-bb-muted); font-size: 12px;">
                  <Clock size={13} />
                  <span>{thread.posted_at}</span>
                </div>
              </td>
              <td>
                <div style="display: flex; align-items: center; gap: 4px; font-size: 12px;" class="mono">
                  <MessageSquare size={13} class="text-slate-400" />
                  <span>{thread.comments}</span>
                </div>
              </td>
              <td class="col-action">
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  disabled={importing}
                  onclick={() => runCommunityImport(thread)}
                >
                  <Download size={13} />
                  <span>导入</span>
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
          <h2 id="import-dialog-title" class="modal-title">{importing ? '正在导入中' : '导入结果'}</h2>
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
              <p>正在导入 {importLabel}，请稍候…</p>
            </div>
          {:else if importFileResults && importFileResults.length > 0}
            {#each importFileResults as result (result.id)}
              <div class="import-summary" data-status={result.status}>
                <div class="result-row">
                  <span class="status status-{result.status}">
                    <span class="pulse-dot"></span>
                    {statusLabel(result.status)}
                  </span>
                  {#if !result.summary}
                    <span class="cell-title">{fileLabelById(result.id)}</span>
                  {/if}
                </div>
                {#if result.summary}
                  <ImportSummaryItems summary={result.summary} />
                {/if}
                {#if result.summary}
                  <div class="mono cell-sub text-slate-400" style="margin-top: 6px;">{fileLabelById(result.id)}</div>
                {/if}
                {#if result.error}
                  <div class="banner-error" style="margin-top: 8px;">{result.error}</div>
                {/if}
              </div>
            {/each}
          {:else if importCommunityResults && importCommunityResults.length > 0}
            {#each importCommunityResults as result, index (`${result.tid}-${index}`)}
              <div class="import-summary" data-status={result.status}>
                <div class="result-row">
                  <span class="status status-{result.status}">
                    <span class="pulse-dot"></span>
                    {statusLabel(result.status)}
                  </span>
                  {#if !result.summary}
                    <span class="cell-title">{result.thread_title}</span>
                  {/if}
                </div>
                {#if result.summary}
                  <ImportSummaryItems summary={result.summary} />
                {/if}
                {#if result.share_url}
                  <div class="mono cell-sub text-slate-400" style="margin-top: 6px;">{result.share_url}</div>
                {/if}
                {#if result.error}
                  <div class="banner-error" style="margin-top: 8px;">{result.error}</div>
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
