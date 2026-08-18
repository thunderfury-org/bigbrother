<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import Folder from '@lucide/svelte/icons/folder';
  import Clapperboard from '@lucide/svelte/icons/clapperboard';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import {
    ApiError,
    deleteMediaDirs,
    listMediaDirs,
    searchMediaDirs,
    type MediaDirItem,
  } from '../lib/api';

  type Crumb = { dir_id: number | null; name: string };

  let keyword = $state('');
  let mode: 'browse' | 'search' = $state('browse');
  let crumbs: Crumb[] = $state([{ dir_id: null, name: '库根' }]);
  let items: MediaDirItem[] = $state([]);
  let loading = $state(false);
  let errorMessage = $state('');
  let successMessage = $state('');

  let selected = $state(new Map<number, string>());
  let confirming = $state(false);
  let deleting = $state(false);

  const currentParentId = $derived(crumbs[crumbs.length - 1]?.dir_id ?? null);
  const deletableItems = $derived(items.filter((item) => item.deletable));
  const selectedItems = $derived(
    Array.from(selected.entries()).map(([dir_id, relative_path]) => ({ dir_id, relative_path })),
  );

  function itemPath(item: MediaDirItem): string {
    if (item.relative_path) return item.relative_path;
    const parent = crumbs.slice(1).map((crumb) => crumb.name).join('/');
    return parent ? `${parent}/${item.display_name}` : item.display_name;
  }

  function clearSelection() {
    selected = new Map();
    confirming = false;
  }

  async function loadBrowse(parentId: number | null) {
    loading = true;
    errorMessage = '';
    try {
      const page = await listMediaDirs(parentId);
      items = page.items;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `加载失败 ${err.status}: ${err.body}` : String(err);
      items = [];
    } finally {
      loading = false;
    }
  }

  async function loadSearch(query: string) {
    loading = true;
    errorMessage = '';
    try {
      const page = await searchMediaDirs(query);
      items = page.items;
    } catch (err) {
      errorMessage = err instanceof ApiError ? `搜索失败 ${err.status}: ${err.body}` : String(err);
      items = [];
    } finally {
      loading = false;
    }
  }

  async function refresh() {
    if (mode === 'search') {
      await loadSearch(keyword.trim());
    } else {
      await loadBrowse(currentParentId);
    }
  }

  function search() {
    const query = keyword.trim();
    if (!query) return;
    mode = 'search';
    successMessage = '';
    clearSelection();
    loadSearch(query);
  }

  function reset() {
    keyword = '';
    mode = 'browse';
    crumbs = [{ dir_id: null, name: '库根' }];
    successMessage = '';
    errorMessage = '';
    clearSelection();
    loadBrowse(null);
  }

  function openFolder(item: MediaDirItem) {
    if (item.deletable) return;
    crumbs = [...crumbs, { dir_id: item.dir_id, name: item.display_name }];
    successMessage = '';
    clearSelection();
    loadBrowse(item.dir_id);
  }

  function jumpTo(index: number) {
    if (index >= crumbs.length - 1) return;
    crumbs = crumbs.slice(0, index + 1);
    mode = 'browse';
    successMessage = '';
    clearSelection();
    loadBrowse(crumbs[index].dir_id);
  }

  function toggleSelect(item: MediaDirItem) {
    if (!item.deletable) return;
    const next = new Map(selected);
    if (next.has(item.dir_id)) {
      next.delete(item.dir_id);
    } else {
      next.set(item.dir_id, itemPath(item));
    }
    selected = next;
    confirming = false;
  }

  function selectAll() {
    selected = new Map(deletableItems.map((item) => [item.dir_id, itemPath(item)]));
    confirming = false;
  }

  function deselectAll() {
    clearSelection();
  }

  async function confirmDelete() {
    if (selectedItems.length === 0) return;
    deleting = true;
    errorMessage = '';
    successMessage = '';
    try {
      await deleteMediaDirs(selectedItems);
      const count = selectedItems.length;
      clearSelection();
      successMessage = `已删除 ${count} 个媒体目录`;
      await refresh();
    } catch (err) {
      errorMessage = err instanceof ApiError ? `删除失败 ${err.status}: ${err.body}` : String(err);
    } finally {
      deleting = false;
    }
  }

  function onFolderKey(event: KeyboardEvent, item: MediaDirItem) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      openFolder(item);
    }
  }

  loadBrowse(null);
