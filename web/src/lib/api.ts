export interface ImportRecordPage {
  items: ImportListItem[];
  next_cursor: number | null;
}

export interface ImportRecordError {
  kind: string;
  message: string;
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
  error?: ImportRecordError | null;
}

export interface ImportEpisodeOutcome {
  episode: number;
  succeeded: boolean;
}

export type ImportSummaryItem =
  | {
      type: 'movie';
      title: string;
      year: string;
      size: number;
      cost_ms: number;
      succeeded: boolean;
    }
  | {
      type: 'tv';
      name: string;
      year: string;
      season: number;
      episodes: ImportEpisodeOutcome[];
      missing_episodes: number[];
      max_episode_number: number;
      number_of_episodes: number;
      total_size: number;
      cost_ms: number;
    }
  | {
      type: 'skipped';
      files: string[];
    };

export interface ImportSummary {
  items: ImportSummaryItem[];
  total_size: number;
  total_cost_ms: number;
  skipped_files: string[];
}

export interface ImportDetail {
  id: number;
  source_kind: string;
  source: string;
  status: string;
  summary: ImportSummary | null;
  error: ImportRecordError | null;
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
  summary?: ImportSummary;
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

export interface ShareImportResult {
  url: string;
  status: string;
  title?: string;
  year?: string;
  size?: number;
  summary?: ImportSummary;
  error?: string;
}

export async function importShareUrl(url: string, description?: string): Promise<ShareImportResult> {
  const payload: { url: string; description?: string } = { url };
  if (description) payload.description = description;
  const res = await fetch('/api/shares/import', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as ShareImportResult;
}


export interface CommunityThreadPage {
  items: CommunityThread[];
}

export interface CommunityThread {
  tid: number;
  title: string;
  tags: string[];
  author: string;
  posted_at: string;
  comments: number;
  likes: number;
  url: string;
}

export async function searchCommunityThreads(keyword: string, limit: number): Promise<CommunityThreadPage> {
  const params = new URLSearchParams();
  if (keyword) params.set('q', keyword);
  params.set('limit', String(limit));
  return fetchJson<CommunityThreadPage>(`/api/community/threads?${params.toString()}`);
}

export interface CommunityImportResult {
  tid: number;
  thread_title: string;
  share_url?: string;
  status: string;
  title?: string;
  year?: string;
  size?: number;
  summary?: ImportSummary;
  error?: string;
}

export interface CommunityImportResponse {
  results: CommunityImportResult[];
}

export async function importCommunityThreads(tids: number[]): Promise<CommunityImportResponse> {
  const res = await fetch('/api/community/threads/import', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ tids }),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as CommunityImportResponse;
}

// ── Subscriptions ──

export interface SubscriptionItem {
  id: number;
  tmdb_id: number;
  media_type: string;
  title_zh: string | null;
  title_en: string | null;
  year: string | null;
  poster_path: string | null;
  overview: string | null;
  create_time: string;
  update_time: string;
}

export interface SubscriptionListResponse {
  items: SubscriptionItem[];
}

export interface CandidateItem {
  tmdb_id: number;
  media_type: string;
  title: string;
  original_title: string;
  year: string | null;
  poster_path: string | null;
  overview: string | null;
}

export interface CandidatesResponse {
  candidates: CandidateItem[];
}

export interface CreateSubscriptionInput {
  tmdb_id: number;
  media_type: string;
  title_zh?: string;
  title_en?: string;
  year?: string;
  poster_path?: string;
  overview?: string;
}

export async function listSubscriptions(): Promise<SubscriptionListResponse> {
  return fetchJson<SubscriptionListResponse>('/api/subscriptions');
}

export async function searchCandidates(query: string): Promise<CandidatesResponse> {
  const params = new URLSearchParams({ query });
  return fetchJson<CandidatesResponse>(`/api/subscriptions/candidates?${params.toString()}`);
}

export async function createSubscription(input: CreateSubscriptionInput): Promise<{ id: number }> {
  const res = await fetch('/api/subscriptions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as { id: number };
}

export async function deleteSubscription(id: number): Promise<void> {
  const res = await fetch(`/api/subscriptions/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
}

export async function rescanSubscription(id: number): Promise<ImportFileResult[]> {
  const res = await fetch(`/api/subscriptions/${encodeURIComponent(id)}/rescan`, {
    method: 'POST',
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as ImportFileResult[];
}

// ── Media dirs ──

export interface MediaDirItem {
  dir_id: number;
  display_name: string;
  deletable: boolean;
  relative_path?: string;
}

export interface MediaDirPage {
  items: MediaDirItem[];
}

export interface MediaDirDeleteItem {
  dir_id: number;
  relative_path: string;
}

export async function listMediaDirs(parentId?: number | null): Promise<MediaDirPage> {
  const params = new URLSearchParams();
  if (parentId != null) params.set('parent_id', String(parentId));
  const qs = params.toString();
  return fetchJson<MediaDirPage>(qs ? `/api/media-dirs?${qs}` : '/api/media-dirs');
}

export async function searchMediaDirs(keyword: string): Promise<MediaDirPage> {
  const params = new URLSearchParams({ q: keyword });
  return fetchJson<MediaDirPage>(`/api/media-dirs?${params.toString()}`);
}

export async function deleteMediaDirs(items: MediaDirDeleteItem[]): Promise<void> {
  const res = await fetch('/api/media-dirs/delete', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ items }),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, body);
  }
}
