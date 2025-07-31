//! DBus RTC wakeup server.

use std::error::Error;
use std::fs::{self, File};
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Read, Seek, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use futures_util::stream::StreamExt;
use rezz::Alarm;
use time::{Duration, OffsetDateTime};
use tokio::sync::{RwLock, watch};
use tokio::time as tokio_time;
use tracing::{debug, error, info, warn};
use zbus::Connection;
use zbus::connection::Builder;
use zbus::fdo::Error as ZBusError;
use zbus::zvariant::OwnedFd;

use crate::logind::{ManagerProxy, PrepareForSleepStream};

/// Database location.
const DB_PATH: &str = "/var/lib/rezz/alarms.db";

/// Update frequency on systems without logind.
const MANUAL_UPDATE_INTERVAL: StdDuration = StdDuration::from_secs(60 * 5);

/// Infinite sleep timeout.
const INFINITY: StdDuration = StdDuration::from_secs(60 * 60 * 24 * 365 * 999);

/// Start the DBus server.
pub async fn launch() {
    let mut rezz = match Rezz::new(DB_PATH).await {
        Ok(rezz) => rezz,
        Err(err) => {
            error!("Could not read alarm DB: {err}");
            return;
        },
    };

    let connection = match create_connection(rezz.clone()).await {
        Ok(connection) => connection,
        Err(err) => {
            error!("Could not create DBus connection: {err}");
            return;
        },
    };

    // Immediately cleanup alarms at startup.
    let mut wait_alarm = tokio_time::sleep(StdDuration::from_secs(0));

    // Get logind suspend stream.
    let mut suspend_stream = match logind_suspend_stream(&connection, &mut rezz).await {
        Ok(suspend_stream) => Some(suspend_stream),
        Err(err) => {
            warn!("Running without logind support: {err}");
            None
        },
    };

    // Listen for db changes.
    let mut alarms_changed = rezz.alarms.read().await.subscribe();

    info!("DBus server started successfully");

    loop {
        tokio::select! {
            // Signal alarm changes to DBus clients.
            _ = alarms_changed.changed() => {
                debug!("Alarms changed");

                let object_server = connection.object_server();
                let iface = object_server.interface::<_, Rezz>("/org/catacombing/rezz").await.unwrap();
                let _ = rezz.alarms_changed(iface.signal_emitter()).await;
            },
            // Update expired alarms.
            _ = wait_alarm => debug!("Alarm expired"),
            // Handle suspend/wakeup.
            is_suspend = await_suspend(&mut suspend_stream) => {
                if is_suspend {
                    debug!("Handling suspend");
                    rezz.on_suspend().await;
                } else {
                    debug!("Handling wakeup");
                    rezz.add_logind_inhibitor(&connection).await;
                }
            }
        }

        // Ensure old alarms are cleaned up.
        let mut alarms = rezz.alarms.write().await;
        alarms.remove_elapsed();

        // Update event loop alarm timeout.
        wait_alarm = match alarms.upcoming() {
            Some(next_alarm) => {
                let alarm_end = next_alarm.unix_time + next_alarm.ring_seconds as i64;
                let seconds = alarm_end.saturating_sub(unix_now());
                tokio_time::sleep(StdDuration::from_secs(seconds as u64))
            },
            None => tokio_time::sleep(INFINITY),
        };
    }
}

/// Establish DBus system bus connection.
async fn create_connection(rezz: Rezz) -> Result<Connection, zbus::Error> {
    Builder::system()?
        .name("org.catacombing.rezz")?
        .serve_at("/org/catacombing/rezz", rezz)?
        .build()
        .await
}

/// Get a stream of logind suspend/wakeup events.
async fn logind_suspend_stream(
    connection: &Connection,
    rezz: &mut Rezz,
) -> Result<PrepareForSleepStream, Box<dyn Error>> {
    // Setup DBus logind suspend listener.
    let logind = ManagerProxy::new(connection).await?;
    let suspend_stream = logind.receive_prepare_for_sleep().await?;

    // Add initial suspend delay inhibitor.
    rezz.add_logind_inhibitor(connection).await;

    Ok(suspend_stream)
}

