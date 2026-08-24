import type { ImportListItem, ImportRecordError, ImportSummary, ImportSummaryItem } from './api';

export function formatEpisodes(episodes: number[]): string {
  if (episodes.length === 0) return '';

  const sorted = [...new Set(episodes)].sort((a, b) => a - b);
  const parts: string[] = [];
  let i = 0;
  while (i < sorted.length) {
    const start = sorted[i];
    let end = start;
    let j = i + 1;
    while (j < sorted.length && sorted[j] === end + 1) {
      end = sorted[j];
      j += 1;
    }
    parts.push(start === end ? formatEpisode(start) : `${formatEpisode(start)}-${formatEpisode(end)}`);
    i = j;
  }
  return parts.join(',');
}

export function formatListTitle(item: Pick<ImportListItem, 'title' | 'year' | 'source'>): string {
  if (item.title) {
    return item.year ? `${item.title} (${item.year})` : item.title;
  }
  return item.source || '—';
}

export function formatSeasonCell(item: Pick<ImportListItem, 'season' | 'episode_summary'>): string {
  if (item.season != null) {
    const padded = String(item.season).padStart(2, '0');
    return `S${padded}${item.episode_summary ? ` ${item.episode_summary}` : ''}`;
  }
  return item.episode_summary ?? '';
}

export function formatErrorLine(error: ImportRecordError): string {
  return error.message ? `${error.kind} · ${error.message}` : error.kind;
}

export function succeededEpisodes(item: Extract<ImportSummaryItem, { type: 'tv' }>): number[] {
  return item.episodes.filter((episode) => episode.succeeded).map((episode) => episode.episode);
}

export function failedEpisodes(item: Extract<ImportSummaryItem, { type: 'tv' }>): number[] {
  return item.episodes.filter((episode) => !episode.succeeded).map((episode) => episode.episode);
}

export function formatSeasonLabel(season: number): string {
  return `S${String(season).padStart(2, '0')}`;
}

function formatEpisode(episode: number): string {
  return `E${String(episode).padStart(2, '0')}`;
}

export function formatCost(ms: number | null | undefined): string {
  if (!ms) return '—';
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function formatSize(bytes: number | null | undefined): string {
  if (bytes == null) return '—';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = Number(bytes);
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

export function statusLabel(status: string): string {
  switch (status) {
    case 'running':
      return '处理中';
    case 'succeeded':
      return '成功';
    case 'partially_failed':
      return '部分失败';
    case 'failed':
      return '失败';
    case 'skipped':
      return '跳过';
    default:
      return status;
  }
}

export type TvSummaryItem = Extract<ImportSummaryItem, { type: 'tv' }>;
export type MovieSummaryItem = Extract<ImportSummaryItem, { type: 'movie' }>;

export type ImportSummaryGroup =
  | { type: 'movie'; title: string; item: MovieSummaryItem }
  | { type: 'tv'; title: string; items: TvSummaryItem[] }
  | { type: 'skipped'; files: string[] };

export function formatMediaTitle(title: string, year?: string | null): string {
  return year ? `${title} (${year})` : title;
}

export function groupSummaryItems(summary: ImportSummary): ImportSummaryGroup[] {
  const groups: ImportSummaryGroup[] = [];
  const skippedFromItems: string[] = [];

  for (const item of summary.items) {
    if (item.type === 'movie') {
      groups.push({
        type: 'movie',
        title: formatMediaTitle(item.title, item.year),
        item,
      });
      continue;
    }
    if (item.type === 'tv') {
      const title = formatMediaTitle(item.name, item.year);
      const last = groups.at(-1);
      if (last?.type === 'tv' && last.title === title) {
        last.items.push(item);
      } else {
        groups.push({ type: 'tv', title, items: [item] });
      }
      continue;
    }
    skippedFromItems.push(...item.files);
  }

  const skipped = summary.skipped_files.length > 0 ? summary.skipped_files : skippedFromItems;
  if (skipped.length > 0) {
    groups.push({ type: 'skipped', files: skipped });
  }
  return groups;
}

export function formatTvOutcome(item: TvSummaryItem): string {
  const parts: string[] = [];
  const succeeded = succeededEpisodes(item);
  const failed = failedEpisodes(item);
  if (succeeded.length > 0) parts.push(formatEpisodes(succeeded));
  if (failed.length > 0) parts.push(`失败 ${formatEpisodes(failed)}`);
  return parts.join(' · ');
}
