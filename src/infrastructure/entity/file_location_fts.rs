use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::error::AppResult;

pub async fn search_location_ids<C>(db: &C, keyword: &str, limit: u64) -> AppResult<Vec<(i64, i64)>>
where
    C: ConnectionTrait,
{
    let Some(query) = fts_query(keyword) else {
        return Ok(Vec::new());
    };
    let name_tokens = format!("{{file_name}} : ({})", query.all_tokens);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT id, MIN(rank) AS rank
            FROM (
                SELECT rowid AS id, 0 AS rank
                FROM file_location_fts
                WHERE file_location_fts MATCH ?
                UNION ALL
                SELECT rowid AS id, 1 AS rank
                FROM file_location_fts
                WHERE file_location_fts MATCH ?
                UNION ALL
                SELECT rowid AS id, 2 AS rank
                FROM file_location_fts
                WHERE file_location_fts MATCH ?
            )
            GROUP BY id
            ORDER BY rank ASC, id ASC
            LIMIT ?
            "#,
            [
                query.name_phrase.into(),
                name_tokens.into(),
                query.any_token.into(),
                limit.into(),
            ],
        ))
        .await?;

    let mut ranked = Vec::with_capacity(rows.len());
    for row in rows {
        ranked.push((row.try_get("", "id")?, row.try_get("", "rank")?));
    }
    Ok(ranked)
}

pub async fn upsert_location_fts<C>(
    db: &C,
    location_id: i64,
    file_name: &str,
    file_path: &str,
    descriptions: &[String],
) -> AppResult<()>
where
    C: ConnectionTrait,
{
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM file_location_fts WHERE rowid = ?",
        [location_id.into()],
    ))
    .await?;
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        r#"
        INSERT INTO file_location_fts(rowid, file_name, file_path, description)
        VALUES (?, ?, ?, ?)
        "#,
        [
            location_id.into(),
            fts_document(file_name).into(),
            fts_document(file_path).into(),
            fts_document(&descriptions.join(" ")).into(),
        ],
    ))
    .await?;
    Ok(())
}

pub async fn missing_location_ids<C>(db: &C, limit: u64) -> AppResult<Vec<i64>>
where
    C: ConnectionTrait,
{
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT fl.id
            FROM file_location fl
            WHERE NOT EXISTS (
                SELECT 1 FROM file_location_fts fts WHERE fts.rowid = fl.id
            )
            LIMIT ?
            "#,
            [limit.into()],
        ))
        .await?;
    rows.into_iter()
        .map(|row| row.try_get("", "id"))
        .collect::<Result<Vec<i64>, _>>()
        .map_err(Into::into)
}

struct FtsQuery {
    name_phrase: String,
    all_tokens: String,
    any_token: String,
}

fn fts_query(keyword: &str) -> Option<FtsQuery> {
    let mut phrase_terms = Vec::new();
    let mut all_parts = Vec::new();
    let mut recall_parts = Vec::new();
    for token in keyword.split_whitespace() {
        let terms = token_terms(token);
        if terms.is_empty() {
            continue;
        }
        let part = terms
            .iter()
            .map(|term| quote_term(term))
            .collect::<Vec<_>>()
            .join(" AND ");
        phrase_terms.extend(terms);
        all_parts.push(part.clone());
        if !is_recall_stopword_token(token) {
            recall_parts.push(part);
        }
    }
    if all_parts.is_empty() || recall_parts.is_empty() {
        None
    } else {
        Some(FtsQuery {
            name_phrase: format!("{{file_name}} : ({})", quote_term(&phrase_terms.join(" "))),
            all_tokens: all_parts.join(" AND "),
            any_token: recall_parts
                .iter()
                .map(|part| format!("({part})"))
                .collect::<Vec<_>>()
                .join(" OR "),
        })
    }
}

const ENGLISH_STOPWORDS: &[&str] = &[
    "a", "an", "the", "in", "on", "of", "and", "or", "to", "for", "is", "at", "by", "with", "from",
];

fn is_english_stopword(term: &str) -> bool {
    ENGLISH_STOPWORDS
        .iter()
        .any(|stop| term.eq_ignore_ascii_case(stop))
}

fn is_recall_stopword_token(token: &str) -> bool {
    if token.chars().any(is_cjk) {
        return false;
    }
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in token.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            terms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    !terms.is_empty() && terms.iter().all(|term| is_english_stopword(term))
}

fn token_terms(token: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_cjk = false;

    for ch in token.chars() {
        if is_cjk(ch) {
            if !current.is_empty() && !current_cjk {
                parts.push(std::mem::take(&mut current));
            }
            current.push(ch);
            current_cjk = true;
        } else if ch.is_alphanumeric() {
            if !current.is_empty() && current_cjk {
                parts.push(std::mem::take(&mut current));
            }
            current.push(ch);
            current_cjk = false;
        } else if !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn fts_document(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev = CharKind::Other;
    for ch in text.chars() {
        let kind = char_kind(ch);
        if needs_break(prev, kind) {
            out.push(' ');
        }
        out.push(ch);
        prev = kind;
    }
    out
}

fn quote_term(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharKind {
    Cjk,
    Alnum,
    Other,
}

fn char_kind(ch: char) -> CharKind {
    if is_cjk(ch) {
        CharKind::Cjk
    } else if ch.is_alphanumeric() {
        CharKind::Alnum
    } else {
        CharKind::Other
    }
}

fn needs_break(prev: CharKind, kind: CharKind) -> bool {
    matches!(
        (prev, kind),
        (CharKind::Cjk, CharKind::Alnum) | (CharKind::Alnum, CharKind::Cjk)
    )
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2CEAF}'
    )
}
