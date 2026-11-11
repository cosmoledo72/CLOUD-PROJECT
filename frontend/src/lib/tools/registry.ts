// Tools of the «Инструменты» section. A new tool is one entry here: the
// section shows it as a tile and /tools/<slug> opens its page.

import BookCopy from '@lucide/svelte/icons/book-copy';
import Calculator from '@lucide/svelte/icons/calculator';
import DraftingCompass from '@lucide/svelte/icons/drafting-compass';
import MapPin from '@lucide/svelte/icons/map-pin';
import ScanSearch from '@lucide/svelte/icons/scan-search';
import type { Component } from 'svelte';

export type ToolStatus = 'soon' | 'beta' | 'ready';

export interface Tool {
  /** Address part: /tools/<slug>. */
  slug: string;
  group: 'estimates' | 'docs' | 'calc';
  title: string;
  /** One line on the tile. */
  summary: string;
  /** What the tool will do (the page while it is being built). */
  plan: string[];
  icon: Component;
  /** Tile colour. */
  color: string;
  status: ToolStatus;
}

export const TOOL_GROUPS: { key: Tool['group']; title: string }[] = [
  { key: 'estimates', title: 'Сметы по СБЦ' },
  { key: 'docs', title: 'Документация' },
  { key: 'calc', title: 'Расчёты' }
];

export const TOOLS: Tool[] = [
  {
    slug: 'sbc-design',
    group: 'estimates',
    title: 'Расчёт сметы по СБЦ (проектирование)',
    summary: 'Смета на проектные работы по справочникам базовых цен',
    plan: [
      'Выбор справочника базовых цен и таблиц по виду объекта',
      'Базовая цена по натуральному показателю, интерполяция и экстраполяция',
      'Корректирующие и усложняющие коэффициенты, распределение по стадиям и разделам',
      'Пересчёт в текущие цены индексом Минстроя',
      'Смета в формате DOCX/XLSX и сохранение в папку проекта'
    ],
    icon: DraftingCompass,
    color: '#2563eb',
    status: 'soon'
  },
  {
    slug: 'sbc-survey',
    group: 'estimates',
    title: 'Расчёт сметы по СБЦ (изыскания)',
    summary: 'Смета на инженерные изыскания: полевые, лабораторные и камеральные работы',
    plan: [
      'Виды изысканий: геодезические, геологические, экологические, гидрометеорологические',
      'Полевые, лабораторные и камеральные работы по таблицам СБЦ',
      'Коэффициенты на условия производства работ и районные',
      'Расходы на внутренний транспорт, организацию и ликвидацию работ',
      'Пересчёт в текущие цены и выгрузка сметы'
    ],
    icon: MapPin,
    color: '#0e7490',
    status: 'soon'
  },
  {
    slug: 'sbc-inspection',
    group: 'estimates',
    title: 'Расчёт сметы по СБЦ (обследование)',
    summary: 'Смета на обследование технического состояния зданий и сооружений',
    plan: [
      'Состав работ: визуальное и детальное (инструментальное) обследование',
      'Объём здания, категория сложности, условия работ',
      'Обмерные работы, вскрытия, лабораторные испытания',
      'Коэффициенты и пересчёт в текущие цены',
      'Смета и сопроводительное письмо из шаблонов портала'
    ],
    icon: ScanSearch,
    color: '#7e22ce',
    status: 'soon'
  },
  {
    slug: 'volume',
    group: 'docs',
    title: 'Сборка тома документации',
    summary: 'Том по ГОСТ из DOCX, PDF и изображений: штампы, нумерация, содержание',
    plan: [
      'Сбор разделов тома из файлов проекта (DOCX, PDF, изображения)',
      'Титульный лист и содержание по реквизитам проекта',
      'Рамки и штампы по ГОСТ Р 21.101, двойная нумерация листов',
      'Порядок разделов перетаскиванием, предпросмотр',
      'Готовый PDF тома в папку проекта'
    ],
    icon: BookCopy,
    color: '#b45309',
    status: 'soon'
  },
  {
    slug: 'calc',
    group: 'calc',
    title: 'Калькуляторы',
    summary: 'Быстрые инженерные и сметные расчёты',
    plan: [
      'Индексы пересчёта цен и НДС',
      'Площади, объёмы и строительный объём здания',
      'Сроки по рабочему календарю: рабочие дни между датами',
      'Другие расчёты — по заявкам сотрудников'
    ],
    icon: Calculator,
    color: '#15803d',
    status: 'soon'
  }
];

export const toolBySlug = (slug: string) => TOOLS.find((t) => t.slug === slug);

export const STATUS_LABEL: Record<ToolStatus, string> = { soon: 'Скоро', beta: 'Бета', ready: '' };
