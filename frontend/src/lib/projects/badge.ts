// Project badge: a coloured square with the first letter of the project name
// or an icon. Colour and icon may be chosen by the project manager; otherwise
// the colour is derived from the project id and the letter from the name.

import Anchor from '@lucide/svelte/icons/anchor';
import Bridge from '@lucide/svelte/icons/bridge';
import Building from '@lucide/svelte/icons/building';
import Church from '@lucide/svelte/icons/church';
import DraftingCompass from '@lucide/svelte/icons/drafting-compass';
import Droplets from '@lucide/svelte/icons/droplets';
import Factory from '@lucide/svelte/icons/factory';
import Flame from '@lucide/svelte/icons/flame';
import HardHat from '@lucide/svelte/icons/hard-hat';
import Hospital from '@lucide/svelte/icons/hospital';
import House from '@lucide/svelte/icons/house';
import Landmark from '@lucide/svelte/icons/landmark';
import MapPin from '@lucide/svelte/icons/map-pin';
import RadioTower from '@lucide/svelte/icons/radio-tower';
import Ruler from '@lucide/svelte/icons/ruler';
import School from '@lucide/svelte/icons/school';
import ShieldCheck from '@lucide/svelte/icons/shield-check';
import Ship from '@lucide/svelte/icons/ship';
import Store from '@lucide/svelte/icons/store';
import TrainFront from '@lucide/svelte/icons/train-front';
import Trees from '@lucide/svelte/icons/trees';
import Warehouse from '@lucide/svelte/icons/warehouse';
import Zap from '@lucide/svelte/icons/zap';
import type { Component } from 'svelte';

/** Colours offered for a badge; white text is readable on each. */
export const BADGE_COLORS = [
  '#1f4e79', '#2563eb', '#0e7490', '#0f766e', '#15803d', '#a16207', '#c2410c', '#b91c1c', '#be185d', '#7e22ce', '#4f46e5', '#475569'
];

/** Icons offered for a badge: key (stored on the project) → icon and label. */
export const BADGE_ICONS: Record<string, { icon: Component; label: string }> = {
  building: { icon: Building, label: 'Здание' },
  house: { icon: House, label: 'Жилой дом' },
  factory: { icon: Factory, label: 'Производство' },
  warehouse: { icon: Warehouse, label: 'Склад' },
  store: { icon: Store, label: 'Торговля' },
  school: { icon: School, label: 'Школа' },
  hospital: { icon: Hospital, label: 'Больница' },
  landmark: { icon: Landmark, label: 'Памятник' },
  church: { icon: Church, label: 'Храм' },
  bridge: { icon: Bridge, label: 'Мост' },
  ship: { icon: Ship, label: 'Порт' },
  anchor: { icon: Anchor, label: 'Гидротехника' },
  'train-front': { icon: TrainFront, label: 'Железная дорога' },
  zap: { icon: Zap, label: 'Энергетика' },
  flame: { icon: Flame, label: 'Газ и тепло' },
  droplets: { icon: Droplets, label: 'Вода' },
  'radio-tower': { icon: RadioTower, label: 'Связь' },
  trees: { icon: Trees, label: 'Благоустройство' },
  'hard-hat': { icon: HardHat, label: 'Стройконтроль' },
  ruler: { icon: Ruler, label: 'Обмеры' },
  'drafting-compass': { icon: DraftingCompass, label: 'Проектирование' },
  'map-pin': { icon: MapPin, label: 'Изыскания' },
  'shield-check': { icon: ShieldCheck, label: 'Экспертиза' }
};

/** Stable colour for a project without its own. */
export function autoBadgeColor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return BADGE_COLORS[h % BADGE_COLORS.length];
}

/** First letter or digit of the name, upper-cased. */
export function badgeLetter(name: string): string {
  const m = name.match(/[\p{L}\p{N}]/u);
  return m ? m[0].toUpperCase() : '?';
}