/// Poll the logind suspend stream.
///
/// Returns `true` on suspend, `false` on unsuspend.
///
/// This will use a fixed timer on systems without logind and will always return
/// `true`.
async fn await_suspend(logind_stream: &mut Option<PrepareForSleepStream>) -> bool {
    match logind_stream {
        Some(stream) => {
            let next_event = stream.next().await;
            next_event
                .and_then(|event| event.message().body().deserialize::<bool>().ok())
                .unwrap_or(true)
        },
        None => {
            tokio_time::sleep(MANUAL_UPDATE_INTERVAL).await;
            true
        },
    }
}

/// Register logind inhibitor.
async fn inhibit(
    connection: &Connection,
    what: &str,
    who: &str,
    why: &str,
    mode: &str,
) -> zbus::Result<OwnedFd> {
    let logind = ManagerProxy::new(connection).await?;
    let inhibitor = logind.inhibit(what, who, why, mode).await?;
    Ok(inhibitor)
}

struct Rezz {
    alarms: Arc<RwLock<Store>>,
    inhibitor: Option<OwnedFd>,
}

impl Clone for Rezz {
    fn clone(&self) -> Self {
        Self { alarms: self.alarms.clone(), inhibitor: None }
    }
}

impl Rezz {
    async fn new(db: impl AsRef<Path>) -> Result<Self, IoError> {
        let alarms = Arc::new(RwLock::new(Store::new(db)?));
        Ok(Self { alarms, inhibitor: Default::default() })
    }

    /// Pre-sleep hook.
    async fn on_suspend(&mut self) {
        // Remove outdated alarms.
        {
            let mut alarms = self.alarms.write().await;
            alarms.remove_elapsed();
        }

        // Ensure next alarm is scheduled.
        self.schedule_nearest().await;

        // Drop inhibitor to initiate suspend.
        self.inhibitor.take();
    }

    /// Update logind sleep delay inhibitor.
    async fn add_logind_inhibitor(&mut self, connection: &Connection) {
        let inhibitor = inhibit(connection, "sleep", "Rezz", "RTC clock updates", "delay").await;

        self.inhibitor = match inhibitor {
            Ok(inhibitor) => Some(inhibitor),
            Err(err) => {
                error!("Could not register logind sleep inhibitor: {err}");
                return;
            },
        };
    }

    /// Ensure the next wakeup is not after the closest alarm.
    async fn schedule_nearest(&self) {
        let alarms = self.alarms.read().await;

        // Get nearest alarm.
        let next_alarm = match alarms.upcoming() {
            Some(next_alarm) => next_alarm,
            None => return,
        };

        // Get staged RTC alarm, if any.
        let wakeup = match rezz::get_wakeup() {
            Ok(wakeup) => wakeup,
            Err(err) => {
                error!("Could not read WKALM: {err}");
                None
            },
        };

        // Ignore alarms beyond the scheduled one.
        let current_time = OffsetDateTime::now_utc();
        let time = OffsetDateTime::UNIX_EPOCH + Duration::seconds(next_alarm.unix_time);
        if wakeup.is_some_and(|wakeup| wakeup > current_time && time >= wakeup) {
            return;
        }

        // Set a new RTC alarm.
        if let Err(err) = rezz::set_wakeup(time) {
            error!("Could set WKALM: {err}");
        }
    }
}

#[zbus::interface(name = "org.catacombing.rezz")]
impl Rezz {
    async fn add_alarm(&mut self, alarm: Alarm) -> Result<(), ZBusError> {
        let id = alarm.id.clone();
        let added = {
            let mut alarms = self.alarms.write().await;
            alarms.add(alarm)
        };

        if !added {
            let msg = format!("ID {id:?} already exists");
            error!("Could not add alarm: {msg}");

            return Err(ZBusError::InvalidArgs(msg));
        }

        // Ensure timely RTC clock updates without logind.
        self.schedule_nearest().await;

        Ok(())
    }

