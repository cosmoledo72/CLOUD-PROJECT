// Typed client for the WorkHub REST API. All requests are same-origin and
// authenticated by the HttpOnly session cookie.

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const init: RequestInit = { method, credentials: 'same-origin', headers: {} };
  if (body instanceof FormData) {
    init.body = body;
  } else if (body !== undefined) {
    init.body = JSON.stringify(body);
    (init.headers as Record<string, string>)['Content-Type'] = 'application/json';
  }
  const res = await fetch(`/api${path}`, init);
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) {
    throw new ApiError(res.status, data?.error ?? 'error', data?.message ?? res.statusText);
  }
  return data as T;
}

export const api = {
  get: <T>(p: string) => request<T>('GET', p),
  post: <T>(p: string, b?: unknown) => request<T>('POST', p, b ?? {}),
  patch: <T>(p: string, b: unknown) => request<T>('PATCH', p, b),
  put: <T>(p: string, b: unknown) => request<T>('PUT', p, b),
  del: <T>(p: string) => request<T>('DELETE', p)
};

// ---------------------------------------------------------------------------
// Models (mirror the Rust structs)
// ---------------------------------------------------------------------------

export type Access = 'None' | 'Read' | 'Comment' | 'Edit' | 'Full';
export const accessRank: Record<Access, number> = { None: 0, Read: 1, Comment: 2, Edit: 3, Full: 4 };
export const can = (a: Access | undefined, need: Access) => accessRank[a ?? 'None'] >= accessRank[need];

export interface Me {
  id: string;
  tenant_id: string;
  tenant_slug: string;
  tenant_name: string;
  email: string;
  display_name: string;
  role: 'owner' | 'admin' | 'member' | 'guest';
  totp_enabled: boolean;
  crm_role: CrmRole;
  position: string;
  phone: string;
  /** Portal modules that are on: projects, planning, time, templates, storage, kb, chats, crm, team. */
  modules: ModuleKey[];
}

export type ModuleKey = 'projects' | 'planning' | 'time' | 'templates' | 'storage' | 'kb' | 'chats' | 'crm' | 'team';

export type CrmRole = 'none' | 'user' | 'manager';

export interface Folder {
  id: string;
  parent_id: string | null;
  kind: 'my' | 'common' | 'project' | 'regular';
  name: string;
  owner_id: string;
  created_at: string;
}

export interface FileItem {
  id: string;
  folder_id: string;
  name: string;
  extension: string;
  owner_id: string;
  owner_name: string;
  current_version: number;
  size_bytes: number;
  is_editing: boolean;
  updated_at: string;
}

export interface FolderView {
  folder: Folder;
  access: Access;
  breadcrumbs: { id: string; name: string }[];
  folders: Folder[];
  files: FileItem[];
}

export interface Version {
  version: number;
  size_bytes: number;
  author_name: string | null;
  created_at: string;
}

export interface Share {
  id: string;
  user_id: string | null;
  group_id: string | null;
  principal_name: string;
  access: number;
}

export interface UserItem {
  id: string;
  email: string;
  display_name: string;
  role: string;
  is_active: boolean;
  totp_enabled: boolean;
  crm_role: CrmRole;
  /** Invited and has not set a password yet. */
  invite_pending: boolean;
  created_at: string;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  status: 'active' | 'paused' | 'closed';
  owner_id: string;
  folder_id: string | null;
  created_at: string;
  task_total: number;
  task_done: number;
  start_date: string | null;
  end_date: string | null;
  fields: FieldValues;
  auto_schedule: boolean;
  baseline_saved_at: string | null;
  company_id: string | null;
  company_name: string | null;
  deal_id: string | null;
}

export interface Member {
  user_id: string;
  display_name: string;
  email: string;
  role: 'manager' | 'member' | 'viewer';
}

export type TaskStatus = 'open' | 'in_progress' | 'review' | 'done';

