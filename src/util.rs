use ratatui::layout::Rect;

/// Filtra `items` mantendo apenas os que contêm, em qualquer ordem, todos os
/// termos (separados por espaço) presentes em `query` (case-insensitive).
pub fn filter_items(items: &[String], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let terms: Vec<&str> = q.split_whitespace().collect();

    if terms.is_empty() {
        return items.to_vec();
    }

    items
        .iter()
        .filter(|e| {
            let lower = e.to_lowercase();
            terms.iter().all(|t| lower.contains(t))
        })
        .cloned()
        .collect()
}

/// Calcula um retângulo de tamanho fixo centralizado dentro de `r`.
pub fn centered_fixed_rect(width: u16, height: u16, r: Rect) -> Rect {
    let col = r.width.saturating_sub(width) / 2;
    let row = r.height.saturating_sub(height) / 2;
    Rect::new(col, row, width.min(r.width), height.min(r.height))
}

/// Retorna o final de `text` que cabe em `width` colunas. Usado nos campos
/// de uma linha só (Paragraph sem wrap trunca do fim automaticamente): sem
/// isso, o cursor digitado no fim do texto some da área visível assim que o
/// conteúdo passa da largura da caixa, dando a impressão de que novos
/// caracteres digitados não estão sendo registrados.
pub fn scroll_tail(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        text.to_string()
    } else {
        chars[chars.len() - width..].iter().collect()
    }
}

/// Glifo de cursor para campos de senha mascarados, alternando conforme a
/// paridade de `len` (o tamanho atual do valor digitado). Um campo mascarado
/// só mostra asteriscos idênticos, então uma vez que a caixa enche e passa a
/// rolar (`scroll_tail`), digitar mais um caractere não muda visualmente
/// nada — a janela desliza sobre um texto que parece igual em qualquer
/// posição. Alternar o glifo do cursor a cada tecla dá o "isso mudou" que a
/// rolagem sozinha não consegue dar nesse caso.
pub fn mask_cursor(len: usize) -> char {
    if len % 2 == 0 { '█' } else { '▓' }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_items_empty_query_returns_all() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(filter_items(&items, ""), items);
    }

    #[test]
    fn filter_items_single_term_is_case_insensitive() {
        let items = vec!["GitHub/user".to_string(), "Bank/acct".to_string()];
        assert_eq!(filter_items(&items, "github"), vec!["GitHub/user".to_string()]);
    }

    #[test]
    fn filter_items_multi_term_requires_all_terms() {
        let items = vec![
            "Work/GitHub".to_string(),
            "Personal/GitHub".to_string(),
            "Work/GitLab".to_string(),
        ];
        assert_eq!(
            filter_items(&items, "work github"),
            vec!["Work/GitHub".to_string()]
        );
    }

    #[test]
    fn filter_items_no_match_returns_empty() {
        let items = vec!["a".to_string()];
        assert!(filter_items(&items, "zzz").is_empty());
    }

    #[test]
    fn centered_fixed_rect_centers_within_bounds() {
        let outer = Rect::new(0, 0, 100, 40);
        let inner = centered_fixed_rect(50, 10, outer);
        assert_eq!(inner, Rect::new(25, 15, 50, 10));
    }

    #[test]
    fn centered_fixed_rect_clamps_when_larger_than_outer() {
        let outer = Rect::new(0, 0, 20, 5);
        let inner = centered_fixed_rect(50, 10, outer);
        assert_eq!(inner.width, 20);
        assert_eq!(inner.height, 5);
    }

    #[test]
    fn scroll_tail_returns_text_unchanged_when_it_fits() {
        assert_eq!(scroll_tail("abc", 5), "abc");
        assert_eq!(scroll_tail("abc", 3), "abc");
    }

    #[test]
    fn scroll_tail_keeps_only_the_end_when_overflowing() {
        assert_eq!(scroll_tail("abcdef", 3), "def");
        assert_eq!(scroll_tail("abcdef█", 4), "def█");
    }

    #[test]
    fn mask_cursor_alternates_with_each_char_typed() {
        let glyphs: Vec<char> = (0..4).map(mask_cursor).collect();
        assert_eq!(glyphs, vec!['█', '▓', '█', '▓']);
    }
}
