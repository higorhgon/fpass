//! Backend do `pass` (the standard unix password manager).
//!
//! Segurança:
//! - A autenticação é do gpg-agent/pinentry — o fpass NUNCA vê a senha mestra.
//! - Segredos de entradas trafegam em `Zeroizing<String>` e são zerados no drop.
//! - Listagem lê o filesystem diretamente (arquivos .gpg), sem decifrar nada.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zeroize::Zeroizing;

pub struct PassStore {
    root: PathBuf,
}

/// Campos estruturados de uma entrada no formato convencional do pass:
/// linha 1 = senha; linhas seguintes = "chave: valor"; linhas desconhecidas
/// são preservadas em `extra` para não perder dados de outras ferramentas.
#[derive(Default)]
pub struct EntryData {
    pub password: Zeroizing<String>,
    pub username: String,
    pub url: String,
    pub extra: Vec<String>,
}

/// Executa `pass` com os args dados; helper único usado por todas as operações
/// (elimina a duplicação de Command::new que existia no fpass 1.x).
fn run_pass(
    root: &Path,
    args: &[&str],
    stdin_data: Option<&[u8]>,
) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("pass");
    cmd.args(args)
        .env("PASSWORD_STORE_DIR", root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.stdin(if stdin_data.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Não foi possível executar 'pass': {}", e))?;

    if let Some(data) = stdin_data {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "stdin indisponível".to_string())?;
        stdin
            .write_all(data)
            .map_err(|e| format!("Erro escrevendo no pass: {}", e))?;
        // stdin é dropado aqui, fechando o pipe.
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Erro aguardando o pass: {}", e))?;

    if output.status.success() {
        Ok(output)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(err.trim().to_string())
    }
}

impl PassStore {
    /// Resolve o store: config > $PASSWORD_STORE_DIR > ~/.password-store.
    pub fn open(config_path: Option<&str>) -> Result<Self, String> {
        let root = if let Some(p) = config_path {
            PathBuf::from(p)
        } else if let Ok(env) = std::env::var("PASSWORD_STORE_DIR") {
            PathBuf::from(env)
        } else {
            let home = std::env::var("HOME").map_err(|_| "HOME não definido".to_string())?;
            PathBuf::from(home).join(".password-store")
        };

        if !root.join(".gpg-id").exists() {
            return Err(format!(
                "Password store não inicializado em {} — rode: pass init <SEU-GPG-KEY-ID>",
                root.display()
            ));
        }
        Ok(Self { root })
    }

    #[allow(dead_code)] // útil para debugging/futuro suporte a múltiplos stores
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Lista entradas (arquivos .gpg, caminho relativo sem extensão) e grupos
    /// (diretórios). Não decifra nada — leitura pura de filesystem.
    pub fn list(&self) -> (Vec<String>, Vec<String>) {
        let mut entries = Vec::new();
        let mut groups = Vec::new();
        walk(&self.root, &self.root, &mut entries, &mut groups);
        entries.sort();
        groups.sort();
        (entries, groups)
    }

    /// Decifra e retorna a entrada estruturada.
    pub fn show(&self, entry: &str) -> Result<EntryData, String> {
        let output = run_pass(&self.root, &["show", entry], None)?;
        let content = Zeroizing::new(String::from_utf8_lossy(&output.stdout).into_owned());
        Ok(parse_entry(&content))
    }

    /// Cria/sobrescreve a entrada com conteúdo multi-linha.
    pub fn insert(&self, entry: &str, data: &EntryData) -> Result<(), String> {
        let content = build_entry_content(data);
        run_pass(&self.root, &["insert", "-m", "-f", entry], Some(content.as_bytes()))?;
        Ok(())
    }

    /// Renomeia/move uma entrada.
    pub fn mv(&self, from: &str, to: &str) -> Result<(), String> {
        run_pass(&self.root, &["mv", "-f", from, to], None)?;
        Ok(())
    }

    /// Remove uma entrada.
    pub fn rm(&self, entry: &str) -> Result<(), String> {
        run_pass(&self.root, &["rm", "-f", entry], None)?;
        Ok(())
    }

    /// Remove um grupo (diretório) vazio.
    pub fn rmdir_empty(&self, group: &str) -> Result<(), String> {
        let dir = self.root.join(group);
        fs::remove_dir(&dir).map_err(|e| format!("Erro removendo grupo: {}", e))
    }
}

fn walk(root: &Path, dir: &Path, entries: &mut Vec<String>, groups: &mut Vec<String>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Ignora .git, .gpg-id, .extensions e demais ocultos
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if let Ok(rel) = path.strip_prefix(root) {
                groups.push(rel.to_string_lossy().into_owned());
            }
            walk(root, &path, entries, groups);
        } else if name.ends_with(".gpg") {
            if let Ok(rel) = path.strip_prefix(root) {
                let rel = rel.to_string_lossy();
                entries.push(rel.trim_end_matches(".gpg").to_string());
            }
        }
    }
}

