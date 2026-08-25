<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import Filter from '@lucide/svelte/icons/filter';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Copy from '@lucide/svelte/icons/copy';
  import Check from '@lucide/svelte/icons/check';
  import {
    ApiError,
    getImport,
    listImports,
    type ImportDetail,
    type ImportListItem,
    type ListImportsFilter,
  } from '../lib/api';
  import ImportSummaryItems from '../lib/ImportSummaryItems.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import { toasts } from '../lib/toast.svelte';
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
  let detail: ImportDetail | null = $state(null);
  let detailLoading = $state(false);
  let copiedLink = $state(false);

  const stats = $derived.by(() => {
    const total = items.length;
    const succeeded = items.filter((i) => i.status === 'succeeded').length;
    const running = items.filter((i) => i.status === 'running').length;
    const failed = items.filter((i) => i.status === 'failed' || i.status === 'partially_failed').length;
    const sizeBytes = items.reduce((acc, i) => acc + (i.total_size || 0), 0);
    const successRate = total > 0 ? ((succeeded / total) * 100).toFixed(1) : '—';
    return {
      total,
      succeeded,
      running,
      failed,
      sizeStr: formatSize(sizeBytes),
      successRate,
    };
  });

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
    try {
      const page = await listImports({ ...filter, cursor });
      items = page.items;
      nextCursor = page.next_cursor;
    } catch (err) {
      const msg = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      toasts.error(msg);
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
      const msg = err instanceof ApiError ? `加载详情失败 ${err.status}: ${err.body}` : String(err);
      toasts.error(msg);
    } finally {
      detailLoading = false;
    }
  }

  function closeDetail() {
    detail = null;
  }

  function copyText(text: string) {
    void navigator.clipboard.writeText(text).then(() => {
      copiedLink = true;
      setTimeout(() => { copiedLink = false; }, 2000);
      toasts.success('已复制到剪贴板');
    });
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

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && detail) {
        closeDetail();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  load({}, undefined);
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">
      导入历史
      {#if items.length > 0}
        <span class="page-count">{items.length} 条记录</span>
      {/if}
    </h1>
  </header>

  <!-- Metrics Cards -->
  {#if items.length > 0 && !loading}
    <div class="stats-row">
      <div class="stat-card">
        <span class="stat-label">当前页记录</span>
        <div class="stat-val">{stats.total}</div>
      </div>
      <div class="stat-card">
        <span class="stat-label">成功率</span>
        <div class="stat-val" style="color: #34d399;">
          {stats.successRate === '—' ? '—' : stats.successRate + '%'}
          <span class="stat-sub">{stats.succeeded} 成功</span>
        </div>
      </div>
      <div class="stat-card">
        <span class="stat-label">处理中 / 异常</span>
        <div class="stat-val" style="color: {stats.running > 0 ? '#fbbf24' : stats.failed > 0 ? '#fb7185' : '#ffffff'};">
          {stats.running} / {stats.failed}
          <span class="stat-sub">{stats.running > 0 ? '进行中' : '全部结束'}</span>
        </div>
      </div>
      <div class="stat-card">
        <span class="stat-label">总入库体积</span>
        <div class="stat-val">{stats.sizeStr}</div>
      </div>
    </div>
  {/if}

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); apply(); }}>
    <label class="field">
      <span class="field-label">状态</span>
      <select bind:value={form.status} class="select">
        <option value="">全部状态</option>
        <option value="running">处理中</option>
        <option value="succeeded">成功</option>
        <option value="partially_failed">部分失败</option>
        <option value="failed">失败</option>
        <option value="skipped">跳过</option>
      </select>
    </label>
    <label class="field">
      <span class="field-label">来源渠道</span>
      <select bind:value={form.source_kind} class="select">
        <option value="">全部来源</option>
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
      <span class="field-label">起始时间</span>
      <input type="datetime-local" bind:value={form.since} class="input" />
    </label>
    <label class="field">
      <span class="field-label">截止时间</span>
      <input type="datetime-local" bind:value={form.until} class="input" />
    </label>
    <button type="submit" class="btn btn-primary">
      <Filter size={14} />
      <span>筛选</span>
    </button>
    <button type="button" onclick={reset} class="btn btn-ghost">
      <RotateCcw size={14} />
      <span>重置</span>
    </button>
  </form>

  {#if loading}
    <div class="data-table-wrap" style="padding: 16px;">
      {#each Array(6) as _, i (i)}
        <div style="display: flex; gap: 16px; padding: 12px 0; border-bottom: 1px solid var(--color-bb-line);">
          <Skeleton width="80px" height="24px" />
          <Skeleton width="260px" height="24px" />
          <Skeleton width="100px" height="24px" />
          <Skeleton width="80px" height="24px" />
          <Skeleton width="90px" height="24px" />
        </div>
      {/each}
    </div>
  {:else if items.length === 0}
    <div class="empty">没有匹配的导入记录</div>
  {:else}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>标题与详情</th>
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
              tabindex="0"
              onclick={() => openDetail(item.id)}
              onkeydown={(event) => onRowKey(event, item.id)}
            >
              <td>
                <span class="status status-{item.status}">
                  <span class="pulse-dot"></span>
                  {statusLabel(item.status)}
                </span>
              </td>
              <td>
                <div class="cell-stack">
                  <span class="cell-title" title={formatListTitle(item)}>{formatListTitle(item)}</span>
                  {#if item.error}
                    <span class="cell-sub text-rose-400" title={formatErrorLine(item.error)}>{formatErrorLine(item.error)}</span>
                  {/if}
                </div>
              </td>
              <td class="mono">{formatSeasonCell(item) || '—'}</td>
              <td>{sourceLabel(item.source_kind)}</td>
              <td class="mono">{formatSize(item.total_size)}</td>
              <td class="mono">{formatCost(item.cost_ms)}</td>
              <td class="text-slate-400">{formatTimestamp(item.created_at)}</td>
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
            <p>正在读取导入详情…</p>
          </div>
        {:else if detail}
          <div class="detail-grid">
            <div>
              <span class="detail-label">来源渠道</span>
              <span class="detail-value">{sourceLabel(detail.source_kind)}</span>
            </div>
            <div>
              <span class="detail-label">任务状态</span>
              <span class="status status-{detail.status}">
                <span class="pulse-dot"></span>
                {statusLabel(detail.status)}
              </span>
            </div>
            <div>
              <div style="display: flex; align-items: center; justify-content: space-between;">
                <span class="detail-label">来源链接 / 标识</span>
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  style="padding: 2px 8px; font-size: 11px;"
                  onclick={() => copyText(detail?.source ?? '')}
                >
                  {#if copiedLink}
                    <Check size={12} class="text-emerald-400" />
                    <span>已复制</span>
                  {:else}
                    <Copy size={12} />
                    <span>复制</span>
                  {/if}
                </button>
              </div>
              {#if detail.source.startsWith('http')}
                <a href={detail.source} target="_blank" rel="noopener" class="detail-link">
                  {detail.source} <ExternalLink size={12} class="inline" />
                </a>
              {:else}
                <span class="detail-value mono">{detail.source}</span>
              {/if}
            </div>
            <div>
              <span class="detail-label">创建时间</span>
              <span class="detail-value text-slate-300">{formatTimestamp(detail.created_at)}</span>
            </div>
            {#if detail.finished_at}
              <div>
                <span class="detail-label">完成时间</span>
                <span class="detail-value text-slate-300">{formatTimestamp(detail.finished_at)}</span>
              </div>
            {/if}
            {#if detail.error}
              <div>
                <span class="detail-label">异常与错误</span>
                <div class="detail-error">
                  <span class="detail-error-kind">{detail.error.kind}</span>
                  {#if detail.error.message}
                    <p style="color: #cbd5e1; font-size: 13px; margin-top: 4px;">{detail.error.message}</p>
                  {/if}
                </div>
              </div>
            {/if}
            {#if detail.summary}
              <div>
                <span class="detail-label">入库结构明细</span>
                <ImportSummaryItems summary={detail.summary} />
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </aside>
  {/if}
</section>
