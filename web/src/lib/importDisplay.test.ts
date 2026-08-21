import { describe, expect, test } from 'vitest';
import {
  failedEpisodes,
  formatEpisodes,
  formatErrorLine,
  formatListTitle,
  formatSeasonCell,
  formatSeasonLabel,
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
