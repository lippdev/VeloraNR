# VeloraNR

## Como trabalhar com o LippStack

Consulte o SKILL.md da skill aplicável e os arquivos de referência que ela indicar antes de executar a etapa. Ao começar, diga ao usuário quais skills está usando. Na entrega, distinga o que foi executado do que foi apenas consultado. O catálogo de referência é [lippdev/lippstack](https://github.com/lippdev/lippstack), revisado na revisão 3959e23. Nesta máquina as fontes completas estão em `%LOCALAPPDATA%/VeloraNR/skill-sources/`. Em outra máquina siga a seção “Other agents” do README do LippStack para localizar as mesmas skills. As skills locais da stack ficam em `lippstack/plugins/<plugin>/skills/<nome>/SKILL.md`. Ponytail e PM têm suas próprias pastas `skills/`.

## Interface e protótipos

- Comece as mudanças visuais em HTML independente em prototypes/. Valide a direção com o usuário antes de portar para GPUI. A identidade do VeloraNR usa arte ASCII feita com caracteres, inclusive PS5 e DualSense. Preserve a seleção vertical de bitrate com todos os valores visíveis.
- Use **prototype** para comparar composições e **hallmark** para revisar identidade, hierarquia, tipografia e densidade. Use **unslop-ui** para revisar rótulos, elementos decorativos e textos redundantes.
- Use **apple-design** no comportamento. Ao abrir o seletor de bitrate, o foco deve acompanhar a interação; Esc fecha e devolve o foco ao controle. O comando de conectar deve dar retorno imediato mesmo quando o console demora. A preferência por movimento reduzido deve ser respeitada.
- Use **animate** para uma transição, **choreograph** para coordenar várias e **gestures** nas interações de manipulação direta. Use **remove-animations** em ações frequentes, **review-animations** para revisar o resultado e **animation-performance** para medir fluidez da UI. Meça o pipeline de vídeo separadamente.
- Aplique **break-ui** com rede vazia, IP manual, nome longo, console em repouso, falha ao conectar, janela estreita e navegação por teclado.

## Produto e planejamento

- Use **incoming-request-advisor**, **problem-statement**, **jobs-to-be-done** e **discovery-process** para entender a necessidade. **stakeholder-mapping** serve para decisões envolvendo públicos diferentes.
- Use **brainstorming** para explorar alternativas, **prd-development** para requisitos e critérios de sucesso, **prioritization-advisor** e **roadmap-planning** para ordenar entregas.
- Use **epic-hypothesis**, **epic-breakdown-advisor**, **user-story** e **user-story-splitting** para dividir o escopo. **writing-plans** traduz decisões aprovadas em mudanças verificáveis. **jira-stories** serve para o fluxo autorizado de criação no Jira.

## Implementação, verificação e entrega

- Use **ponytail** como base de implementação. Reaproveite o fluxo existente, prefira a menor mudança que resolva a jornada inteira e verifique o resultado. Use **ponytail-review** em diffs, **ponytail-audit** em revisões de repositório, **ponytail-debt** para atalhos registrados, **ponytail-gain** para métricas e **ponytail-help** para consultar os modos.
- Use **investigate** em crashes, tela preta ou quedas de FPS. Registre condições, hipótese e evidência antes de alterar o pipeline. Use **qa** para exercitar entradas reais, estados de erro e regressões.
- Use **ship** no fluxo de entrega descrito no README. **release-notes** prepara notas de versão e **humanizer** revisa a linguagem preservando o significado. Publicações externas seguem o pedido do usuário.
- O pacote opcional **video-editor-skills** atende à produção de vídeos no Resolve, por exemplo cortes, áudio, legendas e exportação de uma demonstração. Consulte os requisitos e a licença do pacote ao iniciar esse trabalho.
- Os mods savvy-progress, stage-bar, done-ping, guardrails e where-line, os plugins oficiais hookify, claude-md-management, session-report e playwright e o mod cc-plugin-you-should-know dependem do host Claude Code. Em Codex use as capacidades efetivamente disponíveis para o trabalho solicitado.
