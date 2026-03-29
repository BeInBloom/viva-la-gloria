use serde::{Deserialize, Serialize};

use crate::models::ListCardsQuery;

const DEFAULT_CARD_PAGE_SIZE: usize = 24;
const MAX_CARD_PAGE_SIZE: usize = 100;

#[derive(Debug, Deserialize)]
pub struct GeneratePdfRequest {
    pub card_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeneratePdfResponse {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct ListCardsReq {
    pub after: Option<String>,
    pub limit: Option<usize>,
}

impl From<ListCardsReq> for ListCardsQuery {
    fn from(value: ListCardsReq) -> Self {
        Self {
            after: normalize_optional_card_id(value.after),
            limit: clamp_card_page_size(value.limit),
        }
    }
}

pub fn normalize_card_id(card_id: String) -> String {
    let card_id = card_id.trim();
    format!("{card_id:0>3}")
}

fn normalize_optional_card_id(after: Option<String>) -> Option<String> {
    let after = after?;
    let after = after.trim();

    if after.is_empty() {
        return None;
    }

    Some(format!("{after:0>3}"))
}

fn clamp_card_page_size(limit: Option<usize>) -> usize {
    match limit {
        Some(value @ 1..=MAX_CARD_PAGE_SIZE) => value,
        Some(value) if value > MAX_CARD_PAGE_SIZE => MAX_CARD_PAGE_SIZE,
        _ => DEFAULT_CARD_PAGE_SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::{ListCardsQuery, ListCardsReq, normalize_card_id};

    #[test]
    fn normalize_card_id_adds_missing_leading_zeroes() {
        assert_eq!(normalize_card_id("1".to_owned()), "001");
        assert_eq!(normalize_card_id("12".to_owned()), "012");
        assert_eq!(normalize_card_id("123".to_owned()), "123");
    }

    #[test]
    fn normalize_card_id_only_normalizes_width() {
        assert_eq!(normalize_card_id("ab".to_owned()), "0ab");
        assert_eq!(normalize_card_id("abc".to_owned()), "abc");
    }

    #[test]
    fn list_cards_query_uses_defaults() {
        let query: ListCardsQuery = ListCardsReq {
            after: None,
            limit: None,
        }
        .into();

        assert!(query.after.is_none());
        assert_eq!(query.limit, 24);
    }

    #[test]
    fn list_cards_query_normalizes_after_and_clamps_limit() {
        let query: ListCardsQuery = ListCardsReq {
            after: Some("7".to_owned()),
            limit: Some(999),
        }
        .into();

        assert_eq!(query.after.as_deref(), Some("007"));
        assert_eq!(query.limit, 100);
    }

    #[test]
    fn list_cards_query_treats_blank_after_as_missing() {
        let query: ListCardsQuery = ListCardsReq {
            after: Some("   ".to_owned()),
            limit: Some(0),
        }
        .into();

        assert!(query.after.is_none());
        assert_eq!(query.limit, 24);
    }

    #[test]
    fn list_cards_query_trims_after_before_normalizing() {
        let query: ListCardsQuery = ListCardsReq {
            after: Some(" 12 ".to_owned()),
            limit: Some(5),
        }
        .into();

        assert_eq!(query.after.as_deref(), Some("012"));
        assert_eq!(query.limit, 5);
    }
}
