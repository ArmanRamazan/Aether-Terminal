//! MCP tool implementations that read from WorldGraph.

use std::sync::{Mutex, RwLock};

use serde_json::{json, Value};

use aether_core::models::{Diagnostic, ProcessState, Severity};
use aether_core::{AgentAction, WorldGraph};
use aether_predict::models::PredictedAnomaly;

use crate::arbiter::ArbiterQueue;

/// Maximum number of processes returned to prevent AI token overflow.
const MAX_PROCESSES: usize = 50;

/// 1 GiB threshold for high memory recommendations.
const HIGH_MEM_THRESHOLD: u64 = 1_073_741_824;

/// Build system topology JSON from the world graph.
///
/// Returns top 50 processes by CPU usage, their connections, and summary stats.
pub(crate) fn get_system_topology(graph: &RwLock<WorldGraph>) -> Value {
    let world = graph.read().expect("WorldGraph lock poisoned");

    let total_processes = world.process_count();

    // Collect all processes, sort by CPU descending, take top 50.
    let mut procs: Vec<_> = world.processes().collect();
    procs.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));
    let top_procs = &procs[..procs.len().min(MAX_PROCESSES)];

    // Build a set of top pids for filtering connections.
    let top_pids: std::collections::HashSet<u32> = top_procs.iter().map(|p| p.pid).collect();

    // Serialize processes.
    let processes_json: Vec<Value> = top_procs
        .iter()
        .map(|p| {
            json!({
                "pid": p.pid,
                "name": p.name,
                "cpu_percent": p.cpu_percent,
                "mem_bytes": p.mem_bytes,
                "state": p.state,
                "hp": p.hp,
            })
        })
        .collect();

    // Collect connections where source is in top processes.
    let connections_json: Vec<Value> = world
        .edge_pairs_with_data()
        .into_iter()
        .filter(|(src, _, _)| top_pids.contains(src))
        .map(|(_, _, edge)| {
            json!({
                "source_pid": edge.source_pid,
                "dest": edge.dest.to_string(),
                "protocol": edge.protocol,
                "bytes_per_sec": edge.bytes_per_sec,
            })
        })
        .collect();

    // Compute summary stats.
    let total_connections = world.edge_count();
    let (cpu_sum, mem_sum) = procs.iter().fold((0.0_f64, 0_u64), |(cpu, mem), p| {
        (cpu + p.cpu_percent as f64, mem.saturating_add(p.mem_bytes))
    });
    let avg_cpu = if total_processes > 0 {
        cpu_sum / total_processes as f64
    } else {
        0.0
    };

    json!({
        "processes": processes_json,
        "connections": connections_json,
        "summary": {
            "total_processes": total_processes,
            "total_connections": total_connections,
            "avg_cpu": (avg_cpu * 10.0).round() / 10.0,
            "total_memory_bytes": mem_sum,
        }
    })
}

/// Inspect a single process by PID with connections and health recommendations.
///
/// Returns process details, all connections involving this PID, and
/// actionable recommendations based on CPU, memory, HP, and state.
pub(crate) fn inspect_process(graph: &RwLock<WorldGraph>, pid: u32) -> Result<Value, String> {
    let world = graph.read().expect("WorldGraph lock poisoned");

    let proc = world
        .find_by_pid(pid)
        .ok_or_else(|| format!("process with pid {pid} not found"))?;

    let process_json = json!({
        "pid": proc.pid,
        "ppid": proc.ppid,
        "name": proc.name,
        "cpu_percent": proc.cpu_percent,
        "mem_bytes": proc.mem_bytes,
        "state": proc.state,
        "hp": proc.hp,
        "xp": proc.xp,
    });

    let connections: Vec<Value> = world
        .edge_pairs_with_data()
        .into_iter()
        .filter(|(src, dst, _)| *src == pid || *dst == pid)
        .map(|(_, _, edge)| {
            json!({
                "source_pid": edge.source_pid,
                "dest": edge.dest.to_string(),
                "protocol": edge.protocol,
                "bytes_per_sec": edge.bytes_per_sec,
                "state": edge.state,
            })
        })
        .collect();

    let recommendations = build_recommendations(proc);

    Ok(json!({
        "process": process_json,
        "connections": connections,
        "recommendations": recommendations,
    }))
}