    async fn remove_alarm(&self, id: String) -> Result<(), ZBusError> {
        let removed = {
            let mut alarms = self.alarms.write().await;

            // Remove alarm from internal cache.
            match alarms.remove(&id) {
                Some(alarm) => alarm,
                None => {
                    let msg = format!("Cannot remove alarm {id:?}: Invalid ID");
                    warn!(msg);

                    return Err(ZBusError::InvalidArgs(msg));
                },
            }
        };

        // Get currently staged RTC alarms.
        let wakeup = match rezz::get_wakeup() {
            Ok(Some(wakeup)) => wakeup,
            Ok(None) => return Ok(()),
            Err(err) => {
                let msg = format!("Could not read WKALM: {err}");
                error!(msg);

                return Err(ZBusError::Failed(msg));
            },
        };

        // Ignore if staged RTC alarm does not match the alarm.
        let time = OffsetDateTime::UNIX_EPOCH + Duration::seconds(removed.unix_time);
        if time != wakeup {
            return Ok(());
        }

        // Clear the staged RTC alarm.
        if let Err(err) = rezz::clear_wakeup() {
            error!("Could not clear WKALM: {err}");
        }

        // Ensure timely RTC clock updates without logind.
        self.schedule_nearest().await;

        Ok(())
    }

    #[zbus(property)]
    async fn alarms(&self) -> Vec<Alarm> {
        let alarms = self.alarms.read().await;
        alarms.alarms.clone()
    }
}

/// Filesystem-based alarm store.
struct Store {
    alarms: Vec<Alarm>,
    onchange_rx: watch::Receiver<()>,
    onchange_tx: watch::Sender<()>,
    db: File,
}

impl Store {
    fn new(db_path: impl AsRef<Path>) -> Result<Self, IoError> {
        // Create db if necessary and open it.
        let db_path = db_path.as_ref();
        let parent = db_path.parent().ok_or_else(|| {
            let msg = format!("Invalid DB path: {db_path:?}");
            IoError::new(IoErrorKind::InvalidInput, msg)
        })?;
        fs::create_dir_all(parent)?;
        let mut db =
            File::options().read(true).write(true).create(true).truncate(false).open(db_path)?;

        // Parse existing alarms.
        let mut content = String::new();
        db.read_to_string(&mut content)?;
        let alarms = serde_json::from_str(&content).unwrap_or_default();

        // Create update channel.
        let (onchange_tx, onchange_rx) = watch::channel(());

        debug!("Alarms in DB {db_path:?}: {alarms:?}");

        Ok(Self { db, alarms, onchange_rx, onchange_tx })
    }

    /// Subscribe to changes.
    fn subscribe(&self) -> watch::Receiver<()> {
        self.onchange_rx.clone()
    }

    /// Get the next alarm.
    fn upcoming(&self) -> Option<&Alarm> {
        self.alarms.iter().min_by_key(|alarm| alarm.unix_time)
    }

    /// Add a new alarm.
    ///
    /// Returns `true` if the alarm was added and `false` if another alarm with
    /// the
    /// ID ID already exists.
    fn add(&mut self, alarm: Alarm) -> bool {
        if self.alarms.iter().any(|existing_alarm| existing_alarm.id == alarm.id) {
            return false;
        }

        self.alarms.push(alarm);

        self.sync();

        true
    }

    /// Remove an existing alarm.
    fn remove(&mut self, id: &str) -> Option<Alarm> {
        let matching = self.alarms.iter().position(|alarm| alarm.id == id)?;
        let removed = self.alarms.remove(matching);

        self.sync();

        Some(removed)
    }

    /// Remove all elapsed alarms.
    ///
    /// Returns the number of removed elements.
    fn remove_elapsed(&mut self) -> usize {
        let old_len = self.alarms.len();

        self.alarms.retain(|alarm| alarm.unix_time + alarm.ring_seconds as i64 > unix_now());

        // Update database if entries were deleted.
        let removed_count = old_len - self.alarms.len();
        if removed_count > 0 {
            self.sync();
        }

        removed_count
    }

    /// Write all pending DB changes to the filesystem and signal changes.
    fn sync(&mut self) {
        // Signal changes.
        let _ = self.onchange_tx.send(());

        let json = serde_json::to_string(&self.alarms).unwrap();

        // Overwrite the entire file.
        let result = self
            .db
            .set_len(0)
            .and_then(|_| self.db.rewind())
            .and_then(|_| self.db.write_all(json.as_bytes()));

        if let Err(err) = result {
            error!("Failed DB sync: {err}");
        }
    }
}

/// Current unix time.
fn unix_now() -> i64 {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    (now - OffsetDateTime::UNIX_EPOCH).whole_seconds()
}
