//! Usage state and the decay ordering built on it (R-B C3).
//!
//! WHAT WAS MISSING. Every recall hit was forgotten the moment it was rendered:
//! `recall_log` recorded that a recall happened (651 rows on the live DB,
//! 2026-09-27T21:39+08:00) and **no row was ever written back**, so the store
//! could not answer "has this memory ever been used?" — `memories` had 0 of 7
//! usage-ish columns. Ordering was `ORDER BY updated_at DESC` (write time), which
//! the injection contract itself described as "the N most recent, not the N most
//! relevant".
//!
//! THE DECAY IS A RANKING INPUT, NOT A DELETION RULE. MemoryBank's Ebbinghaus
//! curve (`R = e^{-t/S}`, S grows with rehearsal) is adopted as the recency term;
//! the "forget when it decays" half is deliberately NOT adopted — the corpus is a
//! user's technical facts, an automatic drop is unrecoverable, and an erasure
//! must stay an explicit, audited act (R-B B.5/B.6).
//!
//! NULL DISCIPLINE (0020_memory_semantics.sql:26-29): `access_count IS NULL` means
//! "this row predates the counter", which is NOT "used zero times". Every reader
//! here coalesces EXPLICITLY and the three-state distinction stays available to a
//! caller that asks for it.

use chrono::{DateTime, Utc};
use ruagent_store::Db;

use crate::write::DbError;

/// Weight of the recency term added to a caller's base score. One place.
pub const DECAY_LAMBDA: f64 = 0.5;

/// The decay time constant in days at zero recorded uses. With `S = 1 + uses`,
/// a row used 5 times decays 6× slower than one never used.
pub const DECAY_DAYS: f64 = 14.0;

/// How many rows a group reads before the decay ordering cuts them to its limit.
///
/// WHY AN OVERFETCH. The read path can only order by columns SQLite knows; the
/// decay term needs `access_count` and the row's own timestamp, so the ordering
/// has to happen in Rust — on MORE rows than the group keeps, or the usage term
/// could reorder the window but never change which rows are in it.
pub const DECAY_OVERFETCH: u32 = 3;

/// The decay ordering score: `base + DECAY_LAMBDA * e^{-Δt / (S * DECAY_DAYS)}`
/// with `S = 1 + access_count` and **`Δt = now − COALESCE(last_used_at, updated_at)`**.
///
/// WHY THE CLOCK IS `COALESCE(last_used_at, updated_at)` (t31, captain's ruling):
/// the quantity this term is about is "how long since this fact was last
/// RELEVANT", and the only two things the schema knows about that are (a) when
/// the row was last used and (b) when it was written. Before this change the
/// function read `updated_at` alone, so a row that was returned to an agent five
/// times yesterday still decayed from its WRITE time — i.e. the usage counter
/// changed only the curve's steepness (S) and never the clock, which is half of
/// what C3 asked for. `last_used_at IS NULL` (never used since the counter
/// existed) falls back to `updated_at`, which for such a row is exactly the
/// instant it was written: the fallback is not a substitute fact, it is the same
/// fact.
///
/// `access_count = None` (unknown, pre-column) is treated as **0 uses** for the
/// SHAPE of the curve and is documented as such: the alternative — treating an
/// unknown row as infinitely fresh — would rank the 163 historical rows above
/// everything measured. `None` never means "more used than a measured 3".
///
/// An unparseable clock must not invent freshness: it is treated as old, and the
/// two halves are tried in order, so a corrupt `last_used_at` still falls back to
/// a usable `updated_at`.
pub fn decay_score(
    base: f64,
    updated_at: &str,
    last_used_at: Option<&str>,
    now: &str,
    access_count: Option<i64>,
) -> f64 {
    let delta_days = match DateTime::parse_from_rfc3339(now) {
        Ok(n) => {
            let stamp = last_used_at
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .or_else(|| DateTime::parse_from_rfc3339(updated_at).ok());
            match stamp {
                Some(u) => (n - u).num_seconds().max(0) as f64 / 86_400.0,
                // Neither half parses: do not invent freshness.
                None => DECAY_DAYS * 4.0,
            }
        }
        // `now` itself is unusable: the same rule, said once.
        Err(_) => DECAY_DAYS * 4.0,
    };
    let s = 1.0 + access_count.unwrap_or(0).max(0) as f64;
    base + DECAY_LAMBDA * (-delta_days / (s * DECAY_DAYS)).exp()
}

/// The `COALESCE(last_used_at, updated_at)` clock, exposed so a caller (and a
/// probe) can print WHICH instant a score was computed from — the reading the
/// t31 ruling is about. Returns `None` when neither half parses.
pub fn decay_clock(updated_at: &str, last_used_at: Option<&str>) -> Option<DateTime<Utc>> {
    last_used_at
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .or_else(|| DateTime::parse_from_rfc3339(updated_at).ok())
        .map(|d| d.with_timezone(&Utc))
}

