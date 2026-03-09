//! MCP tool implementations that read from WorldGraph.

use std::sync::RwLock;

use serde_json::{json, Value};

use aether_core::WorldGraph;

use aether_core::models::ProcessState;

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
}
