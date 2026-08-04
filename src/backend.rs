//! Camada de abstração sobre os backends de senha suportados: KeePassXC (via
//! `keepassxc-cli`) e `pass` (via gpg). O resto do app (app.rs, db_app.rs,
//! ui.rs) fala apenas com `Backend`/`DbRef`, sem repetir a lógica de comandos
//! de cada gerenciador.

use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use crate::keepass::{self, run_kpcli};
use crate::pass::{self, PassStore};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Keepass,
    Pass,
}

impl BackendKind {
    pub fn label(&self) -> &'static str {
        match self {
            BackendKind::Keepass => "KeePassXC",
            BackendKind::Pass => "pass",
        }
    }
}

/// Referência a um banco de dados encontrado na busca, antes de ser aberto.
#[derive(Clone)]
pub struct DbRef {
    pub path: String,
    pub kind: BackendKind,
}

/// Dados de uma entrada, no superconjunto dos campos dos dois formatos.
/// `extra` só é usado pelo backend `pass`, para preservar linhas não
/// modeladas (ex.: OTP) ao editar uma entrada que já as possuía.
#[derive(Default, Clone)]
pub struct EntryData {
    pub username: String,
    pub password: Zeroizing<String>,
    pub url: String,
    pub notes: String,
    pub extra: Vec<String>,
}

pub enum Backend {
    Keepass { db_path: String, password: Zeroizing<String> },
    Pass { store: PassStore, passphrase: Zeroizing<String> },
}

impl Backend {
    pub fn kind(&self) -> BackendKind {
        match self {
            Backend::Keepass { .. } => BackendKind::Keepass,
            Backend::Pass { .. } => BackendKind::Pass,
        }
    }

    /// Abre e autentica no banco referenciado por `db_ref`, validando a
    /// senha/passphrase antes de devolver um `Backend` pronto para uso.
    pub fn open(db_ref: &DbRef, secret: Zeroizing<String>) -> Result<Self, String> {
        match db_ref.kind {
            BackendKind::Keepass => {
                let result = run_kpcli(&["ls", "-q", &db_ref.path], &[secret.as_str()])?;
                if result.success {
                    Ok(Backend::Keepass { db_path: db_ref.path.clone(), password: secret })
                } else {
                    Err("Senha incorreta!".to_string())
                }
            }
            BackendKind::Pass => {
                let store = PassStore::open(Path::new(&db_ref.path))?;
                store.verify_passphrase(&secret)?;
                Ok(Backend::Pass { store, passphrase: secret })
            }
        }
    }

    /// Lista entradas e grupos, marcando grupos vazios com o sufixo
    /// `/[vazio]` para que apareçam navegáveis (e excluíveis) na lista.
    pub fn list(&self) -> (Vec<String>, Vec<String>) {
        match self {
            Backend::Keepass { db_path, password } => {
                let mut entries = Vec::new();
                let mut groups = Vec::new();
                if let Ok(result) = run_kpcli(&["ls", "-Rfq", db_path], &[password.as_str()]) {
                    let lines: Vec<String> = result.stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    for line in &lines {
                        if line.ends_with('/') { groups.push(line.trim_end_matches('/').to_string()); }
                        else { entries.push(line.clone()); }
                    }
                    for g in &groups {
                        let prefix = format!("{}/", g);
                        let is_empty = !lines.iter().any(|l| l.starts_with(&prefix) && l != &prefix);
                        if is_empty { entries.push(format!("{}/[vazio]", g)); }
                    }
                }
                (entries, groups)
            }
            Backend::Pass { store, .. } => {
                let (mut entries, groups) = store.list();
                for g in &groups {
                    let prefix = format!("{}/", g);
                    let has_children = entries.iter().any(|e| e.starts_with(&prefix)) || groups.iter().any(|o| o.starts_with(&prefix));
                    if !has_children { entries.push(format!("{}/[vazio]", g)); }
                }
                (entries, groups)
            }
        }
    }

    /// Título de exibição de uma entrada. No KeePassXC pode divergir do
    /// último segmento do path (atributo Title independente); no pass o
    /// título é sempre o nome do arquivo.
    pub fn title_for(&self, path: &str, fallback: &str) -> String {
        match self {
            Backend::Keepass { db_path, password } => {
                let t = run_kpcli(&["show", "-q", db_path, path, "-a", "Title"], &[password.as_str()])
                    .map(|r| r.stdout.trim().to_string())
                    .unwrap_or_default();
                if t.is_empty() { fallback.to_string() } else { t }
            }
            Backend::Pass { .. } => fallback.to_string(),
        }
    }

