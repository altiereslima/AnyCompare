"""Create small, deterministic, non-user fixtures for Table Compare."""
import csv
import io
from pathlib import Path
import zipfile
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1] / "tests/fixtures/tables"
ROOT.mkdir(parents=True, exist_ok=True)
LEFT = [
    ["ID", "English", "Português", "Amount"],
    ["b", 'A "quoted", value', "ação; coração", "1.00"],
    ["a", "First line\nsecond line", "olá\tmundo", "2"],
    ["a", "Duplicate key", "chave repetida", "3"],
    ["only-left", "Only here", "órfão", ""],
]
RIGHT = [
    LEFT[0], LEFT[2],
    ["b", 'A "quoted", value', "ação; CORAÇÃO", "1"],
    ["a", "Duplicate key changed", "chave repetida", "3"],
    ["only-right", "Only there", "outro órfão", ""],
]
for ext, delimiter in [("csv", ","), ("tsv", "\t"), ("tab", "\t"), ("ssv", ";"), ("psv", "|")]:
    for side, rows in [("left", LEFT), ("right", RIGHT)]:
        buffer = io.StringIO(newline="")
        csv.writer(buffer, delimiter=delimiter, lineterminator="\r\n").writerows(rows)
        (ROOT / f"{side}.{ext}").write_bytes(b"\xef\xbb\xbf" + buffer.getvalue().encode("utf-8"))

def archive(path, entries):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as file:
        for name, data in entries.items():
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            file.writestr(info, data.encode("utf-8"))

def sheet(rows, changed_formula=False):
    xml = []
    for r, row in enumerate(rows, 1):
        cells = []
        for c, value in enumerate(row):
            address = f"{chr(65+c)}{r}"
            cells.append(f'<c r="{address}" t="inlineStr"><is><t xml:space="preserve">{escape(value)}</t></is></c>')
        xml.append(f'<row r="{r}">{"".join(cells)}</row>')
    # Cached value and formula both tested; AnyCompare never recalculates it.
    xml.append(f'<row r="7"><c r="A7" t="inlineStr"><is><t>formula</t></is></c><c r="B7"><f>{"2+2" if changed_formula else "1+3"}</f><v>4</v></c></row>')
    return f'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:D7"/><sheetData>{"".join(xml)}</sheetData></worksheet>'

for side, rows in [("left", LEFT), ("right", RIGHT)]:
    archive(ROOT / f"{side}.xlsx", {
        "[Content_Types].xml": '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>',
        "_rels/.rels": '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>',
        "xl/workbook.xml": '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Translations" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>',
        "xl/_rels/workbook.xml.rels": '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>',
        "xl/worksheets/sheet1.xml": sheet(rows, side == "right"),
        "xl/worksheets/sheet2.xml": sheet([["ID", "Value"], ["sheet2", "Other worksheet"]]),
    })
    table = []
    for row in rows:
        cells = ''.join('<table:table-cell office:value-type="string"><text:p>' + escape(value).replace('\n', '<text:line-break/>').replace('\t', '<text:tab/>') + '</text:p></table:table-cell>' for value in row)
        table.append('<table:table-row>' + cells + '</table:table-row>')
    archive(ROOT / f"{side}.ods", {
        "mimetype": "application/vnd.oasis.opendocument.spreadsheet",
        "META-INF/manifest.xml": '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>',
        "content.xml": '<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2"><office:body><office:spreadsheet><table:table table:name="Translations">' + ''.join(table) + '</table:table></office:spreadsheet></office:body></office:document-content>',
    })
print("Created CSV/TSV/TAB/SSV/PSV/XLSX/ODS fixture pairs.")
