export interface ImportRecordPage {
  items: ImportListItem[];
  next_cursor: number | null;
}

export interface ImportListItem {
  id: number;
  source_kind: string;
  source: string;
  status: string;
  title: string;
  year: string;
  season?: number;
  episode_summary?: string;
  total_size: number;
  cost_ms: number;
  created_at: string;
  finished_at: string | null;
}

export interface ImportDetail {
  id: number;
  source_kind: string;
  source: string;
  status: string;
  summary: unknown;
  error: { kind: string; message: string } | null;
  created_at: string;
  updated_at: string;
  finished_at: string | null;
}

export interface ListImportsFilter {
  status?: string;
  source_kind?: string;
  since?: string;
  until?: string;
  cursor?: number;
  limit?: number;
}

export class ApiError extends Error {
  readonly status: number;
  readonly body: string;
  constructor(status: number, body: string) {
    super(`HTTP ${status}: ${body}`);
    this.status = status;
    this.body = body;
    this.name = 'ApiError';
  }
}

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as T;
}

export async function listImports(filter: ListImportsFilter): Promise<ImportRecordPage> {
  const params = new URLSearchParams();
  if (filter.status) params.set('status', filter.status);
  if (filter.source_kind) params.set('source_kind', filter.source_kind);
  if (filter.since) params.set('since', filter.since);
  if (filter.until) params.set('until', filter.until);
  if (filter.cursor !== undefined) params.set('cursor', String(filter.cursor));
  if (filter.limit !== undefined) params.set('limit', String(filter.limit));
  const qs = params.toString();
  return fetchJson<ImportRecordPage>(qs ? `/api/imports?${qs}` : '/api/imports');
}

export async function getImport(id: number): Promise<ImportDetail> {
  return fetchJson<ImportDetail>(`/api/imports/${encodeURIComponent(id)}`);
}

export interface FileSearchPage {
  items: FileSearchItem[];
}

export interface FileSearchItem {
  id: number;
  size: number;
  hash_type: string;
  hash_value: string;
  locations: FileLocation[];
}

export interface FileLocation {
  file_name: string;
  file_path: string;
  descriptions: string[];
}

export async function searchFiles(keyword: string, limit: number): Promise<FileSearchPage> {
  const params = new URLSearchParams();
  if (keyword) params.set('q', keyword);
  params.set('limit', String(limit));
  return fetchJson<FileSearchPage>(`/api/files?${params.toString()}`);
}

export interface ImportFileResult {
  id: number;
  status: string;
  title?: string;
  year?: string;
  size?: number;
  error?: string;
}

export interface ImportFilesResponse {
  results: ImportFileResult[];
}

export async function importFiles(ids: number[]): Promise<ImportFilesResponse> {
  const res = await fetch('/api/files/import', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids }),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as ImportFilesResponse;
}
