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
  CriticalAlert,
  HealthAssessment,
} from './types';

const TOKEN_KEY = 'auth_token'

async function request<T>(url: string, options?: RequestInit, timeout = 12000): Promise<T> {
  const controller = new AbortController()
  const timer = window.setTimeout(() => controller.abort(), timeout)

  // Inject Authorization header
  const token = localStorage.getItem(TOKEN_KEY)
  const headers = new Headers(options?.headers)
  if (token) {
    headers.set('Authorization', `Bearer ${token}`)
  }

  try {
    const res = await fetch(url, {
      ...options,
      headers,
      signal: controller.signal,
    })

    // Handle 401 — clear token and redirect to login
    if (res.status === 401) {
      localStorage.removeItem(TOKEN_KEY)
      if (window.location.pathname !== '/login') {
        window.location.href = '/login'
      }
      throw new Error('未授权，请重新登录')
    }

    const rawText = await res.text()

    let json: ApiResponse<T>
    try {
      json = rawText ? (JSON.parse(rawText) as ApiResponse<T>) : { success: false, data: null, message: '空响应' }
    } catch {
      throw new Error(`响应不是有效 JSON，HTTP ${res.status}`)
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
  user: { id: string; username: string; role: string }
}

export interface UserSettings {
  llm_api_key: string
  interpret_api_key: string
  siliconflow_api_key: string
}

export const api = {
  auth: {
    login(username: string, password: string) {
      return request<AuthResponse>('/api/auth/login', jsonRequest('POST', { username, password }));
    },
    register(username: string, password: string) {
      return request<AuthResponse>('/api/auth/register', jsonRequest('POST', { username, password }));
    },
    me() {
      return request<{ id: string; username: string; role: string }>('/api/auth/me');
    },
  },

  user: {
    getSettings() {
      return request<UserSettings>('/api/user/settings');
    },
    updateSettings(data: Partial<UserSettings>) {
      return request<UserSettings>('/api/user/settings', jsonRequest('PUT', data));
    },
  },

  patients: {
    list(params?: { search?: string; page?: number; page_size?: number }) {
      return request<PaginatedList<PatientWithStats>>(`/api/patients${qs(params ?? {})}`);
    },
    get(id: string) {
      return request<Patient>(`/api/patients/${id}`);
    },
    create(data: PatientReq) {
      return request<Patient>('/api/patients', jsonRequest('POST',data));
    },
    update(id: string, data: PatientReq) {
      return request<Patient>(`/api/patients/${id}`, jsonRequest('PUT',data));
    },
    delete(id: string) {
      return request<void>(`/api/patients/${id}`, { method: 'DELETE' });
    },
  },

  reports: {
    listByPatient(patientId: string) {
      return request<ReportSummary[]>(`/api/patients/${patientId}/reports`);
    },
    get(id: string) {
      return request<ReportDetail>(`/api/reports/${id}`);
    },
    getInterpretation(id: string) {
      return request<InterpretationCache | null>(`/api/reports/${id}/interpret-cache`);
    },
    create(patientId: string, data: CreateReportReq) {
      return request<Report>(`/api/patients/${patientId}/reports`, jsonRequest('POST',data));
    },
    update(id: string, data: UpdateReportReq) {
      return request<Report>(`/api/reports/${id}`, jsonRequest('PUT',data));
    },
    delete(id: string) {
      return request<void>(`/api/reports/${id}`, { method: 'DELETE' });
    },
    mergeCheck(patientId: string, data: BatchConfirmReq) {
      return request<MergeCheckResult>(
        `/api/patients/${patientId}/reports/merge-check`,
        jsonRequest('POST',data),
      );
    },
    prefetchNormalize(patientId: string, data: BatchConfirmReq) {
      return request<Record<string, string>>(
        `/api/patients/${patientId}/reports/prefetch-normalize`,
        jsonRequest('POST',data),
        90000,
      );
    },
    batchConfirm(patientId: string, data: BatchConfirmReq) {
      return request<ReportDetail[]>(
        `/api/patients/${patientId}/reports/confirm`,
        jsonRequest('POST',data),
      );
    },
  },

  testItems: {
    listByReport(reportId: string) {
      return request<TestItem[]>(`/api/reports/${reportId}/test-items`);
    },
    create(data: CreateTestItemReq) {
      return request<TestItem>('/api/test-items', jsonRequest('POST',data));
    },
    update(id: string, data: UpdateTestItemReq) {
      return request<TestItem>(`/api/test-items/${id}`, jsonRequest('PUT',data));
    },
    delete(id: string) {
      return request<void>(`/api/test-items/${id}`, { method: 'DELETE' });
    },
  },

  editLogs: {
    list(params?: { page?: number; page_size?: number }) {
      return request<PaginatedList<EditLog>>(`/api/edit-logs${qs(params ?? {})}`);
    },
    listByReport(reportId: string) {
      return request<EditLog[]>(`/api/reports/${reportId}/edit-logs`);
    },
  },

  upload: {
    async file(file: File): Promise<string> {
      const form = new FormData();
      form.append('file', file);
      return request<string>('/api/upload', { method: 'POST', body: form });
    },
  },

  ocr: {
    parse(file: File, timeout = 90000) {
      const form = new FormData();
      form.append('file', file);
      return request<OcrParseResult>('/api/ocr/parse', { method: 'POST', body: form }, timeout);
    },
    suggestGroups(data: SuggestGroupsReq) {
      return request<SuggestGroupsResult>('/api/ocr/suggest-groups', jsonRequest('POST',data));
    },
  },

  temperatures: {
    list(patientId: string) {
      return request<TemperatureRecord[]>(`/api/patients/${patientId}/temperatures`);
    },
    create(patientId: string, data: CreateTemperatureReq) {
      return request<TemperatureRecord>(`/api/patients/${patientId}/temperatures`, jsonRequest('POST',data));
    },
    delete(id: string) {
      return request<void>(`/api/temperatures/${id}`, { method: 'DELETE' });
    },
  },

  trends: {
    getItems(patientId: string) {
      return request<TrendItemInfo[]>(`/api/patients/${patientId}/trend-items`);
    },
    getData(patientId: string, itemName: string, reportType?: string) {
      return request<TrendPoint[]>(
        `/api/patients/${patientId}/trends${qs({ item_name: itemName, report_type: reportType })}`,
      );
    },
  },

  expenses: {
    parse(patientId: string, file: File, timeout = 600000) {
      const form = new FormData();
      form.append('file', file);
      return request<ExpenseParseResponse>(
        `/api/patients/${patientId}/expenses/parse`,
        { method: 'POST', body: form },
        timeout,
      );
    },
    confirm(patientId: string, data: ConfirmExpenseReq) {
      return request<DailyExpenseDetail>(
        `/api/patients/${patientId}/expenses/confirm`,
        jsonRequest('POST', data),
      );
    },
    batchConfirm(patientId: string, data: BatchConfirmExpenseReq) {
      return request<DailyExpenseDetail[]>(
        `/api/patients/${patientId}/expenses/batch-confirm`,
        jsonRequest('POST', data),
      );
    },
    list(patientId: string) {
      return request<DailyExpenseSummary[]>(`/api/patients/${patientId}/expenses`);
    },
    get(id: string) {
      return request<DailyExpenseDetail>(`/api/expenses/${id}`);
    },
    delete(id: string) {
      return request<void>(`/api/expenses/${id}`, { method: 'DELETE' });
    },
    parseChunk(file: File, timeout = 300000) {
      const form = new FormData();
      form.append('file', file);
      return request<ParsedExpenseDay[]>(
        '/api/expenses/parse-chunk',
        { method: 'POST', body: form },
        timeout,
      );
    },
    mergeChunks(data: MergeChunksReq, timeout = 300000) {
      return request<ExpenseParseResponse>(
        '/api/expenses/merge-chunks',
        jsonRequest('POST', data),
        timeout,
      );
    },
    analyze(data: AnalyzeExpenseReq) {
      return request<AnalyzeExpenseResp>(
        '/api/expenses/analyze',
        jsonRequest('POST', data),
        30000,
      );
    },
  },

  medications: {
    list(patientId: string) {
      return request<Medication[]>(`/api/patients/${patientId}/medications`);
    },
    detectedDrugs(patientId: string) {
      return request<DetectedDrug[]>(`/api/patients/${patientId}/detected-drugs`);
    },
    create(patientId: string, data: CreateMedicationReq) {
      return request<Medication>(`/api/patients/${patientId}/medications`, jsonRequest('POST', data));
    },
    update(id: string, data: UpdateMedicationReq) {
      return request<Medication>(`/api/medications/${id}`, jsonRequest('PUT', data));
    },
    delete(id: string) {
      return request<void>(`/api/medications/${id}`, { method: 'DELETE' });
    },
  },

  timeline: {
    get(patientId: string) {
      return request<TimelineEvent[]>(`/api/patients/${patientId}/timeline`);
    },
  },

  admin: {
    backfillCanonicalNames() {
      return request<{ updated: number }>('/api/admin/backfill-canonical-names', {
        method: 'POST',
      });
    },
    listUsers() {
      return request<UserInfo[]>('/api/admin/users');
    },
    updateUserRole(userId: string, role: string) {
      return request<void>(`/api/admin/users/${userId}/role`, jsonRequest('PUT', { role }));
    },
    deleteUser(userId: string) {
      return request<void>(`/api/admin/users/${userId}`, { method: 'DELETE' });
    },
    async downloadBackup() {
      const token = localStorage.getItem(TOKEN_KEY)
      const res = await fetch('/api/admin/backup', {
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
      return request<void>('/api/admin/restore', { method: 'POST', body: form }, 120000)
    },
  },

  stats: {
    criticalAlerts() {
      return request<CriticalAlert[]>('/api/stats/critical-alerts');
    },
  },

  healthAssessment: {
    getCache(patientId: string) {
      return request<{ content: HealthAssessment; created_at: string } | null>(
        `/api/patients/${patientId}/health-assessment-cache`,
      );
    },
  },
};
