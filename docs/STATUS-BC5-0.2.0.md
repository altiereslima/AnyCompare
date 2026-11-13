# AnyCompare 0.2.0 — Tabelas, Versões e início da migração BC5

Entrega local de 02/11/2026. O objetivo vigente é equivalência funcional e visual
com o BC5.2.7.32913. **O aplicativo completo ainda não é 1:1.**

## Executável validado

- `dist/AnyCompare-0.2.0.exe` e `dist/AnyCompare.exe`.
- 23.119.360 bytes.
- SHA-256 `7286c58242c959e0756cf29086bc134a20940d9d2b25ee4321abc231ab33ff6f`.
- 136 testes aprovados, quatro testes manuais ignorados na suíte padrão; clippy
  sem avisos e compilação release concluída.
- 81 inicializações com capturas, saída 0 e stderr vazio; verificações anteriores
  de divisórias, geometria e popup preservadas. Ler `validation/0.2.0`.

## CONFIRMADO EM EXECUÇÃO: correções desta entrega

- Históricos antigos que gravavam CSV como Texto passam a abrir Tabelas. Registros
  novos preservam uma escolha explícita por Texto/Hex. Um teste abre o histórico
  antigo mantendo outra aba de Texto com alterações não salvas.
- Tabelas novas mantêm a primeira linha como dados e a ordem do arquivo. Chaves e
  nomes de colunas continuam configuráveis; nenhuma chave é imposta à coluna A.
- Alinhamento sem chave reconhece registros inseridos/excluídos. Copiar uma linha
  ausente insere na posição alinhada, em vez de acrescentar ao final.
- Letras A/B/C, numeração lógica de registros, miniatura, status por painel,
  rolagem sincronizada, linhas ausentes hachuradas, divisórias e detalhes dos dois
  registros numa grade inferior. A área de detalhes pode ser redimensionada.
- Selecionar uma célula longa não oculta colunas que já estavam visíveis. Busca
  percorre todas as colunas, inclusive as que estão fora da tela.
- A barra de caminhos foi medida pela UIA do Craft 0.3.0: menu 25 px, dock 42 px,
  miniatura 43 px, separador 3 px, campo 23 px de altura, botão carregar 23 px e
  pasta com menu 34 px. O teste confere as coordenadas dos dois lados contra as
  medidas nativas num cliente de 1.443 px. Salvar não aparece nos formatos somente
  leitura; nos delimitados editáveis, só é habilitado quando há alterações.
- A seta de carregar aplica um caminho digitado; pasta abre o seletor; seu menu
  oferece arquivos recentes e Mostrar no Explorador. As listas usam o histórico
  próprio. Nenhuma configuração/histórico do BC5 instalado foi importado.
- Excel/ODS com várias planilhas e HTML com várias tabelas têm abas inferiores,
  indicadores, pareamento por ordem/nome esquerdo/nome direito e pares órfãos.
  A troca de abas foi testada por clique real na UI egui, não só pelo carregador.
- Metadados incluem tamanho/data, formato, codificação, separador e ordem regional
  de datas. Quebras dentro de campos entre aspas não tornam o separador de
  registros “Misto”. Reabrir/Salvar usando codificação são ações diferentes.
- Prova nativa controlada: `1.00`/`1` e diferenças de caixa são importantes por
  padrão; `01/02/2024`/`1/2/2024` são datas iguais. Precisão decimal não usa f64.
  Datas são normalizadas para comparação/exibição, sem reescrever o documento.
- Global.csv: 1.010 registros; Missions.csv: 537; 12 colunas em ambos, zero
  diferenças nos pares fornecidos. Os CSVs e as DLLs do usuário permanecem intactos.
- Comparar Versões abre DLL/EXE PE32/PE32+ e mostra Código, Cabeçalho PE, Fixo,
  Tradução e Variável. A DLL vcruntime140 foi confrontada com relatório nativo.

## Limites e próximos passos

O teste de coordenadas **não comprova igualdade de todos os pixels**. Tipografia
monocromática versus ClearType RGB, glifos dos controles, cores e todos os estados
dos menus continuam na auditoria. Parte dos ícones ativos ainda vem do BC4.

Excel/ODS/HTML são somente leitura; não existe gravação de planilhas com estilos,
macros ou recálculo. Faltam seleção de intervalos, editor direto na célula,
configuração independente por planilha/coluna e alinhamento manual livre. Datas
abreviadas de dois dígitos e formatos regionais não testados não são inferidos.
O alinhamento de registros usa âncoras exatas normalizadas e pareamento dos trechos
alterados; ainda não reproduz todas as heurísticas nativas.

Word Wrap, tema escuro e os demais módulos/novidades continuam marcados em
`BC5-PARIDADE.md` e `MIGRACAO-BC5.md`. O usuário ampliou o pedido para abrir e
confrontar **todas as telas, menus e submenus**, incluindo comportamento. Não
encerrar a continuação apenas com inventários de recursos ou afirmar 1:1.

Computer Use foi tentado nesta sessão, mas o kernel não conseguiu gravar seus
assets (“caminho não encontrado”, erro 3), inclusive após reset. Craft 0.3.0
funcionou para captura e UIA; continuar por ele/REA enquanto essa falha persistir.