    /// Busca só a senha de uma entrada (usado por copiar-senha, evita buscar
    /// os demais atributos à toa).
    pub fn fetch_password(&self, path: &str) -> Result<Zeroizing<String>, String> {
        match self {
            Backend::Keepass { db_path, password } => {
                let result = run_kpcli(&["show", "-q", db_path, path, "-a", "Password"], &[password.as_str()])?;
                Ok(Zeroizing::new(result.stdout.trim().to_string()))
            }
            Backend::Pass { store, passphrase } => {
                let data = store.show_with_passphrase(path, passphrase)?;
                Ok(data.password)
            }
        }
    }

    /// Busca todos os campos de uma entrada.
    pub fn fetch_entry(&self, path: &str) -> Result<EntryData, String> {
        match self {
            Backend::Keepass { db_path, password } => {
                let get = |attr: &str| -> String {
                    run_kpcli(&["show", "-q", db_path, path, "-a", attr], &[password.as_str()])
                        .map(|r| r.stdout.trim().to_string())
                        .unwrap_or_default()
                };
                Ok(EntryData {
                    username: get("UserName"),
                    password: Zeroizing::new(get("Password")),
                    url: get("URL"),
                    notes: get("Notes"),
                    extra: Vec::new(),
                })
            }
            Backend::Pass { store, passphrase } => {
                let data = store.show_with_passphrase(path, passphrase)?;
                let notes = data.extra.join("\n");
                Ok(EntryData { username: data.username, password: data.password, url: data.url, notes, extra: data.extra })
            }
        }
    }

    /// Cria uma entrada nova em `path` (dentro de `group`, já criado se
    /// necessário).
    pub fn add_entry(&self, path: &str, group: &str, data: &EntryData) -> Result<(), String> {
        match self {
            Backend::Keepass { db_path, password } => {
                if !group.is_empty() {
                    let _ = run_kpcli(&["mkdir", "-q", db_path, group], &[password.as_str()]);
                }
                let result = run_kpcli(
                    &["add", "-q", "-p", "-u", &data.username, "--url", &data.url, "--notes", &data.notes, db_path, path],
                    &[password.as_str(), data.password.as_str(), data.password.as_str()],
                )?;
                if result.success { Ok(()) } else { Err(result.stderr) }
            }
            Backend::Pass { store, .. } => {
                let pd = pass::EntryData { password: data.password.clone(), username: data.username.clone(), url: data.url.clone(), extra: data.extra.clone() };
                store.insert(path, &pd)
            }
        }
    }

    /// Edita a entrada em `old_path`, movendo-a para `new_path` se o path
    /// mudou (grupo e/ou título).
    pub fn edit_entry(&self, old_path: &str, new_path: &str, group: &str, data: &EntryData) -> Result<(), String> {
        match self {
            Backend::Keepass { db_path, password } => {
                if !group.is_empty() {
                    let _ = run_kpcli(&["mkdir", "-q", db_path, group], &[password.as_str()]);
                }
                if new_path != old_path {
                    // O comando 'mv' do keepassxc-cli espera [database] [origem] [grupo_destino].
                    // Se o destino for a raiz, o grupo_destino deve ser "/".
                    let dest_group = if group.is_empty() { "/" } else { group };
                    let _ = run_kpcli(&["mv", "-q", db_path, old_path, dest_group], &[password.as_str()]);
                }
                let result = run_kpcli(
                    &["edit", "-q", "-p", "-u", &data.username, "--url", &data.url, "--notes", &data.notes, db_path, new_path],
                    &[password.as_str(), data.password.as_str(), data.password.as_str()],
                )?;
                if result.success { Ok(()) } else { Err(result.stderr) }
            }
            Backend::Pass { store, .. } => {
                let pd = pass::EntryData { password: data.password.clone(), username: data.username.clone(), url: data.url.clone(), extra: data.extra.clone() };
                store.insert(old_path, &pd)?;
                if new_path != old_path {
                    store.mv(old_path, new_path)?;
                }
                Ok(())
            }
        }
    }

    pub fn remove_entry(&self, path: &str) -> Result<(), String> {
        match self {
            Backend::Keepass { db_path, password } => {
                let result = run_kpcli(&["rm", "-q", db_path, path], &[password.as_str()])?;
                if result.success { Ok(()) } else { Err(result.stderr) }
            }
            Backend::Pass { store, .. } => store.rm(path),
        }
    }

