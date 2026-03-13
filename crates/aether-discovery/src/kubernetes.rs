//! Kubernetes service discovery via the kube API.
//!
//! Queries pods and services by namespace and label selector,
//! building [`Target`] entries for each discovered resource.

use std::collections::HashMap;
use std::time::SystemTime;

use async_trait::async_trait;
use k8s_openapi::api::core::v1::{Pod, Service};
use kube::api::{Api, ListParams};
use kube::Client;
use tracing::info;

use aether_core::models::{Endpoint, EndpointType, Target, TargetKind};
use aether_core::traits::ServiceDiscovery;

/// Discovers services and pods via the Kubernetes API.
#[derive(Clone)]
pub struct KubernetesDiscovery {
    client: Client,
    namespace: String,
    label_selector: Option<String>,
}

impl std::fmt::Debug for KubernetesDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KubernetesDiscovery")
            .field("namespace", &self.namespace)
            .field("label_selector", &self.label_selector)
            .finish_non_exhaustive()
    }
}

impl KubernetesDiscovery {
    /// Connect using in-cluster config or local kubeconfig.
    ///
    /// Defaults to the `"default"` namespace when `namespace` is `None`.
    pub async fn new(
        namespace: Option<String>,
        label_selector: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Client::try_default().await?;
        Ok(Self {
            client,
            namespace: namespace.unwrap_or_else(|| "default".to_owned()),
            label_selector,
        })
    }

    /// Build from an existing [`Client`] (useful for testing or shared clients).
    pub fn from_client(
        client: Client,
        namespace: String,
        label_selector: Option<String>,
    ) -> Self {
        Self {
            client,
            namespace,
            label_selector,
        }
    }

    fn list_params(&self) -> ListParams {
        let mut lp = ListParams::default();
        if let Some(sel) = &self.label_selector {
            lp = lp.labels(sel);
        }
        lp
    }

    async fn discover_pods(&self) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let list = pods.list(&self.list_params()).await?;
        let now = SystemTime::now();

        let mut targets = Vec::with_capacity(list.items.len());
        for pod in &list.items {
            let meta = &pod.metadata;
            let pod_name = meta.name.as_deref().unwrap_or("unknown");
            let ns = meta.namespace.as_deref().unwrap_or(&self.namespace);

            let mut labels: HashMap<String, String> = meta
                .labels
                .iter()
                .flatten()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            labels.insert("k8s.namespace".to_owned(), ns.to_owned());

            let pod_ip = pod
                .status
                .as_ref()
                .and_then(|s| s.pod_ip.as_deref());

            let endpoints = extract_pod_endpoints(pod, pod_ip);

            targets.push(Target {
                id: format!("k8s-pod-{}-{}", ns, pod_name),
                name: pod_name.to_owned(),
                kind: TargetKind::Pod,
                endpoints,
                labels,
                discovered_at: now,
            });
        }

        Ok(targets)
    }

    async fn discover_services(
        &self,
    ) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>> {
        let services: Api<Service> = Api::namespaced(self.client.clone(), &self.namespace);
        let list = services.list(&self.list_params()).await?;
        let now = SystemTime::now();

        let mut targets = Vec::with_capacity(list.items.len());
        for svc in &list.items {
            let meta = &svc.metadata;
            let svc_name = meta.name.as_deref().unwrap_or("unknown");
            let ns = meta.namespace.as_deref().unwrap_or(&self.namespace);

            let mut labels: HashMap<String, String> = meta
                .labels
                .iter()
                .flatten()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            labels.insert("k8s.namespace".to_owned(), ns.to_owned());

            let endpoints = extract_service_endpoints(svc, svc_name, ns);

            targets.push(Target {
                id: format!("k8s-svc-{}-{}", ns, svc_name),
                name: svc_name.to_owned(),
                kind: TargetKind::Service,
                endpoints,
                labels,
                discovered_at: now,
            });
        }

        Ok(targets)
    }
}

#[async_trait]
impl ServiceDiscovery for KubernetesDiscovery {
    async fn discover(&self) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            namespace = %self.namespace,
            label_selector = ?self.label_selector,
            "starting kubernetes discovery"
        );

        let (pods, services) =
            tokio::try_join!(self.discover_pods(), self.discover_services())?;

        let total = pods.len() + services.len();
        let mut targets = Vec::with_capacity(total);
        targets.extend(pods);
        targets.extend(services);

        info!(count = total, "kubernetes discovery complete");
        Ok(targets)
    }

    fn name(&self) -> &str {
        "kubernetes"
    }
}

/// Extract endpoints from pod container ports.
fn extract_pod_endpoints(pod: &Pod, pod_ip: Option<&str>) -> Vec<Endpoint> {
    let Some(spec) = &pod.spec else {
        return Vec::new();
    };

    let ip = match pod_ip {
        Some(ip) => ip,
        None => return Vec::new(),
    };

    let mut endpoints = Vec::new();
    for container in &spec.containers {
        let Some(ports) = &container.ports else {
            continue;
        };
        for port in ports {
            let cp = port.container_port;
            let proto = port.protocol.as_deref().unwrap_or("TCP");
            let scheme = if proto == "UDP" { "udp" } else { "tcp" };
            endpoints.push(Endpoint {
                url: format!("{scheme}://{ip}:{cp}"),
                endpoint_type: EndpointType::TcpProbe,
            });
        }
    }
    endpoints
}

