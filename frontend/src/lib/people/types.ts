// Employee card (GET /api/users/{id}/profile).
export type UserFieldType = 'text' | 'textarea' | 'date' | 'number';
export interface UserFieldDef { id: string; label: string; field_type: UserFieldType }
export interface CustomValue extends UserFieldDef { value: string }

export interface ProfileUser {
  id: string;
  email: string;
  display_name: string;
  role: 'owner' | 'admin' | 'member' | 'guest';
  is_active: boolean;
  position: string;
  department: string;
  phone: string;
  phone_mobile: string;
  telegram: string;
  last_name: string;
  first_name: string;
  middle_name: string;
  manager_id: string | null;
  manager_name: string | null;
  hired_on: string | null;
  /** Day and month («14.03»). */
  birthday: string | null;
  avatar_at: string | null;
  last_seen_at: string | null;
  created_at: string;
  crm_role: 'none' | 'user' | 'manager';
  finance_role: 'none' | 'view' | 'edit';
  totp_enabled: boolean;
  invite_pending: boolean;
  // The person and administrators only:
  birth_date?: string | null;
  doc_short_name?: string;
  doc_position?: string;
  name_genitive?: string;
  name_dative?: string;
  nrs_number?: string;
  qual_cert?: string;
  power_of_attorney?: string;
  signature_at?: string | null;
  // The person, administrators and finance:
  hour_rate?: number | null;
}

export interface Profile {
  user: ProfileUser;
  can_edit: boolean;
  can_admin: boolean;
  rate_visible: boolean;
  groups: { id: string; name: string }[];
  projects: { id: string; name: string; status: 'active' | 'paused' | 'closed'; role: 'manager' | 'member' | 'viewer' }[];
  stats: { open_tasks: number; overdue_tasks: number; hours_month: number | null };
  custom: CustomValue[];
}
