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
}
