use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::Celsius;
use crate::db::model::Database;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::util::Invoke;

const SUSTAINED_HIGH: Duration = Duration::from_secs(10 * 60);
const SUSTAINED_CRITICAL: Duration = Duration::from_secs(60);
/// Degrees below the warning limit that end an episode.
const HYSTERESIS: f64 = 5.0;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, VisitVariants,
)]
#[serde(rename_all = "camelCase")]
pub enum SensorKind {
    Cpu,
    Gpu,
    Memory,
    Storage,
    Network,
    Chipset,
    System,
    Other,
}

rpc_toolkit::reflect_ts!(SensorKind);

impl SensorKind {
    fn name(self) -> String {
        match self {
            Self::Cpu => t!("system.temperature.sensor.cpu"),
            Self::Gpu => t!("system.temperature.sensor.gpu"),
            Self::Memory => t!("system.temperature.sensor.memory"),
            Self::Storage => t!("system.temperature.sensor.storage"),
            Self::Network => t!("system.temperature.sensor.network"),
            Self::Chipset => t!("system.temperature.sensor.chipset"),
            Self::System => t!("system.temperature.sensor.system"),
            Self::Other => t!("system.temperature.sensor.other"),
        }
        .into()
    }
}

/// One part's representative reading, with the limits it is judged against.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureSensor {
    /// The lm-sensors chip name.
    pub id: String,
    pub kind: SensorKind,
    pub temperature: Celsius,
    /// Sustained readings at or above this are hot.
    pub high: Celsius,
    pub critical: Celsius,
}

rpc_toolkit::reflect_ts!(TemperatureSensor);

pub(super) async fn read() -> Result<Vec<TemperatureSensor>, Error> {
    Ok(parse(
        &serde_json::from_slice(
            &Command::new("sensors")
                .arg("-j")
                .invoke(ErrorKind::Filesystem)
                .await?,
        )
        .with_kind(ErrorKind::Deserialization)?,
    ))
}

fn parse(chips: &serde_json::Value) -> Vec<TemperatureSensor> {
    let mut sensors: Vec<_> = chips
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, features)| {
            let kind = kind(id)?;
            // `temp1` is each chip's package or composite reading.
            let reading = features
                .as_object()?
                .values()
                .filter_map(|feature| feature.as_object())
                .find(|feature| feature.contains_key("temp1_input"))?;
            let field = |name| reading.get(name).and_then(|v| v.as_f64());
            let temperature = field("temp1_input").filter(|t| *t > 0.0 && *t < 125.0)?;
            let (high, critical) = limits(kind, field("temp1_max"), field("temp1_crit"));
            Some(TemperatureSensor {
                id: id.clone(),
                kind,
                temperature: Celsius(temperature),
                high: Celsius(high),
                critical: Celsius(critical),
            })
        })
        .collect();
    if sensors.iter().any(|s| s.kind == SensorKind::Cpu) {
        sensors.retain(|s| s.kind != SensorKind::System);
    }
    sensors.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.id.cmp(&b.id)));
    sensors
}

/// `None` for Super I/O chips, whose `temp1` is an unlabeled board input.
fn kind(id: &str) -> Option<SensorKind> {
    use SensorKind::*;
    let chip = id.split('-').next().unwrap_or(id);
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| chip.starts_with(p));
    Some(match chip {
        "coretemp" | "k10temp" | "k8temp" | "zenpower" => Cpu,
        "amdgpu" | "radeon" | "nouveau" => Gpu,
        "jc42" | "spd5118" => Memory,
        "nvme" | "drivetemp" => Storage,
        "acpitz" => System,
        _ if starts(&["nct", "it8", "f71", "w83"]) => return None,
        _ if starts(&["iwlwifi", "r8169", "mt79", "ath1"]) => Network,
        _ if starts(&["pch_"]) => Chipset,
        _ if chip.ends_with("_thermal") => {
            if starts(&["cpu", "soc", "bigcore", "littlecore", "cluster"]) {
                Cpu
            } else if starts(&["gpu"]) {
                Gpu
            } else if starts(&["ddr"]) {
                Memory
            } else {
                Other
            }
        }
        _ => Other,
    })
}

fn limits(kind: SensorKind, high: Option<f64>, critical: Option<f64>) -> (f64, f64) {
    let plausible = |t: &f64| (30.0..150.0).contains(t);
    let (default_high, default_critical): (f64, f64) = match kind {
        SensorKind::Storage => (70.0, 80.0),
        SensorKind::Memory => (85.0, 95.0),
        _ => (85.0, 100.0),
    };
    let critical = critical.filter(plausible);
    let high = high
        .filter(plausible)
        .or(critical.map(|c| c - 10.0))
        .unwrap_or(default_high);
    let critical = critical.unwrap_or(default_critical.max(high + 5.0));
    (high.min(critical), critical)
}

struct Episode {
    high_since: Option<Instant>,
    critical_since: Option<Instant>,
    notified: Option<NotificationLevel>,
}

