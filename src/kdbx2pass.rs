//! Importa um banco KeePassXC (.kdbx) para um password-store do `pass`,
//! encriptando cada entrada para os destinatários GPG informados.
//!
//! Porta do antigo `kdbx2pass.sh` (bash + python) para Rust: nenhum arquivo
//! temporário é escrito em disco (o CSV exportado — que contém todas as
//! senhas em texto puro — fica só em memória, em `Zeroizing<String>`, ao
//! invés do `mktemp` + `shred` do script original). O CSV é então quebrado
//! em `Vec<Vec<String>>` (não `Zeroizing`) pra facilitar o parse; a coluna
//! de senha de cada linha é zerada manualmente logo depois de extraída
//! (ver `run`), então nenhuma senha sobrevive além do tempo de processar
//! sua própria entrada.

use std::io::Write;
use std::process::{Command, Stdio};
use zeroize::{Zeroize, Zeroizing};

/// Nomes de grupo raiz que o KeePassXC usa/gera (varia por versão/locale —
/// instalações em pt-BR chamam o grupo raiz de "Senhas"); entradas nesses
/// grupos vão direto pra raiz do password-store, sem criar uma subpasta com
/// esse nome.
const ROOT_GROUP_NAMES: [&str; 5] = ["", "Principal", "Root", "Senhas", "/"];

pub fn run(db_path: &str, recipients: &[String]) -> Result<(), String> {
    if recipients.is_empty() {
        return Err("Uso: fpass --kdbx2pass <banco.kdbx> <KEYID1> [KEYID2...]".to_string());
    }

    // 1. Garante que o password-store está inicializado pros destinatários certos.
    let mut init_args = vec!["init"];
    init_args.extend(recipients.iter().map(|s| s.as_str()));
    run_pass_cmd(&init_args, None)?;

    // 2. Exporta o kdbx pra CSV (pede a senha mestra do kdbx uma vez).
    let password = Zeroizing::new(
        rpassword::prompt_password(format!("Senha mestra de {}: ", db_path))
            .map_err(|e| format!("Erro lendo senha: {}", e))?,
    );
    let csv = export_kdbx_csv(db_path, &password)?;

    // 3. Faz o parse e importa cada entrada.
    let mut rows = parse_csv(&csv);
    if rows.is_empty() {
        return Err("CSV exportado está vazio.".to_string());
    }
    let header = rows.remove(0);
    let col = |name: &str| header.iter().position(|h| h == name);
    let idx_group = col("Group");
    let idx_title = col("Title");
    let idx_username = col("Username");
    let idx_password = col("Password");
    let idx_url = col("URL");
    let idx_notes = col("Notes");
    let get = |row: &[String], idx: Option<usize>| -> String { idx.and_then(|i| row.get(i)).cloned().unwrap_or_default() };

    for row in rows.iter_mut() {
        let raw_group = get(row, idx_group);
        let raw_group = raw_group.trim_matches('/').replace("/./", "/");
        let title = get(row, idx_title);
        let title = if title.trim().is_empty() { "sem-titulo".to_string() } else { title.trim().to_string() };
        let username = get(row, idx_username);
        let entry_password = Zeroizing::new(get(row, idx_password));
        // A cópia em texto puro dentro de `row` já foi extraída acima;
        // zera na hora pra não deixar a senha sobrevivendo em memória não
        // protegida pelo resto da importação (`rows` só é dropado no fim).
        if let Some(i) = idx_password {
            if let Some(cell) = row.get_mut(i) { cell.zeroize(); }
        }
        let url = get(row, idx_url);
        let notes = get(row, idx_notes);

        let safe_title = sanitize_title(&title);

        let mut segments: Vec<&str> = if raw_group.is_empty() { Vec::new() } else { raw_group.split('/').collect() };
        if segments.first().is_some_and(|s| ROOT_GROUP_NAMES.contains(s)) {
            segments.remove(0);
        }
        let group = segments.join("/");
        let entry_path = if group.is_empty() { safe_title } else { format!("{}/{}", group, safe_title) };

        // Monta o conteúdo multi-linha no formato que o pass espera:
        // linha 1 = senha, linhas seguintes = metadados.
        let mut content = Zeroizing::new(entry_password.to_string());
        if !username.is_empty() { content.push_str(&format!("\nusername: {}", username)); }
        if !url.is_empty() { content.push_str(&format!("\nurl: {}", url)); }
        if !notes.is_empty() { content.push_str(&format!("\nnotes: {}", notes)); }

        println!("Importando: {}", entry_path);
        run_pass_cmd(&["insert", "-m", "-f", &entry_path], Some(content.as_bytes()))?;
    }

    println!("Conversão concluída. Revise com: pass");
    Ok(())
}