/// Record that these memories were actually returned to an agent.
///
/// The first write for a historical row CREATES a value (`COALESCE(.., 0) + 1`):
/// that is honest — the row IS being used now — and the "was it used before this
/// counter existed" question stays answered by NULL-until-first-use.
///
/// Returns how many rows were updated. An id that does not exist is skipped by
/// the WHERE clause rather than counted: the count is a reading, not a claim.
pub async fn record_usage(db: &Db, ids: &[i64]) -> Result<usize, DbError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let ids: Vec<i64> = ids.to_vec();
    let now = Utc::now().to_rfc3339();
    db.call(move |conn| -> Result<usize, rusqlite::Error> {
        let mut n = 0usize;
        for id in &ids {
            n += conn.execute(
                "UPDATE memories
                    SET access_count = COALESCE(access_count, 0) + 1,
                        last_used_at = ?2
                  WHERE id = ?1 AND deleted_at IS NULL",
                rusqlite::params![id, now],
            )?;
        }
        Ok(n)
    })
    .await?
    .map_err(DbError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C3: the recency term must be a real ordering input — a heavily used row
    /// outranks an identical but stale one, and the difference is not a constant
    /// (it decays).
    #[test]
    fn use_and_freshness_move_the_score() {
        let now = "2026-09-27T12:00:00Z";
        let fresh = decay_score(0.0, "2026-09-27T11:00:00Z", None, now, Some(5));
        let stale = decay_score(0.0, "2026-08-28T12:00:00Z", None, now, Some(0));
        let used_but_old = decay_score(0.0, "2026-08-28T12:00:00Z", None, now, Some(5));
        println!(
            "READING C3 fresh+used={fresh:.4} stale+unused={stale:.4} stale+used={used_but_old:.4}"
        );
        assert!(fresh > used_but_old, "freshness must matter");
        assert!(used_but_old > stale, "reuse must slow the decay");
        assert!(stale > 0.0, "decay never reaches zero — no silent drop");
    }

    /// **The judgement the t31 ruling is about** (RV-B-1): two memories with the
    /// SAME content and the SAME write time, one used five times just now and one
    /// never used since 30 days ago, must score **strictly** differently.
    ///
    /// On the pre-t31 shape this test could not be written: `decay_score` did not
    /// accept `last_used_at` at all, so the two rows got identical inputs and
    /// therefore identical outputs (`used=0.500000 unused=0.500000`). The reading
    /// below is what makes "usage decay" a claim instead of an empty one.
    #[test]
    fn a_used_row_strictly_outranks_an_unused_one_with_the_same_write_time() {
        let now = "2026-09-27T12:00:00Z";
        // same content (same base), SAME updated_at: only the use clock differs
        let written_at = "2026-08-28T12:00:00Z";
        let used = decay_score(0.0, written_at, Some(now), now, Some(5));
        let unused = decay_score(0.0, written_at, Some("2026-08-28T12:00:00Z"), now, Some(0));
        println!(
            "READING C3(Δt) used=5,last_used=now -> {used:.6} | used=0,last_used=now-30d -> {unused:.6}"
        );
        assert!(
            used > unused,
            "used={used:.6} must strictly precede unused={unused:.6}"
        );
        // and the clock really is the COALESCE of the two columns
        assert_eq!(
            decay_clock(written_at, Some(now)).unwrap(),
            chrono::DateTime::parse_from_rfc3339(now)
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            decay_clock(written_at, None).unwrap(),
            chrono::DateTime::parse_from_rfc3339(written_at)
                .unwrap()
                .with_timezone(&Utc),
            "NULL last_used_at falls back to updated_at — the same fact for a never-used row"
        );
        // a corrupt last_used_at must not invent freshness; it falls back
        assert_eq!(
            decay_clock(written_at, Some("not-a-timestamp")).unwrap(),
            chrono::DateTime::parse_from_rfc3339(written_at)
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    /// NULL is unknown, not zero uses: the score treats it as 0 uses and the
    /// distinction is documented, so a reader cannot turn NULL into "used more".
    #[test]
    fn unknown_usage_is_not_treated_as_heavily_used() {
        let now = "2026-09-27T12:00:00Z";
        let null = decay_score(0.0, "2026-08-28T12:00:00Z", None, now, None);
        let zero = decay_score(0.0, "2026-08-28T12:00:00Z", None, now, Some(0));
        println!("READING C3 NULL row score={null:.4} vs zero-use row {zero:.4}");
        assert_eq!(
            null, zero,
            "None is shaped like 0 uses, never like a used row"
        );
        assert!(null < decay_score(0.0, "2026-08-28T12:00:00Z", None, now, Some(9)));
    }

    #[tokio::test]
    async fn record_usage_counts_rows_and_is_repeatable() {
        let db = Db::open_in_memory().unwrap();
        let w = crate::write::MemoryWrite {
            store: crate::MemoryStore::Observation,
            namespace: crate::Namespace::parse("user").unwrap(),
            content: "used fact".into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        };
        let crate::write::WriteOutcome::Inserted(id) =
            crate::write::write_memory(&db, &w).await.unwrap()
        else {
            panic!("insert")
        };
        assert_eq!(record_usage(&db, &[id]).await.unwrap(), 1);
        assert_eq!(record_usage(&db, &[id]).await.unwrap(), 1);
        assert_eq!(record_usage(&db, &[id + 999]).await.unwrap(), 0);
        let (n, last): (Option<i64>, Option<String>) = db
            .call(move |conn| {
                conn.query_row(
                    "SELECT access_count, last_used_at FROM memories WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING C3 access_count={n:?} last_used_at={last:?}");
        assert_eq!(n, Some(2));
        assert!(last.is_some());
    }
}
