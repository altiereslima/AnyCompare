# OPERAÇÃO REA — BEYOND COMPARE 4.4.7 PRO

## ENGENHARIA REVERSA NATIVA, AUDITORIA VISUAL E RECONSTRUÇÃO FUNCIONAL 1:1

### MISSÃO PRINCIPAL

Utilize intensivamente o **Reverse Engineer Anything (REA)** para investigar o Beyond Compare 4.4.7 Pro original para Windows, compreender sua arquitetura e produzir as evidências necessárias para reconstruir o AnyCompare com máxima fidelidade.

O objetivo é alcançar uma implementação equivalente ao aplicativo original em três dimensões:

1. **Fidelidade visual:** reproduzir janelas, menus, formulários, controles, ícones, dimensões, posicionamento e comportamento visual.
2. **Fidelidade estrutural:** reconstruir a hierarquia real dos componentes, menus, comandos, eventos e dependências.
3. **Fidelidade funcional:** reproduzir o comportamento dos comandos, ferramentas, comparações, editores e operações do aplicativo.

Não quero apenas um aplicativo visualmente semelhante. Quero uma reconstrução fundamentada na investigação do programa original, com rastreabilidade entre cada componente e suas evidências.

**A investigação deve partir do aplicativo original, nunca de suposições sobre sua implementação.**

---

# FASE 1 — PREPARAÇÃO REAL DO REA

Estamos trabalhando em um computador Windows, com os arquivos organizados em pastas locais.

Não presuma nomes de pastas nem arquivos compactados.

### 1.1 — Identificar os diretórios

Solicite os caminhos das pastas caso não estejam disponíveis.

Identifique:

- Diretório do Beyond Compare original.
- Diretório do AnyCompare.
- Executável principal de cada aplicação.
- DLLs e dependências.
- Recursos externos.
- Arquivos-fonte e formulários da nossa implementação.
- Ferramentas de compilação e análise já instaladas.

Não realize varreduras fora dos diretórios autorizados.

### 1.2 — Verificar a conexão REA

Primeiro verifique se as ferramentas MCP do REA já estão disponíveis.

Quando estiverem disponíveis, utilize-as diretamente, sem executar diagnósticos desnecessários.

Se houver problemas de conexão, use os procedimentos descritos no arquivo:

`.agents/skills/reverse-engineer-anything/SKILL.md`

E na documentação:

`references/connection-and-recovery.md`

Quando necessário, execute:

`rea doctor --json`

`rea providers --json`

Não instale nem modifique ferramentas ou configurações sem avaliar a necessidade, mostrar as alterações propostas e obter autorização quando exigida.

### 1.3 — Identificar o provedor nativo

Descubra os provedores efetivamente disponíveis, como Ghidra ou IDA.

Use `binary_session` para consultar os recursos disponíveis e suas limitações.

Não presuma que todas as ferramentas funcionam no Windows.

Para Ghidra no Windows, consulte `docs/windows-ghidra-p0.md`, especialmente as diferenças entre a versão do repositório e os pacotes publicados.

Não utilize operações que o provedor selecionado não suporte.

---

# FASE 2 — INSPEÇÃO COMPLETA DOS RECURSOS PE

**PRIORIDADE MÁXIMA.**

Antes de iniciar a decompilação ampla, investigue os recursos incorporados ao executável original.

Utilize:

`inspect_pe_resources`

Essa ferramenta deve ser usada diretamente sobre o arquivo PE original, sem exigir uma sessão de análise nativa previamente aberta.

### Investigue:

- Identificadores de recursos.
- Nomes de recursos.
- Idiomas.
- Estrutura das entradas PE.
- Intervalos de bytes.
- Tamanhos dos recursos.
- Hashes SHA-256.
- Grupos de ícones.
- Referências a imagens individuais.
- Recursos RCDATA.
- Manifestos.
- Strings e outros tipos de recursos encontrados.

Construa uma árvore dos recursos reais presentes no executável.

**Atenção:** o inventário do REA não significa que o conteúdo de todos os recursos foi decodificado.

Quando encontrar recursos opacos relevantes, investigue seu formato com ferramentas complementares apropriadas.

## 2.1 — Investigação dos recursos Delphi

Verifique se existem evidências de formulários Delphi, como estruturas DFM.

Caso existam, tente recuperar:

