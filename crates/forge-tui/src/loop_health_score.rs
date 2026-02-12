//! Loop health scoring with SLA timer/breach surfacing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopHealthSample {
    pub loop_id: String,
    pub is_live: bool,
    pub queue_oldest_age_s: u64,
    pub recent_error_rate_pct: u8,
    pub run_recency_s: u64,
    pub sla_max_queue_age_s: u64,
    pub sla_max_run_recency_s: u64,
    pub sla_max_error_rate_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopHealthScore {
    pub loop_id: String,
    pub score: u8,
    pub health_label: String,
    pub queue_sla_remaining_s: i64,
    pub run_sla_remaining_s: i64,
    pub error_budget_remaining_pct: i16,
    pub sla_breaches: Vec<String>,
    pub probable_causes: Vec<String>,
}

#[must_use]
pub fn compute_loop_health_scores(samples: &[LoopHealthSample]) -> Vec<LoopHealthScore> {
    let mut scores = samples.iter().filter_map(score_sample).collect::<Vec<_>>();
    scores.sort_by(|a, b| a.score.cmp(&b.score).then(a.loop_id.cmp(&b.loop_id)));
    scores
}

fn score_sample(sample: &LoopHealthSample) -> Option<LoopHealthScore> {
    let loop_id = sample.loop_id.trim().to_owned();
    if loop_id.is_empty() {
        return None;
    }

    let queue_budget = if sample.sla_max_queue_age_s == 0 {
        3_600
    } else {
        sample.sla_max_queue_age_s
    };
    let run_budget = if sample.sla_max_run_recency_s == 0 {
        900
    } else {
        sample.sla_max_run_recency_s
    };
    let error_budget = if sample.sla_max_error_rate_pct == 0 {
        10
    } else {
        sample.sla_max_error_rate_pct
    };

    let queue_sla_remaining_s = queue_budget as i64 - sample.queue_oldest_age_s as i64;
    let run_sla_remaining_s = run_budget as i64 - sample.run_recency_s as i64;
    let error_budget_remaining_pct =
        i16::from(error_budget).saturating_sub(i16::from(sample.recent_error_rate_pct));

    let mut score = 100i16;
    let mut breaches = Vec::new();
    let mut causes = Vec::new();

    if !sample.is_live {
        score -= 50;
        breaches.push("liveness:offline".to_owned());
        causes.push("loop runner not live".to_owned());
    }

    if queue_sla_remaining_s < 0 {
        let overrun = (-queue_sla_remaining_s) as u64;
        let penalty = 10 + scaled_penalty(overrun, queue_budget, 20);
        score -= penalty as i16;
        breaches.push(format!(
            "queue-age:{}s>{}s",
            sample.queue_oldest_age_s, queue_budget
        ));
        causes.push("queue backlog exceeding SLA".to_owned());
    }

    if run_sla_remaining_s < 0 {
        let overrun = (-run_sla_remaining_s) as u64;
        let penalty = 10 + scaled_penalty(overrun, run_budget, 20);
        score -= penalty as i16;
        breaches.push(format!(
            "run-recency:{}s>{}s",
            sample.run_recency_s, run_budget
        ));
        causes.push("loop not executing within recency SLA".to_owned());
    }

    if error_budget_remaining_pct < 0 {
        let overrun = error_budget_remaining_pct.unsigned_abs() as u64;
        let penalty = 8 + scaled_penalty(overrun, error_budget as u64, 20);
        score -= penalty as i16;
        breaches.push(format!(
            "error-rate:{}%>{}%",
            sample.recent_error_rate_pct, error_budget
        ));
        causes.push("recent error rate above SLA".to_owned());
    }

    if causes.is_empty() {
        causes.push("no immediate health risks".to_owned());
    }

    let score = score.clamp(0, 100) as u8;
    let health_label = if score >= 80 {
        "healthy"
    } else if score >= 50 {
        "degraded"
    } else {
        "critical"
    };

    Some(LoopHealthScore {
        loop_id,
        score,
        health_label: health_label.to_owned(),
        queue_sla_remaining_s,
        run_sla_remaining_s,
        error_budget_remaining_pct,
        sla_breaches: breaches,
        probable_causes: causes,
    })
}

fn scaled_penalty(overrun: u64, budget: u64, max_penalty: u64) -> u64 {
    if budget == 0 {
        return max_penalty;
    }
    let ratio_times_100 = overrun.saturating_mul(100) / budget;
    ratio_times_100.min(max_penalty)
}

