//! The propagation engine, through its one JSON entry point
//! (`hfcast::service::run`).
//!
//! Requests are built here and nowhere else, so a path forecast and a map
//! can never describe different stations, models or sunspot numbers.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Error;
use crate::bands::MIN_CARD_FREQ_MHZ;
use crate::geo::UtcDate;
use crate::ssn::{self, Basis};
use crate::station::Station;

/// Man-made noise at a residential site, dBW in 1 Hz: VOACAP's default.
pub const NOISE_DBW: i32 = -145;

/// Which model answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Model {
    /// The classic monthly model, reproduced faithfully.
    Voacap,
    /// The engine's daily model over the same physics (the default).
    #[default]
    Truecast,
}

impl Model {
    pub fn key(self) -> &'static str {
        match self {
            Model::Voacap => "voacap",
            Model::Truecast => "truecast",
        }
    }
}

/// Live readings, when the device has them. The effective SSN replaces the
/// month's figure (VOACAP) or conditions the daily index (Truecast); the
/// recent Kp widens the correction's downward spread.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Nowcast {
    pub effective_ssn: f64,
    pub kp_max_24h: f64,
}

/// Everything a run needs besides where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct RunContext {
    pub station: Station,
    pub model: Model,
    pub date: UtcDate,
    pub nowcast: Option<Nowcast>,
}

impl RunContext {
    /// The sunspot number this run states, and where it came from. For
    /// Truecast without live readings this is the table's month, shown to
    /// the reader; the engine derives its own daily index.
    pub fn ssn(&self) -> (f64, Basis) {
        match self.nowcast {
            Some(n) => (n.effective_ssn, Basis::Nowcast),
            None => ssn::for_month(self.date.year, self.date.month),
        }
    }

    /// The part of the context that changes an answer, for cache keys.
    pub fn signature(&self) -> String {
        let day = if self.model == Model::Truecast {
            self.date.day
        } else {
            0
        };
        let now = self.nowcast.map_or("-".to_string(), |n| {
            format!("{:.0}:{:.1}", n.effective_ssn, n.kp_max_24h)
        });
        format!(
            "{}|{}|{:04}-{:02}-{:02}|{}",
            self.station.signature(),
            self.model.key(),
            self.date.year,
            self.date.month,
            day,
            now
        )
    }
}

/// The engine, with a writable directory for the files a run reads beside
/// the compiled-in coefficients (antenna definitions) and the ones it
/// synthesises (Truecast below an index of zero).
#[derive(Clone, Debug)]
pub struct Engine {
    work_dir: PathBuf,
}

impl Engine {
    pub fn new(work_dir: impl Into<PathBuf>) -> Engine {
        Engine {
            work_dir: work_dir.into(),
        }
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// The request fields every run shares: data root, antenna, date, model,
    /// power and requirement.
    pub fn base_request(&self, ctx: &RunContext) -> Result<Map<String, Value>, Error> {
        let station = ctx.station.clamped();
        let mut req = Map::new();
        match station.antenna.voa_file() {
            None => {
                req.insert("itshfbc".into(), json!("<embedded>"));
            }
            Some(file) => {
                self.write_once(&file.path, &file.text)?;
                req.insert(
                    "itshfbc".into(),
                    json!(format!("<embedded>+{}", self.work_dir.display())),
                );
                req.insert(
                    "txAntenna".into(),
                    json!({
                        "file": file.name,
                        "beamDeg": station.antenna.engine_beam_deg(),
                        "minFreq": MIN_CARD_FREQ_MHZ,
                    }),
                );
            }
        }
        req.insert("month".into(), json!(ctx.date.month));
        req.insert("year".into(), json!(ctx.date.year));
        match ctx.model {
            Model::Voacap => {
                req.insert("ssn".into(), json!(ctx.ssn().0));
            }
            Model::Truecast => {
                req.insert("engine".into(), json!("truecast"));
                req.insert("day".into(), json!(ctx.date.day));
                if let Some(n) = ctx.nowcast {
                    req.insert("essn".into(), json!(n.effective_ssn));
                }
                let dir = self.work_dir.join("truecast");
                fs::create_dir_all(&dir)?;
                req.insert("workDir".into(), json!(dir.display().to_string()));
            }
        }
        req.insert("watts".into(), json!(station.watts));
        req.insert(
            "requiredSnrDb".into(),
            json!(station.mode.required_snr_db()),
        );
        req.insert("noiseDbw".into(), json!(NOISE_DBW));
        Ok(req)
    }

    /// One request object in, one answer out. The engine's refusals come
    /// back as [`Error::Engine`] with its own message.
    pub fn run<T: for<'de> Deserialize<'de>>(&self, req: &Map<String, Value>) -> Result<T, Error> {
        let text = serde_json::to_string(req).map_err(|e| Error::Engine(e.to_string()))?;
        let answer = hfcast::service::run(&text).map_err(Error::Engine)?;
        serde_json::from_str(&answer).map_err(|e| Error::Engine(format!("unreadable answer: {e}")))
    }

    /// Writes a file under the work directory unless it is already there
    /// with these contents. Through a temporary name, because several threads
    /// may want the same antenna at once.
    fn write_once(&self, relative: &str, text: &str) -> Result<(), Error> {
        let path = self.work_dir.join(relative);
        if fs::read_to_string(&path).is_ok_and(|have| have == text) {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension(
            format!("tmp{:?}", std::thread::current().id()).replace(['(', ')'], ""),
        );
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::antenna::AntennaKind;

    pub(crate) fn ctx(model: Model) -> RunContext {
        RunContext {
            station: Station::default(),
            model,
            date: UtcDate {
                year: 2026,
                month: 10,
                day: 25,
                hour: 12,
            },
            nowcast: None,
        }
    }

    #[test]
    fn isotropic_needs_no_files() {
        let dir = std::env::temp_dir().join("hfp-engine-test-iso");
        let req = Engine::new(&dir).base_request(&ctx(Model::Voacap)).unwrap();
        assert_eq!(req["itshfbc"], "<embedded>");
        assert!(req.get("txAntenna").is_none());
        assert!(req.get("ssn").is_some() && req.get("engine").is_none());
    }

    #[test]
    fn antenna_goes_into_the_overlay() {
        let dir = std::env::temp_dir().join("hfp-engine-test-dipole");
        let mut c = ctx(Model::Truecast);
        c.station.antenna.kind = AntennaKind::Dipole;
        c.station.antenna.beam_deg = 350.0;
        let req = Engine::new(&dir).base_request(&c).unwrap();
        assert!(req["itshfbc"].as_str().unwrap().starts_with("<embedded>+"));
        assert_eq!(req["txAntenna"]["file"], "hfp/d10g0.voa");
        assert!(dir.join("antennas/hfp/d10g0.voa").exists());
        // Truecast with no readings states no index: the engine derives it.
        assert!(req.get("ssn").is_none() && req.get("essn").is_none());
        assert_eq!(req["day"], 25);
    }
}
