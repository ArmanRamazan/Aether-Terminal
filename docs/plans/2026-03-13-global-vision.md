# Aether — Global Vision

> Система, которая автоматически диагностирует, лечит и оптимизирует production-кластер с одобрения человека.

## Конечная цель

Aether — платформа из 5 проектов, образующих self-healing infrastructure:

```
                    Human Approval (Arbiter)
                           ▲
                           │
┌──────────────┐    ┌──────┴───────┐    ┌──────────────┐
│   Aether     │───▶│  Auto-Fix    │◀───│   Service    │
│   Terminal   │    │  Agent       │    │   Graph      │
│  (observer)  │    │  (healer)    │    │  (topology)  │
└──────┬───────┘    └──────────────┘    └──────┬───────┘
       │                   ▲                    │
       │            ┌──────┴───────┐            │
       └───────────▶│  K8s Auto-   │◀───────────┘
                    │  scaler      │
                    │  (muscles)   │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │  Custom      │
                    │  Orchestrator│
                    │  (runtime)   │
                    └──────────────┘
```

| # | Проект | Роль | Аналогия |
|---|--------|------|----------|
| 1 | **Aether-Terminal** | Наблюдение, диагностика, метрики, event bus | Глаза + нервная система |
| 2 | **K8s Autoscaler** | Автоскейлинг ML inference на Kubernetes | Мышцы |
| 3 | **Service Graph** | Граф зависимостей сервисов, трафик, topology | Карта |
| 4 | **Auto-Fix Agent** | Автоматический debug, patch, remediation | Мозг |
| 5 | **Custom Orchestrator** | Собственный container runtime (замена K8s) | Скелет |

## Принципы

1. **Human-in-the-loop** — любое действие проходит через Arbiter (approve/deny). Автоматизация возможна, но по умолчанию — с одобрения.

2. **Три уровня интеллекта** — каждый следующий опционален:
   - L1: Детерминистические правила (всегда работает, предсказуемо)
   - L2: ML inference (паттерны, прогнозы)
   - L3: AI-агенты (контекстные решения, runbooks)

3. **Аддитивная архитектура** — новый код дополняет существующий, не переписывает. Каждый crate — самостоятельный адаптер с чётким API. Добавление нового источника данных или нового output — один новый crate, ноль изменений в существующих.

4. **Kubernetes-first, orchestrator-agnostic** — сейчас таргетим K8s. API абстрагирован через traits, чтобы замена на Custom Orchestrator была заменой одного адаптера.

5. **Event-driven** — все компоненты общаются через events. Aether публикует DiagnosticCreated/Resolved, другие проекты подписываются через gRPC stream или event bus.

## Aether-Terminal в этой системе

Aether-Terminal — первый и фундаментальный проект. Он предоставляет:

- **Сбор данных** — Prometheus scrape, network probing, log parsing, auto-discovery
- **Диагностика** — детерминистические правила + ML + AI agent analysis
- **Event Bus** — gRPC streaming API для внешних сервисов
- **Arbiter** — единая точка одобрения действий от всех 5 проектов
- **Визуализация** — TUI, Web UI, 3D граф процессов

Остальные 4 проекта — потребители Aether API. Они получают диагнозы, предлагают действия, Aether маршрутизирует через Arbiter.
