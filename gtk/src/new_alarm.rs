//! UI for creating a new alarm.

use alarm::Alarms;
use gtk4::glib::MainContext;
use gtk4::prelude::*;
use gtk4::{
    Adjustment, Align, Button, DropDown, Expression, Label, Orientation, PolicyType,
    ScrolledWindow, StringList,
};
use rezz::Alarm;
use time::{Duration, OffsetDateTime, Time};
use uuid::Uuid;

use crate::navigation::{Navigator, Page};

/// Height of hour/minute labels.
const TIME_LABEL_HEIGHT: i32 = 75;

/// Width of hour/minute labels.
const TIME_LABEL_WIDTH: i32 = 75;

/// Number of time labels visible at once.
const TIME_SLOT_COUNT: i32 = 3;

/// UI for adding a new alarm.
pub struct NewAlarmPage {
    container: gtk4::Box,
    ring_duration_input: RingDurationInput,
    time_input: TimeInput,
}

impl NewAlarmPage {
    /// Get the UI for adding a new alarm.
    pub fn new(navigator: Navigator) -> Self {
        let ring_duration_input = RingDurationInput::new();
        let time_input = TimeInput::new();
        let menu_buttons = MenuButtons::new();

        let container = gtk4::Box::new(Orientation::Vertical, 0);
        container.append(ring_duration_input.widget());
        container.append(time_input.widget());
        container.append(menu_buttons.widget());
        container.set_valign(Align::End);
        container.set_margin_top(25);
        container.set_margin_end(25);
        container.set_margin_bottom(25);
        container.set_margin_start(25);

        // Add confirm/cancel button handlers.
        let confirm_navigator = navigator.clone();
        let confirm_duration = ring_duration_input.clone();
        let confirm_time = time_input.clone();
        menu_buttons.on_confirm(move || {
            Self::confirm(&confirm_navigator, &confirm_duration, &confirm_time)
        });
        menu_buttons.on_cancel(move || navigator.pop());

        Self { container, ring_duration_input, time_input }
    }

    /// Reset the page to its default content.
    pub fn reset(&self) {
        self.ring_duration_input.reset();
        self.time_input.reset();
    }

    /// Confirm alarm creation
    fn confirm(
        navigator: &Navigator,
        ring_duration_input: &RingDurationInput,
        time_input: &TimeInput,
    ) {
        let ring_duration = ring_duration_input.duration().seconds();
        let unix_time = time_input.unix_time();
        let id = Uuid::new_v4().to_string();

        // Schedule the alarm.
        MainContext::default().spawn(async move {
            let alarm = Alarm::new(&id, unix_time, ring_duration);
            if let Err(err) = Alarms.add(alarm).await {
                crate::show_error(err.to_string());
            }
        });

        navigator.pop();
    }
}

impl Page<gtk4::Box> for NewAlarmPage {
    fn id() -> &'static str {
        "new_alarm"
    }

    fn widget(&self) -> &gtk4::Box {
        &self.container
    }
}

/// Ring duration input.
#[derive(Clone)]
struct RingDurationInput {
    container: gtk4::Box,
    dropdown: DropDown,
}

impl RingDurationInput {
    fn new() -> Self {
        let container = gtk4::Box::new(Orientation::Vertical, 10);

        let label = Label::new(Some("Ringing duration"));
        label.set_halign(Align::Start);
        container.append(&label);

        let options: Vec<_> = RingDuration::all().iter().map(RingDuration::label).collect();
        let dropdown = DropDown::new(Some(StringList::new(&options)), None::<Expression>);
        dropdown.set_selected(Self::default_offset());
        container.append(&dropdown);

        Self { dropdown, container }
    }

    /// Offset of the default option.
    fn default_offset() -> u32 {
        RingDuration::all().iter().position(|d| d == &RingDuration::default()).unwrap() as u32
    }

    /// Get the GTK widget.
    fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    /// Get the selected duration.
    fn duration(&self) -> RingDuration {
        RingDuration::all()[self.dropdown.selected() as usize]
    }

    /// Reset this input to its defaults.
    fn reset(&self) {
        self.dropdown.set_selected(Self::default_offset());
    }
}

/// Ring duration options.
#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub enum RingDuration {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    #[default]
    ThirtyMinutes,
    Forever,
}

