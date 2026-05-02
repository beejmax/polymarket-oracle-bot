use anyhow::{Result, bail};

pub fn timeframe_seconds(timeframe: &str) -> Result<i64> {
    match timeframe {
        "5m" => Ok(5 * 60),
        "15m" => Ok(15 * 60),
        "1h" => Ok(60 * 60),
        other => bail!("unsupported timeframe: {other}"),
    }
}

pub fn floor_window_start(ts: i64, timeframe: &str) -> Result<i64> {
    let seconds = timeframe_seconds(timeframe)?;
    Ok(ts - ts.rem_euclid(seconds))
}

pub fn candidate_window_starts(
    ts: i64,
    timeframe: &str,
    lookback: i32,
    lookahead: i32,
) -> Result<Vec<i64>> {
    let base = floor_window_start(ts, timeframe)?;
    let seconds = timeframe_seconds(timeframe)?;
    Ok((-lookback..=lookahead)
        .map(|offset| base + i64::from(offset) * seconds)
        .collect())
}

pub fn slug_for(slug_prefix: &str, start_ts: i64, timeframe: &str) -> String {
    format!("{slug_prefix}-updown-{timeframe}-{start_ts}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_15m_slugs() {
        assert_eq!(
            slug_for("btc", 1777738500, "15m"),
            "btc-updown-15m-1777738500"
        );
        assert_eq!(
            candidate_window_starts(1777738948, "15m", 1, 1).unwrap(),
            vec![1777737600, 1777738500, 1777739400]
        );
    }
}
