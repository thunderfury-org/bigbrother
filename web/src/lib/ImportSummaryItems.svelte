<script lang="ts">
  import Film from '@lucide/svelte/icons/film';
  import Tv from '@lucide/svelte/icons/tv';
  import FileX from '@lucide/svelte/icons/file-x';
  import type { ImportSummary } from './api';
  import {
    formatCost,
    formatEpisodes,
    formatSeasonLabel,
    formatSize,
    formatTvOutcome,
    groupSummaryItems,
  } from './importDisplay';

  let { summary }: { summary: ImportSummary } = $props();
  const groups = $derived(groupSummaryItems(summary));
</script>

<div class="import-lines">
  {#each groups as group, index (index)}
    {#if group.type === 'movie'}
      <div class="import-group">
        <div class="import-group-title">
          <Film size={14} class="inline text-emerald-400 mr-1" />
          <span>{group.title}</span>
        </div>
        <div class="import-line">
          <span class="import-line-key">电影</span>
          <span class="import-line-main">{group.item.succeeded ? '入库成功' : '入库失败'}</span>
          <span class="import-line-size">{formatSize(group.item.size)}</span>
          <span class="import-line-cost">{formatCost(group.item.cost_ms)}</span>
        </div>
      </div>
    {:else if group.type === 'tv'}
      <div class="import-group">
        <div class="import-group-title">
          <Tv size={14} class="inline text-sky-400 mr-1" />
          <span>{group.title}</span>
        </div>
        {#each group.items as item (item.season)}
          <div class="import-line">
            <span class="import-line-key">{formatSeasonLabel(item.season)}</span>
            <span class="import-line-main">{formatTvOutcome(item) || '无分集'}</span>
            <span class="import-line-size">{formatSize(item.total_size)}</span>
            <span class="import-line-cost">{formatCost(item.cost_ms)}</span>
            {#if item.missing_episodes.length > 0}
              <span class="import-line-note text-amber-400">整季还缺 {formatEpisodes(item.missing_episodes)}</span>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="import-group">
        <div class="import-group-title">
          <FileX size={14} class="inline text-slate-400 mr-1" />
          <span>跳过文件</span>
        </div>
        {#each group.files as file}
          <div class="import-line-file mono">{file}</div>
        {/each}
      </div>
    {/if}
  {/each}
</div>

<style>
  .import-group {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-bb-line);
    border-radius: 8px;
    padding: 12px 14px;
  }
</style>
