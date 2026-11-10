// Counter of my CRM to-dos that are overdue or due today (menu badges).

import { api } from '../api';

class CrmDue {
  overdue = $state(0);
  today = $state(0);
  get count() {
    return this.overdue + this.today;
  }

  async refresh() {
    try {
      const r = await api.get<{ overdue: number; today: number }>('/crm/tasks/summary');
      this.overdue = r.overdue;
      this.today = r.today;
    } catch {
      /* no CRM access or offline: keep the last value */
    }
  }

  reset() {
    this.overdue = 0;
    this.today = 0;
  }
}

export const crmDue = new CrmDue();
