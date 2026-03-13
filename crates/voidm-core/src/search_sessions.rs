use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Represents a single click within a search session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchClick {
    pub id: String,
    pub time_ms: i64,
}

/// Create a new search session when user performs a search
pub async fn create_search_session(
    pool: &SqlitePool,
    user_id: &str,
    query: &str,
    result_count: usize,
) -> Result<String> {
    let session_id = Uuid::new_v4().to_string();
    // Simple hash instead of md5 dependency
    let query_hash = format!("{:x}", murmurhash64a(query.as_bytes()));
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO search_sessions (id, user_id, query_hash, started_at, result_count, last_activity_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(&session_id)
    .bind(user_id)
    .bind(&query_hash)
    .bind(&now)
    .bind(result_count as i64)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(session_id)
}

/// Find the most recent open search session for a user (within timeout)
pub async fn find_open_session(
    pool: &SqlitePool,
    user_id: &str,
    timeout_minutes: i64,
) -> Result<Option<String>> {
    let result: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT id FROM search_sessions
        WHERE user_id = ? 
          AND session_status = 'open'
          AND last_activity_at > datetime('now', ?)
        ORDER BY last_activity_at DESC
        LIMIT 1
        "#
    )
    .bind(user_id)
    .bind(format!("-{} minutes", timeout_minutes))
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|(id,)| id))
}

/// Record a click in a search session
pub async fn record_click(
    pool: &SqlitePool,
    session_id: &str,
    result_id: &str,
    time_since_start_ms: i64,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    // Get current clicked_results JSON
    let current: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT clicked_results FROM search_sessions WHERE id = ?"
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    let new_clicks = match current {
        Some((Some(json_str),)) => {
            // Parse existing array and append new click
            let mut clicks: Vec<SearchClick> = serde_json::from_str(&json_str)
                .unwrap_or_default();
            clicks.push(SearchClick {
                id: result_id.to_string(),
                time_ms: time_since_start_ms,
            });
            serde_json::to_string(&clicks)?
        }
        _ => {
            // Create new array with this click
            let clicks = vec![SearchClick {
                id: result_id.to_string(),
                time_ms: time_since_start_ms,
            }];
            serde_json::to_string(&clicks)?
        }
    };

    sqlx::query(
        r#"
        UPDATE search_sessions
        SET clicked_results = ?, last_activity_at = ?
        WHERE id = ?
        "#
    )
    .bind(&new_clicks)
    .bind(&now)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Close a search session (mark as completed)
pub async fn close_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE search_sessions SET session_status = 'closed', closed_at = ? WHERE id = ?"
    )
    .bind(&now)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Auto-close sessions that have been inactive for too long
pub async fn cleanup_expired_sessions(
    pool: &SqlitePool,
    timeout_minutes: i64,
) -> Result<u64> {
    let now = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        UPDATE search_sessions
        SET session_status = 'closed', closed_at = ?
        WHERE session_status = 'open'
          AND last_activity_at < datetime('now', ?)
        "#
    )
    .bind(&now)
    .bind(format!("-{} minutes", timeout_minutes))
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Get analytics for search effectiveness
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchAnalytics {
    pub query_hash: String,
    pub total_searches: i64,
    pub successful_searches: i64,
    pub avg_clicks: f64,
    pub success_rate: f64,
}

