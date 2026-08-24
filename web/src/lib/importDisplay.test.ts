import { describe, expect, test } from 'vitest';
import {
  failedEpisodes,
  formatCost,
  formatEpisodes,
  formatErrorLine,
  formatListTitle,
  formatSeasonCell,
  formatSeasonLabel,
  formatSize,
  formatMediaTitle,
  formatTvOutcome,
  groupSummaryItems,
  statusLabel,
  succeededEpisodes,
} from './importDisplay';

describe('formatEpisodes', () => {
  test('returns empty string for no episodes', () => {
    expect(formatEpisodes([])).toBe('');
  });

  test('formats single, ranges, and gaps like the backend', () => {
    expect(formatEpisodes([1])).toBe('E01');
    expect(formatEpisodes([1, 2, 3])).toBe('E01-E03');
    expect(formatEpisodes([1, 3, 5])).toBe('E01,E03,E05');
    expect(formatEpisodes([1, 2, 4, 5, 7])).toBe('E01-E02,E04-E05,E07');
    expect(formatEpisodes([3, 1, 2, 2])).toBe('E01-E03');
  });
});

describe('formatListTitle', () => {
  test('prefers summary title and year', () => {
    expect(formatListTitle({ title: '入青云', year: '2025', source: 'https://example' })).toBe(
      '入青云 (2025)',
    );
  });

  test('falls back to source when title is empty', () => {
    expect(formatListTitle({ title: '', year: '', source: 'https://115cdn.com/s/abc' })).toBe(
      'https://115cdn.com/s/abc',
    );
  });
});

describe('formatSeasonCell', () => {
  test('shows S01 without a this-run fraction', () => {
    expect(formatSeasonCell({ season: 1 })).toBe('S01');
  });

  test('appends this-run episode summary', () => {
    expect(formatSeasonCell({ season: 1, episode_summary: 'E15' })).toBe('S01 E15');
    expect(formatSeasonCell({ season: 1, episode_summary: 'E01-E14 失败' })).toBe('S01 E01-E14 失败');
  });
});

describe('formatErrorLine', () => {
  test('shows kind only when message was stripped', () => {
    expect(formatErrorLine({ kind: 'internal', message: '' })).toBe('internal');
  });

  test('includes excerpt when present', () => {
    expect(formatErrorLine({ kind: 'network', message: 'upstream timeout' })).toBe(
      'network · upstream timeout',
    );
  });
});

describe('tv episode splits', () => {
  const item = {
    type: 'tv' as const,
    name: 'Show',
    year: '2025',
    season: 1,
    episodes: [
      { episode: 15, succeeded: true },
      { episode: 16, succeeded: false },
    ],
    missing_episodes: [1, 2],
    max_episode_number: 16,
    number_of_episodes: 20,
    total_size: 0,
    cost_ms: 1,
  };

  test('separates this-run succeeded and failed episodes', () => {
    expect(succeededEpisodes(item)).toEqual([15]);
    expect(failedEpisodes(item)).toEqual([16]);
    expect(formatSeasonLabel(item.season)).toBe('S01');
  });
});

describe('formatCost', () => {
  test('treats missing and zero as a dash', () => {
    expect(formatCost(undefined)).toBe('—');
    expect(formatCost(0)).toBe('—');
  });

  test('keeps milliseconds under one second', () => {
    expect(formatCost(250)).toBe('250ms');
  });

  test('formats seconds with one decimal', () => {
    expect(formatCost(1200)).toBe('1.2s');
  });
});

describe('formatSize', () => {
  test('treats missing bytes as a dash', () => {
    expect(formatSize(undefined)).toBe('—');
    expect(formatSize(null)).toBe('—');
  });

  test('formats bytes and larger units', () => {
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(1536)).toBe('1.5 KiB');
    expect(formatSize(1_048_576)).toBe('1.0 MiB');
  });
});

describe('statusLabel', () => {
  test('maps known statuses and falls back to the raw value', () => {
    expect(statusLabel('running')).toBe('处理中');
    expect(statusLabel('succeeded')).toBe('成功');
    expect(statusLabel('partially_failed')).toBe('部分失败');
    expect(statusLabel('failed')).toBe('失败');
    expect(statusLabel('skipped')).toBe('跳过');
    expect(statusLabel('other')).toBe('other');
  });
});

describe('formatMediaTitle', () => {
  test('appends year when present', () => {
    expect(formatMediaTitle('我的三体', '2014')).toBe('我的三体 (2014)');
    expect(formatMediaTitle('Show')).toBe('Show');
  });
});

describe('groupSummaryItems', () => {
  const tv = (season: number, name = '我的三体'): Extract<import('./api').ImportSummaryItem, { type: 'tv' }> => ({
    type: 'tv',
    name,
    year: '2014',
    season,
    episodes: [
      { episode: 1, succeeded: true },
      { episode: 2, succeeded: true },
    ],
    missing_episodes: [],
    max_episode_number: 2,
    number_of_episodes: 2,
    total_size: 1000,
    cost_ms: 800,
  });

  test('groups consecutive seasons of the same show', () => {
    const groups = groupSummaryItems({
      items: [tv(1), tv(2), tv(3)],
      total_size: 3000,
      total_cost_ms: 2400,
      skipped_files: [],
    });
    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject({ type: 'tv', title: '我的三体 (2014)' });
    if (groups[0].type === 'tv') {
      expect(groups[0].items.map((item) => item.season)).toEqual([1, 2, 3]);
    }
  });

  test('splits different shows and keeps skipped files once', () => {
    const groups = groupSummaryItems({
      items: [tv(1), tv(1, '黑镜'), { type: 'skipped', files: ['a.nfo'] }],
      total_size: 0,
      total_cost_ms: 0,
      skipped_files: ['a.nfo', 'b.txt'],
    });
    expect(groups.map((group) => group.type)).toEqual(['tv', 'tv', 'skipped']);
    expect(groups[0]).toMatchObject({ title: '我的三体 (2014)' });
    expect(groups[1]).toMatchObject({ title: '黑镜 (2014)' });
    if (groups[2].type === 'skipped') {
      expect(groups[2].files).toEqual(['a.nfo', 'b.txt']);
    }
  });
});

describe('formatTvOutcome', () => {
  test('joins succeeded and failed episode ranges', () => {
    expect(
      formatTvOutcome({
        type: 'tv',
        name: 'Show',
        year: '2025',
        season: 1,
        episodes: [
          { episode: 1, succeeded: true },
          { episode: 2, succeeded: true },
          { episode: 3, succeeded: false },
        ],
        missing_episodes: [4],
        max_episode_number: 4,
        number_of_episodes: 4,
        total_size: 0,
        cost_ms: 1,
      }),
    ).toBe('E01-E02 · 失败 E03');
  });
});
