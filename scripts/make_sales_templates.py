#!/usr/bin/env python3
# Usage: python3 scripts/make_sales_templates.py backend/assets/templates
"""Generate sample sales-document templates (DOCX) with {{placeholders}}."""
import sys
from docx import Document
from docx.enum.table import WD_TABLE_ALIGNMENT
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.shared import Cm, Pt

OUT = sys.argv[1]


def base():
    d = Document()
    st = d.styles["Normal"]
    st.font.name = "Arial"
    st.element.rPr.rFonts.set(qn("w:eastAsia"), "Arial")
    st.font.size = Pt(10)
    st.paragraph_format.space_after = Pt(3)
    for s in d.sections:
        s.left_margin = Cm(2)
        s.right_margin = Cm(1.5)
        s.top_margin = Cm(1.5)
        s.bottom_margin = Cm(1.5)
    return d


def para(d, text="", bold=False, size=None, align=None, after=None, italic=False):
    p = d.add_paragraph()
    if text:
        r = p.add_run(text)
        r.bold = bold
        r.italic = italic
        if size:
            r.font.size = Pt(size)
    if align:
        p.alignment = align
    if after is not None:
        p.paragraph_format.space_after = Pt(after)
    return p


def borders(table, size=4):
    tbl = table._tbl
    pr = tbl.tblPr
    b = OxmlElement("w:tblBorders")
    for edge in ("top", "left", "bottom", "right", "insideH", "insideV"):
        e = OxmlElement(f"w:{edge}")
        e.set(qn("w:val"), "single")
        e.set(qn("w:sz"), str(size))
        e.set(qn("w:color"), "000000")
        b.append(e)
    pr.append(b)


def widths(table, cm_list):
    """Fixed column widths that Word and LibreOffice both respect."""
    table.autofit = False
    tbl = table._tbl
    layout = OxmlElement("w:tblLayout")
    layout.set(qn("w:type"), "fixed")
    tbl.tblPr.append(layout)
    grid = tbl.tblGrid
    for i, gc in enumerate(grid.findall(qn("w:gridCol"))):
        gc.set(qn("w:w"), str(int(cm_list[i] * 567)))
    for row in table.rows:
        for i, c in enumerate(row.cells[: len(cm_list)]):
            c.width = Cm(cm_list[i])


def cell(c, text, bold=False, align=None, size=None):
    c.text = ""
    p = c.paragraphs[0]
    r = p.add_run(text)
    r.bold = bold
    if size:
        r.font.size = Pt(size)
    if align:
        p.alignment = align
    p.paragraph_format.space_after = Pt(0)


def items(d, unit="усл."):
    t = d.add_table(rows=2, cols=6)
    borders(t)
    heads = ["№", "Наименование работ, услуг", "Кол-во", "Ед.", "Цена, руб.", "Сумма, руб."]
    for i, h in enumerate(heads):
        cell(t.rows[0].cells[i], h, bold=True, align=WD_ALIGN_PARAGRAPH.CENTER)
    vals = ["1", "{{doc.subject}}", "1", unit, "{{doc.amount}}", "{{doc.amount}}"]
    for i, v in enumerate(vals):
        cell(t.rows[1].cells[i], v, align=WD_ALIGN_PARAGRAPH.RIGHT if i >= 4 else (WD_ALIGN_PARAGRAPH.CENTER if i in (0, 2, 3) else None))
    widths(t, [1.0, 8.4, 1.6, 1.4, 2.6, 2.6])
    totals = [("Итого:", "{{doc.amount}}"), ("{{doc.vat_text}}", "")]
    for label, value in totals:
        p = para(d, align=WD_ALIGN_PARAGRAPH.RIGHT, after=0)
        r = p.add_run(label + ("  " + value if value else ""))
        r.bold = True


def requisites_table(d, left_title, left, right_title, right):
    t = d.add_table(rows=1, cols=2)
    for col, (title, lines) in enumerate(((left_title, left), (right_title, right))):
        c = t.rows[0].cells[col]
        c.text = ""
        p = c.paragraphs[0]
        p.add_run(title).bold = True
        for line in lines:
            q = c.add_paragraph(line)
            q.paragraph_format.space_after = Pt(0)
    return t


ORG_LINES = [
    "{{org.full_name}}",
    "ИНН {{org.inn}}, КПП {{org.kpp}}",
    "ОГРН {{org.ogrn}}",
    "Адрес: {{org.legal_address}}",
    "Р/с {{org.account}}",
    "в {{org.bank_name}}",
    "БИК {{org.bik}}, к/с {{org.corr_account}}",
    "Тел.: {{org.phone}}, {{org.email}}",
]
CLIENT_LINES = [
    "{{client.full_name}}",
    "ИНН {{client.inn}}, КПП {{client.kpp}}",
    "ОГРН {{client.ogrn}}",
    "Адрес: {{client.legal_address}}",
    "Р/с {{client.account}}",
    "в {{client.bank_name}}",
    "БИК {{client.bik}}, к/с {{client.corr_account}}",
    "Тел.: {{client.phone}}, {{client.email}}",
]


