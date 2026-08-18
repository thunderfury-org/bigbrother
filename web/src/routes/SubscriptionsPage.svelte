<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import {
    ApiError,
    listSubscriptions,
    searchCandidates,
    createSubscription,
    deleteSubscription,
    rescanSubscription,
    type SubscriptionItem,
    type CandidateItem,
    type ImportFileResult,
  } from '../lib/api';

  let items: SubscriptionItem[] = $state([]);
  let loading = $state(false);
  let errorMessage = $state('');

  let searchQuery = $state('');
  let searching = $state(false);
  let candidates: CandidateItem[] = $state([]);
  let searchError = $state('');

  let adding = $state(false);

  let rescanningId = $state<number | null>(null);
  let rescanResults: ImportFileResult[] | null = $state(null);
  let rescanError = $state('');

  let confirmDeleteId = $state<number | null>(null);

  async function load() {
    loading = true;
    errorMessage = '';
    try {
      const resp = await listSubscriptions();
      items = resp.items;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      loading = false;
    }
  }

  async function doSearch() {
    const q = searchQuery.trim();
    if (!q) return;
    searching = true;
    searchError = '';
    candidates = [];
    try {
      const resp = await searchCandidates(q);
      candidates = resp.candidates;
    } catch (err) {
      searchError = err instanceof ApiError ? `搜索失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      searching = false;
    }
  }

  async function addCandidate(c: CandidateItem) {
    adding = true;
    errorMessage = '';
    try {
      await createSubscription({
        tmdb_id: c.tmdb_id,
        media_type: c.media_type,
        title_zh: c.title || undefined,
        title_en: c.original_title !== c.title ? c.original_title : undefined,
      });
      candidates = candidates.filter(
        (x) => !(x.tmdb_id === c.tmdb_id && x.media_type === c.media_type)
      );
      await load();
    } catch (err) {
      errorMessage = err instanceof ApiError ? `添加失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      adding = false;
    }
  }

  function isAlreadySubscribed(c: CandidateItem): boolean {
    return items.some((s) => s.tmdb_id === c.tmdb_id && s.media_type === c.media_type);
  }

  async function doDelete(id: number) {
    errorMessage = '';
    try {
      await deleteSubscription(id);
      confirmDeleteId = null;
      await load();
    } catch (err) {
      errorMessage = err instanceof ApiError ? `删除失败 ${err.status}: ${err.body}` : String(err);
    }
  }

  async function doRescan(id: number) {
    rescanningId = id;
    rescanResults = null;
    rescanError = '';
    try {
      rescanResults = await rescanSubscription(id);
    } catch (err) {
      rescanError = err instanceof ApiError ? `重扫失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      rescanningId = null;
    }
  }

  function closeRescan() {
    rescanResults = null;
    rescanError = '';
  }

  function mediaTypeLabel(mt: string): string {
    return mt === 'movie' ? '电影' : mt === 'tv' ? '剧集' : mt;
  }

  function displayTitle(item: SubscriptionItem): string {
    return item.title_zh || item.title_en || `TMDB #${item.tmdb_id}`;
  }

  function formatTimestamp(value: string | null | undefined): string {
    if (!value) return '—';
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? value : d.toLocaleString();
  }

  function rescanStatusLabel(status: string): string {
    switch (status) {
      case 'succeeded': return '成功';
      case 'failed': return '失败';
      case 'skipped': return '跳过';
      default: return status;
    }
  }

  load();
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">订阅管理</h1>
    {#if items.length > 0}
      <span class="page-count">{items.length} 个订阅</span>
    {/if}
  </header>

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); doSearch(); }}>
    <label class="field field-grow">
      <span class="field-label">TMDB 搜索</span>
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="输入电影或剧集名称…"
        class="input"
      />
    </label>
    <button type="submit" class="btn btn-primary" disabled={searching || !searchQuery.trim()}>
      {searching ? '搜索中…' : '搜索'}
    </button>
  </form>

  {#if searchError}
    <div class="banner banner-error">{searchError}</div>
  {/if}

  {#if candidates.length > 0}
    <div class="panel">
      <div class="panel-header">
        <h2 class="panel-title">搜索结果</h2>
      </div>
      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>类型</th>
              <th>标题</th>
              <th>TMDB</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {#each candidates as c (c.tmdb_id + ':' + c.media_type)}
              <tr>
                <td>{mediaTypeLabel(c.media_type)}</td>
                <td>
                  <div class="cell-stack">
                    <span class="cell-title">{c.title}</span>
                    {#if c.original_title && c.original_title !== c.title}
                      <span class="cell-sub">{c.original_title}</span>
                    {/if}
                  </div>
                </td>
                <td class="mono">{c.tmdb_id}</td>
                <td>
                  <button
                    type="button"
                    class="btn btn-ghost btn-sm"
                    disabled={adding || isAlreadySubscribed(c)}
                    onclick={() => addCandidate(c)}
                  >
                    {isAlreadySubscribed(c) ? '已订阅' : '添加'}
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  {#if errorMessage}
    <div class="banner banner-error">{errorMessage}</div>
  {/if}

  {#if loading}
    <div class="loading">
      <div class="loading-bar"></div>
      <p>正在加载…</p>
    </div>
  {:else if items.length === 0}
    <div class="empty">还没有订阅，搜索并添加 TMDB 目标开始导入</div>
  {:else}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>类型</th>
            <th>标题</th>
            <th>中/英</th>
            <th>TMDB</th>
            <th>创建时间</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr>
              <td>{mediaTypeLabel(item.media_type)}</td>
              <td class="cell-title">{displayTitle(item)}</td>
              <td>
                <div class="cell-stack">
                  {#if item.title_zh}
                    <span>{item.title_zh}</span>
                  {/if}
                  {#if item.title_en && item.title_en !== item.title_zh}
                    <span class="cell-sub">{item.title_en}</span>
                  {/if}
                  {#if !item.title_zh && !item.title_en}
                    <span class="cell-sub">—</span>
                  {/if}
                </div>
              </td>
              <td class="mono">{item.tmdb_id}</td>
              <td>{formatTimestamp(item.create_time)}</td>
              <td>
                <div class="row-actions">
                  <button
                    type="button"
                    class="btn btn-ghost btn-sm"
                    disabled={rescanningId === item.id}
                    onclick={() => doRescan(item.id)}
                  >
                    {rescanningId === item.id ? '重扫中…' : '重扫'}
                  </button>
                  {#if confirmDeleteId === item.id}
                    <button
                      type="button"
                      class="btn btn-danger btn-sm"
                      onclick={() => doDelete(item.id)}
                    >
                      确认删除
                    </button>
                    <button
                      type="button"
                      class="btn btn-ghost btn-sm"
                      onclick={() => { confirmDeleteId = null; }}
                    >
                      取消
                    </button>
                  {:else}
                    <button
                      type="button"
                      class="btn btn-danger-ghost btn-sm"
                      onclick={() => { confirmDeleteId = item.id; }}
                    >
                      删除
                    </button>
                  {/if}
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#if rescanResults || rescanError}
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
    <div class="drawer-backdrop" onclick={closeRescan} role="presentation"></div>
    <aside class="drawer">
      <header class="drawer-header">
        <h2 class="drawer-title">重扫结果</h2>
        <button type="button" onclick={closeRescan} class="drawer-close" aria-label="关闭">
          <X size={16} />
        </button>
      </header>

      <div class="drawer-body">
        {#if rescanError}
          <div class="banner banner-error">{rescanError}</div>
        {:else if rescanResults}
          {#if rescanResults.length === 0}
            <div class="empty">没有找到匹配的文件</div>
          {:else}
            {#each rescanResults as r (r.id)}
              <div class="rescan-item">
                <span class="status status-{r.status}">{rescanStatusLabel(r.status)}</span>
                <span class="cell-title">{r.title ?? '—'}{r.year ? ` (${r.year})` : ''}</span>
                {#if r.error}
                  <span class="banner-error">{r.error}</span>
                {/if}
              </div>
            {/each}
          {/if}
        {/if}
      </div>
    </aside>
  {/if}
</section>
