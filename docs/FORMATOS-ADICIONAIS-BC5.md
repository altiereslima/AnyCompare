# Formatos adicionais do BC5 para Windows — revisão de 05/11/2026

**Confirmado na documentação oficial:** o [catálogo de formatos adicionais](https://www.scootersoftware.com/kb/moreformats?version=5&platform=Windows) oferece definições importáveis. Algumas substituem o formato padrão conforme a ordem escolhida. Várias entradas são variantes do mesmo formato; o nome da extensão não basta para reproduzir o comportamento de cada uma.

**Confirmado no código do AnyCompare:** as definições de terceiros ainda não são importadas. A comparação de um arquivo como texto comum não comprova suporte às regras, à sintaxe ou à conversão de um formato específico.

| Família | O que existe | O que falta |
|---|---|---|
| CSV | Comparação tabular; comparação textual explícita possível | Definição importável e prioridade configurável de formatos |
| HTML | Várias tabelas `.htm`/`.html`, somente leitura | Extensões ASP relacionadas, normalização do HTML e extração em texto como modos distintos |
| Excel e OpenOffice | Leitura de várias folhas Excel e ODS; valores e fórmulas armazenadas | Conversões para texto/XML/VBA, outros documentos Office/OpenOffice, DBF e as variantes com processamento externo |
| Ada, Bash, Batch, CSS, ColdFusion, JavaScript, JSON, Lua, Matlab, PHP, RC, Verilog, VHDL | Comparação como texto quando o documento é textual | Gramáticas e regras específicas |
| INI, JSON e XML com normalização | Texto sem normalização | Conversores e critérios de ordenação/formatação por variante |
| Java | Comparação textual do fonte | Descompilação de `.class` e formatação Java |
| Access | Sem leitor específico | Comparação de tabelas e extração textual |
| Word e PowerPoint | Sem conversores específicos | Extração de conteúdo e, separadamente, de módulos VBA |
| PDF, PalmDOC e WordPerfect | Sem conversão específica | Extratores de texto de cada formato |
| Metadados/cabeçalhos de imagens | Sem comparação específica | Leitura de metadados e relatórios, separados de Comparar Imagens |

A próxima etapa é implementar regras de seleção por formato e testar cada saída com fixtures. Importação, conversão e execução de programas auxiliares são capacidades distintas; nenhuma foi habilitada apenas com base neste catálogo.