    /// Renomeia o último segmento de `old_group` para `new_name`, preservando
    /// o grupo pai e toda a subestrutura (subgrupos/entradas) abaixo dele.
    pub fn rename_group(&self, old_group: &str, new_name: &str) -> Result<(), String> {
        let parent = match old_group.rfind('/') { Some(idx) => &old_group[..idx], None => "" };
        let new_group = if parent.is_empty() { new_name.to_string() } else { format!("{}/{}", parent, new_name) };

        match self {
            Backend::Pass { store, .. } => store.mv(old_group, &new_group),
            Backend::Keepass { db_path, password } => {
                // keepassxc-cli não tem um comando nativo de renomear grupo:
                // recria a subestrutura sob o novo nome, move cada entrada
                // para lá preservando o caminho relativo, e por fim remove a
                // árvore antiga (agora vazia) da folha para a raiz.
                let (entries, groups) = self.list();
                let old_prefix = format!("{}/", old_group);

                // Sempre cria o grupo novo, mesmo se `old_group` não tiver
                // nenhum filho — senão um grupo vazio "some" em vez de ser
                // renomeado (mkdir/mv abaixo só criam destinos para conteúdo
                // que de fato existir dentro de `old_group`).
                let _ = run_kpcli(&["mkdir", "-q", db_path, &new_group], &[password.as_str()]);

                for g in &groups {
                    if let Some(rel) = g.strip_prefix(&old_prefix) {
                        let dest = format!("{}/{}", new_group, rel);
                        let _ = run_kpcli(&["mkdir", "-q", db_path, &dest], &[password.as_str()]);
                    }
                }
                for e in &entries {
                    if e.ends_with("/[vazio]") { continue; }
                    let Some(rel) = e.strip_prefix(&old_prefix) else { continue; };
                    let dest_group = match rel.rfind('/') {
                        Some(idx) => format!("{}/{}", new_group, &rel[..idx]),
                        None => new_group.clone(),
                    };
                    let _ = run_kpcli(&["mkdir", "-q", db_path, &dest_group], &[password.as_str()]);
                    let _ = run_kpcli(&["mv", "-q", db_path, e, &dest_group], &[password.as_str()]);
                }

                let mut old_tree: Vec<&String> = groups.iter().filter(|g| *g == old_group || g.starts_with(&old_prefix)).collect();
                old_tree.sort_by_key(|g| std::cmp::Reverse(g.len()));
                for g in old_tree {
                    let _ = run_kpcli(&["rmdir", "-q", db_path, g], &[password.as_str()]);
                }
                Ok(())
            }
        }
    }

    pub fn remove_group(&self, group: &str) -> Result<(), String> {
        match self {
            Backend::Keepass { db_path, password } => {
                let result = run_kpcli(&["rmdir", "-q", db_path, group], &[password.as_str()])?;
                if result.success { Ok(()) } else { Err(result.stderr) }
            }
            Backend::Pass { store, .. } => store.rm_group(group),
        }
    }
}

/// Busca bancos de dados KeePassXC (.kdbx) em `search_path` e stores do pass
/// dentro de `~/.password-store`.
pub fn find_databases(search_path: &str) -> Vec<DbRef> {
    let mut dbs: Vec<DbRef> = keepass::find_databases(search_path)
        .into_iter()
        .map(|p| DbRef { path: p, kind: BackendKind::Keepass })
        .collect();

    let home = std::env::var("HOME").unwrap_or_default();
    let store_root = PathBuf::from(format!("{}/.password-store", home));
    let mut pass_roots = Vec::new();
    find_pass_stores(&store_root, &mut pass_roots);
    dbs.extend(pass_roots.into_iter().map(|p| DbRef { path: p.to_string_lossy().into_owned(), kind: BackendKind::Pass }));

    dbs
}

/// Procura diretórios com `.gpg-id`: cada um é um password-store do `pass`.
/// Não desce dentro de um diretório já identificado como store — tudo que
/// há ali pertence a ele, não são stores independentes.
fn find_pass_stores(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    if dir.join(".gpg-id").exists() {
        out.push(dir.to_path_buf());
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return; };
    for item in read.flatten() {
        let path = item.path();
        if path.is_dir() {
            find_pass_stores(&path, out);
        }
    }
}
