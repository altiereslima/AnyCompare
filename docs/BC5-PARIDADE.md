# Paridade com Beyond Compare 5.2.7.32913

Pedido ativo de 02/11/2026: **abrir todas as telas, menus e submenus, testar o
funcionamento e confrontar visual pixel a pixel**. O alvo 1:1 ainda não foi
atingido. Marcar concluído somente o cenário específico com evidência de execução.

Atualização com a [prévia 0.2.5](STATUS-BC5-0.2.5-PREVIA.md) em
`dist/preview/AnyCompare-0.2.5.exe`. A matriz indica
capacidades verificadas individualmente; ainda não há uma porcentagem auditável
para toda a paridade.

- [x] Menus contextuais básicos de Mídia, Registro e Versões: seleção do painel,
  Copiar, Abrir, fechamento e despachos de Abrir Com/Explorer testados sem janelas.
- [x] **Registro → Abrir Com → Editor de Texto:** base de editor independente,
  seleção do lado, salvar/cancelar/codificação e integração com a comparação.
  Inglês **Registry → Open With → Text Editor** validado em teste oculto.
- [x] Pesquisa/substituição básicas do editor: Ctrl+F/H, F3/Shift+F3, Ctrl+G,
  Unicode/regex, histórico, foco e Undo de texto separados da navegação; PT/EN.
- [x] Editor independente: navegação entre linhas editadas e marcadores 0–9,
  com testes de inserção, desfazer, salvamento e Word Wrap. Menus PT/EN.
- [x] Mídia: mudança de rótulos e contagens PT/EN sem recarregar; Registro:
  nomes do usuário mantidos literalmente na grade, traduzindo o valor padrão.
- [ ] Demais comandos e pixels do editor, incluindo Find in Files, sintaxe,
  fontes, agrupamento IME e desempenho em arquivos grandes.
- [ ] Explorer ainda não reproduz o menu de contexto nativo completo do Windows;
  reprodução de mídia e escrita de registro continuam ausentes.

Evidência e limites: [teste oculto de 04/11](validation/0.2.1/CONTEXT-HIDDEN-2026-11-04.md).
As tabelas abaixo distinguem a entrega 0.2.0 do fonte 0.2.1 quando indicado.

## Novidades da versão 5

- [x] Base de Tabelas com várias planilhas Excel/ODS e várias tabelas HTML.
- [x] Abas de planilhas/tabelas, pares órfãos e alinhamento por nomes/ordem.
- [x] Alinhamento de colunas por nomes sem diferenciar caixa/espaços; múltiplas chaves.
- [x] Decimal exato, tolerância configurável e diferenças importantes/não importantes.
- [x] Datas básicas com ordem regional do SO e escolha DMA/MDA/AMD na sessão.
- [x] Base de Comparar Versões PE32/PE32+ com confronto nativo da DLL fornecida.
- [x] Tabelas delimitadas: edição na grade, ações contextuais da célula clicada
  e alinhamento manual básico de linhas (F7), com testes ocultos.
- [x] Tabelas com seleção de intervalos e células disjuntas, Ctrl+A nas linhas
  visíveis, cópia e exclusão em lote, com testes ocultos.
- [x] Rolagem automática horizontal e vertical durante seleção por arrasto em
  Tabelas, com teste oculto nos dois sentidos. A interação nativa do BC5 ainda
  precisa ser comparada.
- [ ] Tabelas completas com regras independentes
  por coluna/planilha, alinhamento manual de colunas e folhas, todos os formatos.
- [ ] Integração MUI externa e todos os campos/comandos de Comparar Versões.
- [x] Word Wrap básico no editor independente, sem modificar as linhas do arquivo.
- [x] **Word Wrap em Text Compare**, com mapa compartilhado entre painéis,
  seleção/edição e conteúdo original preservados. Geometria nativa ainda aberta.
- [ ] Tema escuro completo e escolha conforme o sistema.
- [ ] Picture Compare e suas opções/relatórios.
- [ ] Media Compare: formatos/tags de áudio e vídeo, reprodução e comparações.
- [ ] Registry Compare, incluindo arquivos e todas as operações da sessão.
- [ ] Text Merge, Folder Merge, Folder Sync, Text Edit e Patch completos.
- [ ] Melhorias de Home/gerenciamento de sessões e refinamentos de abas/foco.
- [ ] DPI por monitor e nitidez equivalente em todas as escalas/telas.
- [ ] Integração do menu de contexto do Windows 11 e controle de versão.
- [ ] Atributos adicionais, políticas administrativas e novidades 5.1/5.2 aplicáveis.
- [ ] Archives, perfis remotos/nuvem, conversores e relatórios completos.
- [x] Revisar os [formatos adicionais oficiais](FORMATOS-ADICIONAIS-BC5.md).
- [ ] Importar formatos e executar as conversões verificadas, com seleção de
  regras por extensão, modo e prioridade.

