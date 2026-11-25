# AGENTS.md — Diretrizes de Engenharia do Meraki Video Editor

Você está contribuindo para o **Meraki**, um editor de vídeo open-source, nativo em Rust, focado em estabilidade extrema (zero travamentos) e interface minimalista amigável ao usuário final.

---

## 1. Arquitetura e Fronteiras Invioláveis (Cargo Workspace)

O projeto é dividido em 4 crates. **Nunca viole a direção das dependências:**

`meraki-app` -> `meraki-ui` -> `meraki-engine` -> `meraki-core`

1. **`meraki-core` (Domínio Puro):**
   - **PROIBIDO** importar `iced`, `gstreamer`, `glib` ou fazer I/O de disco/rede.
   - Contém apenas estruturas de dados (`Project`, `Timeline`, `Track`, `Clip`, `Keyframe`, `MediaAsset`, `ExportPreset`), regras de negócio e `Undo/Redo` (Command Pattern).
   - Toda mutação na timeline deve possuir teste unitário em `meraki-core`.

2. **`meraki-engine` (Motor Multimídia GES + Anti-Freeze):**
   - Isola 100% do `gstreamer`, `gstreamer-editing-services` (GES) e `glib::MainLoop` em threads dedicadas.
   - **PROIBIDO** expor tipos do `gstreamer` ou `glib` na API pública consumida por `meraki-ui`. Toda comunicação ocorre exclusivamente via `protocol.rs`.
   - Eventos de alta frequência (`FrameReady`, `AudioLevels`, `PlayheadTick`) devem usar envio não bloqueante (`try_send`): se a UI estiver ocupada, descarte o quadro antigo em vez de travar a thread de vídeo.
   - Pedidos de `Seek` devem passar sempre pelo slot atômico de coalescência (`request_seek`) para descartar timestamps obsoletos durante scrubbing rápido.

3. **`meraki-ui` (Interface Nativa em `iced` + `wgpu`):**
   - A thread de UI nunca pode bloquear aguardando disco, decodificação ou GStreamer.
   - Toda ação pesada despacha um `EngineCommand` via `EngineHandle` e retorna imediatamente.
   - Widgets customizados (Timeline, Waveform, Equalizador Visual, Medidores dB) devem ser desenhados com `iced::widget::Canvas` ou shaders `wgpu`.

4. **`meraki-app` (Orquestração do Binário):**
   - Inicializa `tracing`, sobe a thread do motor (`meraki-engine`), conecta o `EngineHandle` à aplicação `iced` e garante encerramento limpo (`EngineCommand::Shutdown`).

---

## 2. Design System Obrigatório (Identidade Visual Meraki)

Mantenha fidelidade estrita à paleta e hierarquia visual do projeto:

- **Fundo Base (Timeline e janela):** `#111218`
- **Painéis Principais (Sidebar, Topbar, Inspetor):** `#181922`
- **Cards, Inputs, Trilhas e Superfícies Elevadas:** `#212330`
- **Bordas Sutis (`1px`):** `#2A2D3D`
- **Cor de Destaque (Aba ativa, Playhead, Sliders, Botão Exportar):** `#735BF2`
- **Destaque Hover / Secundário:** `#8670F5`
- **Texto Principal e Formas de Onda:** `#E2E4F0`
- **Texto Secundário / Ícones Inativos:** `#8B8FA3`
- **Arredondamento:** `8px` para painéis/cards, `6px` para botões e clipes na timeline.

### Estrutura Fixa das 5 Abas (Sidebar Esquerda, de Cima para Baixo)
Não adicione abas de correção de cor avançada ou comentários colaborativos. Mantenha exatamente:
1. **Arquivos** — Árvore de pastas, botão `+ Importar Mídia`, grade de arquivos com filtros rápidos (`Todos`, `Vídeos`, `Áudios`, `Imagens`) e pré-visualização com marcação In/Out.
2. **Edição** — Gaveta compacta de mídia, player central expandido com seletor de qualidade de prévia (`720p`/`1080p`), inspetor contextual à direita (`Escala`, `Posição X/Y`, `Opacidade`, `Velocidade`) e timeline inferior com alças laterais brancas de corte.
3. **Efeitos** — Biblioteca categorizada (`Transições`, `Títulos & Textos`, `Efeitos de Vídeo`), overlay interativo sobre o player e controle simplificado de keyframes (`Linear` vs `Suave [Ease]`).
4. **Áudio** — Player compacto, presets de voz (`Voz de Podcast`, `Voz Clara`, `Voz de Rádio`), equalizador visual de 4 bandas (`Graves`, `Médios-Graves`, `Médios`, `Agudos`), toggles de melhoria (`Redução de Ruído`, `Nivelador de Voz`), mixer vertical (M/S/Master com dB) e timeline de áudio expandida com linha de volume.
5. **Entrega** — Cartões de predefinições (`YouTube`, `TikTok & Reels`, `Instagram`, `WhatsApp Leve`, `Alta Qualidade ProRes`, `Apenas Áudio`), resumo com duração/tamanho estimado/escolha de capa, configurações finais à direita e fila de tarefas inferior.

---

## 3. Divisão de Papéis no Modo Orchestrator (Antigravity)

- **Neon (Arquitetura & Sistemas):** Contratos em `protocol.rs`, núcleo `meraki-core`, integração `gstreamer-rs`/GES, gerenciamento de threads, Ring Buffer, proxies 720p e pipelines de exportação.
- **Hydrogen (UI & Testes):** Telas e componentes em `meraki-ui` usando `theme.rs`, presets de dados, ícones SVG, testes unitários de `meraki-core` e refatorações repetitivas.
- **Sol6.2 / Astra (Escalação Sob Demanda):** Usar apenas para investigar deadlocks entre GLib/Tokio/Iced, negociação de Caps no GStreamer, vazamentos de memória em buffers de vídeo ou shaders `wgpu` complexos.

---

## 4. Critérios de Conclusão de Tarefa (Definition of Done)

Nenhuma subtarefa pode ser marcada como concluída antes de executar e passar em:
1. `cargo check --workspace`
2. `cargo test -p meraki-core`
3. `cargo clippy --workspace --all-targets -- -D warnings`

**Regras de Qualidade em Rust:**
- Proibido `.unwrap()` ou `.expect()` em código de produção (permitido apenas em testes unitários).
- Use `thiserror` para erros estruturados e propague falhas de runtime para a UI via `EngineEvent::Error`.
- Sempre mantenha arquivos pequenos e focados (< 400 linhas por arquivo sempre que possível).
