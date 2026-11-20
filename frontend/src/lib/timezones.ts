// Time zones offered for a portal: Russian zones first, then everything the browser knows.

export const ruZones: [string, string][] = [
  ['Europe/Kaliningrad', 'Калининград (UTC+2)'], ['Europe/Moscow', 'Москва (UTC+3)'], ['Europe/Samara', 'Самара (UTC+4)'],
  ['Asia/Yekaterinburg', 'Екатеринбург (UTC+5)'], ['Asia/Omsk', 'Омск (UTC+6)'], ['Asia/Novosibirsk', 'Новосибирск (UTC+7)'],
  ['Asia/Krasnoyarsk', 'Красноярск (UTC+7)'], ['Asia/Irkutsk', 'Иркутск (UTC+8)'], ['Asia/Yakutsk', 'Якутск (UTC+9)'],
  ['Asia/Vladivostok', 'Владивосток (UTC+10)'], ['Asia/Sakhalin', 'Сахалин (UTC+11)'], ['Asia/Magadan', 'Магадан (UTC+11)'],
  ['Asia/Kamchatka', 'Камчатка (UTC+12)']
];

/** Other IANA zones of the browser, without the Russian ones above. */
export function otherZones(): string[] {
  try {
    const known = new Set(ruZones.map(([z]) => z));
    return (Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf('timeZone').filter((z) => !known.has(z));
  } catch {
    return [];
  }
}

/** «Екатеринбург (UTC+5)» for a Russian zone, the IANA name otherwise. */
export function zoneLabel(zone: string): string {
  return ruZones.find(([z]) => z === zone)?.[1] ?? zone;
}