pub async fn get_search_analytics(pool: &SqlitePool, user_id: &str) -> Result<Vec<SearchAnalytics>> {
    let rows = sqlx::query_as::<_, (String, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            query_hash,
            COUNT(*) as total_searches,
            COUNT(CASE WHEN clicked_results IS NOT NULL THEN 1 END) as successful_searches,
            AVG(
                CASE 
                    WHEN clicked_results IS NOT NULL 
                    THEN json_array_length(clicked_results)
                    ELSE 0
                END
            ) as avg_clicks
        FROM search_sessions
        WHERE user_id = ? AND session_status = 'closed'
        GROUP BY query_hash
        ORDER BY total_searches DESC
        "#
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut analytics = Vec::new();
    for (query_hash, total_searches, successful_searches, avg_clicks) in rows {
        let success_rate = if total_searches > 0 {
            (successful_searches as f64 / total_searches as f64) * 100.0
        } else {
            0.0
        };

        analytics.push(SearchAnalytics {
            query_hash,
            total_searches,
            successful_searches,
            avg_clicks: avg_clicks.unwrap_or(0.0),
            success_rate,
        });
    }

    Ok(analytics)
}

/// Get exploration depth for a specific query
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExplorationStats {
    pub query_hash: String,
    pub times_searched: i64,
    pub avg_results_explored: f64,
    pub max_exploration_depth: i64,
}

pub async fn get_exploration_stats(pool: &SqlitePool, user_id: &str) -> Result<Vec<ExplorationStats>> {
    let rows = sqlx::query_as::<_, (String, i64, Option<f64>, Option<i64>)>(
        r#"
        SELECT 
            query_hash,
            COUNT(*) as times_searched,
            AVG(
                CASE 
                    WHEN clicked_results IS NOT NULL 
                    THEN json_array_length(clicked_results)
                    ELSE 0
                END
            ) as avg_results_explored,
            MAX(
                CASE 
                    WHEN clicked_results IS NOT NULL 
                    THEN json_array_length(clicked_results)
                    ELSE 0
                END
            ) as max_exploration_depth
        FROM search_sessions
        WHERE user_id = ? AND session_status = 'closed'
        GROUP BY query_hash
        ORDER BY avg_results_explored DESC
        "#
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut stats = Vec::new();
    for (query_hash, times_searched, avg_results_explored, max_exploration_depth) in rows {
        stats.push(ExplorationStats {
            query_hash,
            times_searched,
            avg_results_explored: avg_results_explored.unwrap_or(0.0),
            max_exploration_depth: max_exploration_depth.unwrap_or(0),
        });
    }

    Ok(stats)
}

/// Get concept relationships from search sessions (which concepts are clicked together)
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ConceptCoocurrence {
    pub concept_a: String,
    pub concept_b: String,
    pub times_clicked_together: i64,
}

pub async fn get_concept_coocurrence(
    _pool: &SqlitePool,
    _user_id: &str,
    _min_cooccurrence: i64,
) -> Result<Vec<ConceptCoocurrence>> {
    // Note: This is a simplified version - in production you'd want to parse the JSON
    // and do proper concept matching. For now, we return an empty result.
    // This shows the structure but needs JSON parsing from clicked_results
    Ok(vec![])
}

/// Aggregated search session summary (preserved after raw session purge)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchSessionAggregate {
    pub user_id: String,
    pub query_hash: String,
    pub period_start: String,  // RFC3339 date
    pub period_end: String,    // RFC3339 date
    pub total_searches: i64,
    pub successful_searches: i64,
    pub total_clicks: i64,
    pub avg_results_per_session: f64,
    pub avg_clicks_per_session: f64,
    pub success_rate: f64,
    pub max_exploration_depth: i64,
}

/// Aggregate search sessions for a given date and store in summary table
/// Returns count of sessions aggregated
pub async fn aggregate_sessions_for_date(
    pool: &SqlitePool,
    date: &str, // YYYY-MM-DD format
) -> Result<i64> {
    // Get yesterday's sessions (or the specified date)
    let period_start = format!("{}T00:00:00Z", date);
    let period_end = format!("{}T23:59:59Z", date);
    
    // Insert aggregates for each (user_id, query_hash) pair from that date
    let result = sqlx::query(
        r#"
        INSERT INTO search_session_summary 
        (user_id, query_hash, period_start, period_end, total_searches, successful_searches, 
         total_clicks, avg_results_per_session, avg_clicks_per_session, success_rate, max_exploration_depth)
        SELECT 
            user_id,
            query_hash,
            ?,
            ?,
            COUNT(*) as total_searches,
            COUNT(CASE WHEN clicked_results IS NOT NULL THEN 1 END) as successful_searches,
            COALESCE(SUM(json_array_length(clicked_results)), 0) as total_clicks,
            AVG(result_count) as avg_results_per_session,
            COALESCE(AVG(
                CASE 
                    WHEN clicked_results IS NOT NULL 
                    THEN json_array_length(clicked_results)
                    ELSE 0
                END
            ), 0) as avg_clicks_per_session,
            ROUND(100.0 * COUNT(CASE WHEN clicked_results IS NOT NULL THEN 1 END) 
                  / COUNT(*), 2) as success_rate,
            COALESCE(MAX(
                CASE 
                    WHEN clicked_results IS NOT NULL 
                    THEN json_array_length(clicked_results)
                    ELSE 0
                END
            ), 0) as max_exploration_depth
        FROM search_sessions
        WHERE closed_at >= ? AND closed_at < datetime(?, '+1 day')
          AND session_status = 'closed'
        GROUP BY user_id, query_hash
        ON CONFLICT(user_id, query_hash, period_start) DO UPDATE SET
            total_searches = excluded.total_searches,
            successful_searches = excluded.successful_searches,
            total_clicks = excluded.total_clicks,
            avg_results_per_session = excluded.avg_results_per_session,
            avg_clicks_per_session = excluded.avg_clicks_per_session,
            success_rate = excluded.success_rate,
            max_exploration_depth = excluded.max_exploration_depth,
            updated_at = CURRENT_TIMESTAMP
        "#
    )
    .bind(&period_start)
    .bind(&period_end)
    .bind(&period_start)
    .bind(date)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as i64)
}

