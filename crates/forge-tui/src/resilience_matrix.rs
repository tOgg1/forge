//! Resilience test matrix for degraded TUI runtime environments.

pub const NETWORK_STALE_BLOCK_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradationKind {
    MissingProfiles,
    DbLockContention,
    PartialData,
    NetworkInterruption,
}

impl DegradationKind {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::MissingProfiles => "missing-profiles",
            Self::DbLockContention => "db-lock-contention",
            Self::PartialData => "partial-data",
            Self::NetworkInterruption => "network-interruption",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResilienceStatus {
    Healthy,
    Degraded,
    Blocked,
}

impl ResilienceStatus {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Blocked => "blocked",
        }
    }

    #[must_use]
    pub fn severity_rank(self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::Degraded => 1,
            Self::Blocked => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResilienceMatrixInput {
    pub profile_count: usize,
    pub db_lock_contention: bool,
    pub partial_data_ratio_percent: u8,
    pub network_online: bool,
    pub last_network_success_epoch_s: Option<i64>,
    pub now_epoch_s: i64,
}

impl Default for ResilienceMatrixInput {
    fn default() -> Self {
        Self {
            profile_count: 1,
            db_lock_contention: false,
            partial_data_ratio_percent: 0,
            network_online: true,
            last_network_success_epoch_s: None,
            now_epoch_s: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResilienceMatrixRow {
    pub kind: DegradationKind,
    pub status: ResilienceStatus,
    pub evidence: String,
    pub expected_behavior: String,
    pub operator_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResilienceMatrixReport {
    pub overall_status: ResilienceStatus,
    pub degraded_count: usize,
    pub blocked_count: usize,
    pub rows: Vec<ResilienceMatrixRow>,
}

#[must_use]
pub fn build_resilience_matrix(input: &ResilienceMatrixInput) -> ResilienceMatrixReport {
    let mut rows = vec![
        evaluate_missing_profiles(input),
        evaluate_db_lock_contention(input),
        evaluate_partial_data(input),
        evaluate_network_interruption(input),
    ];

    rows.sort_by(|left, right| {
        right
            .status
            .severity_rank()
            .cmp(&left.status.severity_rank())
            .then(left.kind.cmp(&right.kind))
    });

    let degraded_count = rows
        .iter()
        .filter(|row| row.status == ResilienceStatus::Degraded)
        .count();
    let blocked_count = rows
        .iter()
        .filter(|row| row.status == ResilienceStatus::Blocked)
        .count();
    let overall_status = rows
        .iter()
        .map(|row| row.status)
        .max_by_key(|status| status.severity_rank())
        .unwrap_or(ResilienceStatus::Healthy);

    ResilienceMatrixReport {
        overall_status,
        degraded_count,
        blocked_count,
        rows,
    }
}

fn evaluate_missing_profiles(input: &ResilienceMatrixInput) -> ResilienceMatrixRow {
    if input.profile_count > 0 {
        return ResilienceMatrixRow {
            kind: DegradationKind::MissingProfiles,
            status: ResilienceStatus::Healthy,
            evidence: format!("profile_count={}", input.profile_count),
            expected_behavior: "profile-dependent actions remain enabled".to_owned(),
            operator_action: "none".to_owned(),
        };
    }

    ResilienceMatrixRow {
        kind: DegradationKind::MissingProfiles,
        status: ResilienceStatus::Blocked,
        evidence: "profile_count=0".to_owned(),
        expected_behavior:
            "loop creation and profile-bound actions are disabled; existing views stay readable"
                .to_owned(),
        operator_action: "create/import at least one valid profile before retrying".to_owned(),
    }
}

fn evaluate_db_lock_contention(input: &ResilienceMatrixInput) -> ResilienceMatrixRow {
    if !input.db_lock_contention {
        return ResilienceMatrixRow {
            kind: DegradationKind::DbLockContention,
            status: ResilienceStatus::Healthy,
            evidence: "db_lock_contention=false".to_owned(),
            expected_behavior: "reads/writes proceed normally".to_owned(),
            operator_action: "none".to_owned(),
        };
    }

    ResilienceMatrixRow {
        kind: DegradationKind::DbLockContention,
        status: ResilienceStatus::Degraded,
        evidence: "db_lock_contention=true".to_owned(),
        expected_behavior:
            "TUI keeps last good snapshot visible while retrying with jittered backoff".to_owned(),
        operator_action: "release stale lock holder or wait for lock owner completion".to_owned(),
    }
}

fn evaluate_partial_data(input: &ResilienceMatrixInput) -> ResilienceMatrixRow {
    let ratio = input.partial_data_ratio_percent.min(100);
    if ratio == 0 {
        return ResilienceMatrixRow {
            kind: DegradationKind::PartialData,
            status: ResilienceStatus::Healthy,
            evidence: "partial_data_ratio=0%".to_owned(),
            expected_behavior: "full dataset rendered".to_owned(),
            operator_action: "none".to_owned(),
        };
    }

    let status = if ratio >= 80 {
        ResilienceStatus::Blocked
    } else {
        ResilienceStatus::Degraded
    };
    let operator_action = if status == ResilienceStatus::Blocked {
        "halt mutating actions and restore upstream writer/parity".to_owned()
    } else {
        "inspect parser warnings and continue in reduced-read mode".to_owned()
    };

    ResilienceMatrixRow {
        kind: DegradationKind::PartialData,
        status,
        evidence: format!("partial_data_ratio={ratio}%"),
        expected_behavior:
            "render known fields, mark unknown fields, avoid panic on missing records".to_owned(),
        operator_action,
    }
}

fn evaluate_network_interruption(input: &ResilienceMatrixInput) -> ResilienceMatrixRow {
    if input.network_online {
        return ResilienceMatrixRow {
            kind: DegradationKind::NetworkInterruption,
            status: ResilienceStatus::Healthy,
            evidence: "network_online=true".to_owned(),
            expected_behavior: "live remote polling enabled".to_owned(),
            operator_action: "none".to_owned(),
        };
    }

    let age_s = input
        .last_network_success_epoch_s
        .map(|last| (input.now_epoch_s - last).max(0))
        .unwrap_or(i64::MAX);
    let status = if age_s > NETWORK_STALE_BLOCK_SECS {
        ResilienceStatus::Blocked
    } else {
        ResilienceStatus::Degraded
    };

    ResilienceMatrixRow {
        kind: DegradationKind::NetworkInterruption,
        status,
        evidence: format!("network_online=false stale_for={}s", age_s),
        expected_behavior: "switch to cached/offline mode and suspend remote mutation requests"
            .to_owned(),
        operator_action: "restore network path or fail over to local daemon endpoint".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_resilience_matrix, DegradationKind, ResilienceMatrixInput, ResilienceStatus,
        NETWORK_STALE_BLOCK_SECS,
    };

    #[test]
    fn missing_profiles_is_blocked() {
        let report = build_resilience_matrix(&ResilienceMatrixInput {
            profile_count: 0,
            ..ResilienceMatrixInput::default()
        });
        let row = find_row(&report, DegradationKind::MissingProfiles);
        assert_eq!(row.status, ResilienceStatus::Blocked);
        assert_eq!(report.overall_status, ResilienceStatus::Blocked);
    }

    #[test]
    fn db_lock_contention_is_degraded() {
        let report = build_resilience_matrix(&ResilienceMatrixInput {
            db_lock_contention: true,
            ..ResilienceMatrixInput::default()
        });
        let row = find_row(&report, DegradationKind::DbLockContention);
        assert_eq!(row.status, ResilienceStatus::Degraded);
        assert!(row.expected_behavior.contains("retrying"));
    }

    #[test]
    fn partial_data_thresholds_map_to_degraded_and_blocked() {
        let degraded = build_resilience_matrix(&ResilienceMatrixInput {
            partial_data_ratio_percent: 45,
            ..ResilienceMatrixInput::default()
        });
        assert_eq!(
            find_row(&degraded, DegradationKind::PartialData).status,
            ResilienceStatus::Degraded
        );

        let blocked = build_resilience_matrix(&ResilienceMatrixInput {
            partial_data_ratio_percent: 90,
            ..ResilienceMatrixInput::default()
        });
        assert_eq!(
            find_row(&blocked, DegradationKind::PartialData).status,
            ResilienceStatus::Blocked
        );
    }

    #[test]
    fn network_interruption_degrades_then_blocks_by_staleness() {
        let degraded = build_resilience_matrix(&ResilienceMatrixInput {
            network_online: false,
            now_epoch_s: 1_000,
            last_network_success_epoch_s: Some(1_000 - NETWORK_STALE_BLOCK_SECS + 10),
            ..ResilienceMatrixInput::default()
        });
        assert_eq!(
            find_row(&degraded, DegradationKind::NetworkInterruption).status,
            ResilienceStatus::Degraded
        );

        let blocked = build_resilience_matrix(&ResilienceMatrixInput {
            network_online: false,
            now_epoch_s: 2_000,
            last_network_success_epoch_s: Some(2_000 - NETWORK_STALE_BLOCK_SECS - 1),
            ..ResilienceMatrixInput::default()
        });
        assert_eq!(
            find_row(&blocked, DegradationKind::NetworkInterruption).status,
            ResilienceStatus::Blocked
        );
    }

    #[test]
    fn combined_matrix_counts_and_orders_by_severity() {
        let report = build_resilience_matrix(&ResilienceMatrixInput {
            profile_count: 0,
            db_lock_contention: true,
            partial_data_ratio_percent: 10,
            network_online: false,
            now_epoch_s: 500,
            last_network_success_epoch_s: Some(-200),
        });

        assert_eq!(report.blocked_count, 2);
        assert_eq!(report.degraded_count, 2);
        assert_eq!(report.rows.len(), 4);
        assert_eq!(report.rows[0].status, ResilienceStatus::Blocked);
        assert_eq!(report.rows[1].status, ResilienceStatus::Blocked);
        assert_eq!(report.rows[2].status, ResilienceStatus::Degraded);
    }

    fn find_row(
        report: &super::ResilienceMatrixReport,
        kind: DegradationKind,
    ) -> &super::ResilienceMatrixRow {
        report
            .rows
            .iter()
            .find(|row| row.kind == kind)
            .unwrap_or_else(|| panic!("missing row: {}", kind.slug()))
    }
}
