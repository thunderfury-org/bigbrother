<script lang="ts">
  import {
    ApiError,
    getImport,
    listImports,
    type ImportDetail,
    type ImportListItem,
    type ListImportsFilter,
  } from '../lib/api';

  type FilterForm = {
    status: string;
    source_kind: string;
    since: string;
    until: string;
  };

  let form: FilterForm = $state({ status: '', source_kind: '', since: '', until: '' });
  let appliedFilter: ListImportsFilter = $state({});
  let cursorStack: number[] = $state([]);
  let items: ImportListItem[] = $state([]);
  let nextCursor: number | null = $state(null);
  let loading = $state(false);
  let errorMessage = $state('');
  let detail: ImportDetail | null = $state(null);
  let detailLoading = $state(false);

  function toIso(local: string): string | undefined {
    if (!local) return undefined;
    const d = new Date(local);
    if (Number.isNaN(d.getTime())) return undefined;
    return d.toISOString();
  }

  function filterFromForm(values: FilterForm): ListImportsFilter {
    return {
      status: values.status || undefined,
      source_kind: values.source_kind || undefined,
      since: toIso(values.since),
      until: toIso(values.until),
    };
  }

  async function load(filter: ListImportsFilter, cursor: number | undefined) {
    loading = true;
    errorMessage = '';
    try {
      const page = await listImports({ ...filter, cursor });
      items = page.items;
      nextCursor = page.next_cursor;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      items = [];
      nextCursor = null;
    } finally {
      loading = false;
    }
  }

  function apply() {
    appliedFilter = filterFromForm(form);
    cursorStack = [];
    load(appliedFilter, undefined);
  }

  function reset() {
    form = { status: '', source_kind: '', since: '', until: '' };
    appliedFilter = {};
    cursorStack = [];
    load({}, undefined);
  }

  function next() {
    if (nextCursor == null) return;
    const newStack = [...cursorStack, nextCursor];
    cursorStack = newStack;
    load(appliedFilter, nextCursor);
  }

  function prev() {
    if (cursorStack.length === 0) return;
    const newStack = cursorStack.slice(0, -1);
    cursorStack = newStack;
    const back = newStack.length > 0 ? newStack[newStack.length - 1] : undefined;
    load(appliedFilter, back);
  }

  async function openDetail(id: number) {
    detailLoading = true;
    try {
      detail = await getImport(id);
    } catch (err) {
      errorMessage = err instanceof ApiError ? `加载详情失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      detailLoading = false;
    }
  }

  function closeDetail() {
    detail = null;
  }

  function formatTimestamp(value: string | null | undefined): string {
    if (!value) return '—';
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? value : d.toLocaleString();
  }

  function formatCost(ms: number): string {
    if (!ms) return '—';
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function formatSeason(item: ImportListItem): string {
    if (item.season != null) {
      const padded = String(item.season).padStart(2, '0');
      return `S${padded}${item.episode_summary ? ' ' + item.episode_summary : ''}`;
    }
    return item.episode_summary ?? '';
  }

  function formatTitle(item: ImportListItem): string {
    if (!item.title) return '—';
    return item.year ? `${item.title} (${item.year})` : item.title;
  }

  function formatSize(bytes: number): string {
    if (!bytes) return '—';
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    let value = Number(bytes);
    let unitIndex = 0;
    while (value >= 1024 && unitIndex < units.length - 1) {
      value /= 1024;
      unitIndex += 1;
    }
    return `${unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'running': return '处理中';
      case 'succeeded': return '成功';
      case 'partially_failed': return '部分失败';
      case 'failed': return '失败';
      case 'skipped': return '跳过';
      default: return status;
    }
  }

  function sourceLabel(kind: string): string {
    switch (kind) {
      case 'pan123': return '123云盘';
      case 'pan189': return '天翼云盘';
      case 'pan115': return '115网盘';
      case 'quark': return '夸克网盘';
      case 'telegram': return 'Telegram';
      default: return kind;
    }
  }

  function sourceIcon(kind: string): string {
    switch (kind) {
      case 'pan123': return '☁';
      case 'pan189': return '📡';
      case 'pan115': return '💾';
      case 'quark': return '🌀';
      case 'telegram': return '✈';
      default: return '📁';
    }
  }

  load({}, undefined);
</script>

<section>
  <!-- Page header -->
  <header class="page-header">
    <div>
      <h1 class="page-title">导入历史</h1>
      <p class="page-subtitle">IMPORT HISTORY</p>
    </div>
    {#if items.length > 0}
      <span class="page-count">{items.length} 条记录</span>
    {/if}
  </header>

  <!-- Filter bar -->
  <form
    class="filter-bar"
    onsubmit={(e) => { e.preventDefault(); apply(); }}
  >
    <label class="filter-field">
      <span class="filter-label">状态</span>
      <select bind:value={form.status} class="filter-select">
        <option value="">全部</option>
        <option value="running">处理中</option>
        <option value="succeeded">成功</option>
        <option value="partially_failed">部分失败</option>
        <option value="failed">失败</option>
        <option value="skipped">跳过</option>
      </select>
    </label>
    <label class="filter-field">
      <span class="filter-label">来源</span>
      <select bind:value={form.source_kind} class="filter-select">
        <option value="">全部</option>
        <option value="pan123">123云盘</option>
        <option value="pan189">天翼云盘</option>
        <option value="pan115">115网盘</option>
        <option value="quark">夸克网盘</option>
        <option value="telegram">Telegram</option>
        <option value="other">其他</option>
      </select>
    </label>
    <label class="filter-field">
      <span class="filter-label">从</span>
      <input type="datetime-local" bind:value={form.since} class="filter-input" />
    </label>
    <label class="filter-field">
      <span class="filter-label">到</span>
      <input type="datetime-local" bind:value={form.until} class="filter-input" />
    </label>
    <div class="filter-actions">
      <button type="submit" class="btn-gold">筛选</button>
      <button type="button" onclick={reset} class="btn-ghost">重置</button>
    </div>
  </form>

  <!-- Error message -->
  {#if errorMessage}
    <div class="error-banner">
      <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
        <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
      </svg>
      <span>{errorMessage}</span>
    </div>
  {/if}

  <!-- Cards grid -->
  {#if loading}
    <div class="loading-state">
      <div class="loading-bar"></div>
      <p>正在加载…</p>
    </div>
  {:else if items.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
          <path d="M3 3h18v18H3zM8 12h8M12 8v8" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="empty-text">没有匹配的导入记录</p>
    </div>
  {:else}
    <div class="cards-grid">
      {#each items as item, i (item.id)}
        <button
          type="button"
          class="import-card"
          style="animation-delay: {Math.min(i * 60, 600)}ms"
          onclick={() => openDetail(item.id)}
        >
          <!-- Film strip perforations on card -->
          <div class="card-perfs" aria-hidden="true">
            {#each { length: 6 } as _}
              <span class="card-perf"></span>
            {/each}
          </div>

          <div class="card-body">
            <!-- Status indicator -->
            <span class="status-dot status-{item.status}" aria-hidden="true"></span>

            <span class="card-source">{sourceIcon(item.source_kind)} {sourceLabel(item.source_kind)}</span>
            <h3 class="card-title" title={formatTitle(item)}>{formatTitle(item)}</h3>

            {#if formatSeason(item)}
              <span class="card-season">{formatSeason(item)}</span>
            {/if}

            <div class="card-meta">
              <span class="badge badge-{item.status}">{statusLabel(item.status)}</span>
            </div>

            <div class="card-details">
              {#if item.cost_ms}
                <span class="detail-item">
                  <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z" clip-rule="evenodd"/></svg>
                  {formatCost(item.cost_ms)}
                </span>
              {/if}
              {#if item.total_size}
                <span class="detail-item">
                  <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path d="M4 4a2 2 0 00-2 2v1h16V6a2 2 0 00-2-2H4z"/><path fill-rule="evenodd" d="M18 9H2v5a2 2 0 002 2h12a2 2 0 002-2V9zM4 13a1 1 0 011-1h1a1 1 0 110 2H5a1 1 0 01-1-1zm5-1a1 1 0 100 2h1a1 1 0 100-2H9z" clip-rule="evenodd"/></svg>
                  {formatSize(item.total_size)}
                </span>
              {/if}
            </div>

            <time class="card-time">{formatTimestamp(item.created_at)}</time>
          </div>
        </button>
      {/each}
    </div>

    <!-- Pagination -->
    <div class="pagination">
      <button type="button" onclick={prev} disabled={cursorStack.length === 0 || loading} class="btn-ghost">
        <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path fill-rule="evenodd" d="M12.707 5.293a1 1 0 010 1.414L9.414 10l3.293 3.293a1 1 0 01-1.414 1.414l-4-4a1 1 0 010-1.414l4-4a1 1 0 011.414 0z" clip-rule="evenodd"/></svg>
        上一页
      </button>
      <button type="button" onclick={next} disabled={nextCursor == null || loading} class="btn-ghost">
        下一页
        <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/></svg>
      </button>
    </div>
  {/if}

  <!-- Detail drawer -->
  {#if detail || detailLoading}
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="drawer-backdrop" onclick={closeDetail} role="presentation"></div>
    <aside class="drawer">
      <header class="drawer-header">
        <div>
          <p class="detail-eyebrow">IMPORT DOSSIER</p>
          <h2 class="detail-title">
            导入详情
            {#if detail}<span class="detail-id">#{detail.id}</span>{/if}
          </h2>
        </div>
        <button type="button" onclick={closeDetail} class="drawer-close" aria-label="关闭">
          <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
            <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/>
          </svg>
        </button>
      </header>

      <div class="drawer-body">
        {#if detailLoading}
          <div class="loading-state">
            <div class="loading-bar"></div>
            <p>加载中…</p>
          </div>
        {:else if detail}
          <div class="detail-grid">
            <div class="detail-field">
              <span class="detail-label">来源</span>
              <span class="detail-value">{sourceLabel(detail.source_kind)}</span>
            </div>
            <div class="detail-field">
              <span class="detail-label">状态</span>
              <span class="badge badge-{detail.status}">{statusLabel(detail.status)}</span>
            </div>
            <div class="detail-field detail-field--full">
              <span class="detail-label">链接</span>
              {#if detail.source.startsWith('http')}
                <a href={detail.source} target="_blank" rel="noopener" class="detail-link">{detail.source}</a>
              {:else}
                <span class="detail-value">{detail.source}</span>
              {/if}
            </div>
            <div class="detail-field">
              <span class="detail-label">创建时间</span>
              <span class="detail-value">{formatTimestamp(detail.created_at)}</span>
            </div>
            {#if detail.finished_at}
              <div class="detail-field">
                <span class="detail-label">完成时间</span>
                <span class="detail-value">{formatTimestamp(detail.finished_at)}</span>
              </div>
            {/if}
            {#if detail.error}
              <div class="detail-field detail-field--full">
                <span class="detail-label">错误信息</span>
                <div class="detail-error">
                  <span class="detail-error-kind">{detail.error.kind}</span>
                  <p>{detail.error.message}</p>
                </div>
              </div>
            {/if}
            {#if detail.summary}
              <div class="detail-field detail-field--full">
                <span class="detail-label">导入摘要</span>
                <pre class="detail-json">{JSON.stringify(detail.summary, null, 2)}</pre>
              </div>
            {/if}
          </div>
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

  /* ── Filter bar ── */
  .filter-bar {
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
  .filter-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .filter-label {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-bb-text-muted);
  }
  .filter-select,
  .filter-input {
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
  .filter-select:focus,
  .filter-input:focus {
    border-color: var(--color-bb-gold);
  }
  .filter-actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
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
  .btn-gold:hover {
    background: linear-gradient(135deg, var(--color-bb-gold), var(--color-bb-gold-light));
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
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

  /* ── Cards grid ── */
  .cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
  }

  /* ── Import card ── */
  .import-card {
    position: relative;
    display: block;
    text-align: left;
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
    border-radius: 6px;
    overflow: hidden;
    cursor: pointer;
    padding: 0;
    font-family: var(--font-body);
    color: inherit;
    transition: all 0.3s ease;
    animation: card-enter 0.5s ease both;
  }
  .import-card:hover {
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

  /* Film strip perforations on card */
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

  .status-dot {
    position: absolute;
    top: 14px;
    right: 16px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .status-dot.status-succeeded { background: var(--color-bb-green); box-shadow: 0 0 8px color-mix(in srgb, var(--color-bb-green) 40%, transparent); }
  .status-dot.status-running { background: var(--color-bb-blue); animation: pulse 2s infinite; }
  .status-dot.status-failed { background: var(--color-bb-red); box-shadow: 0 0 8px color-mix(in srgb, var(--color-bb-red) 40%, transparent); }
  .status-dot.status-partially_failed { background: var(--color-bb-amber); box-shadow: 0 0 8px color-mix(in srgb, var(--color-bb-amber) 40%, transparent); }
  .status-dot.status-skipped { background: var(--color-bb-text-muted); }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .card-source {
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
    padding-right: 20px;
  }

  .card-season {
    display: inline-block;
    margin-top: 6px;
    padding: 2px 8px;
    background: color-mix(in srgb, var(--color-bb-gold) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 15%, transparent);
    border-radius: 3px;
    font-size: 12px;
    font-family: var(--font-mono);
    font-weight: 500;
    color: var(--color-bb-gold-light);
  }

  .card-meta {
    margin-top: 10px;
  }

  .badge {
    display: inline-block;
    padding: 2px 10px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .badge-succeeded { background: color-mix(in srgb, var(--color-bb-green) 15%, transparent); color: var(--color-bb-green); }
  .badge-running { background: color-mix(in srgb, var(--color-bb-blue) 15%, transparent); color: var(--color-bb-blue); }
  .badge-partially_failed { background: color-mix(in srgb, var(--color-bb-amber) 15%, transparent); color: var(--color-bb-amber); }
  .badge-failed { background: color-mix(in srgb, var(--color-bb-red) 15%, transparent); color: var(--color-bb-red); }
  .badge-skipped { background: var(--color-bb-muted); color: var(--color-bb-text-muted); }

  .card-details {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 10px;
  }
  .detail-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--color-bb-text-muted);
  }
  .detail-item svg {
    opacity: 0.6;
  }

  .card-time {
    display: block;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid color-mix(in srgb, var(--color-bb-gold) 6%, transparent);
    font-size: 11px;
    color: var(--color-bb-text-muted);
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

  /* ── Pagination ── */
  .pagination {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
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
  .detail-id {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--color-bb-gold);
    margin-left: 8px;
  }

  .detail-grid {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .detail-field--full {
    width: 100%;
  }
  .detail-label {
    display: block;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--color-bb-text-muted);
    margin-bottom: 4px;
  }
  .detail-value {
    font-size: 14px;
    color: var(--color-bb-text);
    word-break: break-all;
  }
  .detail-link {
    font-size: 13px;
    font-family: var(--font-mono);
    color: var(--color-bb-gold);
    text-decoration: none;
    word-break: break-all;
    transition: color 0.2s;
  }
  .detail-link:hover {
    color: var(--color-bb-gold-light);
  }
  .detail-error {
    padding: 12px 16px;
    background: color-mix(in srgb, var(--color-bb-red) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bb-red) 20%, transparent);
    border-radius: 4px;
  }
  .detail-error-kind {
    display: inline-block;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #f08080;
    margin-bottom: 4px;
  }
  .detail-error p {
    font-size: 13px;
    color: var(--color-bb-text);
    line-height: 1.5;
  }
  .detail-json {
    padding: 16px;
    background: var(--color-bb-card);
    border: 1px solid color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.6;
    color: var(--color-bb-text);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* ── Responsive ── */
  @media (max-width: 768px) {
    .cards-grid {
      grid-template-columns: 1fr;
    }
    .filter-bar {
      flex-direction: column;
      align-items: stretch;
    }
    .filter-actions {
      margin-left: 0;
    }
    .drawer {
      width: 100vw;
    }
  }
  @media (min-width: 1200px) {
    .cards-grid {
      grid-template-columns: repeat(3, 1fr);
    }
  }
</style>