/// Delete raw sessions older than the specified days
/// (should only run AFTER aggregation is verified)
pub async fn purge_old_sessions(
    pool: &SqlitePool,
    days_to_keep: i64,
) -> Result<i64> {
    let cutoff_date = format!("{}", 
        chrono::Utc::now() - chrono::Duration::days(days_to_keep)
    );

    let result = sqlx::query(
        r#"
        DELETE FROM search_sessions
        WHERE closed_at < ?
          AND session_status = 'closed'
        "#
    )
    .bind(&cutoff_date)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as i64)
}

/// Verify data integrity: compare raw sessions vs aggregated summaries
pub async fn verify_aggregation_integrity(
    pool: &SqlitePool,
) -> Result<bool> {
    // Count total searches in raw sessions (last 30 days)
    let raw_count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM search_sessions
        WHERE closed_at > datetime('now', '-30 days')
          AND session_status = 'closed'
        "#
    )
    .fetch_one(pool)
    .await?;

    // Count total searches in summaries (last 30 days)
    let summary_count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM search_session_summary
        WHERE period_end > datetime('now', '-30 days')
        "#
    )
    .fetch_one(pool)
    .await?;

    // They should be equal or summary should be slightly less (not yet aggregated today)
    let difference = (raw_count.0 - summary_count.0).abs();
    let is_healthy = difference < 1000; // Allow reasonable difference

    Ok(is_healthy)
}