#[derive(Default)]
pub(super) struct ThermalWatch {
    episodes: BTreeMap<String, Episode>,
}

impl ThermalWatch {
    pub(super) async fn check(
        &mut self,
        db: &TypedPatchDb<Database>,
        sensors: &[TemperatureSensor],
    ) {
        let now = Instant::now();
        // Missing readings end the observed episode.
        self.episodes
            .retain(|id, _| sensors.iter().any(|s| &s.id == id));
        for sensor in sensors {
            let Some(level) = self.observe(sensor, now) else {
                continue;
            };
            let name = sensor.kind.name();
            let (title, message) = if level == NotificationLevel::Error {
                (
                    t!("system.temperature.critical-title", sensor = name),
                    t!(
                        "system.temperature.critical-message",
                        sensor = name,
                        id = sensor.id,
                        temperature = sensor.temperature,
                        critical = sensor.critical
                    ),
                )
            } else {
                (
                    t!("system.temperature.high-title", sensor = name),
                    t!(
                        "system.temperature.high-message",
                        sensor = name,
                        id = sensor.id,
                        temperature = sensor.temperature,
                        high = sensor.high,
                        minutes = SUSTAINED_HIGH.as_secs() / 60
                    ),
                )
            };
            db.mutate(|db| notify(db, None, level, title.into(), message.into(), ()))
                .await
                .result
                .log_err();
        }
    }

