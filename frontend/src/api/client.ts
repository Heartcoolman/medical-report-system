import type {
  ApiResponse,
  PaginatedList,
  InterpretationCache,
  Patient,
  PatientWithStats,
  PatientReq,
  Report,
  ReportDetail,
  ReportSummary,
  CreateReportReq,
  UpdateReportReq,
  TestItem,
  CreateTestItemReq,
  UpdateTestItemReq,
  EditLog,
  OcrParseResult,
  SuggestGroupsReq,
  SuggestGroupsResult,
  BatchConfirmReq,
  MergeCheckResult,
  TrendPoint,
  TrendItemInfo,
  TemperatureRecord,
  CreateTemperatureReq,
  ExpenseParseResponse,
  ConfirmExpenseReq,
  BatchConfirmExpenseReq,
  DailyExpenseDetail,
  DailyExpenseSummary,
  AnalyzeExpenseReq,
  AnalyzeExpenseResp,
  ParsedExpenseDay,
  MergeChunksReq,
  Medication,
  CreateMedicationReq,
  UpdateMedicationReq,
  DetectedDrug,
  TimelineEvent,
  UserInfo,
  AuditLog,
  HealthAssessment,
  RiskPrediction,
  DeviceSession,
} from './types';

const TOKEN_KEY = 'auth_token'
const REFRESH_TOKEN_KEY = 'refresh_token'

// API base path — read from Vite env variable, default to '/api' for backward compatibility.
// Set VITE_API_BASE=/api/v1 in .env to use the versioned endpoint.
export const API_BASE = (import.meta.env.VITE_API_BASE as string | undefined) || '/api';

// Build-time version injected by Vite (from package.json).
// Web is always deployed with the backend, so the backend skips version checks
// for platform=web. This header is mainly for future auditing.
declare const __APP_VERSION__: string
const APP_VERSION = typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0'

// --- Refresh token lock (prevents concurrent refresh calls) ---
let refreshPromise: Promise<string> | null = null