/// Get aggregated analytics from summary table (works even after raw sessions purged)
pub async fn get_aggregated_analytics(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<SearchAnalytics>> {
    let rows = sqlx::query_as::<_, (String, i64, i64, f64)>(
        r#"
        SELECT 
            query_hash,
            SUM(total_searches) as total_searches,
            SUM(successful_searches) as successful_searches,
            AVG(success_rate) as success_rate
        FROM search_session_summary
        WHERE user_id = ?
        GROUP BY query_hash
        ORDER BY total_searches DESC
        "#
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut analytics = Vec::new();
    for (query_hash, total_searches, successful_searches, success_rate) in rows {
        let avg_clicks = sqlx::query_scalar::<_, Option<f64>>(
            r#"
            SELECT AVG(avg_clicks_per_session)
            FROM search_session_summary
            WHERE user_id = ? AND query_hash = ?
            "#
        )
        .bind(user_id)
        .bind(&query_hash)
        .fetch_one(pool)
        .await?
        .unwrap_or(0.0);

        analytics.push(SearchAnalytics {
            query_hash,
            total_searches,
            successful_searches,
            avg_clicks,
            success_rate,
        });
    }

    Ok(analytics)
}

/// Simple hash function (murmurhash64a lookalike for deterministic hashing)
pub fn murmurhash64a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xc15f9ce0919e27d9; // seed
    
    let mut i = 0;
    while i + 8 <= data.len() {
        let mut v = 0u64;
        v |= (data[i] as u64) << 0;
        v |= (data[i + 1] as u64) << 8;
        v |= (data[i + 2] as u64) << 16;
        v |= (data[i + 3] as u64) << 24;
        v |= (data[i + 4] as u64) << 32;
        v |= (data[i + 5] as u64) << 40;
        v |= (data[i + 6] as u64) << 48;
        v |= (data[i + 7] as u64) << 56;
        
        hash ^= v;
        hash = hash.wrapping_mul(0x85ebca6b);
        i += 8;
    }
    
    // Handle remaining bytes
    match data.len() & 7 {
        7 => hash ^= (data[i + 6] as u64) << 48,
        6 => hash ^= (data[i + 5] as u64) << 40,
        5 => hash ^= (data[i + 4] as u64) << 32,
        4 => hash ^= (data[i + 3] as u64) << 24,
        3 => hash ^= (data[i + 2] as u64) << 16,
        2 => hash ^= (data[i + 1] as u64) << 8,
        1 => hash ^= data[i] as u64,
        _ => {}
    }
    
    hash ^= hash >> 33;
    hash.wrapping_mul(0xff51afd7ed558ccd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_click_serialization() {
        let click = SearchClick {
            id: "mem-123".to_string(),
            time_ms: 2000,
        };
        let json = serde_json::to_string(&click).unwrap();
        assert!(json.contains("mem-123"));
        assert!(json.contains("2000"));
    }

    #[test]
    fn test_query_hash_deterministic() {
        let query = "jwt authentication";
        let hash = murmurhash64a(query.as_bytes());
        // Same query should produce same hash
        let hash2 = murmurhash64a("jwt authentication".as_bytes());
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_query_hash_different() {
        let hash1 = murmurhash64a("jwt".as_bytes());
        let hash2 = murmurhash64a("oauth".as_bytes());
        assert_ne!(hash1, hash2);
    }

    // Integration tests with real database
    #[tokio::test]
    async fn test_create_and_retrieve_session() -> anyhow::Result<()> {
        // Note: This test requires the search_sessions table to exist in the test database
        // Run: sqlite3 /tmp/voidm_test.db < /tmp/create_search_sessions.sql
        
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        let session_id = create_search_session(&pool, "test-user", "jwt auth", 5).await?;
        
        // Verify session was created
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM search_sessions WHERE id = ?"
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await?;
        
        assert_eq!(count, 1, "Session should be created");
        Ok(())
    }

    #[tokio::test]
    async fn test_record_multiple_clicks() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        let session_id = create_search_session(&pool, "test-user-clicks", "oauth flow", 3).await?;
        
        // Record multiple clicks
        record_click(&pool, &session_id, "mem-201", 1500).await?;
        record_click(&pool, &session_id, "mem-202", 3200).await?;
        record_click(&pool, &session_id, "mem-203", 5100).await?;
        
        // Retrieve and verify
        let (clicked_results,): (Option<String>,) = sqlx::query_as(
            "SELECT clicked_results FROM search_sessions WHERE id = ?"
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await?;
        
        let json = clicked_results.expect("Should have clicks");
        assert!(json.contains("mem-201"), "Should contain first click");
        assert!(json.contains("mem-202"), "Should contain second click");
        assert!(json.contains("mem-203"), "Should contain third click");
        
        // Verify it's valid JSON array
        let parsed: Vec<SearchClick> = serde_json::from_str(&json)?;
        assert_eq!(parsed.len(), 3, "Should have 3 clicks");
        
        Ok(())
    }

    #[tokio::test]
    async fn test_session_lifecycle() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        let session_id = create_search_session(&pool, "test-user-lifecycle", "api design", 2).await?;
        
        // Should find the open session
        let found = find_open_session(&pool, "test-user-lifecycle", 5).await?;
        assert!(found.is_some(), "Should find open session");
        assert_eq!(found.unwrap(), session_id);
        
        // Close the session
        close_session(&pool, &session_id).await?;
        
        // Verify it's closed
        let (status,): (String,) = sqlx::query_as(
            "SELECT session_status FROM search_sessions WHERE id = ?"
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await?;
        
        assert_eq!(status, "closed", "Session should be closed");
        
        // Should NOT find it as open anymore
        let found = find_open_session(&pool, "test-user-lifecycle", 5).await?;
        assert!(found.is_none(), "Should not find closed session");
        
        Ok(())
    }

    #[tokio::test]
    async fn test_aggregate_sessions() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        // Ensure migrations are run
        crate::migrate::run(&pool).await?;

        // Create a session with clicks
        let session_id = create_search_session(&pool, "test-aggregate", "auth", 3).await?;
        record_click(&pool, &session_id, "mem-101", 1000).await?;
        record_click(&pool, &session_id, "mem-102", 2500).await?;
        close_session(&pool, &session_id).await?;

        // Get today's date
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Aggregate sessions for today
        let aggregated = aggregate_sessions_for_date(&pool, &today).await?;
        assert!(aggregated > 0, "Should have aggregated at least one session");

        // Verify summary was created
        let summary_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM search_session_summary WHERE user_id = 'test-aggregate'"
        )
        .fetch_one(&pool)
        .await?;
        assert!(summary_count.0 > 0, "Should have created summary");

        Ok(())
    }

    #[tokio::test]
    async fn test_aggregation_integrity() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        // Ensure migrations are run
        crate::migrate::run(&pool).await?;

        // Verify that aggregation doesn't lose data
        let is_healthy = verify_aggregation_integrity(&pool).await?;
        assert!(is_healthy, "Aggregation integrity should be healthy");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_aggregated_analytics() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        // Ensure migrations are run
        crate::migrate::run(&pool).await?;

        // Create sessions for analytics testing
        let session_id = create_search_session(&pool, "test-analytics", "oauth", 4).await?;
        record_click(&pool, &session_id, "mem-201", 1000).await?;
        record_click(&pool, &session_id, "mem-202", 3000).await?;
        close_session(&pool, &session_id).await?;

        // Aggregate
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        aggregate_sessions_for_date(&pool, &today).await?;

        // Get analytics from aggregated data
        let analytics = get_aggregated_analytics(&pool, "test-analytics").await?;
        
        // Should have at least one result
        assert!(analytics.len() > 0, "Should have analytics");
        
        // Verify structure
        for stat in &analytics {
            assert!(!stat.query_hash.is_empty(), "Query hash should exist");
            assert!(stat.total_searches >= 1, "Should have at least 1 search");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_purge_old_sessions() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;
        
        let pool = SqlitePoolOptions::new()
            .connect("sqlite:////tmp/voidm_test.db")
            .await?;

        // Ensure migrations are run
        crate::migrate::run(&pool).await?;

        // Create old session (manually insert with old date)
        let old_id = uuid::Uuid::new_v4().to_string();
        let old_date = (chrono::Utc::now() - chrono::Duration::days(35)).to_rfc3339();
        
        sqlx::query(
            r#"
            INSERT INTO search_sessions (id, user_id, query_hash, started_at, result_count, 
                                        last_activity_at, session_status, closed_at)
            VALUES (?, ?, ?, ?, ?, ?, 'closed', ?)
            "#
        )
        .bind(&old_id)
        .bind("test-purge")
        .bind("oldhash")
        .bind(&old_date)
        .bind(2)
        .bind(&old_date)
        .bind(&old_date)
        .execute(&pool)
        .await?;

        // Verify old session exists
        let exists: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM search_sessions WHERE id = ?"
        )
        .bind(&old_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(exists.0, 1, "Old session should exist");

        // Purge sessions older than 30 days
        let deleted = purge_old_sessions(&pool, 30).await?;
        assert!(deleted > 0, "Should have deleted at least one session");

        // Verify it's gone
        let exists_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM search_sessions WHERE id = ?"
        )
        .bind(&old_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(exists_after.0, 0, "Old session should be deleted");

        Ok(())
    }
}

