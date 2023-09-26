use std::time::{Duration, SystemTime};

use rezz::Alarm;
use tokio::time::{self, Instant, Sleep};
use tokio_stream::StreamExt;
use zbus::Connection;

use crate::audio::AlarmSound;
use crate::dbus::RezzProxy;
use crate::error::Error;

pub mod audio;
mod dbus;
pub mod error;

/// Maximum alarm sound playback duration.
const MAX_RING_DURATION: Duration = Duration::from_secs(600);

/// Primary alarm interface.
pub struct Alarms;

impl Alarms {
    /// Run the alarm daemon.
    ///
    /// This will automatically monitor the alarm database and play an alarm
    /// sound when necessary.
    pub async fn daemon(&self) -> Result<(), Error> {
        // Setup DBus connection.
        let connection = Connection::system().await?;
        let rezz = RezzProxy::new(&connection).await?;

        // Create listener for alarms change.
        let mut alarms = rezz.alarms().await?;
        let mut alarms_stream = rezz.receive_alarms_changed().await;

        // Get next alarm.
        let mut next_alarm = self.next_alarm(&mut alarms);

        loop {
            tokio::select! {
                // Handle alarm updates.
                Some(new_alarms) = alarms_stream.next() => {
                    if let Ok(new_alarms) = new_alarms.get().await {
                        alarms = new_alarms;
                    }
                },
                // Ring the alarm.
                _ = self.wait_alarm(next_alarm) => {
                    if next_alarm.is_some() {
                        if let Err(err) = self.ring_alarm().await {
                            eprintln!("could not ring alarm: {err}");
                        }
                    }
                },
            }

            next_alarm = self.next_alarm(&mut alarms);
        }
    }

    /// Add a new alarm.
    pub async fn add(&self, alarm: Alarm) -> Result<(), Error> {
        let connection = Connection::system().await?;
        let rezz = RezzProxy::new(&connection).await?;
        rezz.add_alarm(alarm.id, alarm.unix_time).await?;
        Ok(())
    }

    /// Remove an existing alarm.
    pub async fn remove(&self, id: String) -> Result<(), Error> {
        let connection = Connection::system().await?;
        let rezz = RezzProxy::new(&connection).await?;
        rezz.remove_alarm(id).await?;
        Ok(())
    }

    /// Load the alarm database.
    ///
    /// This will create the database, to simplify inotify usage.
    pub async fn load(&self) -> Result<Vec<Alarm>, Error> {
        let connection = Connection::system().await?;
        let rezz = RezzProxy::new(&connection).await?;
        let alarms = rezz.alarms().await?;
        Ok(alarms)
    }

    /// Ring an alarm.
    async fn ring_alarm(&self) -> Result<(), Error> {
        let sound = AlarmSound::play()?;
        time::sleep(MAX_RING_DURATION).await;
        sound.stop();
        Ok(())
    }

    /// Get the next alarm.
    ///
    /// This will ignore all elapsed alarms and sort the array to ensure optimal
    /// performance.
    fn next_alarm<'a, 'b>(&'a self, alarms: &'b mut Vec<Alarm>) -> Option<&'b Alarm> {
        // Get seconds since unix epoch.
        let current_secs =
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

        // Get the next non-elapsed alarm.
        alarms.sort_by(|a, b| a.unix_time.cmp(&b.unix_time));
        alarms.iter().skip_while(|alarm| alarm.unix_time as u64 <= current_secs).next()
    }

    /// Convert alarm to tokio async sleep.
    fn wait_alarm(&self, alarm: Option<&Alarm>) -> Sleep {
        // Default to an hour without alarm present.
        let alarm = match alarm {
            Some(alarm) => alarm,
            None => return time::sleep(Duration::from_secs(60 * 60)),
        };

        // Get time until alarm.
        let current_secs =
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        let instant = Instant::now() + Duration::from_secs(alarm.unix_time as u64 - current_secs);
        time::sleep_until(instant)
    }
}
