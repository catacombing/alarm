use alarm::{Alarms, Event, Subscriber};
use gtk4::gdk::Display;
use gtk4::glib::{ExitCode, MainContext};
use gtk4::prelude::*;
use gtk4::{
    AlertDialog, Align, Application, ApplicationWindow, Button, CssProvider, Label, Orientation,
    Window,
};
use rezz::Alarm;
use time::macros::format_description;
use time::util::local_offset::{self, Soundness};
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::navigation::{Navigator, Page};
use crate::new_alarm::NewAlarmPage;
use crate::ringing_alarm::RingingAlarmPage;

pub mod navigation;
mod new_alarm;
mod ringing_alarm;

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
    let new_alarm_page = NewAlarmPage::new(navigator.clone());
    navigator.add(&new_alarm_page);

    // Add ringing alarm page.
    let ringing_alarm_page = RingingAlarmPage::new(navigator.clone());
    navigator.add(&ringing_alarm_page);

    // Add landing page.
    let overview = Overview::new(navigator.clone(), new_alarm_page, ringing_alarm_page);
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
    ringing_alarm_page: RingingAlarmPage,
    container: gtk4::Box,
    alarms: gtk4::Box,
}

impl Overview {
    fn new(
        navigator: Navigator,
        new_alarm_page: NewAlarmPage,
        ringing_alarm_page: RingingAlarmPage,
    ) -> Self {
        let container = gtk4::Box::new(Orientation::Vertical, 0);
        container.set_valign(Align::End);

        // Create alarms container.
        let alarms = gtk4::Box::new(Orientation::Vertical, 0);
        alarms.set_valign(Align::End);
        container.append(&alarms);

        // Button to create new alarms.
        let new_button = Button::with_label("Add Alarm");
        new_button.set_margin_top(25);
        container.append(&new_button);

        // Handle new alarm button press.
        new_button.connect_clicked(move |_| {
            new_alarm_page.reset();
            navigator.show(NewAlarmPage::id());
        });

        Self { container, alarms, ringing_alarm_page }
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
                Some(Event::Ring(alarm)) => self.ringing_alarm_page.ring(alarm).await,
                None => (),
            }
        }
    }

    /// Update the view with new alarms.
    fn update(&mut self, alarms: &[Alarm]) {
        // Create new alarms container.
        let container = gtk4::Box::new(Orientation::Vertical, 0);
        for alarm in alarms {
            container.append(&Self::alarm_components(alarm));
        }

        // Swap containers.
        self.container.remove(&self.alarms);
        self.container.prepend(&container);
        self.alarms = container;
    }

    /// Get the GTK components for an alarm.
    fn alarm_components(alarm: &Alarm) -> gtk4::Box {
        // Convert unix time to local time.
        let utc_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let time = OffsetDateTime::UNIX_EPOCH + Duration::seconds(alarm.unix_time);
        let local_time = time.to_offset(utc_offset);

        let container = gtk4::Box::new(Orientation::Horizontal, 0);
        container.set_margin_start(50);
        container.set_margin_top(10);
        container.set_margin_end(50);
        container.set_margin_bottom(10);

        // Create vertical container to show date below time.
        let datetime_container = gtk4::Box::new(Orientation::Vertical, 0);
        datetime_container.set_hexpand(true);
        container.append(&datetime_container);

        // Add alarm's time.
        let time_format = format_description!("[hour]:[minute]");
        let time_str = local_time.format(&time_format).unwrap();
        let time_label = Label::new(Some(&time_str));
        time_label.add_css_class("overview-alarm-time");
        time_label.set_halign(Align::Start);
        datetime_container.append(&time_label);

        // Add alarms date.
        let date_format = format_description!("[year]-[month]-[day]");
        let date_str = local_time.format(&date_format).unwrap();
        let date_label = Label::new(Some(&date_str));
        date_label.add_css_class("overview-alarm-date");
        date_label.set_halign(Align::Start);
        datetime_container.append(&date_label);

        // Add button to dismiss alarm.
        let button = Button::from_icon_name("edit-delete");
        button.add_css_class("overview-alarm-button");
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

        container
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