</script>

<section>
  <header class="page-header">
    <h1 class="page-title">媒体目录</h1>
    {#if !loading && !errorMessage}
      <span class="page-count">{items.length} 项</span>
    {/if}
  </header>

  <form class="toolbar" onsubmit={(e) => { e.preventDefault(); search(); }}>
    <div class="search-wrap">
      <Search class="search-icon" size={16} />
      <input
        type="text"
        bind:value={keyword}
        placeholder="输入媒体名…"
        class="input"
      />
    </div>
    <button type="submit" class="btn btn-primary" disabled={!keyword.trim()}>搜索</button>
    <button type="button" onclick={reset} class="btn btn-ghost">重置</button>
  </form>

  {#if mode === 'browse'}
    <nav class="crumbs" aria-label="目录路径">
      {#each crumbs as crumb, index}
        {#if index > 0}
          <span class="crumb-sep">/</span>
        {/if}
        {#if index < crumbs.length - 1}
          <button type="button" class="crumb-link" onclick={() => jumpTo(index)}>
            {crumb.name}
          </button>
        {:else}
          <span class="crumb-current">{crumb.name}</span>
        {/if}
      {/each}
    </nav>
  {/if}

  {#if deletableItems.length > 0}
    <div class="work-band">
      <div class="work-band-left">
        <button type="button" onclick={selectAll} class="btn btn-ghost btn-sm">全选</button>
        <button type="button" onclick={deselectAll} class="btn btn-ghost btn-sm">取消</button>
        <span class="work-count">{selected.size} 已选</span>
      </div>
      <button
        type="button"
        onclick={() => { confirming = true; }}
        disabled={selected.size === 0 || deleting}
        class="btn btn-danger btn-sm"
      >
        删除选中 ({selected.size})
      </button>
    </div>
  {/if}

  {#if confirming && selectedItems.length > 0}
    <div class="panel">
      <div class="panel-header">
        <h3 class="panel-title">确认删除</h3>
      </div>
      <p class="confirm-copy">将把远程目录移入回收站，并删除本地 strm</p>
      <ul class="confirm-list">
        {#each selectedItems as item (item.dir_id)}
          <li>{item.relative_path}</li>
        {/each}
      </ul>
      <div class="confirm-actions">
        <button type="button" class="btn btn-danger btn-sm" onclick={confirmDelete} disabled={deleting}>
          {deleting ? '删除中…' : '确认删除'}
        </button>
        <button type="button" class="btn btn-ghost btn-sm" onclick={() => { confirming = false; }} disabled={deleting}>
          取消
        </button>
      </div>
    </div>
  {/if}

  {#if successMessage}
    <div class="banner banner-ok">{successMessage}</div>
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
    <div class="empty">{mode === 'search' ? '没有匹配的媒体目录' : '这个目录是空的'}</div>
  {:else}
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th class="col-check"></th>
            <th>名称</th>
            <th>路径</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.dir_id)}
            {#if item.deletable}
              <tr class:is-selected={selected.has(item.dir_id)}>
                <td class="col-check">
                  <label>
                    <input
                      type="checkbox"
                      checked={selected.has(item.dir_id)}
                      onchange={() => toggleSelect(item)}
                    />
                  </label>
                </td>
                <td>
                  <button type="button" class="name-button" onclick={() => toggleSelect(item)}>
                    <Clapperboard size={15} />
                    <span class="cell-title">{item.display_name}</span>
                  </button>
                </td>
                <td class="mono cell-sub">{itemPath(item)}</td>
              </tr>
            {:else}
              <tr
                class="is-clickable"
                tabindex="0"
                onclick={() => openFolder(item)}
                onkeydown={(event) => onFolderKey(event, item)}
              >
                <td class="col-check"></td>
                <td>
                  <div class="name-cell">
                    <Folder size={15} />
                    <span class="cell-title">{item.display_name}</span>
                    <ChevronRight size={14} />
                  </div>
                </td>
                <td class="mono cell-sub">—</td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>
