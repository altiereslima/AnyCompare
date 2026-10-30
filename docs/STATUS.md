# Estado da reconstrução — histórico de 14/10/2026

**Estado atual 0.1.17.** Leia [STATUS-ABAS-0.1.17.md](STATUS-ABAS-0.1.17.md).
As limitações e contagens abaixo descrevem versões anteriores. Ghidra e a
validação completa dos resumos já avançaram; não usar este histórico como status atual.

## Referência autenticada

- Beyond Compare 4.4.7.28397, PE64, 41.929.704 bytes.
- SHA-256: `aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db`.
- REA 6.5.0, operação `inspect_pe_resources`.
- Evidência: `ev_b51566b63e23caf947294d5f63a7f42387d9e1c29ae6a8276c89bd2d3ecbb0f9`.
- Cobertura declarada pelo REA: completa para a árvore de recursos; 1.471 entradas examinadas, 729 payloads.
- Parser TPF0: 200 formulários completos, zero falhas ou bytes restantes.
- Ações: 501 declarações; comportamento e vínculo entre eventos e código permanecem não comprovados.
- Imagens: 340 por escala, 1.360 no total. Há 16 nomes compartilhados entre HomeImages e HomeSmallImages.
- Grupos ICO: 17; MAINICON incluído no recurso Windows do executável e na janela.

## Falhas corrigidas na base recebida

1. O executável original do AnyCompare encerrava com código 101. O layout calculava altura negativa
   dentro de um `ui.horizontal`, acionando uma assertiva em `egui::Ui::allocate_ui_with_layout_dyn`.
2. A página inicial exibia sessões inventadas e abria executáveis como texto.
3. Abrir um arquivo inválido transformava uma mensagem de erro no próprio conteúdo comparado.
4. Comparar Pastas abria a tela de comparação de texto.
5. Copiar, editar e salvar tinham controles sem implementação.
6. Imagens pequenas sobrescreviam as grandes, e os ícones de 125% e 150% não eram utilizados.
7. O cálculo de diferenças utilizava terminadores de linha, enquanto o renderizador os removia.
8. Fragmentos de palavras eram exibidos em labels separados, alterando o espaçamento original do texto.
9. Clicar em Editar substituía a comparação por editores sem destaques e com quebra automática.
   Na versão 0.1.1 o editor nativo usa o mesmo alinhamento, cores, mapa e marcadores da comparação.
   Lacunas são altura de layout; o texto não recebe linhas fictícias. Ctrl+Z funciona durante edição.
10. A barra mostrava linhas diferentes como contagem principal. Agora mostra seções contíguas,
    usadas também na navegação: 1.263 seções com os dois arquivos resumo fornecidos pelo usuário.
11. Abrir um par deixava uma sessão vazia extra. Agora reutiliza a sessão vazia compatível e evita duplicar pares abertos.

## Preferências e capturas fornecidas pelo usuário

- As capturas em `reference/screenshots` são observações de execução do original fornecidas pelo usuário.
- A faixa vermelha de suporte, vídeos e registro foi excluída expressamente. Não recriar uma faixa promocional.
- Cores medidas: página inicial `#F9F9F9`, árvore `#E3E7ED`, fundo de texto vazio `#F0F0F0`,
  diferenças no painel ativo `#FFE3E3`, painel inativo `#FCE6E6`.
- Menus e barras usam degradês; fonte UI Tahoma e texto Courier New regular quando instaladas no Windows.
  O histórico de fontes do BC4 registra Consolas 10; isso não comprova a configuração ativa de toda sessão.
  Na 0.1.2 a fonte foi ajustada por comparação visual; não é uma recuperação comprovada da fonte ativa.
- Barra de texto quebra grupos em duas linhas em janelas estreitas; cabeçalho exibe data/tamanho reais.
- A identidade da aplicação continua AnyCompare; o executável de referência não foi modificado.

## Cobertura funcional atual

