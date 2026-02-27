// API response wrapper
export interface ApiResponse<T> {
  success: boolean;
  data: T | null;
  message: string;
}

export interface PaginatedList<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
}

export type InterpretationContent = string | { points: string[] } | string[];

export interface InterpretationCache {
  content: InterpretationContent;
  created_at: string;
}

// --- Patient ---

export interface Patient {
  id: string;
  name: string;
  gender: '男' | '女';
  dob: string;
  phone: string;
  id_number: string;
  notes: string;
  created_at: string;
  updated_at: string;
}

export interface PatientWithStats extends Patient {
  report_count: number;
  last_report_date: string;
  total_abnormal: number;
}

export interface PatientReq {
  name: string;
  gender: '男' | '女';
  dob?: string;
  phone: string;
  id_number: string;
  notes?: string;
}

// --- Report ---

export interface Report {
  id: string;
  patient_id: string;
  report_type: string;
  hospital: string;
  report_date: string;
  sample_date: string;
  file_path: string;
  created_at: string;
}

export interface ReportDetail extends Report {
  test_items: TestItem[];
}

export interface ReportSummary extends Report {
  item_count: number;
  abnormal_count: number;
  abnormal_names: string[];
}

export interface CreateReportReq {
  report_type: string;
  hospital: string;
  report_date: string;
  sample_date?: string;
  file_path?: string;
}

export interface UpdateReportReq {
  report_type?: string;
  hospital?: string;
  report_date?: string;
  sample_date?: string;
}

// --- Test Item ---

export type ItemStatus = 'critical_high' | 'high' | 'normal' | 'low' | 'critical_low';

export interface TestItem {
  id: string;
  report_id: string;
  name: string;
  value: string;
  unit: string;
  reference_range: string;
  status: ItemStatus;
  canonical_name: string;
}

export interface CreateTestItemReq {
  report_id: string;
  name: string;
  value: string;
  unit: string;
  reference_range: string;
  status: ItemStatus;
}

// --- OCR ---

export interface FileUploadResult {
  file_id: string;
  url: string;
  original_name: string;
  mime_type: string;
  size: number;
}

export interface OcrParseResult {
  file_id: string;
  file_path: string;
  file_name: string;
  parsed: ParsedReport;
}

export interface ParsedReport {
  report_type: string;
  hospital: string;
  report_date: string;
  sample_date: string;
  items: ParsedItem[];
}

export interface ParsedItem {
  name: string;
  value: string;
  unit: string;
  reference_range: string;
  status: ItemStatus;
}

// --- Suggest Groups ---

export interface SuggestGroupsReq {
  patient_id?: string;
  files: SuggestGroupFile[];
}

export interface SuggestGroupFile {
  file_name: string;
  report_type: string;
  report_date: string;
  sample_date: string;
  item_names: string[];
}

export interface SuggestGroupsResult {
  groups: number[];
  existing_merges: ExistingMerge[];
}

export interface ExistingMerge {
  file_index: number;
  report_id: string;
  report_type: string;
  report_date: string;
}

// --- Batch Confirm ---

export interface BatchConfirmReq {
  reports: BatchReportInput[];
  prefetched_name_map?: Record<string, string>;
  skip_merge_check?: boolean;
}

export interface BatchReportInput {
  existing_report_id?: string;
  report_type: string;
  hospital: string;
  report_date: string;
  sample_date: string;
  file_paths: string[];
  items: ParsedItem[];
}

// --- Merge Check ---

export interface MergeCheckResult {
  merges: MergeInfo[];
}

export interface MergeInfo {
  input_index: number;
  existing_report_id: string;
  existing_report_type: string;
}

// --- Temperature ---

export interface TemperatureRecord {
  id: string;
  patient_id: string;
  recorded_at: string;
  value: number;
  location: string;
  note: string;
  created_at: string;
}

export interface CreateTemperatureReq {
  recorded_at: string;
  value: number;
  location?: string;
  note?: string;
}

// --- Edit Log ---

export interface FieldChange {
  field: string;
  old_value: string;
  new_value: string;
}

export interface EditLog {
  id: string;
  report_id: string;
  patient_id: string;
  action: 'create' | 'update' | 'delete';
  target_type: 'report' | 'test_item';
  target_id: string;
  summary: string;
  changes: FieldChange[];
  created_at: string;
  operator_id?: string | null;
  operator_name?: string | null;
}

