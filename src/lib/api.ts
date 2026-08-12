/**
 * AI Gallery API 客户端
 *
 * 所有页面从这里引用 API 地址和信息获取工具。
 * 后端返回格式: { success: boolean, data: any, error: { code: string, message: string } | null }
 */

export const API_BASE = import.meta.env.API_BASE || 'https://ai-gallery-api.baobaolong12.workers.dev';

// ==================== 类型定义 ====================

export interface ImageItem {
  number: number;
  title: string;
  prompt: string;
  seed: number;
  model: string | null;
  png_url: string;
  tags: string[];
  created_at: string;
  cfg_scale?: number | null;
  steps?: number | null;
  sampler?: string | null;
  width?: number | null;
  height?: number | null;
  source?: string | null;
}

export interface ImageDetail extends ImageItem {
  negative: string | null;
  model_hash: string | null;
  loras: string | null;
}

export interface ImageListResponse {
  items: ImageItem[];
  total: number;
  page: number;
  per_page: number;
}

export interface SearchResponse {
  items: ImageItem[];
  total: number;
}

export interface ClusterResponse {
  nodes: any[];
  edges: any[];
  clusters: any[];
}

export interface GalleryStats {
  total_images: number;
  top_models: [string, number][];
  top_tags: [string, number][];
  top_prompt_tokens: [string, number][];
  seed_distribution: [string, number][];
  by_month: [string, number][];
  top_samplers: [string, number][];
}

export interface DeduceResponse {
  token: string;
  match_count: number;
  co_occurring: [string, number][];
  suggested_prompt: string;
  similar_images: ImageItem[];
}

// ==================== 请求封装 ====================

/** 带超时的 fetch 工具 */
export async function fetchWithTimeout(
  url: string,
  options: RequestInit = {},
  timeoutMs = 10000,
): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { ...options, signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}

/** 解析 API 响应 */
export async function parseApiResponse<T = any>(res: Response): Promise<{ data: T | null; error: string | null }> {
  try {
    const body = await res.json();
    if (body && typeof body === 'object' && 'success' in body) {
      if (body.success) return { data: body.data as T, error: null };
      return { data: null, error: body.error?.message || '请求失败' };
    }
    return { data: body as T, error: null };
  } catch (e: any) {
    return { data: null, error: '响应解析失败: ' + e.message };
  }
}

// ==================== API 函数 ====================

/** 获取图片列表 */
export async function fetchImages(page = 0, perPage = 50): Promise<{ data: ImageListResponse | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/images?page=${page}&per_page=${perPage}`);
  return parseApiResponse<ImageListResponse>(res);
}

/** 获取单张图片详情 */
export async function fetchImageDetail(number: number): Promise<{ data: ImageDetail | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/images/${number}`);
  return parseApiResponse<ImageDetail>(res);
}

/** 搜索图片 */
export async function searchImages(params: { q?: string; model?: string; tag?: string; seed?: number }): Promise<{ data: SearchResponse | null; error: string | null }> {
  const sp = new URLSearchParams();
  if (params.q) sp.set('q', params.q);
  if (params.model) sp.set('model', params.model);
  if (params.tag) sp.set('tag', params.tag);
  if (params.seed) sp.set('seed', String(params.seed));
  const res = await fetchWithTimeout(`${API_BASE}/api/search?${sp}`);
  return parseApiResponse<SearchResponse>(res);
}

/** 获取聚类结果 */
export async function fetchCluster(): Promise<{ data: ClusterResponse | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/cluster`);
  return parseApiResponse<ClusterResponse>(res);
}

/** 获取统计 */
export async function fetchStats(): Promise<{ data: GalleryStats | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/stats`);
  return parseApiResponse<GalleryStats>(res);
}

/** 推演: 查询 prompt token 共现 */
export async function fetchDeduce(token: string): Promise<{ data: DeduceResponse | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/deduce/${encodeURIComponent(token)}`);
  return parseApiResponse<DeduceResponse>(res);
}

// ==================== 上传 & 笔记 ====================

export interface NoteRecord {
  id: number;
  number: number;
  content: string;
  created_at: string;
  updated_at: string;
}

export interface CreateImageParams {
  png_url: string;
  prompt: string;
  negative?: string;
  seed?: number;
  model?: string;
  cfg_scale?: number;
  steps?: number;
  sampler?: string;
  width?: number;
  height?: number;
  loras?: string;
  source?: string;
  tags?: string[];
  title?: string;
}

/** 上传图片文件到 ImgBed（经 Worker 代理）。注意返回是 ImgBed 原始数组格式，不是 {success,data} */
export async function uploadImage(file: File): Promise<{ data: { src: string } | null; error: string | null }> {
  const formData = new FormData();
  formData.append('file', file);
  try {
    const res = await fetchWithTimeout(`${API_BASE}/api/upload`, { method: 'POST', body: formData }, 30000);
    const body = await res.json();
    if (Array.isArray(body) && body.length > 0 && body[0]?.src) {
      return { data: { src: body[0].src }, error: null };
    }
    return { data: null, error: '上传响应格式异常' };
  } catch (e: any) {
    return { data: null, error: '上传失败: ' + e.message };
  }
}

/** 创建图片记录，返回新图片 number */
export async function createImage(params: CreateImageParams): Promise<{ data: { number: number } | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/images`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  });
  return parseApiResponse<{ number: number; created_at: string }>(res);
}

/** 获取图片笔记 */
export async function fetchNotes(number: number): Promise<{ data: NoteRecord[] | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/images/${number}/notes`);
  return parseApiResponse<NoteRecord[]>(res);
}

/** 创建笔记 */
export async function createNote(number: number, content: string): Promise<{ data: { id: number } | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/images/${number}/notes`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
  });
  return parseApiResponse<{ id: number; created_at: string }>(res);
}

/** 删除笔记 */
export async function deleteNote(number: number, noteId: number): Promise<{ data: any | null; error: string | null }> {
  const res = await fetchWithTimeout(`${API_BASE}/api/images/${number}/notes/${noteId}`, { method: 'DELETE' });
  return parseApiResponse<any>(res);
}