use futures::{FutureExt, StreamExt};

use super::*;

fn sample(temperature: f64) -> Metrics {
    Metrics {
        general: MetricsGeneral {
            temperature: Some(Celsius(temperature)),
        },
        memory: MetricsMemory {
            percentage_used: Percentage(0.0),
            total: MebiBytes(1.0),
            available: MebiBytes(1.0),
            used: MebiBytes(0.0),
            zram_total: MebiBytes(0.0),
            zram_available: MebiBytes(0.0),
            zram_used: MebiBytes(0.0),
        },
        cpu: MetricsCpu {
            percentage_used: Percentage(0.0),
            idle: Percentage(100.0),
            user_space: Percentage(0.0),
            kernel_space: Percentage(0.0),
            wait: Percentage(0.0),
        },
        disk: MetricsDisk {
            percentage_used: Percentage(0.0),
            used: GigaBytes(0.0),
            available: GigaBytes(1.0),
            capacity: GigaBytes(1.0),
        },
    }
}

#[test]
fn metrics_observation_requires_the_existing_cache_snapshot() {
    let cache = Watch::new(None);
    let error = observe_metrics(cache.clone()).err().unwrap();
    assert_eq!(error.kind, ErrorKind::NotFound);
    assert_eq!(cache.watcher_count(), 1);
}

#[tokio::test]
async fn metrics_observation_marks_initial_seen_and_coalesces_existing_watch_updates() {
    let cache = Watch::new(Some(sample(1.0)));
    let (initial, mut updates) = observe_metrics(cache.clone()).unwrap();
    assert_eq!(initial.general.temperature.unwrap().0, 1.0);
    assert!(updates.next().now_or_never().is_none());
    cache.send(Some(sample(2.0)));
    cache.send(Some(sample(3.0)));
    assert_eq!(
        updates
            .next()
            .await
            .unwrap()
            .unwrap()
            .general
            .temperature
            .unwrap()
            .0,
        3.0
    );
    assert!(updates.next().now_or_never().is_none());
}

#[tokio::test]
async fn metrics_observation_disposal_drops_its_clone_not_the_sensor_cache() {
    let cache = Watch::new(Some(sample(1.0)));
    let (_, mut updates) = observe_metrics(cache.clone()).unwrap();
    assert_eq!(cache.watcher_count(), 2);
    assert!(updates.next().now_or_never().is_none());
    drop(updates);
    assert_eq!(cache.watcher_count(), 1);
    cache.send(Some(sample(4.0)));
    assert_eq!(cache.read().unwrap().general.temperature.unwrap().0, 4.0);
}

#[test]
fn metrics_follow_typed_and_ordinary_initials_share_the_snapshot_contract() {
    let typed = serde_json::to_value(MetricsFollowResponse {
        guid: None,
        metrics: sample(1.0),
    })
    .unwrap();
    assert!(typed["guid"].is_null());
    assert_eq!(typed["metrics"]["general"]["temperature"]["value"], "1.0");
    let guid = Guid::new();
    let ordinary = serde_json::to_value(MetricsFollowResponse {
        guid: Some(guid.clone()),
        metrics: sample(1.0),
    })
    .unwrap();
    assert_eq!(ordinary["guid"], guid.as_ref());
    assert_eq!(ordinary["metrics"], typed["metrics"]);
}
