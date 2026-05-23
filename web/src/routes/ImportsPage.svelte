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
    return item.episode_summary ?? '—';
  }

  function formatTitle(item: ImportListItem): string {
    if (!item.title) return '—';
    return item.year ? `${item.title} (${item.year})` : item.title;
  }

  function statusClass(status: string): string {
    switch (status) {
      case 'running':
        return 'bg-blue-50 text-blue-700';
      case 'succeeded':
        return 'bg-emerald-50 text-emerald-700';
      case 'partially_failed':
        return 'bg-amber-50 text-amber-800';
      case 'failed':
        return 'bg-rose-50 text-rose-700';
      case 'skipped':
        return 'bg-slate-100 text-slate-600';
      default:
        return 'bg-slate-100 text-slate-700';
    }
  }

  load({}, undefined);
</script>

<section>
  <header class="mb-4 flex items-baseline justify-between">
    <h1 class="text-lg font-semibold text-slate-900">导入历史</h1>
    <span class="text-xs text-slate-500">共 {items.length} 条</span>
  </header>

  <form
    class="mb-4 flex flex-wrap items-end gap-3 rounded border border-slate-200 bg-white p-3"
    onsubmit={(e) => {
      e.preventDefault();
      apply();
    }}
  >
    <label class="flex flex-col text-xs text-slate-600">
      状态
      <select bind:value={form.status} class="mt-1 rounded border border-slate-300 px-2 py-1 text-sm">
        <option value="">全部</option>
        <option value="running">Running</option>
        <option value="succeeded">Succeeded</option>
        <option value="partially_failed">Partially failed</option>
        <option value="failed">Failed</option>
        <option value="skipped">Skipped</option>
      </select>
    </label>
    <label class="flex flex-col text-xs text-slate-600">
      来源
      <select bind:value={form.source_kind} class="mt-1 rounded border border-slate-300 px-2 py-1 text-sm">
        <option value="">全部</option>
        <option value="quark">Quark</option>
        <option value="pan123">Pan123</option>
        <option value="pan189">Pan189</option>
        <option value="pan115">Pan115</option>
        <option value="telegram">Telegram</option>
        <option value="other">Other</option>
      </select>
    </label>
    <label class="flex flex-col text-xs text-slate-600">
      从
      <input
        type="datetime-local"
        bind:value={form.since}
        class="mt-1 rounded border border-slate-300 px-2 py-1 text-sm"
      />
    </label>
    <label class="flex flex-col text-xs text-slate-600">
      到
      <input
        type="datetime-local"
        bind:value={form.until}
        class="mt-1 rounded border border-slate-300 px-2 py-1 text-sm"
      />
    </label>
    <button
      type="submit"
      class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
    >应用</button>
    <button
      type="button"
      onclick={reset}
      class="rounded border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50"
    >重置</button>
  </form>

  {#if errorMessage}
    <div class="mb-3 rounded border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-700">
      {errorMessage}
    </div>
  {/if}

  <div class="overflow-x-auto rounded border border-slate-200 bg-white">
    <table class="w-full text-sm">
      <thead class="bg-slate-50 text-left text-xs uppercase tracking-wider text-slate-500">
        <tr>
          <th class="px-3 py-2">ID</th>
          <th class="px-3 py-2">来源</th>
          <th class="px-3 py-2">链接</th>
          <th class="px-3 py-2">标题</th>
          <th class="px-3 py-2">季集</th>
          <th class="px-3 py-2">状态</th>
          <th class="px-3 py-2">耗时</th>
          <th class="px-3 py-2">开始</th>
          <th class="px-3 py-2">结束</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-slate-100">
        {#each items as item (item.id)}
          <tr
            class="cursor-pointer hover:bg-slate-50"
            onclick={() => openDetail(item.id)}
          >
            <td class="px-3 py-2 text-slate-500">{item.id}</td>
            <td class="px-3 py-2">{item.source_kind}</td>
            <td class="max-w-xs truncate px-3 py-2" title={item.source}>
              {#if item.source.startsWith('http')}
                <a
                  href={item.source}
                  target="_blank"
                  rel="noopener"
                  class="text-blue-600 hover:underline"
                  onclick={(e) => e.stopPropagation()}
                >{item.source}</a>
              {:else}
                {item.source}
              {/if}
            </td>
            <td class="px-3 py-2">{formatTitle(item)}</td>
            <td class="px-3 py-2 text-slate-600">{formatSeason(item)}</td>
            <td class="px-3 py-2">
              <span class="inline-block rounded px-2 py-0.5 text-xs {statusClass(item.status)}">
                {item.status}
              </span>
            </td>
            <td class="px-3 py-2 text-slate-600">{formatCost(item.cost_ms)}</td>
            <td class="px-3 py-2 text-slate-600">{formatTimestamp(item.created_at)}</td>
            <td class="px-3 py-2 text-slate-600">{formatTimestamp(item.finished_at)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if !loading && items.length === 0}
      <div class="px-3 py-6 text-center text-sm text-slate-500">没有匹配的导入记录</div>
    {/if}
    {#if loading}
      <div class="px-3 py-6 text-center text-sm text-slate-500">加载中…</div>
    {/if}
  </div>

  <div class="mt-3 flex justify-end gap-2">
    <button
      type="button"
      onclick={prev}
      disabled={cursorStack.length === 0 || loading}
      class="rounded border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
    >上一页</button>
    <button
      type="button"
      onclick={next}
      disabled={nextCursor == null || loading}
      class="rounded border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
    >下一页</button>
  </div>

  {#if detail || detailLoading}
    <section class="mt-6 rounded border border-slate-200 bg-white p-4">
      <header class="mb-2 flex items-baseline justify-between">
        <h2 class="text-base font-semibold text-slate-900">
          详情 {#if detail}<span class="text-sm text-slate-500">#{detail.id}</span>{/if}
        </h2>
        <button
          type="button"
          onclick={closeDetail}
          class="text-xs text-slate-500 hover:text-slate-700"
        >关闭</button>
      </header>
      {#if detailLoading}
        <p class="text-sm text-slate-500">加载中…</p>
      {:else if detail}
        <pre class="overflow-x-auto rounded bg-slate-50 p-3 text-xs text-slate-800">{JSON.stringify(detail, null, 2)}</pre>
      {/if}
    </section>
  {/if}
</section>
