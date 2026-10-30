"""Add the reviewed encoding workflow translations to both shipped catalogs."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
translations = {
    'Mais opções de pesquisa': 'More Search Options',
    'Inserir elemento de expressão regular': 'Insert Regular Expression Element',
    'Tabulação (\\t)': 'Tab Character (\\t)',
    'Qualquer caractere (.)': 'Any Character (.)',
    'Caractere alfanumérico (\\w)': 'Any Alphanumeric Character (\\w)',
    'Caractere não alfanumérico (\\W)': 'Any Non-alphanumeric Character (\\W)',
    'Dígito (\\d)': 'Any Numeric Character (\\d)',
    'Caractere não numérico (\\D)': 'Any Non-numeric Character (\\D)',
    'Espaço em branco (\\s)': 'Any Whitespace Character (\\s)',
    'Caractere no conjunto ([])': 'Character in Set ([])',
    'Caractere fora do conjunto ([^])': 'Character Not in Set ([^])',
    'Início da linha (^)': 'Beginning of Line (^)',
    'Fim da linha ($)': 'End of Line ($)',
    'Grupo de captura (())': 'Tagged Expression (())',
    'Ou (|)': 'Or (|)',
    'Zero ou mais (*)': '0 Or More Matches (*)',
    'Um ou mais (+)': '1 Or More Matches (+)',
    'Zero ou um (?)': '0 Or 1 Matches (?)',
    'Recolher intervalo filtrado': 'Collapse Filtered Gap',
    'Expandir intervalo filtrado': 'Expand Filtered Gap',
    'Mostrar contagem de linhas filtradas': 'Show Filtered Line Counts',
    'UTF-16 LE (sem BOM)': 'UTF-16 LE (no BOM)',
    'UTF-16 BE (sem BOM)': 'UTF-16 BE (no BOM)',
    'Misto': 'Mixed',
    'Reabrir com codificação': 'Reopen with Encoding',
    'Detectar automaticamente': 'Auto Detect',
    'Erro de codificação': 'Encoding Error',
    'Salvar': 'Save',
    'Este arquivo contém caracteres Unicode que seriam perdidos com a codificação atual. Selecione uma codificação para preservar o texto.':
        'This file contains Unicode characters that would be lost with the current encoding. Select an encoding to preserve the text.',
    'UTF-32 não é suportado. Converta o arquivo para UTF-8 ou UTF-16 antes de abrir.':
        'UTF-32 is not supported. Convert the file to UTF-8 or UTF-16 before opening.',
    'O arquivo contém UTF-8 inválido.': 'The file contains invalid UTF-8.',
    'O arquivo contém UTF-16 inválido.': 'The file contains invalid UTF-16.',
    'Arquivo UTF-16 incompleto.': 'Incomplete UTF-16 file.',
    'O arquivo contém dados incompatíveis com Windows-1252.': 'The file contains data incompatible with Windows-1252.',
    'Este arquivo contém dados binários e não pode ser aberto como texto.': 'This file contains binary data and cannot be opened as text.',
    'Este arquivo contém dados binários ou uma codificação não reconhecida.': 'This file contains binary data or an unrecognized encoding.',
    'O texto contém caracteres que não cabem em ANSI. Selecione UTF-8 antes de salvar.': 'The text contains characters that ANSI cannot represent. Select UTF-8 before saving.',
}
for locale in ('en', 'pt-BR'):
    path = root / 'assets' / 'locales' / f'{locale}.json'
    data = json.loads(path.read_text(encoding='utf-8'))
    data['translations'].update(translations if locale == 'en' else {k: k for k in translations})
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
