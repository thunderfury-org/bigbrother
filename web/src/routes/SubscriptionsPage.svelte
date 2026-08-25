<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import Plus from '@lucide/svelte/icons/plus';
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

  type MediaFilter = 'all' | 'movie' | 'tv';

  let items: SubscriptionItem[] = $state([]);
  let loading = $state(false);
  let errorMessage = $state('');

  let addOpen = $state(false);
  let searchQuery = $state('');
  let searching = $state(false);
  let candidates: CandidateItem[] = $state([]);
  let searchError = $state('');
  let addError = $state('');
  let hasSearched = $state(false);
  let mediaFilter: MediaFilter = $state('all');
  let addingKey = $state<string | null>(null);

  let rescanningId = $state<number | null>(null);
  let rescanResults: ImportFileResult[] | null = $state(null);
  let rescanError = $state('');

  let confirmDeleteId = $state<number | null>(null);
  let deletingId = $state<number | null>(null);

  const visibleCandidates = $derived(
    mediaFilter === 'all' ? candidates : candidates.filter((c) => c.media_type === mediaFilter)
  );

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

  function openAdd() {
    closeRescan();
    addOpen = true;
    addError = '';
  }

  function closeAdd() {
    addOpen = false;
    addError = '';
  }

  async function doSearch() {
    const q = searchQuery.trim();
    if (!q) return;
    searching = true;
    searchError = '';
    addError = '';
    hasSearched = true;
    try {
      const resp = await searchCandidates(q);
      candidates = resp.candidates;
    } catch (err) {
      searchError = err instanceof ApiError ? `搜索失败 ${err.status}: ${err.body}` : String(err);
      candidates = [];
    } finally {
      searching = false;
    }
  }

  function candidateKey(c: CandidateItem): string {
    return `${c.media_type}:${c.tmdb_id}`;
  }

  async function addCandidate(c: CandidateItem) {
    addingKey = candidateKey(c);
    addError = '';
    try {
      await createSubscription({
        tmdb_id: c.tmdb_id,
        media_type: c.media_type,
        title_zh: c.title || c.original_title || undefined,
        title_en: c.original_title && c.original_title !== c.title ? c.original_title : undefined,
        year: c.year ?? undefined,
        poster_path: c.poster_path ?? undefined,
        overview: c.overview ?? undefined,
      });
      await load();
    } catch (err) {
      addError = err instanceof ApiError ? `添加失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      addingKey = null;
    }
  }

  function isAlreadySubscribed(c: CandidateItem): boolean {
    return items.some((s) => s.tmdb_id === c.tmdb_id && s.media_type === c.media_type);
  }

  async function doDelete(id: number) {
    if (deletingId != null) return;
    deletingId = id;
    errorMessage = '';
    try {
      await deleteSubscription(id);
      confirmDeleteId = null;
      await load();
    } catch (err) {
      errorMessage = err instanceof ApiError ? `删除失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      deletingId = null;
    }
  }

  async function doRescan(id: number) {
    addOpen = false;
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

  function originalTitle(item: SubscriptionItem): string | null {
    const primary = displayTitle(item);
    const other = item.title_zh === primary ? item.title_en : item.title_zh;
    return other && other !== primary ? other : null;
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

  function posterUrl(path: string | null | undefined, size: 'w92' | 'w185' = 'w92'): string | null {
    if (!path) return null;
    return `https://image.tmdb.org/t/p/${size}${path}`;
  }

  function tmdbUrl(mediaType: string, id: number): string {
    const kind = mediaType === 'tv' ? 'tv' : 'movie';
    return `https://www.themoviedb.org/${kind}/${id}`;
  }

  function titleWithYear(title: string, year: string | null | undefined): string {
    return year ? `${title} (${year})` : title;
  }

  load();
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">订阅管理</h1>
    <div class="page-actions">
      {#if items.length > 0}
        <span class="page-count">{items.length} 个订阅</span>
      {/if}
      <button type="button" class="btn btn-primary" onclick={openAdd}>
        <Plus size={15} />
        添加订阅
      </button>
    </div>
  </header>

  {#if errorMessage}
    <div class="banner banner-error">{errorMessage}</div>
  {/if}

  {#if loading}
    <div class="loading">
      <div class="loading-bar"></div>
      <p>正在加载…</p>
    </div>
  {:else if items.length === 0}
    <div class="empty">
      <p>还没有订阅，添加 TMDB 目标后才会从频道导入</p>
      <button type="button" class="btn btn-primary" onclick={openAdd}>
        <Plus size={15} />
        添加订阅
      </button>
    </div>
  {:else}
    <div class="media-list">
      {#each items as item (item.id)}
        <article class="media-row">
          {#if posterUrl(item.poster_path)}
            <img
              class="media-poster"
              src={posterUrl(item.poster_path)}
              alt=""
              width="46"
              height="69"
            />
          {:else}
            <div class="media-poster media-poster-fallback">{mediaTypeLabel(item.media_type)}</div>
          {/if}
          <div class="media-body">
            <div class="media-kicker">
              <span class="media-type">{mediaTypeLabel(item.media_type)}</span>
              <a
                class="media-tmdb"
                href={tmdbUrl(item.media_type, item.tmdb_id)}
                target="_blank"
                rel="noreferrer"
              >
                TMDB {item.tmdb_id}
              </a>
              <span class="media-time">{formatTimestamp(item.create_time)}</span>
            </div>
            <div class="cell-title">{titleWithYear(displayTitle(item), item.year)}</div>
            {#if originalTitle(item)}
              <div class="cell-sub">{originalTitle(item)}</div>
            {/if}
            {#if item.overview}
              <p class="media-overview">{item.overview}</p>
            {/if}
          </div>
          <div class="row-actions">
            <button
              type="button"
              class="btn btn-ghost btn-sm"
              disabled={rescanningId === item.id || deletingId != null}
              onclick={() => doRescan(item.id)}
            >
              {rescanningId === item.id ? '重扫中…' : '重扫'}
            </button>
            {#if confirmDeleteId === item.id}
              <button
                type="button"
                class="btn btn-danger btn-sm"
                disabled={deletingId === item.id}
                onclick={() => doDelete(item.id)}
              >
                {deletingId === item.id ? '删除中…' : '确认删除'}
              </button>
              <button
                type="button"
                class="btn btn-ghost btn-sm"
                disabled={deletingId === item.id}
                onclick={() => { confirmDeleteId = null; }}
              >
                取消
              </button>
            {:else}
              <button
                type="button"
                class="btn btn-danger-ghost btn-sm"
                disabled={deletingId != null}
                onclick={() => { confirmDeleteId = item.id; }}
              >
                删除
              </button>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}

  {#if addOpen}
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
    <div class="drawer-backdrop" onclick={closeAdd} role="presentation"></div>
    <aside class="drawer drawer-wide">
      <header class="drawer-header">
        <h2 class="drawer-title">添加订阅</h2>
        <button type="button" onclick={closeAdd} class="drawer-close" aria-label="关闭">
          <X size={16} />
        </button>
      </header>

      <div class="drawer-body">
        <form class="drawer-search" onsubmit={(e) => { e.preventDefault(); doSearch(); }}>
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

        <div class="seg" role="group" aria-label="类型筛选">
          <button
            type="button"
            class="seg-btn"
            class:is-active={mediaFilter === 'all'}
            onclick={() => { mediaFilter = 'all'; }}
          >
            全部
          </button>
          <button
            type="button"
            class="seg-btn"
            class:is-active={mediaFilter === 'movie'}
            onclick={() => { mediaFilter = 'movie'; }}
          >
            电影
          </button>
          <button
            type="button"
            class="seg-btn"
            class:is-active={mediaFilter === 'tv'}
            onclick={() => { mediaFilter = 'tv'; }}
          >
            剧集
          </button>
        </div>

        {#if searchError}
          <div class="banner banner-error">{searchError}</div>
        {/if}

        {#if addError}
          <div class="banner banner-error">{addError}</div>
        {/if}

        {#if searching}
          <div class="loading">
            <div class="loading-bar"></div>
            <p>正在搜索…</p>
          </div>
        {:else if visibleCandidates.length > 0}
          <div class="media-list media-list-plain">
            {#each visibleCandidates as c (candidateKey(c))}
              {@const subscribed = isAlreadySubscribed(c)}
              <article class="media-row">
                {#if posterUrl(c.poster_path, 'w185')}
                  <img
                    class="media-poster media-poster-lg"
                    src={posterUrl(c.poster_path, 'w185')}
                    alt=""
                    width="56"
                    height="84"
                  />
                {:else}
                  <div class="media-poster media-poster-lg media-poster-fallback">
                    {mediaTypeLabel(c.media_type)}
                  </div>
                {/if}
                <div class="media-body">
                  <div class="media-kicker">
                    <span class="media-type">{mediaTypeLabel(c.media_type)}</span>
                    <a
                      class="media-tmdb"
                      href={tmdbUrl(c.media_type, c.tmdb_id)}
                      target="_blank"
                      rel="noreferrer"
                    >
                      TMDB {c.tmdb_id}
                    </a>
                  </div>
                  <div class="cell-title">{titleWithYear(c.title, c.year)}</div>
                  {#if c.original_title && c.original_title !== c.title}
                    <div class="cell-sub">{c.original_title}</div>
                  {/if}
                  {#if c.overview}
                    <p class="media-overview">{c.overview}</p>
                  {/if}
                </div>
                <div class="row-actions">
                  <button
                    type="button"
                    class="btn btn-ghost btn-sm"
                    disabled={addingKey === candidateKey(c) || subscribed}
                    onclick={() => addCandidate(c)}
                  >
                    {#if subscribed}
                      已订阅
                    {:else if addingKey === candidateKey(c)}
                      添加中…
                    {:else}
                      添加
                    {/if}
                  </button>
                </div>
              </article>
            {/each}
          </div>
        {:else if hasSearched}
          <div class="empty">没有匹配的 TMDB 结果</div>
        {:else}
          <div class="empty">搜索电影或剧集名称，从结果里添加订阅</div>
        {/if}
      </div>
    </aside>
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