/// Scan all processes for anomalies and return sorted results.
///
/// Detects: low HP (<50), zombie state, high CPU (>90%).
/// Sorted by severity (critical first), then HP ascending.
pub(crate) fn list_anomalies(graph: &RwLock<WorldGraph>) -> Value {
    let world = graph.read().expect("WorldGraph lock poisoned");

    let mut anomalies: Vec<Value> = world
        .processes()
        .flat_map(detect_anomalies)
        .collect();

    // Sort: critical before warning, then by HP ascending.
    anomalies.sort_by(|a, b| {
        let sev_ord = severity_rank(a).cmp(&severity_rank(b));
        if sev_ord != std::cmp::Ordering::Equal {
            return sev_ord;
        }
        let hp_a = a["hp"].as_f64().unwrap_or(f64::MAX);
        let hp_b = b["hp"].as_f64().unwrap_or(f64::MAX);
        hp_a.partial_cmp(&hp_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    let total = anomalies.len();
    json!({
        "anomalies": anomalies,
        "total": total,
    })
}

/// Detect anomalies for a single process. Returns 0..N anomaly entries.
fn detect_anomalies(proc: &aether_core::models::ProcessNode) -> Vec<Value> {
    let mut results = Vec::new();

    if proc.state == ProcessState::Zombie {
        results.push(build_anomaly(proc, "critical", "Zombie process", "kill"));
    }

    if proc.hp < 50.0 {
        let severity = if proc.hp < 25.0 { "critical" } else { "warning" };
        let reason = format!("HP below 50 (HP: {:.1})", proc.hp);
        let action = "restart";
        results.push(build_anomaly(proc, severity, &reason, action));
    }

    if proc.cpu_percent > 90.0 {
        let reason = format!("CPU above 90% ({:.1}%)", proc.cpu_percent);
        results.push(build_anomaly(proc, "warning", &reason, "investigate"));
    }

    results
}

/// Build a single anomaly JSON object.
fn build_anomaly(
    proc: &aether_core::models::ProcessNode,
    severity: &str,
    reason: &str,
    suggested_action: &str,
) -> Value {
    json!({
        "pid": proc.pid,
        "name": proc.name,
        "hp": proc.hp,
        "reason": reason,
        "severity": severity,
        "suggested_action": suggested_action,
    })
}

/// Map severity string to sort rank (lower = higher priority).
fn severity_rank(anomaly: &Value) -> u8 {
    match anomaly["severity"].as_str() {
        Some("critical") => 0,
        Some("warning") => 1,
        _ => 2,
    }
}

/// Generate health recommendations based on process metrics.
fn build_recommendations(proc: &aether_core::models::ProcessNode) -> Vec<&'static str> {
    let mut recs = Vec::new();
    if proc.cpu_percent > 80.0 {
        recs.push("High CPU usage — investigate potential busy loop or heavy computation");
    }
    if proc.hp < 30.0 {
        recs.push("Low health — consider restarting or investigating resource leaks");
    }
    if proc.state == ProcessState::Zombie {
        recs.push("Zombie process — parent should reap or kill");
    }
    if proc.mem_bytes > HIGH_MEM_THRESHOLD {
        recs.push("High memory usage — check for memory leaks");
    }
    recs
}

/// Parse action string and enqueue in ArbiterQueue for human approval.
///
/// Returns JSON with `status: "pending_approval"` and the assigned action ID.
pub(crate) fn execute_action(
    queue: &Mutex<ArbiterQueue>,
    action: &str,
    pid: u32,
) -> Result<Value, String> {
    let agent_action = match action {
        "kill" => AgentAction::KillProcess { pid },
        "restart" => AgentAction::RestartService {
            name: format!("pid-{pid}"),
        },
        "inspect" => AgentAction::Inspect { pid },
        other => return Err(format!("unknown action: {other}")),
    };

    let mut arbiter = queue.lock().expect("ArbiterQueue lock poisoned");
    let action_id = arbiter.enqueue(agent_action, pid, "mcp-agent");

    Ok(json!({
        "status": "pending_approval",
        "action_id": action_id,
    }))
}

/// Return current AI predictions as JSON.
///
/// Includes per-prediction details (pid, type, confidence, ETA) and summary stats.
pub(crate) fn predict_anomalies(predictions: &Mutex<Vec<PredictedAnomaly>>) -> Value {
    let preds = predictions.lock().expect("predictions lock poisoned");

    let items: Vec<Value> = preds
        .iter()
        .map(|p| {
            json!({
                "pid": p.pid,
                "process_name": p.process_name,
                "anomaly_type": p.anomaly_type,
                "confidence": p.confidence,
                "eta_seconds": p.eta_seconds,
                "recommended_action": p.recommended_action,
            })
        })
        .collect();

    let total = items.len();
    let model_status = if preds.is_empty() {
        "no_predictions"
    } else {
        "active"
    };

    json!({
        "predictions": items,
        "total": total,
        "model_status": model_status,
    })
}

/// Return diagnostics filtered by optional host, severity, and category.
///
/// Serializes each `Diagnostic` to JSON, converting `Instant` fields to
/// elapsed-seconds strings. Includes per-severity stats.
pub(crate) fn get_diagnostics(
    diagnostics: &Mutex<Vec<Diagnostic>>,
    host: Option<&str>,
    severity: Option<&str>,
    category: Option<&str>,
) -> Value {
    let diags = diagnostics.lock().expect("diagnostics lock poisoned");

    let parsed_severity = severity.and_then(parse_severity);

    let items: Vec<Value> = diags
        .iter()
        .filter(|d| {
            if let Some(h) = host {
                if d.host.as_str() != h {
                    return false;
                }
            }
            if let Some(sev) = parsed_severity {
                if d.severity != sev {
                    return false;
                }
            }
            if let Some(cat) = category {
                if !format!("{:?}", d.category).eq_ignore_ascii_case(cat) {
                    return false;
                }
            }
            true
        })
        .map(serialize_diagnostic)
        .collect();

    let (critical, warning, info) = count_severities(&items);
    let total = items.len();

    json!({
        "diagnostics": items,
        "stats": {
            "critical": critical,
            "warning": warning,
            "info": info,
            "total": total,
        }
    })
}

/// Parse a severity filter string into a `Severity` variant.
fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "critical" => Some(Severity::Critical),
        "warning" => Some(Severity::Warning),
        "info" => Some(Severity::Info),
        _ => None,
    }
}

