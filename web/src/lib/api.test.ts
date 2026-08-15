import { afterEach, describe, expect, test, vi } from 'vitest';
import { ApiError, deleteMediaDirs, getImport, listImports, listMediaDirs, searchFiles, searchMediaDirs } from './api';

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubFetchOnce(payload: unknown, init: ResponseInit = { status: 200 }) {
  const fetchMock = vi.fn<typeof fetch>(async () => new Response(JSON.stringify(payload), {
    ...init,
    headers: { 'content-type': 'application/json' },
  }));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function stubFetchPlainOnce(body: string, init: ResponseInit) {
  const fetchMock = vi.fn<typeof fetch>(async () => new Response(body, init));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

describe('listImports', () => {
  test('calls /api/imports with no query params when filter is empty', async () => {
    const fetchMock = stubFetchOnce({ items: [], next_cursor: null });

    const page = await listImports({});

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/imports');
    expect(page).toEqual({ items: [], next_cursor: null });
  });

  test('serializes filters into query string', async () => {
    const fetchMock = stubFetchOnce({ items: [], next_cursor: null });

    await listImports({
      status: 'failed',
      source_kind: 'quark',
      since: '2026-05-01T00:00:00Z',
      until: '2026-05-23T00:00:00Z',
      cursor: 42,
      limit: 50,
    });

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe(
      '/api/imports?status=failed&source_kind=quark&since=2026-05-01T00%3A00%3A00Z&until=2026-05-23T00%3A00%3A00Z&cursor=42&limit=50',
    );
  });

  test('serializes cursor=0 (boundary, not skipped as falsy)', async () => {
    const fetchMock = stubFetchOnce({ items: [], next_cursor: null });

    await listImports({ cursor: 0 });

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/imports?cursor=0');
  });

  test('treats null next_cursor as no more pages', async () => {
    stubFetchOnce({ items: [{ id: 1 }], next_cursor: null });

    const page = await listImports({});

    expect(page.next_cursor).toBeNull();
  });
});

describe('getImport', () => {
  test('calls /api/imports/{id}', async () => {
    const fetchMock = stubFetchOnce({ id: 123, status: 'succeeded' });

    const detail = await getImport(123);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/imports/123');
    expect(detail).toMatchObject({ id: 123, status: 'succeeded' });
  });
});

describe('searchFiles', () => {
  test('omits q when keyword is empty (server treats it as empty default)', async () => {
    const fetchMock = stubFetchOnce({ items: [] });

    await searchFiles('', 50);

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/files?limit=50');
  });

  test('serializes keyword and limit', async () => {
    const fetchMock = stubFetchOnce({ items: [] });

    await searchFiles('movie 2024 BD', 100);

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/files?q=movie+2024+BD&limit=100');
  });
});

describe('listMediaDirs', () => {
  test('calls /api/media-dirs with no query when parent is omitted', async () => {
    const fetchMock = stubFetchOnce({ items: [] });

    await listMediaDirs();

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/media-dirs');
  });

  test('serializes parent_id', async () => {
    const fetchMock = stubFetchOnce({ items: [] });

    await listMediaDirs(12);

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/media-dirs?parent_id=12');
  });
});

describe('searchMediaDirs', () => {
  test('serializes keyword', async () => {
    const fetchMock = stubFetchOnce({ items: [] });

    await searchMediaDirs('bad 2024');

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/media-dirs?q=bad+2024');
  });
});

describe('deleteMediaDirs', () => {
  test('posts items to /api/media-dirs/delete', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => new Response(null, { status: 204 }));
    vi.stubGlobal('fetch', fetchMock);

    await deleteMediaDirs([
      { dir_id: 21, relative_path: '电影/Inception (2010) {tmdb-27205}' },
    ]);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/media-dirs/delete');
    expect(init).toMatchObject({ method: 'POST' });
    expect(JSON.parse(String(init?.body))).toEqual({
      items: [{ dir_id: 21, relative_path: '电影/Inception (2010) {tmdb-27205}' }],
    });
  });

  test('rejects non-2xx with ApiError', async () => {
    stubFetchPlainOnce('not a media directory', { status: 400 });

    await expect(
      deleteMediaDirs([{ dir_id: 2, relative_path: '电影' }]),
    ).rejects.toMatchObject({
      status: 400,
      body: 'not a media directory',
    });
  });
});

describe('error handling', () => {
  test('rejects non-2xx with ApiError carrying status and body', async () => {
    stubFetchPlainOnce('invalid status: bogus', { status: 400 });

    await expect(listImports({ status: 'bogus' })).rejects.toBeInstanceOf(ApiError);

    stubFetchPlainOnce('invalid status: bogus', { status: 400 });
    try {
      await listImports({ status: 'bogus' });
      expect.unreachable('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).status).toBe(400);
      expect((err as ApiError).body).toBe('invalid status: bogus');
    }
  });

  test('500 surfaces as ApiError', async () => {
    stubFetchPlainOnce('internal', { status: 500 });

    await expect(getImport(1)).rejects.toMatchObject({
      status: 500,
      body: 'internal',
    });
  });
});
