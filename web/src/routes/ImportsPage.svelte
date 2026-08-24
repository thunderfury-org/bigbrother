<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import {
    ApiError,
    getImport,
    listImports,
    type ImportDetail,
    type ImportListItem,
    type ListImportsFilter,
  } from '../lib/api';
  import ImportSummaryItems from '../lib/ImportSummaryItems.svelte';
  import {
    formatCost,
    formatErrorLine,
    formatListTitle,
    formatSeasonCell,
    formatSize,
    statusLabel,
  } from '../lib/importDisplay';

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

  function onRowKey(event: KeyboardEvent, id: number) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      openDetail(id);
    }
  }

  function formatTimestamp(value: string | null | undefined): string {
    if (!value) return '—';
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? value : d.toLocaleString();
  }

  function sourceLabel(kind: string): string {
    switch (kind) {
      case 'pan123': return '123云盘';
      case 'pan189': return '天翼云盘';
      case 'pan115': return '115网盘';
      case 'quark': return '夸克网盘';
      case 'telegram': return 'Telegram';
      case 'file_index': return '文件索引';
      default: return kind;
    }
  }

  load({}, undefined);
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">导入历史</h1>
    {#if items.length > 0}
      <span class="page-count">{items.length} 条记录</span>
    {/if}
  </header>

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); apply(); }}>
    <label class="field">
      <span class="field-label">状态</span>
      <select bind:value={form.status} class="select">
        <option value="">全部</option>
        <option value="running">处理中</option>
        <option value="succeeded">成功</option>
        <option value="partially_failed">部分失败</option>
        <option value="failed">失败</option>
        <option value="skipped">跳过</option>
      </select>
    </label>
    <label class="field">
      <span class="field-label">来源</span>
      <select bind:value={form.source_kind} class="select">
        <option value="">全部</option>
        <option value="pan123">123云盘</option>
        <option value="pan189">天翼云盘</option>
        <option value="pan115">115网盘</option>
        <option value="quark">夸克网盘</option>
        <option value="telegram">Telegram</option>
        <option value="file_index">文件索引</option>
        <option value="other">其他</option>
      </select>
    </label>
    <label class="field">
      <span class="field-label">从</span>
      <input type="datetime-local" bind:value={form.since} class="input" />
    </label>
    <label class="field">
      <span class="field-label">到</span>
      <input type="datetime-local" bind:value={form.until} class="input" />
    </label>
    <button type="submit" class="btn btn-primary">筛选</button>
    <button type="button" onclick={reset} class="btn btn-ghost">重置</button>
  </form>

  {#if errorMessage}
    <div class="banner banner-error">{errorMessage}</div>
  {/if}

  {#if loading}
    <div class="loading">
      <div class="loading-bar"></div>
      <p>正在加载…</p>
    </div>
  {:else if items.length === 0}
    <div class="empty">没有匹配的导入记录</div>
  {:else}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>标题</th>
            <th>分集</th>
            <th>来源</th>
            <th>大小</th>
            <th>耗时</th>
            <th>时间</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr
              class="is-clickable"
              data-status={item.status}
              tabindex="0"
              onclick={() => openDetail(item.id)}
              onkeydown={(event) => onRowKey(event, item.id)}
            >
              <td><span class="status status-{item.status}">{statusLabel(item.status)}</span></td>
              <td>
                <div class="cell-stack">
                  <span class="cell-title" title={formatListTitle(item)}>{formatListTitle(item)}</span>
                  {#if item.error}
                    <span class="cell-sub" title={formatErrorLine(item.error)}>{formatErrorLine(item.error)}</span>
                  {/if}
                </div>
              </td>
              <td class="mono">{formatSeasonCell(item) || '—'}</td>
              <td>{sourceLabel(item.source_kind)}</td>
              <td class="mono">{formatSize(item.total_size)}</td>
              <td class="mono">{formatCost(item.cost_ms)}</td>
              <td>{formatTimestamp(item.created_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="pager">
      <button type="button" onclick={prev} disabled={cursorStack.length === 0 || loading} class="btn btn-ghost">
        上一页
      </button>
      <button type="button" onclick={next} disabled={nextCursor == null || loading} class="btn btn-ghost">
        下一页
      </button>
    </div>
  {/if}

  {#if detail || detailLoading}
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
    <div class="drawer-backdrop" onclick={closeDetail} role="presentation"></div>
    <aside class="drawer">
      <header class="drawer-header">
        <h2 class="drawer-title">
          导入详情
          {#if detail}<span class="drawer-id">#{detail.id}</span>{/if}
        </h2>
        <button type="button" onclick={closeDetail} class="drawer-close" aria-label="关闭">
          <X size={16} />
        </button>
      </header>

      <div class="drawer-body">
        {#if detailLoading}
          <div class="loading">
            <div class="loading-bar"></div>
            <p>加载中…</p>
          </div>
        {:else if detail}
          <div class="detail-grid">
            <div>
              <span class="detail-label">来源</span>
              <span class="detail-value">{sourceLabel(detail.source_kind)}</span>
            </div>
            <div>
              <span class="detail-label">状态</span>
              <span class="status status-{detail.status}">{statusLabel(detail.status)}</span>
            </div>
            <div>
              <span class="detail-label">链接</span>
              {#if detail.source.startsWith('http')}
                <a href={detail.source} target="_blank" rel="noopener" class="detail-link">{detail.source}</a>
              {:else}
                <span class="detail-value">{detail.source}</span>
              {/if}
            </div>
            <div>
              <span class="detail-label">创建时间</span>
              <span class="detail-value">{formatTimestamp(detail.created_at)}</span>
            </div>
            {#if detail.finished_at}
              <div>
                <span class="detail-label">完成时间</span>
                <span class="detail-value">{formatTimestamp(detail.finished_at)}</span>
              </div>
            {/if}
            {#if detail.error}
              <div>
                <span class="detail-label">错误信息</span>
                <div class="detail-error">
                  <span class="detail-error-kind">{detail.error.kind}</span>
                  {#if detail.error.message}
                    <p>{detail.error.message}</p>
                  {/if}
                </div>
              </div>
            {/if}
            {#if detail.summary}
              <ImportSummaryItems summary={detail.summary} />
            {/if}
          </div>
        {/if}
      </div>
    </aside>
  {/if}
</section>