#[cfg(test)]
mod tests {
    use super::{compute_loop_health_scores, LoopHealthSample};

    #[test]
    fn healthy_loop_has_high_score_and_no_breaches() {
        let scores = compute_loop_health_scores(&[LoopHealthSample {
            loop_id: "loop-a".to_owned(),
            is_live: true,
            queue_oldest_age_s: 20,
            recent_error_rate_pct: 1,
            run_recency_s: 30,
            sla_max_queue_age_s: 300,
            sla_max_run_recency_s: 120,
            sla_max_error_rate_pct: 10,
        }]);
        assert_eq!(scores[0].score, 100);
        assert_eq!(scores[0].health_label, "healthy");
        assert!(scores[0].sla_breaches.is_empty());
        assert_eq!(scores[0].probable_causes, vec!["no immediate health risks"]);
    }

    #[test]
    fn offline_loop_is_degraded_with_liveness_cause() {
        let scores = compute_loop_health_scores(&[LoopHealthSample {
            loop_id: "loop-off".to_owned(),
            is_live: false,
            queue_oldest_age_s: 10,
            recent_error_rate_pct: 0,
            run_recency_s: 20,
            sla_max_queue_age_s: 300,
            sla_max_run_recency_s: 120,
            sla_max_error_rate_pct: 10,
        }]);
        assert_eq!(scores[0].health_label, "degraded");
        assert!(scores[0]
            .sla_breaches
            .iter()
            .any(|entry| entry == "liveness:offline"));
        assert!(scores[0]
            .probable_causes
            .iter()
            .any(|entry| entry.contains("runner not live")));
    }

    #[test]
    fn sla_breaches_surface_queue_error_and_recency() {
        let scores = compute_loop_health_scores(&[LoopHealthSample {
            loop_id: "loop-bad".to_owned(),
            is_live: true,
            queue_oldest_age_s: 900,
            recent_error_rate_pct: 42,
            run_recency_s: 600,
            sla_max_queue_age_s: 300,
            sla_max_run_recency_s: 120,
            sla_max_error_rate_pct: 10,
        }]);
        assert_eq!(scores[0].health_label, "critical");
        assert_eq!(scores[0].sla_breaches.len(), 3);
        assert!(scores[0].queue_sla_remaining_s < 0);
        assert!(scores[0].run_sla_remaining_s < 0);
        assert!(scores[0].error_budget_remaining_pct < 0);
    }

    #[test]
    fn scores_are_sorted_worst_first() {
        let scores = compute_loop_health_scores(&[
            LoopHealthSample {
                loop_id: "loop-good".to_owned(),
                is_live: true,
                queue_oldest_age_s: 5,
                recent_error_rate_pct: 0,
                run_recency_s: 5,
                sla_max_queue_age_s: 300,
                sla_max_run_recency_s: 120,
                sla_max_error_rate_pct: 10,
            },
            LoopHealthSample {
                loop_id: "loop-bad".to_owned(),
                is_live: false,
                queue_oldest_age_s: 800,
                recent_error_rate_pct: 22,
                run_recency_s: 700,
                sla_max_queue_age_s: 300,
                sla_max_run_recency_s: 120,
                sla_max_error_rate_pct: 10,
            },
        ]);
        assert_eq!(scores[0].loop_id, "loop-bad");
        assert_eq!(scores[1].loop_id, "loop-good");
    }

    #[test]
    fn empty_loop_id_samples_are_skipped() {
        let scores = compute_loop_health_scores(&[
            LoopHealthSample {
                loop_id: "   ".to_owned(),
                is_live: true,
                queue_oldest_age_s: 0,
                recent_error_rate_pct: 0,
                run_recency_s: 0,
                sla_max_queue_age_s: 1,
                sla_max_run_recency_s: 1,
                sla_max_error_rate_pct: 1,
            },
            LoopHealthSample {
                loop_id: "loop-valid".to_owned(),
                is_live: true,
                queue_oldest_age_s: 0,
                recent_error_rate_pct: 0,
                run_recency_s: 0,
                sla_max_queue_age_s: 1,
                sla_max_run_recency_s: 1,
                sla_max_error_rate_pct: 1,
            },
        ]);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].loop_id, "loop-valid");
    }
}
