# fpass

Gerenciador de senhas com interface TUI (Terminal User Interface) para bancos KeePassXC (.kdbx), escrito em Rust.

## Funcionalidades

- Seletor de banco de dados com busca multi-termo e navegação estilo vim
- Modal integrado para desbloqueio de banco com validação de senha
- Listagem, adição, edição e exclusão de entradas via `keepassxc-cli`
- Motor de frecency (frequência + recência) para ordenação inteligente
- Sistema de temas personalizáveis em TOML
- Atalhos de teclado: `j/k`, `gg/G`, `Ctrl+U/D`, `/` para buscar

## Requisitos

- [Rust](https://rustup.rs/) (edition 2024)
- [KeePassXC](https://keepassxc.org/) com `keepassxc-cli` disponível no PATH

## Instalação

```bash
git clone https://github.com/<usuario>/fpass.git
cd fpass
cargo build --release
```

## Uso

```bash
./target/release/fpass
```

O programa busca automaticamente arquivos `.kdbx` no home directory e exibe uma interface interativa para seleção, desbloqueio e gestão de senhas.

## Configuração

Arquivos em `~/.config/fpass/`:

- `config.toml` — caminho de busca, recency e tema ativo
- `themes/*.toml` — definições de cores personalizadas

Exemplo de `config.toml`:

```toml
[general]
path = "~/docs/keepass"
recency = true
theme = "meu-tema"
```

Exemplo de tema em `~/.config/fpass/themes/tema.toml`:

```toml
[theme]
name = "meu-tema"

[colors]
Title = "#00AAAA"
Base = "#CCCCCC"
Guidance = "#666666"
```

## Atalhos

| Tecla | Ação |
|-------|------|
| `j` / `k` | Navegar para baixo/cima |
| `gg` / `G` | Ir para o topo/final |
| `Ctrl+U` / `Ctrl+D` | Meia página para cima/baixo |
| `/` ou `f` | Entrar no modo de busca |
| `Enter` | Selecionar / Confirmar |
| `ESC` / `q` | Cancelar / Sair |
| `Ctrl+C` | Sair do programa |

## Dependências

- [ratatui](https://github.com/ratatui/ratatui) —-renderização TUI
- [crossterm](https://github.com/crossterm-rs/crossterm) — terminal backend
- [sha2](https://crates.io/crates/sha2) — hash SHA-256 para history
- [toml](https://crates.io/crates/toml) e [serde](https://crates.io/crates/serde) — configuração