- Nome dos formulários.
- Classes dos componentes.
- Propriedades.
- Hierarquia dos controles.
- Valores de posicionamento.
- Dimensões.
- Textos.
- Eventos declarados.
- Ícones e referências gráficas.
- Componentes personalizados.

Utilize um decodificador Delphi adequado quando necessário.

Relacione cada recurso recuperado ao seu identificador, hash e localização no executável.

Não invente um formulário quando houver apenas indícios insuficientes de sua existência.

---

# FASE 3 — INICIAR A ENGENHARIA REVERSA NATIVA

Abra o executável principal com:

`open_binary`

Selecione o provedor adequado e confirme a identidade do alvo usando:

`binary_session`

Quando suportado, obtenha um panorama utilizando:

`binary_overview`

Registre:

- Arquitetura.
- Formato executável.
- Identidade SHA-256.
- Provedor selecionado.
- Perfil de análise.
- Seções e imports relevantes.
- Limitações encontradas.

Comece investigando o sistema de criação da interface.

### Ferramentas prioritárias

Utilize conforme disponibilizadas pelo servidor:

- `search_strings`
- `search_procedures`
- `list_procedures`
- `procedure_address`
- `analyze_function`
- `xrefs`
- `procedure_callers`
- `procedure_callees`
- `inspect_native_instruction`
- `resolve_native_call_targets`
- `trace_native_values`
- `build_call_path`

Use `trace_native_ui_action` somente se houver suporte comprovado para o alvo e o provedor selecionados.

Não trate operações específicas de macOS como se fossem ferramentas de automação da interface Windows.

## 3.1 — Estratégia de investigação

Em vez de decompilar indiscriminadamente milhares de funções:

1. Identifique uma string, recurso, comando ou procedimento relevante.
2. Localize suas referências.
3. Examine as funções correspondentes.
4. Rastreie chamadas e dependências.
5. Investigue o fluxo de dados.
6. Identifique condições e ramificações.
7. Relacione os resultados aos componentes da interface.
8. Registre a evidência.
9. Avance para as referências relacionadas.

Reutilize resultados já obtidos.

Não repita análises caras quando os dados já estiverem armazenados.

---

# FASE 4 — ENGENHARIA REVERSA TOTAL DOS MENUS

**ESTA É A PRIORIDADE PRINCIPAL DO PROJETO.**

O AnyCompare atualmente apresenta diferenças nos menus, submenus e funcionalidades.

Quero identificar todos os menus recuperáveis do Beyond Compare original, sua organização e o código que os controla.

### Para cada menu e submenu, investigue:

1. Texto exato.
2. Identificador interno.
3. Hierarquia.
4. Ordem dos itens.
5. Separadores.
6. Ícones.
7. Atalhos.
8. Teclas de acesso.
9. Estados marcados/desmarcados.
10. Estados habilitados/desabilitados.
11. Condições de visibilidade.
12. Tipo de sessão.
13. Componente responsável.
14. Evento associado.
15. Handler.
16. Endereço nativo.
17. Procedimentos chamados.
18. Dependências.
19. Resultado da ação.
20. Evidências disponíveis.

### Rastreabilidade exigida

Tente estabelecer o fluxo:

**MENU → COMANDO → EVENTO → HANDLER → FUNÇÃO → DEPENDÊNCIAS → RESULTADO**

Inspecione procedimentos e referências cruzadas usando o REA.

Não associe um handler apenas por semelhança de nomes.

### Contextos obrigatórios

Investigue os menus separadamente nos contextos:

- Tela inicial.
- Comparação de pastas.
- Comparação de textos.
- Comparação hexadecimal.
- Comparação de tabelas.
- Comparação de imagens.
- Comparação de áudio.
- Mesclagem.
- Sincronização.
- Configurações.
- Menus de contexto.

Não declare a estrutura completa apenas porque encontrou os menus de uma única janela.

Verifique também menus gerados dinamicamente.

---

# FASE 5 — ENGENHARIA REVERSA DOS FORMULÁRIOS E JANELAS

Investigue cada formulário original identificado.

Para cada janela, construa um modelo estrutural contendo:

- Identificador.
- Nome interno.
- Classe.
- Tipo.
- Dimensões.
- Posição.
- Componentes filhos.
- Hierarquia.
- Propriedades visuais.
- Textos originais.
- Ícones.
- Eventos.
- Handlers.
- Dependências.
- Procedimentos de abertura.
- Procedimentos de fechamento.
- Persistência das configurações.

