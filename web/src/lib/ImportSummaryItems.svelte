<script lang="ts">
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
        <div class="import-group-title">{group.title}</div>
        <div class="import-line">
          <span class="import-line-key">电影</span>
          <span class="import-line-main">{group.item.succeeded ? '入库成功' : '入库失败'}</span>
          <span class="import-line-size">{formatSize(group.item.size)}</span>
          <span class="import-line-cost">{formatCost(group.item.cost_ms)}</span>
        </div>
      </div>
    {:else if group.type === 'tv'}
      <div class="import-group">
        <div class="import-group-title">{group.title}</div>
        {#each group.items as item (item.season)}
          <div class="import-line">
            <span class="import-line-key">{formatSeasonLabel(item.season)}</span>
            <span class="import-line-main">{formatTvOutcome(item) || '无分集'}</span>
            <span class="import-line-size">{formatSize(item.total_size)}</span>
            <span class="import-line-cost">{formatCost(item.cost_ms)}</span>
            {#if item.missing_episodes.length > 0}
              <span class="import-line-note">整季还缺 {formatEpisodes(item.missing_episodes)}</span>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="import-group">
        <div class="import-group-title">跳过文件</div>
        {#each group.files as file}
          <div class="import-line-file mono">{file}</div>
        {/each}
      </div>
    {/if}
  {/each}
</div>
