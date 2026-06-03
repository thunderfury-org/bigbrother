<script lang="ts">
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

  // Search candidates
  let searchQuery = $state('');
  let searching = $state(false);
  let candidates: CandidateItem[] = $state([]);
  let searchError = $state('');

  // Adding
  let adding = $state(false);

  // Rescan
  let rescanningId = $state<number | null>(null);
  let rescanResults: ImportFileResult[] | null = $state(null);
  let rescanError = $state('');

  // Confirm delete
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

  function mediaTypeIcon(mt: string): string {
    return mt === 'movie' ? '🎬' : mt === 'tv' ? '📺' : '📁';
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
  <!-- Page header -->
  <header class="page-header">
    <div>
      <h1 class="page-title">订阅管理</h1>
      <p class="page-subtitle">SUBSCRIPTIONS</p>
    </div>
    {#if items.length > 0}
      <span class="page-count">{items.length} 个订阅</span>
    {/if}
  </header>

  <!-- Search bar -->
  <form
    class="search-bar"
    onsubmit={(e) => { e.preventDefault(); doSearch(); }}
  >
    <label class="search-field">
      <span class="search-label">TMDB 搜索</span>
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="输入电影或剧集名称…"
        class="search-input"
      />
    </label>
    <div class="search-actions">
      <button type="submit" class="btn-gold" disabled={searching || !searchQuery.trim()}>
        {searching ? '搜索中…' : '搜索'}
      </button>
    </div>
  </form>

  <!-- Search error -->
  {#if searchError}
    <div class="error-banner">
      <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
        <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
      </svg>
      <span>{searchError}</span>
    </div>
  {/if}

  <!-- Candidates -->
  {#if candidates.length > 0}
    <div class="candidates-section">
      <h2 class="candidates-title">搜索结果</h2>
      <div class="candidates-list">
        {#each candidates as c (c.tmdb_id + ':' + c.media_type)}
          <div class="candidate-row">
            <div class="candidate-info">
              <span class="candidate-icon">{mediaTypeIcon(c.media_type)}</span>
              <div class="candidate-text">
                <span class="candidate-name">{c.title}</span>
                {#if c.original_title && c.original_title !== c.title}
                  <span class="candidate-original">{c.original_title}</span>
                {/if}
              </div>
              <span class="candidate-type">{mediaTypeLabel(c.media_type)}</span>
              <span class="candidate-tmdb">TMDB {c.tmdb_id}</span>
            </div>
            <button
              type="button"
              class="btn-ghost btn-sm"
              disabled={adding || isAlreadySubscribed(c)}
              onclick={() => addCandidate(c)}
            >
              {isAlreadySubscribed(c) ? '已订阅' : '添加'}
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- General error -->
  {#if errorMessage}
    <div class="error-banner">
      <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
        <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
      </svg>
      <span>{errorMessage}</span>
    </div>
  {/if}

  <!-- Subscriptions list -->
  {#if loading}
    <div class="loading-state">
      <div class="loading-bar"></div>
      <p>正在加载…</p>
    </div>
  {:else if items.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
          <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>
      <p class="empty-text">还没有订阅，搜索并添加 TMDB 目标开始导入</p>
    </div>
  {:else}
    <div class="cards-grid">
      {#each items as item, i (item.id)}
        <div class="sub-card" style="animation-delay: {Math.min(i * 60, 600)}ms">
          <div class="card-perfs" aria-hidden="true">
            {#each { length: 6 } as _}
              <span class="card-perf"></span>
            {/each}
          </div>

          <div class="card-body">
            <span class="card-type-badge">
              {mediaTypeIcon(item.media_type)} {mediaTypeLabel(item.media_type)}
            </span>

            <h3 class="card-title" title={displayTitle(item)}>{displayTitle(item)}</h3>

            <div class="card-titles">
              {#if item.title_zh}
                <span class="card-lang-title">{item.title_zh}</span>
              {/if}
              {#if item.title_en && item.title_en !== item.title_zh}
                <span class="card-lang-title card-lang-en">{item.title_en}</span>
              {/if}
            </div>

            <div class="card-meta-row">
              <span class="card-tmdb">TMDB {item.tmdb_id}</span>
            </div>

            <time class="card-time">{formatTimestamp(item.create_time)}</time>

            <div class="card-actions">
              <button
                type="button"
                class="btn-ghost btn-sm"
                disabled={rescanningId === item.id}
                onclick={() => doRescan(item.id)}
              >
                {rescanningId === item.id ? '重扫中…' : '重扫'}
              </button>
              {#if confirmDeleteId === item.id}
                <button
                  type="button"
                  class="btn-danger btn-sm"
                  onclick={() => doDelete(item.id)}
                >
                  确认删除
                </button>
                <button
                  type="button"
                  class="btn-ghost btn-sm"
                  onclick={() => { confirmDeleteId = null; }}
                >
                  取消
                </button>
              {:else}
                <button
                  type="button"
                  class="btn-ghost btn-sm btn-danger-outline"
                  onclick={() => { confirmDeleteId = item.id; }}
                >
                  删除
                </button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Rescan results drawer -->
  {#if rescanResults || rescanError}
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
    <div class="drawer-backdrop" onclick={closeRescan} role="presentation"></div>
    <aside class="drawer">
      <header class="drawer-header">
        <div>
          <p class="detail-eyebrow">RESCAN RESULTS</p>
          <h2 class="detail-title">重扫结果</h2>
        </div>
        <button type="button" onclick={closeRescan} class="drawer-close" aria-label="关闭">
          <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
            <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/>
          </svg>
        </button>
      </header>

      <div class="drawer-body">
        {#if rescanError}
          <div class="error-banner">
            <span>{rescanError}</span>
          </div>
        {:else if rescanResults}
          {#if rescanResults.length === 0}
            <div class="empty-state">
              <p class="empty-text">没有找到匹配的文件</p>
            </div>
          {:else}
            <div class="rescan-list">
              {#each rescanResults as r (r.id)}
                <div class="rescan-item rescan-{r.status}">
                  <span class="rescan-status">{rescanStatusLabel(r.status)}</span>
                  <span class="rescan-title">{r.title ?? '—'}{r.year ? ` (${r.year})` : ''}</span>
                  {#if r.error}
                    <span class="rescan-error">{r.error}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    </aside>
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
    align-items: end;
    gap: 12px;
    padding: 16px 20px;
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
    border-radius: 6px;
    margin-bottom: 24px;
  }
  .search-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .search-label {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-bb-text-muted);
  }
  .search-input {
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
  .search-input:focus {
    border-color: var(--color-bb-gold);
  }
  .search-input::placeholder {
    color: var(--color-bb-muted);
  }
  .search-actions {
    display: flex;
    gap: 8px;
  }

  /* ── Buttons ── */
  .btn-gold {
    padding: 7px 20px;
    background: linear-gradient(135deg, var(--color-bb-gold-dim), var(--color-bb-gold));
    color: var(--color-bb-void);
    font-family: var(--font-display);
    font-size: 15px;
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
    opacity: 0.45;
    cursor: not-allowed;
  }

  .btn-ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
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
  .btn-ghost:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .btn-sm {
    padding: 5px 12px;
    font-size: 12px;
  }

  .btn-danger {
    padding: 5px 12px;
    font-size: 12px;
    background: color-mix(in srgb, var(--color-bb-red) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bb-red) 40%, transparent);
    border-radius: 4px;
    color: #f08080;
    font-family: var(--font-body);
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .btn-danger:hover {
    background: color-mix(in srgb, var(--color-bb-red) 25%, transparent);
    border-color: var(--color-bb-red);
  }

  .btn-danger-outline {
    color: var(--color-bb-text-muted);
    border-color: color-mix(in srgb, var(--color-bb-gold) 15%, transparent);
  }
  .btn-danger-outline:hover:not(:disabled) {
    color: #f08080;
    border-color: color-mix(in srgb, var(--color-bb-red) 40%, transparent);
    background: color-mix(in srgb, var(--color-bb-red) 6%, transparent);
  }

  /* ── Candidates ── */
  .candidates-section {
    margin-bottom: 24px;
  }
  .candidates-title {
    font-family: var(--font-display);
    font-size: 18px;
    letter-spacing: 0.06em;
    color: var(--color-bb-cream);
    margin-bottom: 12px;
  }
  .candidates-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .candidate-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
    border-radius: 6px;
    transition: border-color 0.2s ease;
  }
  .candidate-row:hover {
    border-color: color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
  }
  .candidate-info {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1;
  }
  .candidate-icon {
    font-size: 18px;
    flex-shrink: 0;
  }
  .candidate-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .candidate-name {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-bb-cream);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .candidate-original {
    font-size: 12px;
    color: var(--color-bb-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .candidate-type {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-bb-gold-dim);
    flex-shrink: 0;
  }
  .candidate-tmdb {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-bb-text-muted);
    flex-shrink: 0;
  }

  /* ── Cards grid ── */
  .cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
  }

  /* ── Subscription card ── */
  .sub-card {
    position: relative;
    display: block;
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
    border-radius: 6px;
    overflow: hidden;
    padding: 0;
    font-family: var(--font-body);
    color: inherit;
    transition: all 0.3s ease;
    animation: card-enter 0.5s ease both;
  }
  .sub-card:hover {
    border-color: color-mix(in srgb, var(--color-bb-gold) 35%, transparent);
    transform: translateY(-3px);
    box-shadow:
      0 8px 32px rgba(0, 0, 0, 0.4),
      0 0 0 1px color-mix(in srgb, var(--color-bb-gold) 8%, transparent),
      inset 0 1px 0 color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
  }

  @keyframes card-enter {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .card-perfs {
    display: flex;
    justify-content: space-evenly;
    padding: 0 8px;
    margin-top: 0;
  }
  .card-perf {
    width: 6px;
    height: 3px;
    background: color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
    border-radius: 0 0 1.5px 1.5px;
  }

  .card-body {
    position: relative;
    padding: 14px 18px 16px;
  }

  .card-type-badge {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--color-bb-gold-dim);
  }

  .card-title {
    font-family: var(--font-display);
    font-size: 22px;
    letter-spacing: 0.04em;
    color: var(--color-bb-cream);
    margin-top: 6px;
    line-height: 1.15;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-titles {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 8px;
  }
  .card-lang-title {
    font-size: 12px;
    color: var(--color-bb-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-lang-en {
    font-style: italic;
  }

  .card-meta-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 10px;
  }
  .card-tmdb {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-bb-text-muted);
    padding: 2px 8px;
    background: color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
    border-radius: 3px;
  }

  .card-time {
    display: block;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid color-mix(in srgb, var(--color-bb-gold) 6%, transparent);
    font-size: 11px;
    color: var(--color-bb-text-muted);
  }

  .card-actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }

  /* ── Loading ── */
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

  /* ── Empty state ── */
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

  /* ── Error banner ── */
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

  /* ── Drawer ── */
  .drawer-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 90;
    animation: backdrop-in 0.3s ease;
  }
  @keyframes backdrop-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    width: 480px;
    max-width: 100vw;
    height: 100vh;
    background: var(--color-bb-night);
    border-left: 1px solid color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
    z-index: 100;
    display: flex;
    flex-direction: column;
    animation: drawer-in 0.35s cubic-bezier(0.16, 1, 0.3, 1);
    box-shadow: -8px 0 40px rgba(0, 0, 0, 0.5);
  }
  @keyframes drawer-in {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }

  .drawer-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 24px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
    flex-shrink: 0;
  }

  .drawer-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
    border-radius: 4px;
    background: transparent;
    color: var(--color-bb-text-muted);
    cursor: pointer;
    transition: all 0.2s ease;
    flex-shrink: 0;
  }
  .drawer-close:hover {
    color: var(--color-bb-gold-light);
    border-color: color-mix(in srgb, var(--color-bb-gold) 40%, transparent);
    background: color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
  }

  .drawer-body {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
  }

  .detail-eyebrow {
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.25em;
    color: var(--color-bb-gold-dim);
  }
  .detail-title {
    font-family: var(--font-display);
    font-size: 24px;
    letter-spacing: 0.06em;
    color: var(--color-bb-cream);
    margin-top: 4px;
  }

  /* ── Rescan results ── */
  .rescan-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .rescan-item {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--color-bb-deep);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
    border-radius: 4px;
  }
  .rescan-status {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 2px 8px;
    border-radius: 3px;
  }
  .rescan-succeeded .rescan-status {
    background: color-mix(in srgb, var(--color-bb-green) 15%, transparent);
    color: var(--color-bb-green);
  }
  .rescan-failed .rescan-status {
    background: color-mix(in srgb, var(--color-bb-red) 15%, transparent);
    color: var(--color-bb-red);
  }
  .rescan-skipped .rescan-status {
    background: var(--color-bb-muted);
    color: var(--color-bb-text-muted);
  }
  .rescan-title {
    font-size: 13px;
    color: var(--color-bb-text);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rescan-error {
    font-size: 12px;
    color: #f08080;
    width: 100%;
  }

  /* ── Responsive ── */
  @media (max-width: 768px) {
    .cards-grid {
      grid-template-columns: 1fr;
    }
    .search-bar {
      flex-direction: column;
      align-items: stretch;
    }
    .search-actions {
      margin-left: 0;
    }
    .drawer {
      width: 100vw;
    }
    .candidate-row {
      flex-direction: column;
      align-items: flex-start;
      gap: 8px;
    }
  }
  @media (min-width: 1200px) {
    .cards-grid {
      grid-template-columns: repeat(3, 1fr);
    }
  }
</style>