    /// Requires sustained threshold readings; hysteresis retains notification suppression.
    fn observe(&mut self, sensor: &TemperatureSensor, now: Instant) -> Option<NotificationLevel> {
        let t = sensor.temperature.0;
        if t < sensor.high.0 - HYSTERESIS {
            self.episodes.remove(&sensor.id);
            return None;
        }
        if t < sensor.high.0 && !self.episodes.contains_key(&sensor.id) {
            return None;
        }
        let episode = self.episodes.entry(sensor.id.clone()).or_insert(Episode {
            high_since: None,
            critical_since: None,
            notified: None,
        });
        if t >= sensor.high.0 {
            episode.high_since.get_or_insert(now);
        } else {
            episode.high_since = None;
        }
        if t >= sensor.critical.0 {
            episode.critical_since.get_or_insert(now);
        } else {
            episode.critical_since = None;
        }
        let level = if episode
            .critical_since
            .is_some_and(|since| now - since >= SUSTAINED_CRITICAL)
        {
            NotificationLevel::Error
        } else if episode
            .high_since
            .is_some_and(|since| now - since >= SUSTAINED_HIGH)
        {
            NotificationLevel::Warning
        } else {
            return None;
        };
        match episode.notified {
            Some(NotificationLevel::Error) => None,
            Some(NotificationLevel::Warning) if level == NotificationLevel::Warning => None,
            _ => {
                episode.notified = Some(level.clone());
                Some(level)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sensor(id: &str, temperature: f64) -> TemperatureSensor {
        TemperatureSensor {
            id: id.into(),
            kind: SensorKind::Cpu,
            temperature: Celsius(temperature),
            high: Celsius(80.0),
            critical: Celsius(100.0),
        }
    }

    #[test]
    fn takes_one_reading_per_chip_with_its_own_limits() {
        let sensors = parse(&json!({
            "coretemp-isa-0000": {
                "Adapter": "ISA adapter",
                "Package id 0": { "temp1_input": 52.0, "temp1_max": 80.0, "temp1_crit": 100.0 },
                "Core 0": { "temp2_input": 50.0, "temp2_max": 80.0, "temp2_crit": 100.0 }
            },
            "nvme-pci-0100": {
                "Adapter": "PCI adapter",
                "Composite": { "temp1_input": 41.85, "temp1_max": 81.85, "temp1_min": -273.15, "temp1_crit": 84.85 },
                "Sensor 1": { "temp2_input": 41.85 }
            },
            "k10temp-pci-00c3": { "Adapter": "PCI adapter", "Tctl": { "temp1_input": 61.5 } }
        }));
        assert_eq!(
            sensors,
            vec![
                TemperatureSensor {
                    id: "coretemp-isa-0000".into(),
                    kind: SensorKind::Cpu,
                    temperature: Celsius(52.0),
                    high: Celsius(80.0),
                    critical: Celsius(100.0),
                },
                TemperatureSensor {
                    id: "k10temp-pci-00c3".into(),
                    kind: SensorKind::Cpu,
                    temperature: Celsius(61.5),
                    high: Celsius(85.0),
                    critical: Celsius(100.0),
                },
                TemperatureSensor {
                    id: "nvme-pci-0100".into(),
                    kind: SensorKind::Storage,
                    temperature: Celsius(41.85),
                    high: Celsius(81.85),
                    critical: Celsius(84.85),
                },
            ]
        );
    }

    #[test]
    fn hides_readings_that_say_nothing() {
        let sensors = parse(&json!({
            "acpitz-acpi-0": { "temp1": { "temp1_input": 27.8, "temp1_crit": 119.0 } },
            "nct6798-isa-0290": { "SYSTIN": { "temp1_input": 34.0 } },
            "iwlwifi_1-virtual-0": { "temp1": { "temp1_input": -128.0 } },
            "cpu_thermal-virtual-0": { "temp1": { "temp1_input": 48.3 } }
        }));
        assert_eq!(
            sensors.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["cpu_thermal-virtual-0"]
        );
    }

    #[test]
    fn keeps_the_acpi_zone_when_it_is_the_only_system_reading() {
        let sensors = parse(&json!({
            "acpitz-acpi-0": { "temp1": { "temp1_input": 44.0, "temp1_crit": 105.0 } }
        }));
        assert_eq!(sensors[0].kind, SensorKind::System);
        assert_eq!(
            (sensors[0].high, sensors[0].critical),
            (Celsius(95.0), Celsius(105.0))
        );
    }

    #[test]
    fn ignores_implausible_limits() {
        assert_eq!(
            limits(SensorKind::Storage, Some(65261.85), Some(-273.15)),
            (70.0, 80.0)
        );
        assert_eq!(limits(SensorKind::Storage, Some(95.0), None), (95.0, 100.0));
        assert_eq!(
            limits(SensorKind::Cpu, Some(105.0), Some(100.0)),
            (100.0, 100.0)
        );
    }

    #[test]
    fn an_empty_snapshot_has_no_readings() {
        for snapshot in [
            json!({}),
            json!({"iwlwifi_1-virtual-0": {"temp1": {"temp1_input": -128.0}}}),
        ] {
            assert!(parse(&snapshot).is_empty());
        }
    }

    #[tokio::test]
    async fn missing_readings_end_episodes_before_a_sensor_returns() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let db =
            TypedPatchDb::<Database>::load_unchecked(PatchDb::open(temp.path().join("db")).await?);
        let mut watch = ThermalWatch::default();
        let start = Instant::now();
        assert_eq!(watch.observe(&sensor("cpu", 101.0), start), None);
        watch.check(&db, &[]).await;
        assert!(watch.episodes.is_empty());
        assert_eq!(
            watch.observe(&sensor("cpu", 101.0), start + Duration::from_secs(600)),
            None
        );
        Ok(())
    }

    #[test]
    fn cooling_below_a_limit_restarts_its_dwell_timer() {
        let mut watch = ThermalWatch::default();
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        assert_eq!(watch.observe(&sensor("cpu", 86.0), at(0)), None);
        assert_eq!(watch.observe(&sensor("cpu", 77.0), at(600)), None);
        assert_eq!(watch.observe(&sensor("cpu", 86.0), at(604)), None);
        assert_eq!(watch.observe(&sensor("cpu", 86.0), at(1200)), None);
        assert_eq!(
            watch.observe(&sensor("cpu", 86.0), at(1204)),
            Some(NotificationLevel::Warning)
        );
        assert_eq!(watch.observe(&sensor("cpu", 101.0), at(1208)), None);
        assert_eq!(watch.observe(&sensor("cpu", 97.0), at(1268)), None);
        assert_eq!(watch.observe(&sensor("cpu", 101.0), at(1272)), None);
        assert_eq!(watch.observe(&sensor("cpu", 101.0), at(1328)), None);
        assert_eq!(
            watch.observe(&sensor("cpu", 101.0), at(1332)),
            Some(NotificationLevel::Error)
        );
    }

    #[test]
    fn a_startup_spike_never_notifies() {
        let mut watch = ThermalWatch::default();
        let start = Instant::now();
        for secs in (0..300).step_by(4) {
            let t = if secs < 240 { 90.0 } else { 60.0 };
            assert_eq!(
                watch.observe(&sensor("cpu", t), start + Duration::from_secs(secs)),
                None
            );
        }
        assert!(watch.episodes.is_empty());
    }

    #[test]
    fn sustained_heat_warns_once_then_escalates_once() {
        let mut watch = ThermalWatch::default();
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        assert_eq!(watch.observe(&sensor("cpu", 86.0), at(0)), None);
        assert_eq!(
            watch.observe(&sensor("cpu", 86.0), at(600)),
            Some(NotificationLevel::Warning)
        );
        assert_eq!(watch.observe(&sensor("cpu", 77.0), at(604)), None);
        assert_eq!(watch.observe(&sensor("cpu", 88.0), at(608)), None);
        assert_eq!(watch.observe(&sensor("cpu", 88.0), at(1208)), None);
        assert_eq!(watch.observe(&sensor("cpu", 101.0), at(1212)), None);
        assert_eq!(
            watch.observe(&sensor("cpu", 101.0), at(1272)),
            Some(NotificationLevel::Error)
        );
        assert_eq!(watch.observe(&sensor("cpu", 101.0), at(1300)), None);
        assert_eq!(watch.observe(&sensor("cpu", 70.0), at(1304)), None);
        assert_eq!(watch.observe(&sensor("cpu", 86.0), at(1308)), None);
        assert_eq!(
            watch.observe(&sensor("cpu", 86.0), at(1908)),
            Some(NotificationLevel::Warning)
        );
    }
}
