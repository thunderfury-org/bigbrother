<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import Plus from '@lucide/svelte/icons/plus';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import List from '@lucide/svelte/icons/list';
  import Search from '@lucide/svelte/icons/search';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import {
    ApiError,
    listSubscriptions,
    searchCandidates,
    createSubscription,
    deleteSubscription,
    rescanSubscription,
    type SubscriptionItem,
    type CandidateItem,
    type SubscriptionRescanResult,
  } from '../lib/api';
  import { toasts } from '../lib/toast.svelte';
  import { statusLabel } from '../lib/importDisplay';
  import ImportSummaryItems from '../lib/ImportSummaryItems.svelte';
  import Skeleton from '../lib/Skeleton.svelte';

  type MediaFilter = 'all' | 'movie' | 'tv';
  type ViewMode = 'grid' | 'list';

  let items: SubscriptionItem[] = $state([]);
  let loading = $state(false);
  let viewMode: ViewMode = $state('grid');

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
  let rescanResult: SubscriptionRescanResult | null = $state(null);
  let rescanError = $state('');

  let confirmDeleteId = $state<number | null>(null);
  let deletingId = $state<number | null>(null);

  const visibleCandidates = $derived(
    mediaFilter === 'all' ? candidates : candidates.filter((c) => c.media_type === mediaFilter)
  );

  async function load() {
    loading = true;
    try {
      const resp = await listSubscriptions();
      items = resp.items;
    } catch (err) {
      const msg = err instanceof ApiError ? `加载订阅失败: ${err.body}` : String(err);
      toasts.error(msg);
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
      searchError = err instanceof ApiError ? `搜索失败: ${err.body}` : String(err);
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
      toasts.success(`成功订阅「${c.title || c.original_title}」`);
      await load();
    } catch (err) {
      const msg = err instanceof ApiError ? `添加失败: ${err.body}` : String(err);
      addError = msg;
      toasts.error(msg);
    } finally {
      addingKey = null;
    }
  }

  function isAlreadySubscribed(c: CandidateItem): boolean {
    return items.some((s) => s.tmdb_id === c.tmdb_id && s.media_type === c.media_type);
  }

  async function doDelete(id: number, title: string) {
    if (deletingId != null) return;
    deletingId = id;
    try {
      await deleteSubscription(id);
      confirmDeleteId = null;
      toasts.success(`已删除订阅「${title}」`);
      await load();
    } catch (err) {
      const msg = err instanceof ApiError ? `删除失败: ${err.body}` : String(err);
      toasts.error(msg);
    } finally {
      deletingId = null;
    }
  }

  async function doRescan(item: SubscriptionItem) {
    addOpen = false;
    rescanningId = item.id;
    rescanResult = null;
    rescanError = '';
    const title = displayTitle(item);
    toasts.info(`已下发「${title}」重扫任务`);
    try {
      const result = await rescanSubscription(item.id);
      rescanResult = result;
      if (result.status === 'failed') {
        toasts.error(`「${title}」重扫失败${result.error ? `: ${result.error}` : ''}`);
      } else if (result.status === 'partially_failed') {
        toasts.warning(`「${title}」重扫完成，部分失败`);
      } else if (result.summary) {
        toasts.success(`「${title}」重扫完成`);
      } else {
        toasts.info(`「${title}」重扫完成，未发现新文件`);
      }
    } catch (err) {
      rescanError = err instanceof ApiError ? `重扫失败: ${err.body}` : String(err);
      toasts.error(rescanError);
    } finally {
      rescanningId = null;
    }
  }

  function closeRescan() {
    rescanResult = null;
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

  function posterUrl(path: string | null | undefined, size: 'w92' | 'w185' | 'w300' = 'w300'): string | null {
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

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (addOpen) closeAdd();
        if (rescanResult || rescanError) closeRescan();
        if (confirmDeleteId != null) confirmDeleteId = null;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  load();
</script>

<section>
  <header class="page-header">
    <div>
      <h1 class="page-title">
        订阅管理
        {#if items.length > 0}
          <span class="page-count">{items.length} 个目标</span>
        {/if}
      </h1>
    </div>
    <div class="page-actions">
      <div class="view-toggle" role="group" aria-label="视图模式">
        <button
          type="button"
          class="toggle-btn"
          class:is-active={viewMode === 'grid'}
          onclick={() => { viewMode = 'grid'; }}
          aria-label="网格视图"
        >
          <LayoutGrid size={15} />
          <span>网格</span>
        </button>
        <button
          type="button"
          class="toggle-btn"
          class:is-active={viewMode === 'list'}
          onclick={() => { viewMode = 'list'; }}
          aria-label="列表视图"
        >
          <List size={15} />
          <span>列表</span>
        </button>
      </div>

      <button type="button" class="btn btn-primary" onclick={openAdd}>
        <Plus size={16} />
        <span>添加订阅</span>
      </button>
    </div>
  </header>

  {#if loading}
    <div class="poster-grid">
      {#each Array(6) as _, i (i)}
        <div class="poster-card">
          <Skeleton height="240px" rounded="10px 10px 0 0" />
          <div class="poster-meta">
            <Skeleton height="16px" width="80%" />
            <Skeleton height="12px" width="50%" />
          </div>
        </div>
      {/each}
    </div>
  {:else if items.length === 0}
    <div class="empty">
      <p style="margin-bottom: 12px;">还没有订阅，添加 TMDB 目标后系统将自动从网盘与频道导入</p>
      <button type="button" class="btn btn-primary" onclick={openAdd}>
        <Plus size={16} />
        添加第一个订阅
      </button>
    </div>
  {:else if viewMode === 'grid'}
    <!-- Poster Grid View -->
    <div class="poster-grid">
      {#each items as item (item.id)}
        <article class="poster-card">
          <div class="poster-wrap">
            {#if posterUrl(item.poster_path, 'w300')}
              <img
                class="poster-img"
                src={posterUrl(item.poster_path, 'w300')}
                alt={displayTitle(item)}
                loading="lazy"
              />
            {:else}
              <div class="media-poster-fallback" style="height: 100%; font-size: 13px;">
                {mediaTypeLabel(item.media_type)}
              </div>
            {/if}
            <div class="poster-overlay"></div>
            <span
              class="poster-badge"
              class:badge-tv={item.media_type === 'tv'}
              class:badge-movie={item.media_type === 'movie'}
            >
              {mediaTypeLabel(item.media_type)}
            </span>
          </div>

          <div class="poster-meta">
            <div class="poster-title" title={titleWithYear(displayTitle(item), item.year)}>
              {titleWithYear(displayTitle(item), item.year)}
            </div>
            <div class="poster-sub">
              <span class="poster-sub-title" title={originalTitle(item) ?? ''}>
                {originalTitle(item) || '—'}
              </span>
              <a
                class="media-tmdb"
                href={tmdbUrl(item.media_type, item.tmdb_id)}
                target="_blank"
                rel="noreferrer"
                title="前往 TMDB 页面"
              >
                #{item.tmdb_id}
              </a>
            </div>

            {#if confirmDeleteId === item.id}
              <div class="poster-actions">
                <button
                  type="button"
                  class="btn btn-danger btn-sm"
                  style="flex: 1;"
                  disabled={deletingId === item.id}
                  onclick={() => doDelete(item.id, displayTitle(item))}
                >
                  {deletingId === item.id ? '删除中…' : '确认'}
                </button>
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  style="flex: 1;"
                  disabled={deletingId === item.id}
                  onclick={() => { confirmDeleteId = null; }}
                >
                  取消
                </button>
              </div>
            {:else}
              <div class="poster-actions">
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  style="flex: 1;"
                  disabled={rescanningId === item.id || deletingId != null}
                  onclick={() => doRescan(item)}
                >
                  <RefreshCw size={12} class={rescanningId === item.id ? 'animate-spin' : ''} />
                  <span>{rescanningId === item.id ? '重扫中' : '重扫'}</span>
                </button>
                <button
                  type="button"
                  class="btn btn-danger-ghost btn-sm"
                  style="flex: 1;"
                  disabled={deletingId != null}
                  onclick={() => { confirmDeleteId = item.id; }}
                >
                  <Trash2 size={12} />
                  <span>删除</span>
                </button>
              </div>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {:else}
    <!-- List View -->
    <div class="media-list">
      {#each items as item (item.id)}
        <article class="media-row">
          {#if posterUrl(item.poster_path, 'w185')}
            <img
              class="media-poster"
              src={posterUrl(item.poster_path, 'w185')}
              alt=""
              width="48"
              height="72"
            />
          {:else}
            <div class="media-poster media-poster-fallback">{mediaTypeLabel(item.media_type)}</div>
          {/if}
          <div class="media-body">
            <div class="media-kicker">
              <span
                class="poster-badge"
                style="position: static;"
                class:badge-tv={item.media_type === 'tv'}
                class:badge-movie={item.media_type === 'movie'}
              >
                {mediaTypeLabel(item.media_type)}
              </span>
              <a
                class="media-tmdb"
                href={tmdbUrl(item.media_type, item.tmdb_id)}
                target="_blank"
                rel="noreferrer"
              >
                TMDB {item.tmdb_id} <ExternalLink size={11} style="display: inline;" />
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
              onclick={() => doRescan(item)}
            >
              <RefreshCw size={13} class={rescanningId === item.id ? 'animate-spin' : ''} />
              <span>{rescanningId === item.id ? '重扫中…' : '重扫'}</span>
            </button>
            {#if confirmDeleteId === item.id}
              <button
                type="button"
                class="btn btn-danger btn-sm"
                disabled={deletingId === item.id}
                onclick={() => doDelete(item.id, displayTitle(item))}
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
                <Trash2 size={13} />
                <span>删除</span>
              </button>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}

  <!-- Add Drawer -->
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
          <div class="search-wrap">
            <Search class="search-icon" size={16} />
            <input
              type="text"
              bind:value={searchQuery}
              placeholder="输入电影或剧集名称…"
              class="input"
            />
          </div>
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
            <p>正在搜索 TMDB 候选目标…</p>
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
                    width="58"
                    height="87"
                  />
                {:else}
                  <div class="media-poster media-poster-lg media-poster-fallback">
                    {mediaTypeLabel(c.media_type)}
                  </div>
                {/if}
                <div class="media-body">
                  <div class="media-kicker">
                    <span
                      class="poster-badge"
                      style="position: static;"
                      class:badge-tv={c.media_type === 'tv'}
                      class:badge-movie={c.media_type === 'movie'}
                    >
                      {mediaTypeLabel(c.media_type)}
                    </span>
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
                    class="btn btn-sm"
                    class:btn-primary={!subscribed && addingKey !== candidateKey(c)}
                    class:btn-ghost={subscribed || addingKey === candidateKey(c)}
                    disabled={addingKey === candidateKey(c) || subscribed}
                    onclick={() => addCandidate(c)}
                  >
                    {#if subscribed}
                      已订阅
                    {:else if addingKey === candidateKey(c)}
                      添加中…
                    {:else}
                      + 订阅
                    {/if}
                  </button>
                </div>
              </article>
            {/each}
          </div>
        {:else if hasSearched}
          <div class="empty">没有匹配的 TMDB 结果</div>
        {:else}
          <div class="empty">输入片名开始搜索 TMDB 资源</div>
        {/if}
      </div>
    </aside>
  {/if}

  <!-- Rescan Results Drawer -->
  {#if rescanResult || rescanError}
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
        {:else if rescanResult}
          {#if rescanResult.summary}
            <div class="import-summary" data-status={rescanResult.status}>
              <div class="result-row">
                <span class="status status-{rescanResult.status}">
                  <span class="pulse-dot"></span>
                  {statusLabel(rescanResult.status)}
                </span>
                {#if rescanResult.title}
                  <span class="cell-title">{rescanResult.title}{rescanResult.year ? ` (${rescanResult.year})` : ''}</span>
                {/if}
              </div>
              <ImportSummaryItems summary={rescanResult.summary} />
            </div>
          {:else if rescanResult.status === 'failed'}
            <div class="banner banner-error">{rescanResult.error || '重扫失败'}</div>
          {:else}
            <div class="empty">没有找到匹配的新文件</div>
          {/if}
        {/if}
      </div>
    </aside>
  {/if}
</section>