### Investigue os controles

- Botões.
- Editores.
- Campos de entrada.
- Listas.
- Comboboxes.
- Checkboxes.
- Radio buttons.
- TreeViews.
- ListViews.
- Abas.
- Painéis.
- Barras de ferramentas.
- Barras de status.
- Controles personalizados.

Recupere propriedades e estruturas diretamente dos recursos ou do código sempre que possível.

Quando houver diferenças entre a estrutura estática e o comportamento observado em execução, documente ambas.

---

# FASE 6 — INVESTIGAR O DESENHO DA INTERFACE

Quero descobrir por que nosso AnyCompare apresenta diferenças visuais.

Investigue:

- Criação dos controles.
- Inicialização das janelas.
- Fontes utilizadas.
- Tamanhos e métricas.
- Imagens.
- Iconografia.
- Listas de imagens.
- Layout e alinhamento.
- Cores.
- Estilos visuais.
- Rotinas de pintura.
- Tratamento de temas.
- DPI.
- Escalonamento.
- Redimensionamento.
- Componentes personalizados.

### Quando necessário

Rastreie chamadas a APIs gráficas do Windows e funções relacionadas à criação e manipulação dos controles.

Considere funções da VCL quando sua utilização for comprovada.

Investigue se diferenças visuais resultam de:

- Recursos distintos.
- Componentes diferentes.
- Propriedades incorretas.
- Comportamento de temas do Windows.
- Algoritmos de layout divergentes.
- Rotinas gráficas personalizadas ainda não reconstruídas.

Não atribua automaticamente todas as diferenças ao tema do Windows.

---

# FASE 7 — ENGENHARIA REVERSA FUNCIONAL PROFUNDA

Investigue cada comando e funcionalidade relevante do Beyond Compare.

Priorize recursos que estão ausentes ou apresentam comportamento divergente no AnyCompare.

### Para cada funcionalidade

1. Identifique seu ponto de entrada.
2. Localize o procedimento correspondente.
3. Analise pseudocódigo e assembly relevantes.
4. Identifique funções chamadas.
5. Rastreie parâmetros.
6. Identifique condições.
7. Examine estruturas de dados relevantes.
8. Rastreie a saída.
9. Determine os efeitos observáveis.
10. Registre limitações da investigação.

Utilize `analyze_function` para construir dossiês de funções.

Utilize `trace_native_values` quando houver suporte para rastreamento dos fluxos de valores no provedor selecionado.

Utilize `build_call_path` quando existirem dossiês suficientes para construir um caminho de chamadas verificável.

### Áreas funcionais

- Comparação de diretórios.
- Comparação de texto.
- Edição.
- Comparação hexadecimal.
- Comparação de tabelas.
- Comparação de imagens.
- Comparação de áudio.
- Mesclagem.
- Sincronização.
- Sessões.
- Regras de comparação.
- Filtros.
- Comandos.
- Preferências.
- Salvamento.
- Tratamento de erros.

Não trate o pseudocódigo como recuperação do código-fonte Delphi original.

---

# FASE 8 — SISTEMA DE EVIDÊNCIAS DO REA

Toda conclusão importante deve estar fundamentada em evidências reais.

Utilize:

- `inspect_analysis_view`
- `get_evidence_bundle`
- `export_evidence_bundle`

### Estratégia de retenção

Cada análise deve registrar:

- ID da evidência.
- Identidade do artefato.
- Provedor utilizado.
- Procedimento investigado.
- Endereço.
- Dados observados.
- Inferências.
- Limitações.
- Questões desconhecidas.

Quando uma função já tiver sido analisada, consulte a evidência retida em vez de repetir a análise.

Para resultados extensos, utilize visualizações paginadas, resumos ou seções específicas.

Exporte os conjuntos de evidências necessários antes de encerrar uma sessão.

### Classificação das conclusões

**CONFIRMADO ESTATICAMENTE:** comprovado pelos recursos ou pela análise do binário.

**CONFIRMADO EM EXECUÇÃO:** comprovado por observação do programa em funcionamento.

**INFERIDO:** interpretação fundamentada, mas ainda não confirmada.

**DESCONHECIDO:** informação insuficiente.

**BLOQUEADO:** investigação impedida por uma limitação identificada.