export async function tryRefreshToken(): Promise<string> {
  if (refreshPromise) return refreshPromise

  const refreshToken = localStorage.getItem(REFRESH_TOKEN_KEY)
  if (!refreshToken) throw new Error('no refresh token')

  refreshPromise = fetch(`${API_BASE}/auth/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken }),
  })
    .then(async (res) => {
      if (!res.ok) throw new Error('refresh failed')
      const json = await res.json()
      const newAccessToken: string = json.data.access_token
      const newRefreshToken: string = json.data.refresh_token
      localStorage.setItem(TOKEN_KEY, newAccessToken)
      localStorage.setItem(REFRESH_TOKEN_KEY, newRefreshToken)
      return newAccessToken
    })
    .finally(() => {
      refreshPromise = null
    })

  return refreshPromise
}

// --- update_notice from server responses ---
let _lastUpdateNotice: string | null = null
export function getLastUpdateNotice(): string | null { return _lastUpdateNotice }
export function clearUpdateNotice() { _lastUpdateNotice = null }

async function request<T>(path: string, options?: RequestInit, timeout = 12000): Promise<T> {
  // Prepend API_BASE to the resource path
  const url = `${API_BASE}${path}`

  const controller = new AbortController()
  const timer = window.setTimeout(() => controller.abort(), timeout)

  // Inject Authorization header + client identity headers
  const token = localStorage.getItem(TOKEN_KEY)
  const headers = new Headers(options?.headers)
  if (token) {
    headers.set('Authorization', `Bearer ${token}`)
  }
  headers.set('X-Client-Platform', 'web')
  headers.set('X-Client-Version', APP_VERSION)

  try {
    const res = await fetch(url, {
      ...options,
      headers,
      signal: controller.signal,
    })

    // Handle 401 — try to refresh token, then retry the original request
    if (res.status === 401) {
      try {
        const newToken = await tryRefreshToken()
        // Retry original request with new token
        const retryHeaders = new Headers(options?.headers)
        retryHeaders.set('Authorization', `Bearer ${newToken}`)
        retryHeaders.set('X-Client-Platform', 'web')
        retryHeaders.set('X-Client-Version', APP_VERSION)
        const retryController = new AbortController()
        const retryTimer = window.setTimeout(() => retryController.abort(), timeout)
        try {
          const retryRes = await fetch(url, {
            ...options,
            headers: retryHeaders,
            signal: retryController.signal,
          })
          const retryText = await retryRes.text()
          let retryJson: ApiResponse<T>
          try {
            retryJson = retryText
              ? (JSON.parse(retryText) as ApiResponse<T>)
              : { success: false, data: null, message: 'empty' }
          } catch {
            throw new Error(`retry: invalid JSON, HTTP ${retryRes.status}`)
          }
          if (!retryRes.ok) throw new Error(retryJson.message || `retry failed: ${retryRes.status}`)
          if (!retryJson.success) throw new Error(retryJson.message || 'retry failed')
          return retryJson.data as T
        } finally {
          window.clearTimeout(retryTimer)
        }
      } catch {
        localStorage.removeItem(TOKEN_KEY)
        localStorage.removeItem(REFRESH_TOKEN_KEY)
        if (window.location.pathname !== '/login') {
          window.location.href = '/login'
        }
        throw new Error('session expired')
      }
    }

    const rawText = await res.text()

    let json: ApiResponse<T>
    try {
      json = rawText ? (JSON.parse(rawText) as ApiResponse<T>) : { success: false, data: null, message: '空响应' }
    } catch {
      throw new Error(`响应不是有效 JSON，HTTP ${res.status}`)
    }

    // Capture server update_notice if present
    if ((json as any).update_notice) {
      _lastUpdateNotice = (json as any).update_notice
    }

    if (!res.ok) {
      throw new Error(json.message || `请求失败: ${res.status}`)
    }

    if (!json.success) {
      throw new Error(json.message || '请求失败')
    }

    return json.data as T
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') {
      throw new Error(`请求超时（${timeout / 1000}s）`)
    }
    throw err
  } finally {
    window.clearTimeout(timer)
  }
}

function jsonRequest(method: 'POST' | 'PUT', data: unknown): RequestInit {
  return {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  };
}

function qs(params: Record<string, string | number | undefined>): string {
  const entries = Object.entries(params).filter(
    (entry): entry is [string, string | number] => entry[1] !== undefined,
  );
  if (entries.length === 0) return '';
  return '?' + new URLSearchParams(entries.map(([k, v]) => [k, String(v)])).toString();
}

export interface AuthResponse {
  token: string
  access_token: string
  refresh_token: string
  expires_in: number
  user: { id: string; username: string; role: string }
}

export interface UserSettings {
  llm_api_key: string
  interpret_api_key: string
  zhipu_api_key: string
}

export const api = {
  auth: {
    login(username: string, password: string) {
      return request<AuthResponse>('/auth/login', jsonRequest('POST', {
        username,
        password,
        device_name: navigator.userAgent.slice(0, 50),
        device_type: 'web',
      })).then(data => {
        localStorage.setItem(TOKEN_KEY, data.access_token || data.token)
        localStorage.setItem(REFRESH_TOKEN_KEY, data.refresh_token)
        return data
      });
    },
    register(username: string, password: string) {
      return request<AuthResponse>('/auth/register', jsonRequest('POST', { username, password })).then(data => {
        localStorage.setItem(TOKEN_KEY, data.access_token || data.token)
        localStorage.setItem(REFRESH_TOKEN_KEY, data.refresh_token)
        return data
      });
    },
    me() {
      return request<{ id: string; username: string; role: string }>('/auth/me');
    },
    logout() {
      const rt = localStorage.getItem(REFRESH_TOKEN_KEY)
      localStorage.removeItem(TOKEN_KEY)
      localStorage.removeItem(REFRESH_TOKEN_KEY)
      if (rt) {
        // fire-and-forget
        fetch(`${API_BASE}/auth/logout`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ refresh_token: rt }),
        }).catch(() => {})
      }
    },
    devices() {
      return request<DeviceSession[]>('/auth/devices');
    },
    revokeDevice(id: string) {
      return request<void>(`/auth/devices/${id}`, { method: 'DELETE' });
    },
  },

  user: {
    getSettings() {
      return request<UserSettings>('/user/settings');
    },
    updateSettings(data: Partial<UserSettings>) {
      return request<UserSettings>('/user/settings', jsonRequest('PUT', data));
    },
  },

  patients: {
    list(params?: { search?: string; page?: number; page_size?: number }) {
      return request<PaginatedList<PatientWithStats>>(`/patients${qs(params ?? {})}`);
    },
    get(id: string) {
      return request<Patient>(`/patients/${id}`);
    },
    create(data: PatientReq) {
      return request<Patient>('/patients', jsonRequest('POST',data));
    },
    update(id: string, data: PatientReq) {
      return request<Patient>(`/patients/${id}`, jsonRequest('PUT',data));
    },
    delete(id: string) {
      return request<void>(`/patients/${id}`, { method: 'DELETE' });
    },
  },

  reports: {
    listByPatient(patientId: string, params?: { page?: number; page_size?: number }) {
      return request<PaginatedList<ReportSummary>>(`/patients/${patientId}/reports${qs(params ?? {})}`);
    },
    get(id: string) {
      return request<ReportDetail>(`/reports/${id}`);
    },
    getInterpretation(id: string) {
      return request<InterpretationCache | null>(`/reports/${id}/interpret-cache`);
    },
    create(patientId: string, data: CreateReportReq) {
      return request<Report>(`/patients/${patientId}/reports`, jsonRequest('POST',data));
    },
    update(id: string, data: UpdateReportReq) {
      return request<Report>(`/reports/${id}`, jsonRequest('PUT',data));
    },
    delete(id: string) {
      return request<void>(`/reports/${id}`, { method: 'DELETE' });
    },
    mergeCheck(patientId: string, data: BatchConfirmReq) {
      return request<MergeCheckResult>(
        `/patients/${patientId}/reports/merge-check`,
        jsonRequest('POST',data),
      );
    },
    prefetchNormalize(patientId: string, data: BatchConfirmReq) {
      return request<Record<string, string>>(
        `/patients/${patientId}/reports/prefetch-normalize`,
        jsonRequest('POST',data),
        90000,
      );
    },
    batchConfirm(patientId: string, data: BatchConfirmReq) {
      return request<ReportDetail[]>(
        `/patients/${patientId}/reports/confirm`,
        jsonRequest('POST',data),
      );
    },
  },

  testItems: {
    listByReport(reportId: string) {
      return request<TestItem[]>(`/reports/${reportId}/test-items`);
    },
    create(data: CreateTestItemReq) {
      return request<TestItem>('/test-items', jsonRequest('POST',data));
    },
    update(id: string, data: UpdateTestItemReq) {
      return request<TestItem>(`/test-items/${id}`, jsonRequest('PUT',data));
    },
    delete(id: string) {
      return request<void>(`/test-items/${id}`, { method: 'DELETE' });
    },
  },

  editLogs: {
    list(params?: { page?: number; page_size?: number }) {
      return request<PaginatedList<EditLog>>(`/edit-logs${qs(params ?? {})}`);
    },
    listByReport(reportId: string) {
      return request<EditLog[]>(`/reports/${reportId}/edit-logs`);
    },
  },

  ocr: {
    parse(file: File, timeout = 90000) {
      const form = new FormData();
      form.append('file', file);
      return request<OcrParseResult>('/ocr/parse', { method: 'POST', body: form }, timeout);
    },
    suggestGroups(data: SuggestGroupsReq) {
      return request<SuggestGroupsResult>('/ocr/suggest-groups', jsonRequest('POST',data));
    },
  },

  temperatures: {
    list(patientId: string, params?: { page?: number; page_size?: number }) {
      return request<PaginatedList<TemperatureRecord>>(`/patients/${patientId}/temperatures${qs(params ?? {})}`);
    },
    create(patientId: string, data: CreateTemperatureReq) {
      return request<TemperatureRecord>(`/patients/${patientId}/temperatures`, jsonRequest('POST',data));
    },
    delete(id: string) {
      return request<void>(`/temperatures/${id}`, { method: 'DELETE' });
    },
  },

  trends: {
    getItems(patientId: string) {
      return request<TrendItemInfo[]>(`/patients/${patientId}/trend-items`);
    },
    getData(patientId: string, itemName: string, reportType?: string) {
      return request<TrendPoint[]>(
        `/patients/${patientId}/trends${qs({ item_name: itemName, report_type: reportType })}`,
      );
    },
  },

  expenses: {
    parse(patientId: string, file: File, timeout = 600000) {
      const form = new FormData();
      form.append('file', file);
      return request<ExpenseParseResponse>(
        `/patients/${patientId}/expenses/parse`,
        { method: 'POST', body: form },
        timeout,
      );
    },
    confirm(patientId: string, data: ConfirmExpenseReq) {
      return request<DailyExpenseDetail>(
        `/patients/${patientId}/expenses/confirm`,
        jsonRequest('POST', data),
      );
    },
    batchConfirm(patientId: string, data: BatchConfirmExpenseReq) {
      return request<DailyExpenseDetail[]>(
        `/patients/${patientId}/expenses/batch-confirm`,
        jsonRequest('POST', data),
      );
    },
    list(patientId: string, params?: { page?: number; page_size?: number }) {
      return request<PaginatedList<DailyExpenseSummary>>(`/patients/${patientId}/expenses${qs(params ?? {})}`);
    },
    get(id: string) {
      return request<DailyExpenseDetail>(`/expenses/${id}`);
    },
    delete(id: string) {
      return request<void>(`/expenses/${id}`, { method: 'DELETE' });
    },
    parseChunk(file: File, timeout = 300000) {
      const form = new FormData();
      form.append('file', file);
      return request<ParsedExpenseDay[]>(
        '/expenses/parse-chunk',
        { method: 'POST', body: form },
        timeout,
      );
    },
    mergeChunks(data: MergeChunksReq, timeout = 300000) {
      return request<ExpenseParseResponse>(
        '/expenses/merge-chunks',
        jsonRequest('POST', data),
        timeout,
      );
    },
    analyze(data: AnalyzeExpenseReq) {
      return request<AnalyzeExpenseResp>(
        '/expenses/analyze',
        jsonRequest('POST', data),
        30000,
      );
    },
  },

  medications: {
    list(patientId: string) {
      return request<Medication[]>(`/patients/${patientId}/medications`);
    },
    detectedDrugs(patientId: string) {
      return request<DetectedDrug[]>(`/patients/${patientId}/detected-drugs`);
    },
    create(patientId: string, data: CreateMedicationReq) {
      return request<Medication>(`/patients/${patientId}/medications`, jsonRequest('POST', data));
    },
    update(id: string, data: UpdateMedicationReq) {
      return request<Medication>(`/medications/${id}`, jsonRequest('PUT', data));
    },
    delete(id: string) {
      return request<void>(`/medications/${id}`, { method: 'DELETE' });
    },
  },

  timeline: {
    get(patientId: string) {
      return request<TimelineEvent[]>(`/patients/${patientId}/timeline`);
    },
  },

  admin: {
    backfillCanonicalNames() {
      return request<{ updated: number }>('/admin/backfill-canonical-names', {
        method: 'POST',
      });
    },
    listUsers() {
      return request<UserInfo[]>('/admin/users');
    },
    updateUserRole(userId: string, role: string) {
      return request<void>(`/admin/users/${userId}/role`, jsonRequest('PUT', { role }));
    },
    deleteUser(userId: string) {
      return request<void>(`/admin/users/${userId}`, { method: 'DELETE' });
    },
    auditLogs(params?: { page?: number; page_size?: number }) {
      return request<PaginatedList<AuditLog>>(`/admin/audit-logs${qs(params ?? {})}`);
    },
    async downloadBackup() {
      const token = localStorage.getItem(TOKEN_KEY)
      const res = await fetch(`${API_BASE}/admin/backup`, {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      })
      if (!res.ok) {
        const json = await res.json().catch(() => ({ message: `备份失败: ${res.status}` }))
        throw new Error(json.message || `备份失败: ${res.status}`)
      }
      const blob = await res.blob()
      const disposition = res.headers.get('content-disposition') || ''
      const match = disposition.match(/filename="?([^"]+)"?/)
      const filename = match?.[1] || `yiliao_backup_${new Date().toISOString().slice(0, 10)}.db`
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = filename
      a.click()
      URL.revokeObjectURL(url)
    },
    async restoreBackup(file: File) {
      const form = new FormData()
      form.append('file', file)
      return request<void>('/admin/restore', { method: 'POST', body: form }, 120000)
    },
  },

  healthAssessment: {
    getCache(patientId: string) {
      return request<{ content: HealthAssessment; created_at: string } | null>(
        `/patients/${patientId}/health-assessment-cache`,
      );
    },
  },

  riskPrediction: {
    get(patientId: string, refresh = false) {
      return request<RiskPrediction>(
        `/patients/${patientId}/risk-prediction${refresh ? '?refresh=1' : ''}`,
      );
    },
  },
};
