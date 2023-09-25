//! Alarm clock CLI interface.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::ParseIntError;
use std::process;
use std::str::FromStr;

use alarm::Alarms;
use clap::{Args, Parser, Subcommand};
use rezz::Alarm;
use time::error::ComponentRange;
use time::format_description::well_known::Rfc2822;
use time::{Duration, Month, OffsetDateTime, Time, UtcOffset};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    subcommand: Subcmd,
}

#[derive(Subcommand, Debug)]
enum Subcmd {
    /// Alarm background daemon.
    #[clap(alias = "d")]
    Daemon(DaemonArgs),
    /// Add a new alarm.
    #[clap(alias = "a")]
    Add(AddArgs),
    /// Remove an existing alarm.
    #[clap(alias = "r")]
    Remove(RemoveArgs),
    /// List all alarms.
    #[clap(alias = "l")]
    List(ListArgs),
}

#[derive(Args, Debug)]
struct DaemonArgs {}

#[derive(Args, Debug)]
struct AddArgs {
    /// ID used to delete the alarm.
    #[clap(long)]
    id: Option<String>,
    /// Alarm time in RFC3339 format.
    time: ClapDateTime,
}

#[derive(Args, Debug)]
struct RemoveArgs {
    /// Alarm IDs.
    id: Vec<String>,
}

#[derive(Args, Debug)]
struct ListArgs {}

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    let cli = Cli::parse();

    match cli.subcommand {
        Subcmd::Add(args) => {
            let id = args.id.unwrap_or_else(|| Uuid::new_v4().to_string());
            let unix_time = (args.time.0 - OffsetDateTime::UNIX_EPOCH).whole_seconds();
            let alarm = Alarm { id: id.clone(), unix_time };

            match Alarms.add(alarm).await {
                Ok(()) => println!("Added alarm with ID {id:?}"),
                Err(err) => eprintln!("Could not add alarm: {err}"),
            }
        },
        Subcmd::Remove(args) => {
            for id in &args.id {
                match Alarms.remove(id.clone()).await {
                    Ok(()) => println!("Removed alarm with ID {:?}", args.id),
                    Err(err) => eprintln!("Could not remove alarm: {err}"),
                }
            }
        }
        Subcmd::List(_args) => {
            let alarms = match Alarms.load().await {
                Ok(alarms) => alarms,
                Err(err) => {
                    eprintln!("Could not read alarms database: {err}");
                    process::exit(1);
                },
            };

            // Early return without any alarms.
            if alarms.is_empty() {
                println!("No alarms set");
                return;
            }

            // Print header.
            println!("\x1b[4;1m{: <36}  {: <31}\x1b[0m", "ID", "Alarm Time");

            // Print each alarm.
            for alarm in alarms {
                // Try to convert unix seconds to local time.
                let mut time =
                    OffsetDateTime::UNIX_EPOCH + Duration::seconds(alarm.unix_time as i64);
                if let Ok(offset) = UtcOffset::current_local_offset() {
                    time = time.to_offset(offset);
                }
                let time_str = time.format(&Rfc2822).unwrap();

                println!("{: <36}  {: <31}", alarm.id, time_str);
            }
        },
        Subcmd::Daemon(_args) => {
            if let Err(err) = Alarms.daemon().await {
                eprintln!("Daemon error: {err}");
            }
        },
    }
}

/// DateTime wrapper with `FromStr` implementation.
#[derive(Clone, Debug)]
struct ClapDateTime(OffsetDateTime);

impl FromStr for ClapDateTime {
    type Err = DateTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Get current time.
        let mut now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());

        // Split date and time.
        let (date, time) = match s.split_once('T') {
            Some((date, time)) => (Some(date), time),
            None => (None, s),
        };

        // Override date.
        if let Some(date) = date {
            let mut components = date.splitn(3, '-');

            let year =
                components.next().ok_or_else(|| DateTimeError::InvalidFormat(date.into()))?;
            now.replace_year(i32::from_str(&year)?)?;

            let month =
                components.next().ok_or_else(|| DateTimeError::InvalidFormat(date.into()))?;
            let month_offset = u8::from_str(&month)?.saturating_sub(1);
            now.replace_month(Month::January.nth_next(month_offset))?;

            let day = components.next().ok_or_else(|| DateTimeError::InvalidFormat(date.into()))?;
            now.replace_day(u8::from_str(&day)?)?;
        }

        // Override time.

        let (hour, rest) =
            time.split_once(':').ok_or_else(|| DateTimeError::InvalidFormat(time.into()))?;
        let (minute, second) = match rest.split_once(':') {
            Some((minute, second)) => (minute, Some(second)),
            None => (rest, None),
        };

        let hour = u8::from_str(hour)?;
        let minute = u8::from_str(minute)?;
        let second = second.map_or(Ok(0), u8::from_str)?;
        let time = Time::from_hms(hour, minute, second)?;

        // Add one day if time has already passed.
        if time < now.time() {
            now = now + Duration::days(1);
        }

        now = now.replace_time(time);

        Ok(Self(now))
    }
}

#[derive(Clone, Debug)]
enum DateTimeError {
    ComponentRange(ComponentRange),
    InvalidFormat(String),
    ParseInt(ParseIntError),
}

impl Error for DateTimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ComponentRange(err) => Some(err),
            Self::ParseInt(err) => Some(err),
            _ => None,
        }
    }
}

impl Display for DateTimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(component) => write!(f, "invalid format: {component:?}"),
            Self::ComponentRange(err) => write!(f, "{err}"),
            Self::ParseInt(err) => write!(f, "{err}"),
        }
    }
}

impl From<ComponentRange> for DateTimeError {
    fn from(error: ComponentRange) -> Self {
        Self::ComponentRange(error)
    }
}

impl From<ParseIntError> for DateTimeError {
    fn from(error: ParseIntError) -> Self {
        Self::ParseInt(error)
    }
}