Nunca transforme inferências em fatos sem evidências adicionais.

---

# FASE 9 — INVESTIGAR QUESTÕES NÃO RESOLVIDAS

Utilize o sistema de questões desconhecidas do REA.

Ferramentas:

- `record_unknown`
- `list_unknowns`
- `update_unknown`
- `verify_unknown_resolution`

Registre todas as lacunas relevantes.

### Exemplos

- Não foi localizado o handler de um comando.
- Não foi identificado o procedimento que abre determinado formulário.
- Um ícone foi localizado, mas ainda não foi associado a seu botão.
- O código está presente, mas o comportamento em execução não foi verificado.
- Existem chamadas indiretas não resolvidas.
- O recurso recuperado está incompleto.
- O comportamento depende de uma condição ainda desconhecida.

Priorize a investigação das lacunas que impedem a fidelidade visual ou funcional.

Uma questão só poderá ser marcada como resolvida quando as evidências forem suficientes.

---

# FASE 10 — AUDITORIA DIFERENCIAL DO ANYCOMPARE

Depois de compreender os componentes originais, examine os correspondentes no AnyCompare.

**Não utilize o REA indiscriminadamente para analisar código-fonte disponível.**

Para arquivos-fonte completos, utilize ferramentas normais de análise e comparação de código.

Reserve o REA para investigação de binários, recursos, comportamentos não documentados e evidências de engenharia reversa.

### Ferramentas de comparação do REA

Quando os tipos de evidência forem compatíveis:

- `compare_functions`
- `compare_artifacts`
- `compare_bundles`
- `find_changed_behavior`
- `verify_reconstruction`

Não utilize essas ferramentas para afirmar equivalência global sem verificar os requisitos e a cobertura de cada comparação.

A comparação visual de aplicativos Windows poderá exigir captura de tela e automação complementar.

### Matriz de diferenças

Para cada divergência, registre:

| Campo      | Informação                            |
| ---------- | ------------------------------------- |
| ID         | Identificador único                   |
| Componente | Menu, formulário, controle ou função  |
| Original   | Comportamento ou estrutura comprovada |
| AnyCompare | Implementação atual                   |
| Diferença  | Divergência específica                |
| Evidência  | Identificador REA                     |
| Causa      | Confirmada ou inferida                |
| Correção   | Alteração necessária                  |
| Teste      | Método de validação                   |
| Status     | Pendente, implementado ou validado    |

Não confunda funcionalidade existente com funcionalidade equivalente.

---

# FASE 11 — RECONSTRUÇÃO ORIENTADA POR EVIDÊNCIAS

Após a investigação de cada conjunto de componentes, aplique as descobertas no AnyCompare.

### Ordem de prioridade

1. Estrutura de menus.
2. Submenus ausentes.
3. Formulários e diálogos.
4. Propriedades visuais.
5. Ícones e recursos.
6. Handlers de comandos.
7. Funções ausentes.
8. Comportamentos divergentes.
9. Integração entre componentes.
10. Validação.

Preserve os arquivos originais do Beyond Compare.

Preserve as funcionalidades já estáveis no AnyCompare.

Não implemente comportamentos imaginados apenas para completar uma interface.

Se uma função ainda não puder ser reconstruída, registre sua situação real e continue investigando.

---

# FASE 12 — VALIDAÇÃO 1:1 NO WINDOWS

A análise estática não substitui a validação de comportamento.

Sempre que o ambiente permitir, execute o Beyond Compare original e o AnyCompare em condições equivalentes.

### Validação visual

Compare:

- Dimensões.
- Posições.
- Menus.
- Ícones.
- Fontes.
- Cores.
- Espaçamento.
- Estados dos controles.
- Janelas.
- Diálogos.
- Abas.
- Barras de ferramentas.

Utilize capturas com resolução, DPI e tema equivalentes.

Quando for possível, use comparação automatizada de imagens.

### Validação funcional

Para cada comando:

1. Prepare dados de entrada iguais.
2. Execute no aplicativo original.
3. Registre o resultado.
4. Execute no AnyCompare.
5. Compare os efeitos.
6. Identifique diferenças.
7. Corrija o necessário.
8. Repita o teste.

Use `verify_reconstruction` somente quando houver uma especificação finita compatível com os tipos de evidência exigidos.

Não declare equivalência completa com base em um único teste.

---

# FASE 13 — CHECKPOINTS E CONTINUIDADE

