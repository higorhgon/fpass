use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use zeroize::Zeroizing;

fn clipboard_cmd(is_mac: bool) -> &'static str {
    if is_mac { "pbcopy" } else { "wl-copy" }
}

pub fn copy_to_clipboard(text: &str, is_mac: bool) -> Result<(), String> {
    let mut cmd = Command::new(clipboard_cmd(is_mac));
    if !is_mac {
        // Sinaliza ao wl-clipboard (>=2.2.1) que o conteúdo é sensível: ele expõe
        // o mime type `x-kde-passwordManagerHint`, que gerenciadores de clipboard
        // como o cliphist reconhecem e usam para NÃO persistir o valor no
        // histórico em disco. Isso resolve o problema na origem, em vez de tentar
        // apagar a senha do histórico depois que ela já foi gravada.
        cmd.arg("--sensitive");
    }
    let mut child = cmd.stdin(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

/// Limpa o clipboard. No Wayland, `wl-copy --clear` avisa explicitamente o
/// compositor para descartar a seleção — ao contrário de simplesmente
/// escrever uma string vazia via stdin, que alguns compositores não tratam
/// como uma nova seleção válida e por isso preservam o valor anterior.
fn clear_clipboard(is_mac: bool) {
    if is_mac {
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(b""); }
            let _ = child.wait();
        }
    } else {
        let _ = Command::new("wl-copy").arg("--clear").status();
    }
}

/// Segundos até a senha ser removida do clipboard após ser copiada. Usado
/// tanto para agendar a limpeza quanto para exibir a contagem regressiva na
/// área de dicas da UI.
pub const CLIPBOARD_CLEAR_SECS: u64 = 10;

pub fn spawn_clipboard_clearer(password: Zeroizing<String>, is_mac: bool) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(CLIPBOARD_CLEAR_SECS));
        clear_clipboard(is_mac);
        if !is_mac {
            // Fallback para wl-clipboard < 2.2.1 / cliphist sem suporte ao hint
            // `--sensitive` (ver copy_to_clipboard): tenta apagar a entrada do
            // histórico persistido. Sem efeito (e sem erro) se o cliphist não
            // estiver instalado ou se o hint já tiver evitado o armazenamento.
            let _ = Command::new("cliphist").args(["delete-query", password.as_str()]).status();
        }
        // `password` sai de escopo aqui e o Zeroizing sobrescreve a memória.
    });
}