def invoice():
    d = base()
    para(d, "{{org.name}}", bold=True, size=11, after=0)
    para(d, "{{org.legal_address}}, тел. {{org.phone}}", after=8)
    t = d.add_table(rows=4, cols=3)
    borders(t)
    t.rows[0].cells[0].merge(t.rows[1].cells[0])
    cell(t.rows[0].cells[0], "{{org.bank_name}}\nБанк получателя")
    cell(t.rows[0].cells[1], "БИК")
    cell(t.rows[0].cells[2], "{{org.bik}}")
    cell(t.rows[1].cells[1], "Сч. №")
    cell(t.rows[1].cells[2], "{{org.corr_account}}")
    t.rows[2].cells[0].merge(t.rows[3].cells[0])
    cell(t.rows[2].cells[0], "ИНН {{org.inn}}   КПП {{org.kpp}}\n{{org.full_name}}\nПолучатель")
    cell(t.rows[2].cells[1], "Сч. №")
    t.rows[2].cells[2].merge(t.rows[3].cells[2])
    t.rows[2].cells[1].merge(t.rows[3].cells[1])
    cell(t.rows[2].cells[2], "{{org.account}}")
    widths(t, [10.0, 1.6, 6.0])
    para(d, after=6)
    para(d, "Счёт на оплату № {{doc.number}} от {{doc.date_long}}", bold=True, size=14, after=6)
    p = para(d, after=2)
    p.add_run("Поставщик (исполнитель): ").bold = True
    p.add_run("{{org.full_name}}, ИНН {{org.inn}}, КПП {{org.kpp}}, {{org.legal_address}}")
    p = para(d, after=2)
    p.add_run("Покупатель (заказчик): ").bold = True
    p.add_run("{{client.full_name}}, ИНН {{client.inn}}, КПП {{client.kpp}}, {{client.legal_address}}")
    p = para(d, after=8)
    p.add_run("Основание: ").bold = True
    p.add_run("{{doc.basis}}")
    items(d)
    para(d, after=4)
    para(d, "Всего наименований 1, на сумму {{doc.amount}} руб.", after=0)
    para(d, "{{doc.amount_words}}", bold=True, after=10)
    para(d, "Оплата данного счёта означает согласие с условиями оказания услуг. Счёт действителен к оплате в течение 5 банковских дней.", size=8, italic=True, after=14)
    para(d, "{{org.director_title}} ______________ {{org.director}}", after=10)
    para(d, "Главный бухгалтер ______________ {{org.accountant}}", after=0)
    d.save(f"{OUT}/sales_invoice.docx")


def act():
    d = base()
    para(d, "Акт № {{doc.number}} от {{doc.date_long}}", bold=True, size=14, after=8)
    p = para(d, after=2)
    p.add_run("Исполнитель: ").bold = True
    p.add_run("{{org.full_name}}, ИНН {{org.inn}}, КПП {{org.kpp}}, {{org.legal_address}}")
    p = para(d, after=2)
    p.add_run("Заказчик: ").bold = True
    p.add_run("{{client.full_name}}, ИНН {{client.inn}}, КПП {{client.kpp}}, {{client.legal_address}}")
    p = para(d, after=8)
    p.add_run("Основание: ").bold = True
    p.add_run("{{doc.basis}}")
    items(d)
    para(d, after=4)
    para(d, "Всего оказано услуг на сумму {{doc.amount}} руб.: {{doc.amount_words}}.", after=6)
    para(d, "Вышеперечисленные работы (услуги) выполнены полностью и в срок. Заказчик претензий по объёму, качеству и срокам оказания услуг не имеет.", after=18)
    t = d.add_table(rows=1, cols=2)
    for i, (who, title, name, org) in enumerate(((
        "ИСПОЛНИТЕЛЬ", "{{org.director_title}}", "{{org.director}}", "{{org.name}}"), (
        "ЗАКАЗЧИК", "{{client.director_title}}", "{{client.director}}", "{{client.name}}"))):
        c = t.rows[0].cells[i]
        c.text = ""
        c.paragraphs[0].add_run(who).bold = True
        c.add_paragraph(org)
        c.add_paragraph(title)
        c.add_paragraph("")
        c.add_paragraph("______________ / " + name + " /")
        c.add_paragraph("М.П.")
    d.save(f"{OUT}/sales_act.docx")