/// Serialize a single `Diagnostic` to JSON.
fn serialize_diagnostic(d: &Diagnostic) -> Value {
    let elapsed = d.detected_at.elapsed();
    let elapsed_str = format!("{:.1}s ago", elapsed.as_secs_f64());

    let target_str = format!("{:?}", d.target);

    let evidence: Vec<Value> = d
        .evidence
        .iter()
        .map(|e| {
            json!({
                "metric": e.metric,
                "current": e.current,
                "threshold": e.threshold,
                "context": e.context,
            })
        })
        .collect();

    json!({
        "id": d.id,
        "host": d.host.as_str(),
        "target": target_str,
        "severity": format!("{:?}", d.severity),
        "category": format!("{:?}", d.category),
        "summary": d.summary,
        "evidence": evidence,
        "recommendation": format!("{:?}", d.recommendation.action),
        "detected": elapsed_str,
    })
}

/// Count diagnostics by severity from serialized items.
fn count_severities(items: &[Value]) -> (usize, usize, usize) {
    let mut critical = 0;
    let mut warning = 0;
    let mut info = 0;
    for item in items {
        match item["severity"].as_str() {
            Some("Critical") => critical += 1,
            Some("Warning") => warning += 1,
            Some("Info") => info += 1,
            _ => {}
        }
    }
    (critical, warning, info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{
        ConnectionState, NetworkEdge, ProcessNode, ProcessState, Protocol,
    };
    use glam::Vec3;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn make_process(pid: u32, cpu: f32, mem: u64) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp: 95.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    fn make_edge(source_pid: u32) -> NetworkEdge {
        NetworkEdge {
            source_pid,
            dest_pid: None,
            dest: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 443),
            protocol: Protocol::TCP,
            bytes_per_sec: 1024,
            state: ConnectionState::Established,
        }
    }

    fn graph_with_processes(count: usize) -> RwLock<WorldGraph> {
        let mut g = WorldGraph::new();
        for i in 0..count {
            let pid = (i + 1) as u32;
            g.add_process(make_process(pid, pid as f32 * 1.5, pid as u64 * 1000));
        }
        RwLock::new(g)
    }

    #[test]
    fn test_topology_returns_valid_json_with_processes() {
        let graph = graph_with_processes(3);
        let result = get_system_topology(&graph);

        assert!(result["processes"].is_array());
        assert!(result["connections"].is_array());
        assert!(result["summary"].is_object());

        let procs = result["processes"].as_array().expect("processes array");
        assert_eq!(procs.len(), 3);

        // Verify process fields are present.
        let first = &procs[0];
        assert!(first["pid"].is_number());
        assert!(first["name"].is_string());
        assert!(first["cpu_percent"].is_number());
        assert!(first["mem_bytes"].is_number());
        assert!(first["hp"].is_number());
        assert!(!first["state"].is_null());
    }

    #[test]
    fn test_topology_limits_to_50_processes() {
        let graph = graph_with_processes(80);
        let result = get_system_topology(&graph);

        let procs = result["processes"].as_array().expect("processes array");
        assert_eq!(procs.len(), 50, "should cap at 50 processes");

        // Summary should reflect the true total.
        assert_eq!(result["summary"]["total_processes"], 80);
    }

    #[test]
    fn test_topology_summary_computed_correctly() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1, 10.0, 1000));
        g.add_process(make_process(2, 20.0, 2000));
        g.add_connection(1, 2, make_edge(1));
        let graph = RwLock::new(g);

        let result = get_system_topology(&graph);
        let summary = &result["summary"];

        assert_eq!(summary["total_processes"], 2);
        assert_eq!(summary["total_connections"], 1);
        assert_eq!(summary["avg_cpu"], 15.0); // (10+20)/2
        assert_eq!(summary["total_memory_bytes"], 3000); // 1000+2000
    }

    #[test]
    fn test_topology_empty_graph() {
        let graph = RwLock::new(WorldGraph::new());
        let result = get_system_topology(&graph);

        let procs = result["processes"].as_array().expect("processes array");
        assert!(procs.is_empty());
        assert_eq!(result["summary"]["total_processes"], 0);
        assert_eq!(result["summary"]["avg_cpu"], 0.0);
    }

    #[test]
    fn test_topology_processes_sorted_by_cpu_descending() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1, 5.0, 1000));
        g.add_process(make_process(2, 50.0, 1000));
        g.add_process(make_process(3, 25.0, 1000));
        let graph = RwLock::new(g);

        let result = get_system_topology(&graph);
        let procs = result["processes"].as_array().expect("processes array");

        let cpus: Vec<f64> = procs
            .iter()
            .map(|p| p["cpu_percent"].as_f64().expect("cpu"))
            .collect();
        assert_eq!(cpus, vec![50.0, 25.0, 5.0]);
    }

    // --- inspect_process tests ---

    #[test]
    fn test_inspect_process_returns_data_for_valid_pid() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(42, 10.0, 1000));
        g.add_process(make_process(43, 20.0, 2000));
        g.add_connection(42, 43, make_edge(42));
        let graph = RwLock::new(g);

        let result = inspect_process(&graph, 42).expect("should succeed");

        // Process fields present.
        assert_eq!(result["process"]["pid"], 42);
        assert_eq!(result["process"]["name"], "proc-42");
        assert_eq!(result["process"]["cpu_percent"], 10.0);
        assert_eq!(result["process"]["ppid"], 1);
        assert_eq!(result["process"]["xp"], 0);

        // Connection included.
        let conns = result["connections"].as_array().expect("connections array");
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0]["source_pid"], 42);

        // No recommendations for healthy process.
        let recs = result["recommendations"].as_array().expect("recommendations");
        assert!(recs.is_empty(), "healthy process should have no recommendations");
    }

    #[test]
    fn test_inspect_process_returns_error_for_invalid_pid() {
        let graph = RwLock::new(WorldGraph::new());
        let result = inspect_process(&graph, 999);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("999"),
            "error should mention the pid"
        );
    }

    #[test]
    fn test_inspect_process_recommendations_high_cpu() {
        let mut g = WorldGraph::new();
        let mut proc = make_process(1, 95.0, 1000);
        proc.hp = 95.0;
        g.add_process(proc);
        let graph = RwLock::new(g);

        let result = inspect_process(&graph, 1).expect("should succeed");
        let recs: Vec<String> = result["recommendations"]
            .as_array()
            .expect("recommendations")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();

        assert!(
            recs.iter().any(|r| r.contains("High CPU")),
            "should recommend for high CPU, got: {recs:?}"
        );
    }

    #[test]
    fn test_inspect_process_recommendations_low_hp() {
        let mut g = WorldGraph::new();
        let mut proc = make_process(1, 10.0, 1000);
        proc.hp = 15.0;
        g.add_process(proc);
        let graph = RwLock::new(g);

        let result = inspect_process(&graph, 1).expect("should succeed");
        let recs: Vec<String> = result["recommendations"]
            .as_array()
            .expect("recommendations")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();

        assert!(
            recs.iter().any(|r| r.contains("Low health")),
            "should recommend for low HP, got: {recs:?}"
        );
    }

    #[test]
    fn test_inspect_process_recommendations_zombie() {
        let mut g = WorldGraph::new();
        let mut proc = make_process(1, 0.0, 0);
        proc.state = ProcessState::Zombie;
        g.add_process(proc);
        let graph = RwLock::new(g);

        let result = inspect_process(&graph, 1).expect("should succeed");
        let recs: Vec<String> = result["recommendations"]
            .as_array()
            .expect("recommendations")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();

        assert!(
            recs.iter().any(|r| r.contains("Zombie")),
            "should recommend for zombie, got: {recs:?}"
        );
    }

    #[test]
    fn test_inspect_process_recommendations_high_memory() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1, 10.0, 2_000_000_000)); // ~2GB
        let graph = RwLock::new(g);

        let result = inspect_process(&graph, 1).expect("should succeed");
        let recs: Vec<String> = result["recommendations"]
            .as_array()
            .expect("recommendations")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();

        assert!(
            recs.iter().any(|r| r.contains("High memory")),
            "should recommend for high memory, got: {recs:?}"
        );
    }

    // --- list_anomalies tests ---

    #[test]
    fn test_list_anomalies_zombie_detected_as_critical() {
        let mut g = WorldGraph::new();
        let mut proc = make_process(1, 5.0, 1000);
        proc.state = ProcessState::Zombie;
        proc.hp = 80.0;
        g.add_process(proc);
        let graph = RwLock::new(g);

        let result = list_anomalies(&graph);
        let anomalies = result["anomalies"].as_array().expect("anomalies array");

        assert_eq!(result["total"], 1);
        assert_eq!(anomalies[0]["pid"], 1);
        assert_eq!(anomalies[0]["severity"], "critical");
        assert!(
            anomalies[0]["reason"].as_str().unwrap().contains("Zombie"),
            "reason should mention zombie"
        );
        assert_eq!(anomalies[0]["suggested_action"], "kill");
    }

    #[test]
    fn test_list_anomalies_high_cpu_detected_as_warning() {
        let mut g = WorldGraph::new();
        let proc = make_process(2, 95.0, 1000);
        g.add_process(proc);
        let graph = RwLock::new(g);

        let result = list_anomalies(&graph);
        let anomalies = result["anomalies"].as_array().expect("anomalies array");

        assert_eq!(result["total"], 1);
        assert_eq!(anomalies[0]["pid"], 2);
        assert_eq!(anomalies[0]["severity"], "warning");
        assert!(
            anomalies[0]["reason"].as_str().unwrap().contains("CPU"),
            "reason should mention CPU"
        );
        assert_eq!(anomalies[0]["suggested_action"], "investigate");
    }

    #[test]
    fn test_list_anomalies_normal_processes_excluded() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1, 10.0, 1000)); // hp=95, cpu=10, running
        g.add_process(make_process(2, 20.0, 2000));
        let graph = RwLock::new(g);

        let result = list_anomalies(&graph);
        assert_eq!(result["total"], 0);
        assert!(
            result["anomalies"].as_array().unwrap().is_empty(),
            "healthy processes should not appear"
        );
    }

    #[test]
    fn test_list_anomalies_sorted_critical_first_then_hp() {
        let mut g = WorldGraph::new();

        // Warning: high CPU, hp=95
        let high_cpu = make_process(1, 95.0, 1000);
        g.add_process(high_cpu);

        // Critical: zombie, hp=80
        let mut zombie = make_process(2, 0.0, 0);
        zombie.state = ProcessState::Zombie;
        zombie.hp = 80.0;
        g.add_process(zombie);

        // Critical: very low HP
        let mut low_hp = make_process(3, 10.0, 1000);
        low_hp.hp = 20.0;
        g.add_process(low_hp);

        let graph = RwLock::new(g);
        let result = list_anomalies(&graph);
        let anomalies = result["anomalies"].as_array().expect("anomalies array");

        // All critical entries should come before warning entries.
        let severities: Vec<&str> = anomalies
            .iter()
            .map(|a| a["severity"].as_str().unwrap())
            .collect();

        let first_warning = severities.iter().position(|&s| s == "warning");
        let last_critical = severities.iter().rposition(|&s| s == "critical");
        if let (Some(fw), Some(lc)) = (first_warning, last_critical) {
            assert!(
                lc < fw,
                "all critical should precede warning, got: {severities:?}"
            );
        }
    }

    #[test]
    fn test_list_anomalies_low_hp_severity_tiers() {
        let mut g = WorldGraph::new();

        // HP=20 → critical (below 25)
        let mut critical_hp = make_process(1, 5.0, 1000);
        critical_hp.hp = 20.0;
        g.add_process(critical_hp);

        // HP=40 → warning (below 50 but above 25)
        let mut warning_hp = make_process(2, 5.0, 1000);
        warning_hp.hp = 40.0;
        g.add_process(warning_hp);

        let graph = RwLock::new(g);
        let result = list_anomalies(&graph);
        let anomalies = result["anomalies"].as_array().expect("anomalies array");

        assert_eq!(result["total"], 2);
        // Critical should come first.
        assert_eq!(anomalies[0]["severity"], "critical");
        assert_eq!(anomalies[0]["pid"], 1);
        assert_eq!(anomalies[1]["severity"], "warning");
        assert_eq!(anomalies[1]["pid"], 2);
    }

    // --- execute_action tests ---

    #[test]
    fn test_execute_action_returns_pending_approval() {
        let queue = Mutex::new(ArbiterQueue::default());
        let result = execute_action(&queue, "kill", 42).expect("should succeed");
        assert_eq!(result["status"], "pending_approval");
        assert!(result["action_id"].is_string());

        let arbiter = queue.lock().unwrap();
        assert_eq!(arbiter.pending().len(), 1);
        assert_eq!(arbiter.pending()[0].pid, 42);
    }

    #[test]
    fn test_execute_action_unknown_action_returns_error() {
        let queue = Mutex::new(ArbiterQueue::default());
        let result = execute_action(&queue, "explode", 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown action"));
    }

    #[test]
    fn test_execute_action_all_valid_actions() {
        let queue = Mutex::new(ArbiterQueue::default());
        for action in &["kill", "restart", "inspect"] {
            let result = execute_action(&queue, action, 1).expect("should succeed");
            assert_eq!(result["status"], "pending_approval");
        }
        assert_eq!(queue.lock().unwrap().pending().len(), 3);
    }

    // --- get_diagnostics tests ---

    use aether_core::models::{
        DiagCategory, DiagTarget, Evidence, Recommendation, RecommendedAction, Severity, Urgency,
    };
    use aether_core::metrics::HostId;
    use std::time::Instant;

    fn make_diagnostic(id: u64, host: &str, severity: Severity, category: DiagCategory) -> Diagnostic {
        Diagnostic {
            id,
            host: HostId::new(host),
            target: DiagTarget::Host(HostId::new(host)),
            severity,
            category,
            summary: format!("test diagnostic {id}"),
            evidence: vec![Evidence {
                metric: "cpu_percent".into(),
                current: 95.0,
                threshold: 80.0,
                trend: None,
                context: "high cpu".into(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: "cpu usage".into(),
                },
                reason: "cpu too high".into(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[test]
    fn test_get_diagnostics_returns_all_unfiltered() {
        let diags = Mutex::new(vec![
            make_diagnostic(1, "host-a", Severity::Critical, DiagCategory::CpuSpike),
            make_diagnostic(2, "host-b", Severity::Warning, DiagCategory::MemoryLeak),
        ]);

        let result = get_diagnostics(&diags, None, None, None);
        let items = result["diagnostics"].as_array().expect("array");
        assert_eq!(items.len(), 2);
        assert_eq!(result["stats"]["total"], 2);
        assert_eq!(result["stats"]["critical"], 1);
        assert_eq!(result["stats"]["warning"], 1);
    }

    #[test]
    fn test_get_diagnostics_filters_by_host() {
        let diags = Mutex::new(vec![
            make_diagnostic(1, "host-a", Severity::Critical, DiagCategory::CpuSpike),
            make_diagnostic(2, "host-b", Severity::Warning, DiagCategory::MemoryLeak),
        ]);

        let result = get_diagnostics(&diags, Some("host-a"), None, None);
        let items = result["diagnostics"].as_array().expect("array");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["host"], "host-a");
    }

    #[test]
    fn test_get_diagnostics_filters_by_severity() {
        let diags = Mutex::new(vec![
            make_diagnostic(1, "local", Severity::Critical, DiagCategory::CpuSpike),
            make_diagnostic(2, "local", Severity::Warning, DiagCategory::MemoryLeak),
            make_diagnostic(3, "local", Severity::Info, DiagCategory::CapacityRisk),
        ]);

        let result = get_diagnostics(&diags, None, Some("warning"), None);
        let items = result["diagnostics"].as_array().expect("array");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["severity"], "Warning");
    }

    #[test]
    fn test_get_diagnostics_filters_by_category() {
        let diags = Mutex::new(vec![
            make_diagnostic(1, "local", Severity::Critical, DiagCategory::CpuSpike),
            make_diagnostic(2, "local", Severity::Warning, DiagCategory::MemoryLeak),
        ]);

        let result = get_diagnostics(&diags, None, None, Some("CpuSpike"));
        let items = result["diagnostics"].as_array().expect("array");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["category"], "CpuSpike");
    }

    #[test]
    fn test_get_diagnostics_empty_returns_zero_stats() {
        let diags = Mutex::new(Vec::new());
        let result = get_diagnostics(&diags, None, None, None);
        assert_eq!(result["stats"]["total"], 0);
        assert_eq!(result["stats"]["critical"], 0);
        assert_eq!(result["stats"]["warning"], 0);
        assert_eq!(result["stats"]["info"], 0);
    }

    #[test]
    fn test_get_diagnostics_serializes_fields() {
        let diags = Mutex::new(vec![
            make_diagnostic(42, "prod-1", Severity::Critical, DiagCategory::MemoryPressure),
        ]);

        let result = get_diagnostics(&diags, None, None, None);
        let item = &result["diagnostics"][0];
        assert_eq!(item["id"], 42);
        assert_eq!(item["host"], "prod-1");
        assert_eq!(item["severity"], "Critical");
        assert_eq!(item["category"], "MemoryPressure");
        assert!(item["summary"].as_str().unwrap().contains("42"));
        assert!(item["detected"].as_str().unwrap().contains("s ago"));
        assert!(item["evidence"].is_array());
    }
}