impl RingDuration {
    /// Get all items in an unspecified, but well-defined order.
    fn all() -> &'static [Self] {
        &[
            Self::OneMinute,
            Self::FiveMinutes,
            Self::FifteenMinutes,
            Self::ThirtyMinutes,
            Self::Forever,
        ]
    }

    /// Get the text label for this option.
    fn label(&self) -> &str {
        match self {
            Self::OneMinute => "1 Minute",
            Self::FiveMinutes => "5 Minutes",
            Self::FifteenMinutes => "15 Minutes",
            Self::ThirtyMinutes => "30 Minutes",
            Self::Forever => "Forever",
        }
    }

    /// Get the ring duration in seconds.
    fn seconds(&self) -> u32 {
        match self {
            Self::OneMinute => 60,
            Self::FiveMinutes => 60 * 5,
            Self::FifteenMinutes => 60 * 15,
            Self::ThirtyMinutes => 60 * 30,
            Self::Forever => u32::MAX,
        }
    }
}

/// Alarm time selection input.
#[derive(Clone)]
struct TimeInput {
    container: gtk4::Box,
    hours: ScrolledWindow,
    minutes: ScrolledWindow,
}

impl TimeInput {
    fn new() -> Self {
        let container = gtk4::Box::new(Orientation::Vertical, 0);
        container.set_halign(Align::Center);
        container.set_margin_top(25);
        container.set_margin_bottom(25);
        container.add_css_class("time-box");

        // Create horizontal box for time selection.
        let time_box = gtk4::Box::new(Orientation::Horizontal, 0);
        time_box.set_halign(Align::Center);
        container.append(&time_box);

        // Create scrolled window for hour selection.
        let hour_labels: Vec<_> = (0..24).map(|hour| hour.to_string()).collect();
        let hours = Self::scroll_buttons(&hour_labels);
        time_box.append(&hours);

        // Hour/Minute separator.
        let time_separator = Label::new(None);
        time_separator.set_markup(r#"<span size="xx-large">:</span>"#);
        time_separator.set_margin_start(10);
        time_separator.set_margin_end(10);
        time_box.append(&time_separator);

        // Create scrolled window for minute selection.
        let minute_labels: Vec<_> = (0..60).map(|hour| hour.to_string()).collect();
        let minutes = Self::scroll_buttons(&minute_labels);
        time_box.append(&minutes);

        // Add label showing the time remaining until the alarm.
        let remaining_label = Label::new(None);
        remaining_label.add_css_class("remaining-label");
        remaining_label.set_margin_top(10);
        remaining_label.set_margin_bottom(10);
        container.append(&remaining_label);

        // Update label when time is changed.
        let minutes_remaining_label = remaining_label.clone();
        let hours_adjustment = hours.vadjustment();
        minutes.vadjustment().connect_value_changed(move |minutes| {
            let minute = Self::scroll_value(minutes);
            let hour = Self::scroll_value(&hours_adjustment);
            let remaining_text = Self::remaining_text(hour, minute);
            minutes_remaining_label.set_label(&remaining_text);
        });
        let minutes_adjustment = minutes.vadjustment();
        hours.vadjustment().connect_value_changed(move |hours| {
            let minute = Self::scroll_value(&minutes_adjustment);
            let hour = Self::scroll_value(hours);
            let remaining_text = Self::remaining_text(hour, minute);
            remaining_label.set_label(&remaining_text);
        });

        Self { container, hours, minutes }
    }

    /// Get the GTK widget.
    fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    /// Create a vertically-scrollable button box.
    ///
    /// This will create a button with the corresponding label text for every
    /// item in `labels`.
    fn scroll_buttons(labels: &[String]) -> ScrolledWindow {
        let label_box = gtk4::Box::new(Orientation::Vertical, 0);
        label_box.add_css_class("time-input-box");

        // Add placeholders at top to center the first label.
        for _ in 0..(TIME_SLOT_COUNT - 1) / 2 {
            let placeholder = gtk4::Box::new(Orientation::Horizontal, 0);
            placeholder.set_size_request(TIME_LABEL_WIDTH, TIME_LABEL_HEIGHT);
            label_box.append(&placeholder);
        }

        // Add all labels.
        for label in labels {
            let label = Label::new(Some(&format!("{label:0>2}")));
            label.set_size_request(TIME_LABEL_WIDTH, TIME_LABEL_HEIGHT);
            label_box.append(&label);
        }

        // Add placeholders at bottom to center last the label.
        for _ in 0..(TIME_SLOT_COUNT - 1) / 2 {
            let placeholder = gtk4::Box::new(Orientation::Horizontal, 0);
            placeholder.set_size_request(TIME_LABEL_WIDTH, TIME_LABEL_HEIGHT);
            label_box.append(&placeholder);
        }

        // Create scrollbox.
        let scroll = ScrolledWindow::new();
        scroll.set_child(Some(&label_box));
        scroll.set_size_request(TIME_LABEL_WIDTH, TIME_LABEL_HEIGHT * TIME_SLOT_COUNT);
        scroll.set_hscrollbar_policy(PolicyType::External);
        scroll.set_vscrollbar_policy(PolicyType::External);

        // Set scroll limits.
        let label_count = (labels.len() as i32 + TIME_SLOT_COUNT - 1) as f64;
        scroll.vadjustment().set_upper(label_count * TIME_LABEL_HEIGHT as f64);

        scroll
    }

    /// Get the selected minute.
    fn unix_time(&self) -> i64 {
        // Translate scrolling position to time.
        let minute = Self::scroll_value(&self.minutes.vadjustment());
        let hour = Self::scroll_value(&self.hours.vadjustment());
        let alarm_time = Self::alarm_time(hour, minute);

        // Convert time to unix time.
        (alarm_time - OffsetDateTime::UNIX_EPOCH).whole_seconds()
    }

    /// Reset this input to its defaults.
    fn reset(&self) {
        // Get current time.
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let mut time = now.time();

        // Add one minute to ensure time is in the future.
        time += Duration::minutes(1);

        // Update inputs.
        let pixel_offset_hours = time.hour() as f64 * TIME_LABEL_HEIGHT as f64;
        self.hours.vadjustment().set_value(pixel_offset_hours);
        let pixel_offset_minutes = time.minute() as f64 * TIME_LABEL_HEIGHT as f64;
        self.minutes.vadjustment().set_value(pixel_offset_minutes);
    }

    /// Convert scrolled window's value to integer.
    fn scroll_value(adjustment: &Adjustment) -> u8 {
        (adjustment.value() / TIME_LABEL_HEIGHT as f64).round() as u8
    }

    /// Get the alarm time from an hour and minute.
    fn alarm_time(hour: u8, minute: u8) -> OffsetDateTime {
        let time = Time::from_hms(hour, minute, 0).unwrap();

        // Get next occurrence of the specified time.
        let mut date_time =
            OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        if time < date_time.time() {
            date_time += Duration::days(1);
        }
        date_time = date_time.replace_time(time);

        date_time
    }

    /// Get the text for the "remaining time until alarm" label.
    fn remaining_text(hour: u8, minute: u8) -> String {
        // Get current and alarm time.
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let alarm_time = Self::alarm_time(hour, minute);

        // Get hours/minutes until alarm.
        let delta = alarm_time - now;
        let hours = delta.whole_hours();
        let minutes = delta.whole_minutes() - 60 * hours;

        // Format hours/minutes.
        let minute_unit = if minutes > 1 { "minutes" } else { "minute" };
        if hours == 0 && minutes == 0 {
            String::from("now")
        } else if hours == 0 {
            format!("in {minutes} {minute_unit}")
        } else {
            let hour_unit = if hours > 1 { "hours" } else { "hour" };
            format!("in {hours} {hour_unit} and {minutes} {minute_unit}")
        }
    }
}

/// Confirm/Cancel buttons.
struct MenuButtons {
    button_box: gtk4::Box,
    cancel_button: Button,
    confirm_button: Button,
}

impl MenuButtons {
    fn new() -> Self {
        let cancel_button = Button::with_label("Cancel");

        let confirm_button = Button::with_label("Confirm");
        confirm_button.set_halign(Align::End);
        confirm_button.set_hexpand(true);

        let button_box = gtk4::Box::new(Orientation::Horizontal, 0);
        button_box.append(&cancel_button);
        button_box.append(&confirm_button);
        button_box.set_hexpand(true);

        Self { button_box, confirm_button, cancel_button }
    }

    /// Get the GTK widget.
    fn widget(&self) -> &gtk4::Box {
        &self.button_box
    }

    /// Add confirm button handler.
    fn on_confirm<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.confirm_button.connect_clicked(move |_| f());
    }

    /// Add cancel button handler.
    fn on_cancel<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.cancel_button.connect_clicked(move |_| f());
    }
}
