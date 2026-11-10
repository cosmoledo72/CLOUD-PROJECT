// Helpers to convert between the company card and the editable form model.
import type { Company, CompanyInput } from '../api';

export function emptyCompany(responsibleId: string | null): CompanyInput {
  return {
    name: '', full_name: '', inn: '', kpp: '', ogrn: '', legal_address: '', postal_address: '', phone: '', email: '',
    website: '', director: '', bank_name: '', bik: '', account: '', corr_account: '', notes: '', tags: [],
    responsible_id: responsibleId, visibility: 'all', group_id: null
  };
}

export function toInput(c: Company): CompanyInput {
  return {
    name: c.name, full_name: c.full_name, inn: c.inn ?? '', kpp: c.kpp ?? '', ogrn: c.ogrn ?? '',
    legal_address: c.legal_address, postal_address: c.postal_address, phone: c.phone, email: c.email,
    website: c.website, director: c.director, bank_name: c.bank_name, bik: c.bik ?? '', account: c.account ?? '',
    corr_account: c.corr_account ?? '', notes: c.notes, tags: [...c.tags], responsible_id: c.responsible_id,
    visibility: c.visibility, group_id: c.group_id
  };
}

/** Empty strings become null for the code fields the API validates. */
export function toPayload(v: CompanyInput) {
  const code = (s: string) => (s.trim() ? s : null);
  return {
    ...v,
    inn: code(v.inn), kpp: code(v.kpp), ogrn: code(v.ogrn), bik: code(v.bik),
    account: code(v.account), corr_account: code(v.corr_account)
  };
}