def quote():
    d = base()
    st = d.styles["Normal"]
    st.font.size = Pt(11)
    para(d, "{{org.name}}", bold=True, size=13, after=0)
    para(d, "{{org.legal_address}} · {{org.phone}} · {{org.email}} · {{org.website}}", size=8, after=2)
    para(d, "ИНН {{org.inn}} · ОГРН {{org.ogrn}}", size=8, after=14)
    p = para(d, align=WD_ALIGN_PARAGRAPH.RIGHT, after=0)
    p.add_run("{{client.director_title}}\n{{client.name}}\n{{client.director}}")
    para(d, "Исх. № {{doc.number}} от {{doc.date}}", after=14)
    para(d, "Коммерческое предложение", bold=True, size=15, align=WD_ALIGN_PARAGRAPH.CENTER, after=10)
    para(d, "Уважаемый(ая) {{contact.name}}!", after=6)
    para(d, "{{org.name}} благодарит вас за интерес к нашим услугам и предлагает выполнить следующие работы: {{doc.subject}}.", after=6)
    t = d.add_table(rows=2, cols=3)
    borders(t)
    for i, h in enumerate(["Наименование работ", "Срок выполнения", "Стоимость, руб."]):
        cell(t.rows[0].cells[i], h, bold=True, align=WD_ALIGN_PARAGRAPH.CENTER)
    cell(t.rows[1].cells[0], "{{doc.subject}}")
    cell(t.rows[1].cells[1], "по согласованию")
    cell(t.rows[1].cells[2], "{{doc.amount}}", align=WD_ALIGN_PARAGRAPH.RIGHT)
    widths(t, [10.0, 4.0, 3.6])
    para(d, after=4)
    para(d, "Итого: {{doc.amount}} руб. ({{doc.amount_words}}). {{doc.vat_text}}.", after=6)
    para(d, "Условия оплаты: аванс 30% по договору, окончательный расчёт — после подписания акта выполненных работ.", after=2)
    para(d, "Предложение действительно 30 дней с даты письма.", after=14)
    para(d, "Контактное лицо: {{deal.responsible}}, {{deal.responsible_email}}.", after=18)
    para(d, "{{org.director_title}}  ______________  {{org.director}}", after=0)
    d.save(f"{OUT}/sales_quote.docx")


def contract():
    d = base()
    st = d.styles["Normal"]
    st.font.size = Pt(10)
    para(d, "ДОГОВОР ПОДРЯДА № {{doc.number}}", bold=True, size=13, align=WD_ALIGN_PARAGRAPH.CENTER, after=6)
    t = d.add_table(rows=1, cols=2)
    cell(t.rows[0].cells[0], "г. ____________")
    cell(t.rows[0].cells[1], "{{doc.date_long}}", align=WD_ALIGN_PARAGRAPH.RIGHT)
    para(d, after=6)
    para(d, "{{client.full_name}}, именуемое в дальнейшем «Заказчик», в лице {{client.director_title}} {{client.director}}, действующего на основании Устава, с одной стороны, и {{org.full_name}}, именуемое в дальнейшем «Подрядчик», в лице {{org.director_title}} {{org.director}}, действующего на основании Устава, с другой стороны, заключили настоящий договор о нижеследующем.", after=8)
    sections = [
        ("1. Предмет договора", [
            "1.1. Подрядчик обязуется выполнить работы: {{doc.subject}}, а Заказчик — принять и оплатить их.",
            "1.2. Объём и состав работ определяются техническим заданием (Приложение № 1).",
        ]),
        ("2. Цена и порядок расчётов", [
            "2.1. Цена работ составляет {{doc.amount}} руб. ({{doc.amount_words}}). {{doc.vat_text}}.",
            "2.2. Заказчик перечисляет аванс 30% цены в течение 5 банковских дней со дня подписания договора. Окончательный расчёт — в течение 10 банковских дней после подписания акта выполненных работ.",
        ]),
        ("3. Сроки выполнения работ", [
            "3.1. Начало работ — в течение 5 рабочих дней с даты поступления аванса. Окончание — {{deal.expected_close}}.",
        ]),
        ("4. Сдача и приёмка работ", [
            "4.1. Результат работ передаётся по акту выполненных работ. Заказчик в течение 5 рабочих дней подписывает акт или направляет мотивированный отказ.",
        ]),
        ("5. Ответственность сторон", [
            "5.1. За нарушение сроков стороны несут ответственность в соответствии с законодательством Российской Федерации.",
        ]),
        ("6. Прочие условия", [
            "6.1. Договор вступает в силу с даты подписания и действует до полного исполнения обязательств.",
            "6.2. Споры разрешаются в арбитражном суде по месту нахождения истца.",
        ]),
    ]
    for title, lines in sections:
        para(d, title, bold=True, after=2)
        for line in lines:
            p = para(d, line, after=2)
            p.alignment = WD_ALIGN_PARAGRAPH.JUSTIFY
    para(d, after=4)
    para(d, "7. Реквизиты и подписи сторон", bold=True, after=4)
    t = requisites_table(d, "Заказчик", CLIENT_LINES + ["", "______________ / {{client.director}} /"],
                         "Подрядчик", ORG_LINES + ["", "______________ / {{org.director}} /"])
    d.save(f"{OUT}/sales_contract.docx")


invoice()
act()
quote()
contract()
print("ok")
