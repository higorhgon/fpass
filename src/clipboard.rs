//! Clipboard com limpeza garantida.
//!
//! Correções em relação ao fpass 1.x:
//! - Limpeza usa `wl-copy --clear` (a flag correta) em vez de escrever string
//!   vazia no stdin — que alguns compositors Wayland ignoravam.
//! - O segredo vive em `Zeroizing<String>` inclusive na thread de limpeza.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use zeroize::Zeroizing;

pub fn copy_to_clipboard(secret: &Zeroizing<String>, is_mac: bool) -> Result<(), String> {
    let cmd_name = if is_mac { "pbcopy" } else { "wl-copy" };
    let mut child = Command::new(cmd_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Erro executando {}: {}", cmd_name, e))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "stdin do clipboard indisponível".to_string())?;
        stdin
            .write_all(secret.as_bytes())
            .map_err(|e| format!("Erro copiando: {}", e))?;
    }
    child
        .wait()
        .map_err(|e| format!("Erro no {}: {}", cmd_name, e))?;
    Ok(())
}

/// Agenda a limpeza do clipboard após `secs` segundos.
/// No Linux/Wayland também remove a senha do histórico do cliphist.
pub fn spawn_clipboard_clearer(secret: Zeroizing<String>, is_mac: bool, secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        if is_mac {
            // pbcopy não tem --clear; escrever entrada vazia limpa no macOS.
            if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
                drop(child.stdin.take()); // fecha o stdin vazio
                let _ = child.wait();
            }
        } else {
            let _ = Command::new("wl-copy")
                .arg("--clear")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if !is_mac {
            let _ = Command::new("cliphist")
                .args(["delete-query", secret.as_str()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        // `secret` (Zeroizing) é zerado na memória ao sair de escopo aqui.
    });
}