export interface UpdateTestItemReq {
  name?: string;
  value?: string;
  unit?: string;
  reference_range?: string;
  status?: ItemStatus;
}

// --- Expense ---

export type ExpenseCategory = 'drug' | 'test' | 'treatment' | 'material' | 'nursing' | 'other';

export interface DailyExpense {
  id: string;
  patient_id: string;
  expense_date: string;
  total_amount: number;
  drug_analysis: string;
  treatment_analysis: string;
  created_at: string;
}

export interface ExpenseItem {
  id: string;
  expense_id: string;
  name: string;
  category: ExpenseCategory;
  quantity: string;
  amount: number;
  note: string;
}

export interface DailyExpenseDetail extends DailyExpense {
  items: ExpenseItem[];
}

export interface DailyExpenseSummary extends DailyExpense {
  item_count: number;
  drug_count: number;
  test_count: number;
  treatment_count: number;
}

export interface ParsedExpenseItem {
  name: string;
  category: string;
  quantity: string;
  amount: number;
  note: string;
}

export interface ParsedExpenseDay {
  expense_date: string;
  total_amount: number;
  items: ParsedExpenseItem[];
}

export interface DayParseResult {
  parsed: ParsedExpenseDay;
  drug_analysis: string;
  treatment_analysis: string;
}

export interface ExpenseParseResponse {
  days: DayParseResult[];
}

export interface ConfirmExpenseReq {
  expense_date: string;
  total_amount: number;
  drug_analysis: string;
  treatment_analysis: string;
  items: {
    name: string;
    category: ExpenseCategory;
    quantity: string;
    amount: number;
    note: string;
  }[];
}

export interface BatchConfirmExpenseReq {
  days: ConfirmExpenseReq[];
}

// --- Expense Chunk Parsing ---

export interface ExpenseChunkResult {
  chunk_index: number;
  days: ParsedExpenseDay[];
}

export interface MergeChunksReq {
  chunks: ExpenseChunkResult[];
}

// --- Expense Analysis ---

export interface AnalyzeExpenseReq {
  items: ParsedExpenseItem[];
}

export interface AnalyzeExpenseResp {
  drug_analysis: string;
  treatment_analysis: string;
}

// --- Medication ---

export interface Medication {
  id: string;
  patient_id: string;
  name: string;
  dosage: string;
  frequency: string;
  start_date: string;
  end_date?: string;
  note: string;
  active: boolean;
  created_at: string;
}

export interface CreateMedicationReq {
  name: string;
  dosage: string;
  frequency: string;
  start_date: string;
  end_date?: string;
  note?: string;
}

export interface UpdateMedicationReq {
  name?: string;
  dosage?: string;
  frequency?: string;
  start_date?: string;
  end_date?: string;
  note?: string;
  active?: boolean;
}

// --- Detected Drug (from expenses) ---

export interface DetectedDrug {
  name: string;
  first_date: string;
  last_date: string;
  occurrence_count: number;
  typical_quantity: string;
  dates: string[];
}

// --- Timeline ---

export interface TimelineEvent {
  event_type: 'report' | 'temperature' | 'expense' | 'medication';
  event_date: string;
  title: string;
  description: string;
  related_id: string;
  created_at: string;
}

// --- Health Assessment ---

export interface HealthAssessment {
  overall_status: string;
  risk_level: string;
  summary: string;
  findings: string[];
  recommendations: string[];
  follow_up_suggestions: string[];
  disclaimer: string;
}

// --- Admin ---

export interface UserInfo {
  id: string;
  username: string;
  role: string;
  created_at: string;
}

// --- Critical Alerts ---

export interface CriticalAlert {
  patient_id: string;
  patient_name: string;
  report_id: string;
  report_type: string;
  report_date: string;
  item_name: string;
  value: string;
  unit: string;
  reference_range: string;
  status: 'CriticalHigh' | 'CriticalLow';
}

// --- Device Session ---

export interface DeviceSession {
  id: string;
  device_name: string;
  device_type: string;
  ip_address: string;
  created_at: string;
  last_used_at: string;
}

// --- Trends ---

export interface TrendPoint {
  report_date: string;
  sample_date: string;
  value: string;
  unit: string;
  status: ItemStatus;
  reference_range: string;
}

export interface TrendItemInfo {
  report_type: string;
  item_name: string;
  count: number;
}