/// Faz o parse do conteúdo decifrado no formato convencional do pass.
pub fn parse_entry(content: &str) -> EntryData {
    let mut data = EntryData::default();
    let mut lines = content.lines();
    if let Some(first) = lines.next() {
        data.password = Zeroizing::new(first.to_string());
    }
    for line in lines {
        let lower = line.to_lowercase();
        if let Some(v) = lower
            .strip_prefix("username:")
            .or_else(|| lower.strip_prefix("user:"))
            .or_else(|| lower.strip_prefix("login:"))
        {
            // usa o valor original (preserva maiúsculas), com base no offset
            let offset = line.len() - v.len();
            data.username = line[offset..].trim().to_string();
        } else if let Some(v) = lower.strip_prefix("url:") {
            let offset = line.len() - v.len();
            data.url = line[offset..].trim().to_string();
        } else if !line.trim().is_empty() {
            data.extra.push(line.to_string());
        }
    }
    data
}

/// Monta o conteúdo multi-linha, preservando as linhas extras não modeladas.
pub fn build_entry_content(data: &EntryData) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::new());
    out.push_str(&data.password);
    out.push('\n');
    if !data.username.is_empty() {
        out.push_str(&format!("username: {}\n", data.username));
    }
    if !data.url.is_empty() {
        out.push_str(&format!("url: {}\n", data.url));
    }
    for line in &data.extra {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_entry() {
        let content = "s3cr3t\nusername: fulano\nurl: https://exemplo.com.br\nnotes: vip\n";
        let d = parse_entry(content);
        assert_eq!(d.password.as_str(), "s3cr3t");
        assert_eq!(d.username, "fulano");
        assert_eq!(d.url, "https://exemplo.com.br");
        assert_eq!(d.extra, vec!["notes: vip"]);
    }

    #[test]
    fn parse_password_only() {
        let d = parse_entry("apenas-a-senha\n");
        assert_eq!(d.password.as_str(), "apenas-a-senha");
        assert!(d.username.is_empty());
        assert!(d.url.is_empty());
        assert!(d.extra.is_empty());
    }

    #[test]
    fn parse_alternative_username_keys() {
        let d = parse_entry("x\nlogin: hg\n");
        assert_eq!(d.username, "hg");
        let d = parse_entry("x\nUser: HG\n");
        assert_eq!(d.username, "HG");
    }

    #[test]
    fn build_preserves_extra_lines() {
        let mut d = EntryData::default();
        d.password = Zeroizing::new("p".into());
        d.username = "u".into();
        d.url = "https://x".into();
        d.extra = vec!["otp: ABCDEF".into(), "notes: manter".into()];
        let content = build_entry_content(&d);
        assert_eq!(
            content.as_str(),
            "p\nusername: u\nurl: https://x\notp: ABCDEF\nnotes: manter\n"
        );
    }

    #[test]
    fn roundtrip_parse_build() {
        let original = "senha\nusername: hg\nurl: https://a.b\ncustom: data\n";
        let d = parse_entry(original);
        let rebuilt = build_entry_content(&d);
        assert_eq!(rebuilt.as_str(), original);
    }
}
