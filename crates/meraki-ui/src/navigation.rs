use crate::icons;

/// As 5 abas fixas de navegação da barra lateral esquerda do Meraki.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceTab {
    #[default]
    Files,
    Edit,
    Effects,
    Audio,
    Delivery,
}

impl WorkspaceTab {
    /// Ordem fixa oficial das abas, de cima para baixo.
    pub const ALL: [Self; 5] = [
        Self::Files,
        Self::Edit,
        Self::Effects,
        Self::Audio,
        Self::Delivery,
    ];

    /// Rótulo exibido na barra lateral e no cabeçalho do espaço de trabalho.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Files => "Arquivos",
            Self::Edit => "Edição",
            Self::Effects => "Efeitos",
            Self::Audio => "Áudio",
            Self::Delivery => "Entrega",
        }
    }

    /// Descrição curta exibida no cabeçalho contextual da aba ativa.
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::Files => "Organize mídias, filtre por tipo e defina trechos In/Out.",
            Self::Edit => "Monte cortes na timeline com pré-visualização fluida e inspetor.",
            Self::Effects => "Aplique transições, títulos e animações simples com keyframes.",
            Self::Audio => "Ajuste presets de voz, equalização em 4 bandas e níveis no mixer.",
            Self::Delivery => "Escolha o destino de exportação e acompanhe a fila de entrega.",
        }
    }

    /// Bytes SVG do ícone correspondente à aba.
    pub const fn icon_bytes(self) -> &'static [u8] {
        match self {
            Self::Files => icons::TAB_FILES,
            Self::Edit => icons::TAB_EDIT,
            Self::Effects => icons::TAB_EFFECTS,
            Self::Audio => icons::TAB_AUDIO,
            Self::Delivery => icons::TAB_DELIVERY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_five_tabs_preserve_official_order_and_labels() {
        assert_eq!(WorkspaceTab::ALL.len(), 5);
        let labels: Vec<&str> = WorkspaceTab::ALL
            .iter()
            .copied()
            .map(WorkspaceTab::label)
            .collect();
        assert_eq!(
            labels,
            vec!["Arquivos", "Edição", "Efeitos", "Áudio", "Entrega"]
        );
    }
}