/// Substitui qualquer caractere fora de [letras/dígitos unicode, `_-. `] por
/// `_`, espelhando o `re.sub(r'[^\w\-. ]', '_', title)` do script original.
fn sanitize_title(title: &str) -> String {
    let safe: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ') { c } else { '_' })
        .collect();
    let trimmed = safe.trim();
    if trimmed.is_empty() { "sem-titulo".to_string() } else { trimmed.to_string() }
}

fn export_kdbx_csv(db_path: &str, password: &str) -> Result<Zeroizing<String>, String> {
    let mut cmd = Command::new("keepassxc-cli");
    cmd.args(["export", "--format", "csv", db_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Não foi possível executar keepassxc-cli: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(password.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let output = child.wait_with_output().map_err(|e| format!("Erro aguardando keepassxc-cli: {}", e))?;
    if output.status.success() {
        Ok(Zeroizing::new(String::from_utf8_lossy(&output.stdout).into_owned()))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn run_pass_cmd(args: &[&str], stdin_data: Option<&[u8]>) -> Result<(), String> {
    let mut cmd = Command::new("pass");
    cmd.args(args).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    cmd.stdin(if stdin_data.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn().map_err(|e| format!("Não foi possível executar 'pass': {}", e))?;
    if let Some(data) = stdin_data {
        let mut stdin = child.stdin.take().ok_or_else(|| "stdin indisponível".to_string())?;
        stdin.write_all(data).map_err(|e| format!("Erro escrevendo no pass: {}", e))?;
    }
    let status = child.wait().map_err(|e| format!("Erro aguardando o pass: {}", e))?;
    if status.success() { Ok(()) } else { Err(format!("'pass {}' falhou", args.join(" "))) }
}

/// Parser de CSV no formato RFC4180: aspas duplas delimitam campos com
/// vírgula/quebra de linha, e `""` dentro de um campo entre aspas é uma
/// aspa literal escapada. Evita puxar uma dependência só pra isso — o
/// Notes exportado pelo KeePassXC é o único campo que costuma ter vírgulas
/// e quebras de linha de verdade, então isso precisa ser robusto.
fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_simple() {
        let rows = parse_csv("a,b,c\n1,2,3\n");
        assert_eq!(rows, vec![vec!["a", "b", "c"], vec!["1", "2", "3"]]);
    }

    #[test]
    fn parse_csv_quoted_field_with_comma_and_newline() {
        let rows = parse_csv("Title,Notes\nfoo,\"linha1,\nlinha2\"\n");
        assert_eq!(rows, vec![vec!["Title", "Notes"], vec!["foo", "linha1,\nlinha2"]]);
    }

    #[test]
    fn parse_csv_escaped_quote() {
        let rows = parse_csv("Title\n\"ele disse \"\"oi\"\"\"\n");
        assert_eq!(rows, vec![vec!["Title"], vec!["ele disse \"oi\""]]);
    }

    #[test]
    fn parse_csv_no_trailing_newline() {
        let rows = parse_csv("a,b\n1,2");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn sanitize_title_replaces_unsafe_chars() {
        assert_eq!(sanitize_title("Banco: Conta #1"), "Banco_ Conta _1");
        assert_eq!(sanitize_title("  "), "sem-titulo");
        assert_eq!(sanitize_title(""), "sem-titulo");
        assert_eq!(sanitize_title("café"), "café");
    }
}