Trabalhe em blocos de investigação com resultados verificáveis.

Ao concluir cada bloco:

1. Preserve as evidências REA.
2. Atualize o inventário de recursos.
3. Atualize o mapa de funções.
4. Atualize o mapa de menus.
5. Atualize o mapa de formulários.
6. Atualize a matriz de diferenças.
7. Registre as questões desconhecidas.
8. Salve correções efetivamente concluídas.
9. Preserve a implementação estável.
10. Registre o próximo ponto de retomada.

Não dependa apenas do histórico da conversa.

Não repita trabalho concluído desnecessariamente.

---

# FASE 14 — RESULTADOS E ENTREGA

Mantenha um dossiê técnico organizado.

### Documentos obrigatórios

**01 — AMBIENTE E PROVEDORES**

Versões, provedores, limitações e identidade dos artefatos.

**02 — INVENTÁRIO PE**

Recursos identificados, posições, hashes e referências.

**03 — FORMULÁRIOS DELPHI**

Estruturas e propriedades recuperadas.

**04 — MENUS E SUBMENUS**

Hierarquias, comandos, atalhos e estados.

**05 — HANDLERS E EVENTOS**

Associações comprovadas entre interface e código.

**06 — MAPA DE FUNÇÕES**

Procedimentos, endereços, pseudocódigo investigado e dependências.

**07 — INVESTIGAÇÃO VISUAL**

Recursos gráficos, propriedades, layout e rotinas de desenho.

**08 — COMPARAÇÃO COM ANYCOMPARE**

Divergências e respectivas evidências.

**09 — QUESTÕES DESCONHECIDAS**

Pendências, contradições e investigações futuras.

**10 — VALIDAÇÃO**

Resultados reais dos testes visuais e funcionais.

**11 — EVIDÊNCIAS EXPORTADAS**

Bundles do REA necessários para reprodução e auditoria.

**12 — CHECKPOINT**

Estado atual e próximo ponto de investigação.

---

# REGRAS FUNDAMENTAIS

1. Use efetivamente o REA, não apenas sua terminologia.
2. Consulte os recursos realmente disponíveis no servidor conectado.
3. Não invente saídas de ferramentas.
4. Preserve a identidade dos artefatos.
5. Preserve as evidências.
6. Diferencie fatos, inferências e desconhecidos.
7. Não presuma suporte de um provedor a recursos não anunciados.
8. Não presuma recuperação automática de formulários Delphi.
9. Não confunda pseudocódigo com código-fonte original.
10. Não considere um menu funcional apenas porque seu texto aparece na tela.
11. Não descarte evidências de conflitos.
12. Não declare equivalência 1:1 sem validação apropriada.
13. Não modifique o executável original.
14. Não execute ações destrutivas durante a inspeção sem autorização.
15. Não repita análises já documentadas.
16. Mantenha checkpoints reproduzíveis.

# PRIMEIRA OPERAÇÃO OBRIGATÓRIA

Inicie pela investigação original, nesta ordem:

**PASSO 1:** Confirmar os diretórios reais e as identidades dos executáveis.

**PASSO 2:** Confirmar o REA e o provedor nativo disponível.

**PASSO 3:** Executar `inspect_pe_resources` no executável original.

**PASSO 4:** Identificar possíveis recursos de formulários e menus.

**PASSO 5:** Abrir o executável com `open_binary`.

**PASSO 6:** Investigar procedimentos associados a menus e formulários.

**PASSO 7:** Produzir o primeiro mapa de referências cruzadas e handlers.

**PASSO 8:** Registrar evidências e lacunas.

**PASSO 9:** Comparar os primeiros componentes mapeados com o AnyCompare.

**PASSO 10:** Apresentar diferenças comprovadas e preparar as primeiras correções.

### INSTRUÇÃO FINAL

**Quero que o REA seja o centro técnico da investigação.**

Não substitua a análise real do executável por suposições.

Não comece reescrevendo menus ou formulários antes de investigar suas estruturas originais.

Primeiro descubra como foram construídos. Depois reconstrua com fidelidade.

Trabalhe de maneira progressiva, documentando cada resultado e avançando sobre as próximas diferenças comprovadas.

**META FINAL: máxima fidelidade visual, estrutural e funcional ao Beyond Compare 4.4.7 Pro, com evidências técnicas verificáveis.**