use std::time::Duration as StdDuration;

use alarm::audio::AlarmSound;
use alarm::{Alarms, Event, Subscriber};
use gtk4::gdk::Display;
use gtk4::glib::{ExitCode, MainContext};
use gtk4::prelude::*;
use gtk4::{
    AlertDialog, Align, Application, ApplicationWindow, Button, CssProvider, Orientation, Window,
};
use rezz::Alarm;
use time::macros::format_description;
use time::util::local_offset::{self, Soundness};
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::navigation::{Navigator, Page};
use crate::new_alarm::NewAlarmPage;

pub mod navigation;
mod new_alarm;

/// Wayland application ID.
const APP_ID: &str = "catacomb.Alarm";

#[tokio::main]
async fn main() -> ExitCode {
    // Allow retrieving local offset despite multi-threading.
    unsafe { local_offset::set_soundness(Soundness::Unsound) };

    // Setup application.
    let application = Application::builder().application_id(APP_ID).build();

    // Load CSS.
    application.connect_startup(|_| {
        // Create stylesheet.
        let provider = CssProvider::new();
        provider.load_from_data(include_str!("../style.css"));

        // Apply stylesheet to the application.
        gtk4::style_context_add_provider_for_display(
            &Display::default().expect("connect to display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    // Handle application activation event.
    application.connect_activate(activate);

    // Run application.
    application.run()
}

/// Bootstrap UI.
fn activate(app: &Application) {
    // Configure window settings.
    let window = ApplicationWindow::builder().application(app).title("Alarm").build();

    // Setup page navigation.
    let navigator = Navigator::new();
    window.set_child(Some(navigator.widget()));

    // Add alarm creation page.
    let new_navigator = navigator.clone();
    let new_alarm_page = NewAlarmPage::new(new_navigator);
    navigator.add(&new_alarm_page);

    // Add landing page.
    let overview_navigator = navigator.clone();
    let overview = Overview::new(overview_navigator, new_alarm_page);
    navigator.add(&overview);

    // Show window.
    navigator.show(Overview::id());
    window.present();

    // Handle overview alarm updates.
    MainContext::default().spawn_local(async {
        overview.listen().await;
    });
}

/// Alarm overview and landing page.
pub struct Overview {
    container: gtk4::Box,
    alarms: gtk4::Box,
}

impl Overview {
    fn new(navigator: Navigator, new_alarm_page: NewAlarmPage) -> Self {
        let container = gtk4::Box::new(Orientation::Vertical, 0);
        container.set_valign(Align::End);

        // Create alarms container.
        let alarms = gtk4::Box::new(Orientation::Vertical, 0);
        alarms.set_valign(Align::End);
        container.append(&alarms);

        // Button to create new alarms.
        let new_button = Button::with_label("Add Alarm");
        container.append(&new_button);

        // Handle new alarm button press.
        new_button.connect_clicked(move |_| {
            new_alarm_page.reset();
            navigator.show(NewAlarmPage::id());
        });

        Self { container, alarms }
    }

    /// Update the view with new alarms.
    fn update(&mut self, alarms: &[Alarm]) {
        let time_format = format_description!("[year]-[month]-[day] [hour]:[minute]");
        let utc_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);

        // Create new alarms container.
        let container = gtk4::Box::new(Orientation::Vertical, 0);
        for alarm in alarms {
            // Add button for each alarm.
            let time = OffsetDateTime::UNIX_EPOCH + Duration::seconds(alarm.unix_time);
            let local_time = time.to_offset(utc_offset);
            let time_str = local_time.format(&time_format).unwrap();
            let button = Button::with_label(&time_str);
            container.append(&button);

            // Remove alarm on button press.
            let id = alarm.id.clone();
            button.connect_clicked(move |_| {
                let id = id.clone();
                MainContext::default().spawn(async move {
                    if let Err(err) = Alarms.remove(id.clone()).await {
                        show_error(err.to_string());
                    }
                });
            });
        }

        // Swap containers.
        self.container.remove(&self.alarms);
        self.container.prepend(&container);
        self.alarms = container;
    }

    /// Update view on new/removed alarms.
    async fn listen(mut self) {
        // Subscribe to DBus events.
        let mut subscriber = match Subscriber::new().await {
            Ok(subscriber) => subscriber,
            Err(err) => {
                show_error(err.to_string());
                return;
            },
        };

        // Seed GTK view with initial alarms.
        self.update(subscriber.alarms());

        loop {
            match subscriber.next().await {
                // Update alarms.
                Some(Event::AlarmsChanged(alarms)) => self.update(alarms),
                // Play alarm sound.
                Some(Event::Ring(alarm)) => {
                    let sound = match AlarmSound::play() {
                        Ok(sound) => sound,
                        Err(err) => {
                            eprintln!("Could not play alarm sound: {err}");
                            continue;
                        },
                    };
                    tokio::time::sleep(StdDuration::from_secs(alarm.ring_seconds as u64)).await;
                    sound.stop();
                },
                None => (),
            }
        }
    }
}

impl Page<gtk4::Box> for Overview {
    fn id() -> &'static str {
        "overview"
    }

    fn widget(&self) -> &gtk4::Box {
        &self.container
    }
}

/// Display an error message in a new window.
pub fn show_error(message: String) {
    let alert = AlertDialog::builder().message(message).build();
    alert.show(None::<&Window>);
}