| Área | Estado | Limite relevante |
| --- | --- | --- |
| Recursos estáticos | Completa dentro da árvore PE observada | Sem equivalência de código ou comportamento |
| Home e seleção de sessões | Implementados | Histórico próprio; não importado do BC4 |
| Ícones e escalas | Recuperados e incorporados | Teste cobre tamanho dos ícones Home em quatro escalas |
| Abas e histórico JSON | Implementados | Sem exportação de workspace do BC4 |
| Texto/diff/cópia de blocos | Implementados | Alinhamento por Myers; heurísticas específicas do BC4 não reproduzidas |
| Edição e salvamento | Integrados às linhas alinhadas na 0.1.1 | UTF-16 exige BOM; ANSI é Windows-1252; regras específicas do BC4 pendentes |
| Pastas | Comparação básica implementada | Lista recursiva, sem árvore agregada, CRC selecionável ou operações de cópia |
| Hexadecimal | Leitura e diferenças implementadas | Alinhamento por posição, sem edição binária |
| Folder Sync | Pendente | Não habilitar botão como se já funcionasse |
| Folder Merge / Text Merge 3 vias | Pendente | Exige plano e validação de conflitos |
| Imagens, tabelas, MP3, registro, versões, XML | Pendentes | Interfaces/ações recuperadas |
| Relatórios, perfis, FTP/SFTP/WebDAV/nuvem, archives | Pendentes | Sem execução de código de componentes auxiliares |
| Fidelidade integral de layout | Em andamento | Telas comparadas por inspeção visual; não há prova pixel a pixel |

## Limitações das ferramentas observadas

O servidor REA não tinha um provedor nativo profundo configurado: Ghidra sem `GHIDRA_INSTALL_DIR`,
IDA sem configuração e Hopper indisponível no Windows. A inspeção efetiva foi a de recursos PE.
Não afirmar que o executável Delphi foi decompilado ou que seus algoritmos foram recuperados.

REA recusou `BCompare.tr` por uma entrada de dados de recurso sem alinhamento.
O erro foi preservado em `reference/rea-translation-resources.json`; nenhuma tradução desse arquivo
foi marcada como verificada. Os textos PT-BR da interface foram escritos a partir das capturas e do contexto.

## Continuação necessária

1. Reproduzir a árvore de pastas, seleções e ações de arquivos com estado de operação real.
2. Implementar Folder Sync com plano revisável de cópias/conflitos e testes em pastas descartáveis.
3. Implementar regras de comparação e as demais opções de edição do original.
4. Implementar mesclagem de três vias e os demais comparadores, incluindo relatórios e perfis.
5. Configurar análise nativa profunda no REA para validar funções e eventos específicos.
6. Validar a interface nas mesmas dimensões/DPI das capturas e cada ação contra o original.

O backup anterior às alterações está em `.codex/backups/before-rea-recovery-2026-10-14.zip`.

## Validação da correção de edição (0.1.1)

16 testes passaram, incluindo entrada de texto Unicode, colagem, desfazer, alternância de edição,
salvamento exato sem lacunas fictícias e navegação por setas no filtro Diferenças. Clippy passou sem avisos.
Sete execuções do executável otimizado geraram capturas, encerraram com código zero e stderr vazio.
Os hashes dos dois arquivos resumo foram iguais antes/depois. A captura usa o renderizador da aplicação;
os testes de entrada usam eventos do egui, sem alegar automação nativa de todas as ações do BC4.
Veja [evidências da versão](validation/VALIDATION.md). A reconstrução completa continua pendente.

## Linhas filtradas e desempenho (0.1.2)

O filtro mostra uma faixa de 9 px por grupo oculto, contagem de linhas e +/− para abrir/recolher
o trecho simultaneamente nos dois painéis. A expansão e edição não inserem caracteres de apresentação.
Os cabeçalhos foram alinhados com os painéis de texto; a fonte foi suavizada visualmente.

A comparação é atualizada no trecho afetado; chunks de palavras imutáveis são compartilhados.
O editor mantém o texto completo e métricas de cursor, criando malhas gráficas apenas perto da área visível.
O resultado do cálculo durante a entrada é reaproveitado; não se calcula a mesma edição duas vezes.

Replay de 12 eventos de digitação nos mesmos resumos, compilação release: média anterior **313,77 ms**,
média final **26,54 ms**, máximo final **33,07 ms**. Mede entrada/layout egui, sem envio à GPU ou latência do Windows.
Não é medição de todas as operações nem garantia para qualquer tamanho de arquivo. Logs preservados em validation.
20 testes automáticos passaram; o benchmark foi executado separadamente. A comparação completa e a localizada
coincidiram nos casos testados de inserção, remoção, substituição, Unicode, linhas vazias e bordas do documento.
