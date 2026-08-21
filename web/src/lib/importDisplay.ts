import type { ImportListItem, ImportRecordError, ImportSummaryItem } from './api';

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
