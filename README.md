# Meraki Video Editor

Editor de vídeo open-source, nativo em **Rust**, focado em estabilidade extrema (arquitetura *anti-freeze*), fluidez em tempo real com **GStreamer Editing Services (GES)** e interface minimalista construída em **Iced + wgpu**.

Repositório oficial: [lindnergui/Editor-de-video-meraki](https://github.com/lindnergui/Editor-de-video-meraki)

---

## Visão Geral das 5 Abas

1. **Arquivos** — Organização em pastas do projeto, importação de mídias, filtros rápidos (`Todos`, `Vídeos`, `Áudios`, `Imagens`), miniaturas reais, progresso de proxy 720p e pré-visualização com marcação `In`/`Out`.
2. **Edição** — Gaveta compacta de mídia, player central com seleção de prévia (`720p`/`1080p`), inspetor contextual (`Escala`, `Posição X/Y`, `Opacidade`, `Velocidade`) e timeline interativa em `Canvas` com zoom, snap, corte e alças laterais.
3. **Efeitos** — Biblioteca categorizada (`Transições`, `Títulos & Textos` na trilha `T1`, `Efeitos de Vídeo`), overlay direto sobre o player e keyframes simplificados (`Linear` vs `Suave [Ease]`).
4. **Áudio** — Presets de voz (`Voz de Podcast`, `Voz Clara`, `Voz de Rádio`), equalizador visual de 4 bandas em `Canvas`, redução de ruído e nivelador de voz em tempo real, mixer vertical (`M`/`S`/`Master` com medidores dB) e linha de automação de volume na timeline.
5. **Entrega** — Predefinições prontas (`YouTube`, `TikTok & Reels`, `Instagram`, `WhatsApp Leve`, `Alta Qualidade ProRes`, `Apenas Áudio`), estimativa de duração/tamanho, escolha de capa e fila assíncrona de exportação.

Projetos podem ser salvos e reabertos no formato `.meraki` (`Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+O`), com cópia automática de recuperação a cada 60 segundos.

---

## Arquitetura Anti-Freeze (4 Crates)

Dependência estrita e unidirecional:

`meraki-app` -> `meraki-ui` -> `meraki-engine` -> `meraki-core`

- **`meraki-core`**: Domínio puro (`Project`, `Timeline`, `Track`, `Clip`, `Keyframe`, `Audio`, `ExportPreset` e `HistoryStack` Undo/Redo), sem dependências de UI, GStreamer ou I/O.
- **`meraki-engine`**: Motor GStreamer + GES isolado em threads dedicadas, com coalescência atômica de `Seek` (`request_seek`), envio não bloqueante de quadros (`try_send` zero-copy com `bytes::Bytes`), geração assíncrona de miniaturas/proxies 720p e pipelines de renderização.
- **`meraki-ui`**: Interface nativa em `iced` (`wgpu`) com fonte `Inter` embutida, cache estático de timeline (`DualCanvasCache`) e comunicação assíncrona exclusiva via `protocol.rs`.
- **`meraki-app`**: Inicializa `tracing`, detecta runtime GStreamer empacotado ao lado do executável, inicia o motor e garante encerramento limpo (`EngineCommand::Shutdown`).

---

## Instalação para Usuários Finais

Baixe a versão mais recente (`v0.1.0`) na página de [Releases](https://github.com/lindnergui/Editor-de-video-meraki/releases):

- **Windows (x64):** Execute `Meraki-Setup-v0.1.0-x64.exe` (inclui todas as DLLs e plugins do GStreamer, atalhos e associação opcional de arquivos `.meraki`).
- **Linux (x86_64 Flatpak):**
  ```bash
  flatpak install --user Meraki-v0.1.0-x86_64.flatpak
  flatpak run io.github.lindnergui.Meraki
  ```

---

## Pré-requisitos de Desenvolvimento & Build Local

### Windows (MSVC x64)
1. Instale o [Rust](https://rustup.rs/) (toolchain `stable-x86_64-pc-windows-msvc`).
2. Instale os pacotes **Runtime** e **Development** do [GStreamer MSVC x86_64](https://gstreamer.freedesktop.org/download/) (`1.24+`) incluindo `gstreamer-editing-services-1.0`.
3. Garanta as variáveis de ambiente (ajuste o caminho se necessário):
   ```powershell
   $env:GSTREAMER_1_0_ROOT_MSVC_X86_64 = "C:\gstreamer\1.0\msvc_x86_64"
   $env:Path = "$env:GSTREAMER_1_0_ROOT_MSVC_X86_64\bin;$env:Path"
   $env:PKG_CONFIG_PATH = "$env:GSTREAMER_1_0_ROOT_MSVC_X86_64\lib\pkgconfig"
   ```

### Linux (Ubuntu / Debian)
```bash
sudo apt-get update && sudo apt-get install -y \
  build-essential pkg-config cmake \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  libges-1.0-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav \
  libasound2-dev libwayland-dev libxkbcommon-dev libx11-dev libvulkan-dev
```

### Comandos de Build e Validação
```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --release -p meraki-app
```

---

## Gerando Pacotes de Distribuição

### Instalador Windows `.exe` (Inno Setup 6)
```powershell
cargo build --release --bin meraki-app
.\packaging\windows\collect_bundle.ps1
iscc /DAppVersion=0.1.0 .\packaging\windows\meraki.iss
```

### Bundle Linux Flatpak
```bash
mkdir -p .cargo
cargo vendor vendor > .cargo/config.toml
flatpak-builder --force-clean --repo=repo build-dir packaging/flatpak/io.github.lindnergui.Meraki.yml
flatpak build-bundle repo Meraki-v0.1.0-x86_64.flatpak io.github.lindnergui.Meraki
```

---

## Licença

Distribuído sob licença dupla **MIT** ([LICENSE-MIT](LICENSE-MIT)) ou **Apache-2.0** ([LICENSE-APACHE](LICENSE-APACHE)), à sua escolha.
