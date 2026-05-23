<script lang="ts">
  import { ApiError, searchFiles, type FileSearchItem } from '../lib/api';

  let keyword = $state('');
  let limit = $state(50);
  let lastQuery = $state('');
  let items: FileSearchItem[] = $state([]);
  let loading = $state(false);
  let errorMessage = $state('');
  let hasSearched = $state(false);

  async function run() {
    const q = keyword.trim();
    lastQuery = q;
    hasSearched = true;
    loading = true;
    errorMessage = '';
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
</script>

<section>
  <header class="mb-4 flex items-baseline justify-between">
    <h1 class="text-lg font-semibold text-slate-900">文件索引</h1>
    {#if hasSearched && !loading && !errorMessage && lastQuery}
      <span class="text-xs text-slate-500">共 {items.length} 条</span>
    {/if}
  </header>

  <form
    class="mb-4 flex flex-wrap items-end gap-3 rounded border border-slate-200 bg-white p-3"
    onsubmit={(e) => {
      e.preventDefault();
      run();
    }}
  >
    <label class="flex flex-1 min-w-[240px] flex-col text-xs text-slate-600">
      关键字
      <input
        type="text"
        bind:value={keyword}
        placeholder="文件名、路径或描述片段"
        class="mt-1 rounded border border-slate-300 px-2 py-1 text-sm"
      />
    </label>
    <label class="flex flex-col text-xs text-slate-600">
      条数
      <select bind:value={limit} class="mt-1 rounded border border-slate-300 px-2 py-1 text-sm">
        <option value={20}>20</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={200}>200</option>
      </select>
    </label>
    <button
      type="submit"
      class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
    >搜索</button>
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
          <th class="px-3 py-2">大小</th>
          <th class="px-3 py-2">哈希</th>
          <th class="px-3 py-2">位置</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-slate-100">
        {#each items as item}
          <tr>
            <td class="whitespace-nowrap px-3 py-2 text-slate-700">{formatSize(item.size)}</td>
            <td class="px-3 py-2 font-mono text-xs text-slate-700" title="{item.hash_type}:{item.hash_value}">
              <span class="mr-1 rounded bg-slate-100 px-1.5 py-0.5 text-[10px] uppercase text-slate-600">
                {item.hash_type}
              </span>
              <span class="break-all">{item.hash_value}</span>
            </td>
            <td class="px-3 py-2">
              <details open>
                <summary class="cursor-pointer text-xs text-slate-600">{item.locations.length} 个位置</summary>
                <ul class="mt-2 space-y-2">
                  {#each item.locations as loc}
                    <li class="rounded bg-slate-50 px-2 py-1.5">
                      <div class="text-sm text-slate-900">{loc.file_name}</div>
                      <div class="font-mono text-xs break-all text-slate-500">{loc.file_path || '/'}</div>
                      {#if loc.descriptions.length}
                        <div class="mt-1 flex flex-wrap gap-1">
                          {#each loc.descriptions as desc}
                            <span class="max-w-xs truncate rounded-full bg-blue-50 px-2 py-0.5 text-xs text-blue-800" title={desc}>{desc}</span>
                          {/each}
                        </div>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </details>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if loading}
      <div class="px-3 py-6 text-center text-sm text-slate-500">加载中…</div>
    {:else if !hasSearched || !lastQuery}
      <div class="px-3 py-6 text-center text-sm text-slate-500">请输入关键字开始搜索</div>
    {:else if items.length === 0}
      <div class="px-3 py-6 text-center text-sm text-slate-500">没有匹配的文件</div>
    {/if}
  </div>
</section>
