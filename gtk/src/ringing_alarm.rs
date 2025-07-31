//! UI for an actively ringing alarm.

use std::cell::Cell;
use std::time::Duration as StdDuration;

use alarm::Alarms;
use alarm::audio::AlarmSound;
use gtk4::glib::MainContext;
use gtk4::pango::WrapMode;
use gtk4::prelude::*;
use gtk4::{Align, Button, Label, Orientation};
use rezz::Alarm;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::navigation::{Navigator, Page};

pub struct RingingAlarmPage {
    navigator: Navigator,
    container: gtk4::Box,
    stop_button: Button,
    name_label: Label,
    time_label: Label,
}

impl RingingAlarmPage {
    pub fn new(navigator: Navigator) -> Self {
        let container = gtk4::Box::new(Orientation::Vertical, 0);
        container.set_vexpand(true);
        container.set_margin_top(25);
        container.set_margin_end(25);
        container.set_margin_bottom(25);
        container.set_margin_start(25);

        // Add label box to spread labels/button apart.
        let label_box = gtk4::Box::new(Orientation::Vertical, 25);
        label_box.set_valign(Align::Center);
        label_box.set_vexpand(true);
        container.append(&label_box);

        // Add label for alarm name.
        let name_label = Label::new(None);
        name_label.add_css_class("ringing-name");
        name_label.set_wrap(true);
        name_label.set_wrap_mode(WrapMode::WordChar);
        label_box.append(&name_label);

        // Add label for alarm time.
        let time_label = Label::new(None);
        time_label.add_css_class("ringing-time");
        label_box.append(&time_label);

        // Add placeholder stop button.
        let stop_button = Button::new();
        container.append(&stop_button);

        Self { navigator, container, stop_button, name_label, time_label }
    }

    /// Ring the specified alarm.
    pub async fn ring(&mut self, alarm: Alarm) {
        // Get hour and minute from the alarm.
        let time = OffsetDateTime::UNIX_EPOCH + Duration::seconds(alarm.unix_time);
        let local_time =
            time.to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));
        let hour = local_time.time().hour();
        let minute = local_time.time().minute();

        // Update labels.
        self.name_label.set_label(&alarm.id);
        self.time_label.set_label(&format!("{hour:0>2}:{minute:0>2}"));

        // Start ringing alarm.
        let sound = match AlarmSound::play() {
            Ok(sound) => sound,
            Err(err) => {
                crate::show_error(err.to_string());
                return;
            },
        };

        // Switch view.
        self.navigator.show(Self::id());

        // Create new alarm button, to ensure we don't leak click handlers.
        self.container.remove(&self.stop_button);
        self.stop_button = Button::with_label("Stop");
        self.container.append(&self.stop_button);

        // Add click listener for stopping the alarm.
        let button_data = Cell::new(Some((alarm.id, sound)));
        let stop_navigator = self.navigator.clone();
        self.stop_button.connect_clicked(move |_| {
            // Cancel alarm on first button press.
            if let Some((id, sound)) = button_data.replace(None) {
                MainContext::default().spawn_local(async {
                    let _ = Alarms.remove(id).await;
                });
                sound.stop();
            }

            stop_navigator.pop();
        });

        // Automatically stop alarm after `ring_seconds` elapsed.
        //
        // This is spawned in the background to avoid blocking our event loop.
        let stop_button = self.stop_button.clone();
        MainContext::default().spawn_local(async move {
            tokio::time::sleep(StdDuration::from_secs(alarm.ring_seconds as u64)).await;
            stop_button.emit_clicked();
        });
    }
}

impl Page<gtk4::Box> for RingingAlarmPage {
    fn id() -> &'static str {
        "ringing_alarm"
    }

    fn widget(&self) -> &gtk4::Box {
        &self.container
    }
}
