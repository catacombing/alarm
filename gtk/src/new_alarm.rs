//! UI for creating a new alarm.

use alarm::Alarms;
use gtk4::glib::MainContext;
use gtk4::prelude::*;
use gtk4::{
    Align, Button, DropDown, Expression, Label, Orientation, PolicyType, ScrolledWindow, StringList,
};
use rezz::Alarm;
use time::{Duration, OffsetDateTime, Time};
use uuid::Uuid;

use crate::navigation::{Navigator, Page};

/// Height of hour/minute labels.
const TIME_BLABEL_HEIGHT: i32 = 75;

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
    time_box: gtk4::Box,
    hours: ScrolledWindow,
    minutes: ScrolledWindow,
}

impl TimeInput {
    fn new() -> Self {
        // Create container for hour selection.
        let hour_labels: Vec<_> = (0..24).map(|hour| hour.to_string()).collect();
        let hours = Self::scroll_buttons(&hour_labels);

        // Hour/Minute separator.
        let time_separator = Label::new(None);
        time_separator.set_markup(r#"<span size="xx-large">:</span>"#);
        time_separator.set_margin_start(10);
        time_separator.set_margin_end(10);

        // Create container for minute selection.
        let minute_labels: Vec<_> = (0..60).map(|hour| hour.to_string()).collect();
        let minutes = Self::scroll_buttons(&minute_labels);

        // Create horizontal box for time selection.
        let time_box = gtk4::Box::new(Orientation::Horizontal, 0);
        time_box.append(&hours);
        time_box.append(&time_separator);
        time_box.append(&minutes);
        time_box.set_halign(Align::Center);
        time_box.set_margin_top(25);
        time_box.set_margin_bottom(25);
        time_box.add_css_class("time-box");

        Self { time_box, hours, minutes }
    }

    /// Get the GTK widget.
    fn widget(&self) -> &gtk4::Box {
        &self.time_box
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
            placeholder.set_size_request(TIME_LABEL_WIDTH, TIME_BLABEL_HEIGHT);
            label_box.append(&placeholder);
        }

        // Add all labels.
        for label in labels {
            let label = Label::new(Some(&format!("{label:0>2}")));
            label.set_size_request(TIME_LABEL_WIDTH, TIME_BLABEL_HEIGHT);
            label_box.append(&label);
        }

        // Add placeholders at bottom to center last the label.
        for _ in 0..(TIME_SLOT_COUNT - 1) / 2 {
            let placeholder = gtk4::Box::new(Orientation::Horizontal, 0);
            placeholder.set_size_request(TIME_LABEL_WIDTH, TIME_BLABEL_HEIGHT);
            label_box.append(&placeholder);
        }

        // Create scrollbox.
        let scroll = ScrolledWindow::new();
        scroll.set_child(Some(&label_box));
        scroll.set_size_request(TIME_LABEL_WIDTH, TIME_BLABEL_HEIGHT * TIME_SLOT_COUNT);
        scroll.set_hscrollbar_policy(PolicyType::External);
        scroll.set_vscrollbar_policy(PolicyType::External);

        // Set scroll limits.
        let label_count = (labels.len() as i32 + TIME_SLOT_COUNT - 1) as f64;
        scroll.vadjustment().set_upper(label_count * TIME_BLABEL_HEIGHT as f64);

        scroll
    }

    /// Get the selected minute.
    fn unix_time(&self) -> i64 {
        // Translate scrolling position to time.
        let pixel_offset_minutes = self.minutes.vadjustment().value();
        let minute = (pixel_offset_minutes / TIME_BLABEL_HEIGHT as f64).round() as u8;
        let pixel_offset_hours = self.hours.vadjustment().value();
        let hour = (pixel_offset_hours / TIME_BLABEL_HEIGHT as f64).round() as u8;
        let time = Time::from_hms(hour, minute, 0).unwrap();

        // Get next occurrence of the specified time.
        let mut date_time =
            OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        if time < date_time.time() {
            date_time += Duration::days(1);
        }
        date_time = date_time.replace_time(time);

        // Convert time to unix time.
        (date_time - OffsetDateTime::UNIX_EPOCH).whole_seconds()
    }

    /// Reset this input to its defaults.
    fn reset(&self) {
        // Get current time.
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let mut time = now.time();

        // Add one minute to ensure time is in the future.
        time += Duration::minutes(1);

        // Update inputs.
        let pixel_offset_hours = time.hour() as f64 * TIME_BLABEL_HEIGHT as f64;
        self.hours.vadjustment().set_value(pixel_offset_hours);
        let pixel_offset_minutes = time.minute() as f64 * TIME_BLABEL_HEIGHT as f64;
        self.minutes.vadjustment().set_value(pixel_offset_minutes);
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
