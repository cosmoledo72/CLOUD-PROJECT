// TipTap setup of the knowledge base editor: text formatting, colors,
// lists and task lists, tables, images and file cards (files stored with the
// page), links to other pages.
import { Node, mergeAttributes, type Extensions } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { TextStyle, Color, BackgroundColor } from '@tiptap/extension-text-style';
import { TableKit } from '@tiptap/extension-table';
import { TaskList, TaskItem } from '@tiptap/extension-list';
import Image from '@tiptap/extension-image';
import { Placeholder } from '@tiptap/extensions';

export interface FileCardAttrs {
  id: string;
  name: string;
  size: number;
  ext: string;
  author: string;
  date: string;
}

/** A file attached inside the page: a card with name, size, author and date. */
export const FileCard = Node.create({
  name: 'fileCard',
  group: 'block',
  atom: true,
  draggable: true,
  addAttributes() {
    const attr = (key: string) => ({
      default: '',
      parseHTML: (el: HTMLElement) => el.getAttribute(`data-${key}`) ?? '',
      renderHTML: (a: Record<string, unknown>) => ({ [`data-${key}`]: String(a[key] ?? '') })
    });
    return { id: attr('id'), name: attr('name'), size: attr('size'), ext: attr('ext'), author: attr('author'), date: attr('date') };
  },
  parseHTML() {
    return [{ tag: 'div[data-type="file-card"]' }];
  },
  renderHTML({ node, HTMLAttributes }) {
    const a = node.attrs as unknown as FileCardAttrs;
    return [
      'div',
      mergeAttributes(HTMLAttributes, { 'data-type': 'file-card', class: 'file-card' }),
      ['span', { class: 'fc-ext', 'data-ext': a.ext }, (a.ext || 'файл').slice(0, 4).toUpperCase()],
      ['a', { href: `/api/kb/files/${a.id}?download=1`, class: 'fc-name' }, a.name],
      ['span', { class: 'fc-meta' }, [sizeText(Number(a.size)), a.author, a.date].filter(Boolean).join(' · ')]
    ];
  }
});

export function sizeText(b: number): string {
  if (!b) return '';
  if (b < 1024) return `${b} Б`;
  if (b < 1024 * 1024) return `${Math.round(b / 1024)} КБ`;
  return `${(b / 1024 / 1024).toFixed(1).replace('.', ',')} МБ`;
}

export function kbExtensions(placeholder = 'Начните писать… «/» — вставить таблицу, картинку, файл'): Extensions {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
      link: { openOnClick: false, autolink: true, defaultProtocol: 'https', HTMLAttributes: { rel: 'noopener noreferrer', target: null } }
    }),
    TextStyle,
    Color,
    BackgroundColor,
    TableKit.configure({ table: { resizable: true } }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Image.configure({ inline: false, allowBase64: false }),
    FileCard,
    Placeholder.configure({ placeholder })
  ];
}

/** Palette for text and background colors (works on light and dark themes). */
export const TEXT_COLORS = ['#b42318', '#c2410c', '#a15c00', '#2f7a3d', '#1d4f7c', '#6941c6', '#8a94a6'];
export const BG_COLORS = ['#fdecc8', '#fde2e1', '#dcf2e3', '#dbe7f5', '#ece4fb', '#eef0f4'];