export interface Task {
  id: string;
  /** Portal-wide number shown as #234. */
  number: number;
  project_id: string;
  project_name: string;
  parent_id: string | null;
  title: string;
  description: string;
  status: TaskStatus;
  priority: number;
  is_milestone: boolean;
  assignee_id: string | null;
  assignee_name: string | null;
  start_date: string | null;
  due_date: string | null;
  progress: number;
  sort_order: number;
  spent_minutes: number;
  updated_at: string;
  label_ids: string[];
  checklist_total: number;
  checklist_done: number;
  comment_count: number;
}

export const priorityLabel: Record<number, string> = { 1: 'Низкий', 2: 'Обычный', 3: 'Высокий', 4: 'Критический' };

export interface TaskLabel {
  id: string;
  name: string;
  color: string;
  sort_order: number;
  task_count: number;
}

export interface ChecklistItem {
  id: string;
  body: string;
  done: boolean;
  done_by: string | null;
  done_at: string | null;
  assignee_id: string | null;
  assignee_name: string | null;
  due_date: string | null;
  sort_order: number;
}

export interface TaskCardData {
  task: Task;
  checklist: ChecklistItem[];
  members: { user_id: string; display_name: string; role: 'manager' | 'member' | 'viewer' }[];
  /** Fields, labels and checklist may be changed (members, managers, the assignee). */
  can_edit: boolean;
  /** Project member or manager (not just the assignee): may delete and link tasks. */
  can_manage: boolean;
  created_by: string | null;
  created_by_name: string | null;
  created_at: string | null;
}

export interface BoardProject {
  id: string;
  name: string;
  status: 'active' | 'paused' | 'closed';
  code: string | null;
  role: 'manager' | 'member' | 'viewer';
}

export interface BoardResponse {
  tasks: Task[];
  dependencies: Dependency[];
  projects: BoardProject[];
  today: string;
  truncated: boolean;
}

export interface BaselineEntry {
  task_id: string;
  start_date: string | null;
  due_date: string | null;
}

export interface Schedule {
  /** Tasks with zero or negative float. */
  critical: string[];
  /** Float in days per dated task (negative = violated dependency). */
  slack: Record<string, number>;
  project_end: string | null;
}

