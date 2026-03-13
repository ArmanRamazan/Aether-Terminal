use aether_core::models::Diagnostic;

/// Serialize a Diagnostic to a JSON string.
///
/// Diagnostic contains `Instant` fields which are not directly serializable,
/// so we build the JSON object manually.
pub(crate) fn diagnostic_to_json(d: &Diagnostic) -> String {
    let evidence: Vec<serde_json::Value> = d
        .evidence
        .iter()
        .map(|e| {
            serde_json::json!({
                "metric": e.metric,
                "current": e.current,
                "threshold": e.threshold,
                "trend": e.trend,
                "context": e.context,
            })
        })
        .collect();

    let val = serde_json::json!({
        "id": d.id,
        "host": format!("{}", d.host),
        "target": format!("{:?}", d.target),
        "severity": format!("{}", d.severity),
        "category": format!("{:?}", d.category),
        "summary": d.summary,
        "evidence": evidence,
        "recommendation": {
            "action": format!("{:?}", d.recommendation.action),
            "reason": d.recommendation.reason,
            "urgency": format!("{:?}", d.recommendation.urgency),
            "auto_executable": d.recommendation.auto_executable,
        },
        "resolved": d.resolved_at.is_some(),
    });

    serde_json::to_string(&val).unwrap_or_else(|_| "{}".to_string())
}