/// Extract endpoints from a Kubernetes Service spec.
fn extract_service_endpoints(svc: &Service, svc_name: &str, namespace: &str) -> Vec<Endpoint> {
    let Some(spec) = &svc.spec else {
        return Vec::new();
    };

    let dns = format!("{svc_name}.{namespace}.svc.cluster.local");

    let Some(ports) = &spec.ports else {
        return vec![Endpoint {
            url: format!("tcp://{dns}"),
            endpoint_type: EndpointType::TcpProbe,
        }];
    };

    ports
        .iter()
        .map(|sp| {
            let port = sp.port;
            Endpoint {
                url: format!("tcp://{dns}:{port}"),
                endpoint_type: EndpointType::TcpProbe,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use k8s_openapi::api::core::v1::{
        Container, ContainerPort, PodSpec, PodStatus, ServicePort, ServiceSpec,
    };
    use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
    use kube::api::ObjectMeta;

    fn make_pod(name: &str, namespace: &str, ip: &str, ports: Vec<(i32, &str)>) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                namespace: Some(namespace.to_owned()),
                labels: Some(BTreeMap::from([
                    ("app".to_owned(), name.to_owned()),
                ])),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "main".to_owned(),
                    ports: Some(
                        ports
                            .into_iter()
                            .map(|(p, proto)| ContainerPort {
                                container_port: p,
                                protocol: Some(proto.to_owned()),
                                ..Default::default()
                            })
                            .collect(),
                    ),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                pod_ip: Some(ip.to_owned()),
                ..Default::default()
            }),
        }
    }

    fn make_service(name: &str, namespace: &str, ports: Vec<i32>) -> Service {
        Service {
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                namespace: Some(namespace.to_owned()),
                labels: Some(BTreeMap::from([
                    ("app".to_owned(), name.to_owned()),
                ])),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                ports: Some(
                    ports
                        .into_iter()
                        .map(|p| ServicePort {
                            port: p,
                            target_port: Some(IntOrString::Int(p)),
                            ..Default::default()
                        })
                        .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_extract_pod_endpoints() {
        let pod = make_pod("web", "default", "10.0.0.1", vec![(8080, "TCP"), (9090, "TCP")]);
        let endpoints = extract_pod_endpoints(&pod, Some("10.0.0.1"));

        assert_eq!(endpoints.len(), 2, "should have two endpoints");
        assert_eq!(endpoints[0].url, "tcp://10.0.0.1:8080");
        assert_eq!(endpoints[1].url, "tcp://10.0.0.1:9090");
        assert_eq!(endpoints[0].endpoint_type, EndpointType::TcpProbe);
    }

    #[test]
    fn test_extract_pod_endpoints_no_ip() {
        let pod = make_pod("web", "default", "10.0.0.1", vec![(8080, "TCP")]);
        let endpoints = extract_pod_endpoints(&pod, None);
        assert!(endpoints.is_empty(), "no endpoints without pod IP");
    }

    #[test]
    fn test_extract_service_endpoints() {
        let svc = make_service("api", "production", vec![80, 443]);
        let endpoints = extract_service_endpoints(&svc, "api", "production");

        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].url, "tcp://api.production.svc.cluster.local:80");
        assert_eq!(endpoints[1].url, "tcp://api.production.svc.cluster.local:443");
    }

    #[test]
    fn test_k8s_target_construction() {
        let pod = make_pod("nginx", "monitoring", "10.0.1.5", vec![(80, "TCP")]);
        let pod_ip = pod.status.as_ref().and_then(|s| s.pod_ip.as_deref());
        let meta = &pod.metadata;
        let pod_name = meta.name.as_deref().unwrap_or("unknown");
        let ns = meta.namespace.as_deref().unwrap_or("default");

        let mut labels: HashMap<String, String> = meta
            .labels
            .iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        labels.insert("k8s.namespace".to_owned(), ns.to_owned());

        let target = Target {
            id: format!("k8s-pod-{}-{}", ns, pod_name),
            name: pod_name.to_owned(),
            kind: TargetKind::Pod,
            endpoints: extract_pod_endpoints(&pod, pod_ip),
            labels,
            discovered_at: SystemTime::now(),
        };

        assert_eq!(target.id, "k8s-pod-monitoring-nginx");
        assert_eq!(target.name, "nginx");
        assert_eq!(target.kind, TargetKind::Pod);
        assert_eq!(target.endpoints.len(), 1);
        assert_eq!(target.endpoints[0].url, "tcp://10.0.1.5:80");
        assert_eq!(target.labels.get("k8s.namespace"), Some(&"monitoring".to_owned()));
        assert_eq!(target.labels.get("app"), Some(&"nginx".to_owned()));
    }
}