## Telas e menus — estado do aplicativo

Os **12 tipos da Home do original** foram conferidos na imagem do usuário e na
árvore UIA da cópia isolada. “Base parcial” significa uma sessão própria existente;
não significa que todos os comandos ou pixels estejam equivalentes.

| Tipo exibido na Home do BC5 | Estado no AnyCompare |
|---|---|
| Comparar Pastas | Base parcial |
| Combinar Pastas (Folder Merge) | **Pendente** |
| Sincronizar Pastas | **Pendente** |
| Comparar Textos | Base parcial |
| Mesclar Texto | **Pendente** |
| Editar texto, sessão independente | Edição, busca, marcadores e navegação na prévia 0.2.3; comandos/visual completos pendentes |
| Comparar Hex | Base parcial |
| Comparar Mídia | Propriedades e menus em somente leitura; reprodução e paridade pendentes |
| Comparar Imagens | **Pendente** |
| Comparar Registro | Arquivos `.reg` somente leitura no fonte 0.2.1; editor/operações completas pendentes |
| Comparar Tabelas | Base parcial, entrega 0.2.0 |
| Comparar Versões | Base parcial, entrega 0.2.0 |

Correções visuais apontadas após a entrega 0.2.0:

- [x] **Duplo clique em Tabelas delimitadas:** edição dentro da célula, seleção
  do conteúdo, confirmação e cancelamento testados com fixtures. Planilhas e
  HTML continuam em somente leitura.
- [x] **Botão direito em Tabelas:** a ação usa a célula e o lado clicados;
  operações de edição e desfazer exercitadas. Conferir a lista de referência em
  `PAUSA-2026-11-02-HOME-BC5.md`.
- [ ] **Paridade completa do menu de células:** ícones, divisórias, geometria,
  classificação e submenus restantes.

- [ ] Miniatura/faixa lateral deve abranger caminhos, grade e detalhes inferiores,
  como no original, em vez de terminar na grade.
- [ ] Indicadores triangulares azuis da coluna selecionada nos dois painéis e
  nos detalhes inferiores.
- [ ] Setas dos seletores de formato, codificação e dados da célula.
- [ ] Demais pixels/estados dos controles: auditoria completa ainda aberta.

| Família | Implementação atual | Aceitação 1:1 |
|---|---|---|
| Home / sessões / abas | Implementação parcial, histórico próprio | Pendente |
| Texto | Comparar/editar, Word Wrap, filtros, pesquisa, cópia, codificações | Pendente; falta confrontar pixels e comandos restantes |
| Pastas | Comparação, árvore, filtros e operações parciais | Pendente |
| Hex | Alinhamento, seleção e controles parciais | Pendente; edição incompleta |
| Tabelas | CSV, Excel, ODS, HTML; entrega 0.2.0 validada | Pendente; limites no status da entrega |
| Versões | Parser PE e sessão própria | Pendente |
| Mesclar texto | Ausente como sessão completa | Pendente |
| Mesclar pastas | Ausente como sessão completa | Pendente |
| Sincronizar pastas | Ausente como sessão completa | Pendente |
| Editar texto / Patch | Editor independente parcial; Patch ausente | Pendente |
| Imagens | Ausente | Pendente |
| Mídia | Leitura de propriedades e menus básicos no fonte 0.2.1 | Pendente |
| Registro | Leitura, menus básicos e abertura no editor na prévia 0.2.1 | Pendente; operações completas ausentes |
| Formatos / opções / perfis / relatórios | Implementação parcial | Pendente |

Para cada família registrar: vazia/carregada, esquerda/direita, seleção,
alterações/salvar/cancelar, todos os menus/submenus, diálogos, atalhos, rolagem,
redimensionamento, erros e idiomas PT/EN. Usar fixtures próprias; arquivos do
usuário apenas para leitura. Comparar capturas na mesma escala e no mesmo estado.

## Evidências e exclusões

Entrega atual: `STATUS-BC5-0.2.0.md` e `validation/0.2.0`. Referência binária e
fontes oficiais: `MIGRACAO-BC5.md`. Recursos PE presentes não significam funções
implementadas. Uma captura sem erro não prova que todos os controles funcionam.

Faixa vermelha, selo, link promocional e painel web foram excluídos pelo usuário.
Não analisar nem alterar licenciamento. Não modificar o executável/perfil instalado.