export interface Dependency {
  task_id: string;
  depends_on_id: string;
  kind: 'FS' | 'SS' | 'FF' | 'SF';
  lag_days: number;
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`;
  const units = ['КБ', 'МБ', 'ГБ'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

export function formatDate(iso: string | null): string {
  if (!iso) return '—';
  // Date-only values (YYYY-MM-DD) are calendar dates: format without time-zone shifts.
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (m) return `${m[3]}.${m[2]}.${m[1]}`;
  return new Date(iso).toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric' });
}

export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });
}

export const statusLabel: Record<TaskStatus, string> = {
  open: 'Открыта',
  in_progress: 'В работе',
  review: 'На проверке',
  done: 'Выполнена'
};

export const roleLabel: Record<string, string> = {
  owner: 'Владелец',
  admin: 'Администратор',
  member: 'Сотрудник',
  guest: 'Гость',
  manager: 'Руководитель',
  viewer: 'Наблюдатель'
};

/** Localised display name for system root folders. */
export function folderName(name: string): string {
  return ({ 'My documents': 'Мои документы', 'Common documents': 'Общие документы' } as Record<string, string>)[name] ?? name;
}

/** File kind used for icons and the "open in editor" action. */
export function docKind(ext: string): 'word' | 'cell' | 'slide' | 'pdf' | 'other' {
  if (['doc', 'docx', 'docm', 'dotx', 'odt', 'rtf', 'txt'].includes(ext)) return 'word';
  if (['xls', 'xlsx', 'xlsm', 'ods', 'csv'].includes(ext)) return 'cell';
  if (['ppt', 'pptx', 'pptm', 'odp'].includes(ext)) return 'slide';
  if (ext === 'pdf') return 'pdf';
  return 'other';
}

// ---------------------------------------------------------------------------
// Administration
// ---------------------------------------------------------------------------

export type FieldType = 'text' | 'textarea' | 'number' | 'date' | 'select' | 'user' | 'checkbox' | 'company';

/** Value of a «company» requisite: a CRM directory entry or a typed-in name (id = null). */
export interface CompanyRef {
  id: string | null;
  name: string;
  inn: string | null;
}

export interface FieldDef {
  id: string;
  key: string;
  label: string;
  section: string;
  field_type: FieldType;
  options: string[];
  help_text: string;
  is_required: boolean;
  show_in_list: boolean;
  is_archived: boolean;
  sort_order: number;
}

export type FieldValues = Record<string, string | number | boolean | null | CompanyRef>;

/** The requisite that is the project name («Наименование объекта»); mirrors `fields::name_def` on the server. */
export function nameDef(defs: FieldDef[]): FieldDef | undefined {
  return defs.find(
    (d) =>
      !d.is_archived &&
      (d.field_type === 'text' || d.field_type === 'textarea') &&
      (d.key === 'object_name' || d.label.trim().toLowerCase() === 'наименование объекта')
  );
}

export const fieldTypeLabel: Record<FieldType, string> = {
  text: 'Строка',
  textarea: 'Многострочный текст',
  number: 'Число',
  date: 'Дата',
  select: 'Список значений',
  user: 'Сотрудник',
  checkbox: 'Флажок (да/нет)',
  company: 'Контрагент из CRM'
};

export interface AuditEntry {
  id: number;
  user_id: string | null;
  user_name: string | null;
  action: string;
  target_type: string | null;
  target_id: string | null;
  target_name: string | null;
  details: Record<string, unknown>;
  created_at: string;
}

export interface Overview {
  users_total: number;
  users_active: number;
  users_mfa: number;
  users_guests: number;
  sessions_active: number;
  files_total: number;
  storage_used: number;
  storage_quota: number;
  projects_active: number;
  projects_total: number;
  tasks_open: number;
  tasks_overdue: number;
  docs_editing: number;
}

export const actionLabel: Record<string, string> = {
  'tenant.created': 'Портал создан',
  'portal.updated': 'Изменены настройки портала',
  'auth.login': 'Вход в систему',
  'auth.login_failed': 'Неудачная попытка входа',
  'auth.mfa_failed': 'Неверный код 2FA',
  'auth.mfa_enabled': 'Включена 2FA',
  'auth.mfa_disabled': 'Отключена 2FA',
  'auth.password_changed': 'Смена пароля',
  'user.created': 'Создан пользователь',
  'user.updated': 'Изменён пользователь',
  'user.password_reset': 'Сброшен пароль пользователя',
  'user.mfa_reset': 'Сброшена 2FA пользователя',
  'user.sessions_revoked': 'Завершены сеансы пользователя',
  'group.deleted': 'Удалена группа',
  'folder.created': 'Создана папка',
  'folder.deleted': 'Удалена папка',
  'file.created': 'Создан файл',
  'file.deleted': 'Удалён файл',
  'file.saved': 'Сохранён документ',
  'file.forcesaved': 'Промежуточное сохранение',
  'share.created': 'Выдан доступ',
  'project.created': 'Создан проект',
  'project.deleted': 'Удалён проект',
  'project.requisites_updated': 'Изменены реквизиты проекта',
  'project.rescheduled': 'Сроки задач сдвинуты по связям',
  'project.baseline_saved': 'Сохранён базовый план',
  'project_fields.created': 'Добавлен реквизит',
  'project_fields.updated': 'Изменён реквизит',
  'project_fields.deleted': 'Удалён реквизит',
  'project_fields.preset_applied': 'Загружен типовой набор реквизитов',
  'branding.updated': 'Изменено оформление',
  'branding.asset_uploaded': 'Загружен логотип',
  'branding.asset_removed': 'Удалён логотип',
  'template.created': 'Добавлен шаблон документа',
  'template.updated': 'Заменён файл шаблона',
  'template.deleted': 'Удалён шаблон документа',
  'template.generated': 'Сформирован документ из шаблона',
  'trash.restored': 'Восстановлено из корзины',
  'trash.purged': 'Удалено навсегда',
  'trash.emptied': 'Корзина очищена',
  'crm.company_created': 'CRM: добавлен контрагент',
  'crm.company_updated': 'CRM: изменён контрагент',
  'crm.company_deleted': 'CRM: удалён контрагент',
  'crm.companies_imported': 'CRM: импорт контрагентов',
  'crm.deal_created': 'CRM: создана сделка',
  'crm.deal_deleted': 'CRM: удалена сделка',
  'crm.stages_updated': 'CRM: изменены этапы воронки',
  'mail.settings_updated': 'Изменены настройки почты',
  'mail.test': 'Отправлено тестовое письмо',
  'user.invited': 'Отправлено приглашение',
  'user.invite_accepted': 'Приглашение принято',
  'auth.reset_requested': 'Запрошено восстановление пароля',
  'auth.password_reset_by_link': 'Пароль восстановлен по ссылке'
};

// ---------------------------------------------------------------------------
// Mail
// ---------------------------------------------------------------------------

export type MailSecurity = 'none' | 'starttls' | 'tls';

export interface MailSettings {
  enabled: boolean;
  host: string;
  port: number;
  security: MailSecurity;
  username: string;
  has_password: boolean;
  from_email: string;
  from_name: string;
  reply_to: string;
}

export interface OutboxItem {
  id: string;
  kind: string;
  to_email: string;
  subject: string;
  status: 'queued' | 'sent' | 'failed';
  attempts: number;
  last_error: string | null;
  created_at: string;
  sent_at: string | null;
  next_attempt_at: string;
}

export const mailKindLabel: Record<string, string> = {
  invite: 'Приглашение',
  reset: 'Восстановление пароля',
  password_changed: 'Пароль изменён'
};

/** Name in «quotes» unless it already carries its own (ООО «Ромашка»). */
export function quoted(name: string): string {
  return /[«"]/.test(name) ? name : `«${name}»`;
}

/** Russian-friendly message for errors of the public auth pages. */
export function authError(e: unknown): string {
  if (!(e instanceof ApiError)) return 'Сервер недоступен';
  if (e.status === 429) return 'Слишком много запросов. Повторите позже.';
  if (/at least 10/.test(e.message)) return 'Пароль должен быть не короче 10 символов.';
  return plainMessage(e.message);
}

/** Server message without the technical prefix ("bad request: …"), capitalised. */
export function plainMessage(m: string): string {
  const s = m.replace(/^(bad request|conflict): /, '');
  return s.charAt(0).toUpperCase() + s.slice(1);
}

// --- Registration and invitations ---------------------------------------------

export type RegistrationMode = 'off' | 'request' | 'domain';

export interface RegistrationSettings {
  mode: RegistrationMode;
  domains: string[];
  role: 'member' | 'guest';
  group_ids: string[];
  mail_available: boolean;
  /** Requests waiting for a decision. */
  pending: number;
}

export interface RegistrationRequest {
  id: string;
  email: string;
  display_name: string;
  comment: string;
  status: 'unconfirmed' | 'pending' | 'approved' | 'rejected';
  email_confirmed: boolean;
  created_at: string;
  decided_by_name: string | null;
  decided_at: string | null;
  reject_reason: string;
  /** The address belongs to one of the registration domains. */
  domain_match: boolean;
}

export interface Invitation {
  id: string;
  email: string;
  display_name: string;
  role: string;
  invited_by_name: string | null;
  message: string;
  created_at: string;
  sent_at: string | null;
  expires_at: string | null;
  expired: boolean;
  is_active: boolean;
  projects: { id: string; name: string; role: string }[];
  groups: string[];
}

export interface InviteResult {
  email?: string;
  input?: string;
  display_name?: string;
  status: 'invited' | 'exists' | 'already_invited' | 'invalid';
  id?: string;
  invite_url?: string;
  emailed?: boolean;
}

export interface DocTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  extension: 'docx' | 'xlsx';
  size_bytes: number;
  placeholders: string[];
  updated_at: string;
  scope: 'project' | 'sales';
  doc_kind: DocKind | null;
}

export interface PlaceholderInfo {
  key: string;
  label: string;
  group: string;
}

export interface TrashItem {
  item_type: 'file' | 'folder';
  id: string;
  name: string;
  kind: string;
  size_bytes: number;
  deleted_at: string;
  deleted_by: string | null;
  deleted_by_name: string | null;
  location: string | null;
  purge_at: string;
  restorable: boolean;
}

/** Local calendar date as YYYY-MM-DD (for {{date}} in generated documents). */
export function localToday(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** Multipart upload helper (returns parsed JSON). */
export function upload<T>(path: string, file: File): Promise<T> {
  const fd = new FormData();
  fd.append('file', file, file.name);
  return api.post<T>(path, fd);
}

/** Human-readable value of a requisite. */
export function formatFieldValue(def: FieldDef, v: unknown, users: { id: string; display_name: string }[] = []): string {
  if (v === null || v === undefined || v === '') return '—';
  switch (def.field_type) {
    case 'number':
      return typeof v === 'number' ? v.toLocaleString('ru-RU', { maximumFractionDigits: 2 }) : String(v);
    case 'date':
      return formatDate(String(v));
    case 'checkbox':
      return v ? 'Да' : 'Нет';
    case 'user':
      return users.find((u) => u.id === v)?.display_name ?? '—';
    case 'company':
      return typeof v === 'object' ? (v as CompanyRef).name || '—' : String(v);
    default:
      return String(v);
  }
}

// ---------------------------------------------------------------------------
// CRM
// ---------------------------------------------------------------------------

export type Visibility = 'all' | 'responsible' | 'group';

export interface CompanyInput {
  name: string;
  full_name: string;
  inn: string;
  kpp: string;
  ogrn: string;
  legal_address: string;
  postal_address: string;
  phone: string;
  email: string;
  website: string;
  director: string;
  bank_name: string;
  bik: string;
  account: string;
  corr_account: string;
  notes: string;
  tags: string[];
  responsible_id: string | null;
  visibility: Visibility;
  group_id: string | null;
}

export interface Company extends Omit<CompanyInput, 'inn' | 'kpp' | 'ogrn' | 'bik' | 'account' | 'corr_account'> {
  id: string;
  inn: string | null;
  kpp: string | null;
  ogrn: string | null;
  bik: string | null;
  account: string | null;
  corr_account: string | null;
  responsible_name: string | null;
  created_by: string | null;
  created_at: string;
}

export interface CompanyListItem {
  id: string;
  name: string;
  inn: string | null;
  kpp: string | null;
  phone: string;
  email: string;
  tags: string[];
  visibility: Visibility;
  responsible_id: string | null;
  responsible_name: string | null;
  deals_open: number;
  deals_amount: number;
  projects: number;
  last_activity_at: string | null;
}

export interface Contact {
  id: string;
  company_id: string | null;
  company_name: string | null;
  last_name: string;
  first_name: string;
  middle_name: string;
  position: string;
  phone: string;
  email: string;
  notes: string;
}

export interface Stage {
  id: string;
  name: string;
  kind: 'open' | 'won' | 'lost';
  probability: number;
  color: string;
  sort_order: number;
}

export interface Deal {
  next_task_date: string | null;
  next_task_title: string | null;
  id: string;
  title: string;
  company_id: string | null;
  company_name: string | null;
  contact_id: string | null;
  contact_name: string | null;
  stage_id: string;
  stage_kind: 'open' | 'won' | 'lost';
  probability: number;
  amount: number;
  expected_close: string | null;
  responsible_id: string | null;
  responsible_name: string | null;
  project_id: string | null;
  project_name: string | null;
  description: string;
  lost_reason: string;
  sort_order: number;
  created_by: string | null;
  created_at: string;
  updated_at: string;
  closed_at: string | null;
}

export type ActivityKind = 'note' | 'call' | 'meeting' | 'email' | 'system';

export interface Activity {
  id: string;
  kind: ActivityKind;
  body: string;
  happened_at: string;
  author_id: string | null;
  author_name: string | null;
  company_id: string | null;
  deal_id: string | null;
  deal_title: string | null;
  contact_id: string | null;
  contact_name: string | null;
}

export const activityLabel: Record<ActivityKind, string> = {
  note: 'Заметка',
  call: 'Звонок',
  meeting: 'Встреча',
  email: 'Письмо',
  system: 'Событие'
};

export const crmRoleLabel: Record<CrmRole, string> = {
  none: 'Нет доступа',
  user: 'Сотрудник',
  manager: 'Руководитель продаж'
};

/** Money in roubles: 1 250 000 ₽ (kopecks shown only when present). */
export function formatMoney(n: number): string {
  return n.toLocaleString('ru-RU', { minimumFractionDigits: 0, maximumFractionDigits: 2 }) + '\u00a0₽';
}

export function contactName(c: Pick<Contact, 'last_name' | 'first_name' | 'middle_name'>): string {
  return [c.last_name, c.first_name, c.middle_name].filter(Boolean).join(' ');
}

// ---------------------------------------------------------------------------
// Notifications and comments
// ---------------------------------------------------------------------------

export interface NotificationItem {
  id: string;
  kind: string;
  title: string;
  body: string;
  link: string;
  actor_id: string | null;
  actor_name: string | null;
  created_at: string;
  read_at: string | null;
}

export interface NotificationPref {
  kind: string;
  label: string;
  group: string;
  in_app: boolean;
  email: boolean;
  default_in_app: boolean;
  default_email: boolean;
}

export interface TaskComment {
  id: string;
  author_id: string | null;
  author_name: string | null;
  body: string;
  created_at: string;
}

/** Short relative time: «только что», «5 мин назад», «вчера, 14:05», a date. */
export function timeAgo(iso: string, now = Date.now()): string {
  const t = new Date(iso);
  const sec = Math.max(0, Math.round((now - t.getTime()) / 1000));
  if (sec < 60) return 'только что';
  if (sec < 3600) return `${Math.floor(sec / 60)} мин назад`;
  const today = new Date(now);
  const sameDay = t.toDateString() === today.toDateString();
  const hm = t.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  if (sameDay) return sec < 6 * 3600 ? `${Math.floor(sec / 3600)} ч назад` : `сегодня, ${hm}`;
  const y = new Date(now);
  y.setDate(y.getDate() - 1);
  if (t.toDateString() === y.toDateString()) return `вчера, ${hm}`;
  return t.toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' }) + `, ${hm}`;
}

// ---------------------------------------------------------------------------
// CRM to-dos
// ---------------------------------------------------------------------------

export type CrmTaskKind = 'call' | 'meeting' | 'email' | 'todo';

export const crmTaskKindLabel: Record<CrmTaskKind, string> = {
  call: 'Звонок',
  meeting: 'Встреча',
  email: 'Письмо',
  todo: 'Дело'
};

export interface CrmTask {
  id: string;
  kind: CrmTaskKind;
  title: string;
  notes: string;
  due_date: string;
  due_time: string | null;
  company_id: string | null;
  company_name: string | null;
  deal_id: string | null;
  deal_title: string | null;
  contact_id: string | null;
  contact_name: string | null;
  contact_phone: string | null;
  assignee_id: string;
  assignee_name: string | null;
  created_by: string | null;
  created_at: string;
  done_at: string | null;
  done_by_name: string | null;
  result: string;
  can_edit: boolean;
}

/** «сегодня», «завтра», «вчера» or a short date, plus the time when set. */
export function taskWhen(t: Pick<CrmTask, 'due_date' | 'due_time'>, today: string): string {
  const day = (iso: string) => Date.UTC(+iso.slice(0, 4), +iso.slice(5, 7) - 1, +iso.slice(8, 10)) / 86_400_000;
  const diff = day(t.due_date) - day(today);
  const d =
    diff === 0 ? 'сегодня' : diff === 1 ? 'завтра' : diff === -1 ? 'вчера'
      : new Date(t.due_date + 'T00:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', weekday: diff > 1 && diff < 7 ? 'short' : undefined });
  return t.due_time ? `${d}, ${t.due_time.slice(0, 5)}` : d;
}

// ---------------------------------------------------------------------------
// Sales documents
// ---------------------------------------------------------------------------

export type DocKind = 'quote' | 'contract' | 'invoice' | 'act';

export const docKindLabel: Record<DocKind, string> = {
  quote: 'Коммерческое предложение',
  contract: 'Договор',
  invoice: 'Счёт',
  act: 'Акт'
};

export type DocStatus = 'draft' | 'sent' | 'signed' | 'cancelled';

export const docStatusLabel: Record<DocStatus, string> = {
  draft: 'Черновик',
  sent: 'Отправлен',
  signed: 'Подписан',
  cancelled: 'Аннулирован'
};

export interface SalesDoc {
  id: string;
  kind: DocKind;
  number: string;
  doc_date: string;
  company_id: string | null;
  company_name: string;
  deal_id: string | null;
  deal_title: string | null;
  basis_id: string | null;
  basis_number: string | null;
  subject: string;
  amount: number;
  vat_rate: string;
  vat_amount: number;
  status: DocStatus;
  file_id: string | null;
  file_name: string | null;
  created_by_name: string | null;
  created_at: string;
}

export const vatLabel = (r: string) => (r === 'none' ? 'Без НДС' : `НДС ${r}%`);

export interface OrgRequisites {
  full_name: string;
  inn: string;
  kpp: string;
  ogrn: string;
  legal_address: string;
  postal_address: string;
  phone: string;
  email: string;
  website: string;
  director: string;
  director_title: string;
  accountant: string;
  bank_name: string;
  bik: string;
  account: string;
  corr_account: string;
  vat_rate: string;
}

// ---------------------------------------------------------------------------
// Knowledge base, chats, team
// ---------------------------------------------------------------------------

export interface KbNode {
  id: string;
  parent_id: string | null;
  title: string;
  icon: string;
  sort_order: number;
  updated_at: string;
}

export interface KbPage {
  id: string;
  parent_id: string | null;
  title: string;
  icon: string;
  body: string;
  created_by: string | null;
  created_by_name: string | null;
  updated_by_name: string | null;
  created_at: string;
  updated_at: string;
}

export interface KbHit {
  id: string;
  title: string;
  icon: string;
  snippet: string;
  updated_at: string;
}

export interface ChatChannel {
  id: string;
  kind: 'project' | 'group' | 'direct';
  name: string;
  project_id: string | null;
  other_user_id: string | null;
  last_message_at: string | null;
  last_body: string | null;
  last_author: string | null;
  unread: number;
  created_by: string | null;
}

export interface TeamMember {
  id: string;
  display_name: string;
  email: string;
  role: Me['role'];
  position: string;
  phone: string;
  is_active: boolean;
  group_ids: string[];
  open_tasks: number;
  overdue_tasks: number;
  projects: number;
}

export interface TeamGroup {
  id: string;
  name: string;
  members: number;
}

/** Two-letter initials for avatars. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  return ((parts[0]?.[0] ?? '?') + (parts[1]?.[0] ?? '')).toUpperCase();
}

/** Stable pleasant hue per person for avatar backgrounds. */
export function avatarHue(id: string): number {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}
