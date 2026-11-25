mod drawer;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{Element, Length, Size, Subscription, Task, time, window};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub fn main() -> Result<(), Box<dyn Error>> {
    let drawers = drawer::load()?;

    iced::application(
        move || Closet::new(drawers.clone()),
        Closet::update,
        Closet::view,
    )
    .title("Tie")
    .window_size(Size::new(300.0, 500.0))
    .theme(|_: &Closet| iced::Theme::Dark)
    .decorations(false)
    .exit_on_close_request(false)
    .subscription(Closet::subscription)
    .run()?;

    Ok(())
}

struct Closet {
    window_id: Option<window::Id>,
    tray_icon: Option<TrayIcon>,
    window_hidden: bool,
    window_pinned: bool,
    drawers: drawer::Manager,
}

#[derive(Debug, Clone)]
enum Message {
    WindowOpened(window::Id),
    CloseRequested(window::Id),
    PollTray,
    DragWindow,
    HideWindow,
    TogglePin,
    Drawer(drawer::Message),
}

impl Closet {
    fn new(drawers: Arc<[drawer::Drawer]>) -> Self {
        Self {
            window_id: None,
            tray_icon: None,
            window_hidden: false,
            window_pinned: false,
            drawers: drawer::Manager::new(drawers),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(id) => {
                self.window_id = Some(id);

                #[cfg(target_os = "macos")]
                if let Err(error) = set_dock_icon() {
                    eprintln!("Failed to set macOS Dock icon: {error}");
                }

                if self.tray_icon.is_none() {
                    match create_tray_icon() {
                        Ok(icon) => self.tray_icon = Some(icon),
                        Err(error) => eprintln!("Failed to create system tray icon: {error}"),
                    }
                }

                self.drawers.refresh().map(Message::Drawer)
            }
            Message::CloseRequested(id) if Some(id) == self.window_id => self.hide_window(id),
            Message::CloseRequested(_) => Task::none(),
            Message::DragWindow => self.window_id.map_or_else(Task::none, window::drag),
            Message::HideWindow => self
                .window_id
                .map_or_else(Task::none, |id| self.hide_window(id)),
            Message::TogglePin => self.toggle_pin(),
            Message::PollTray => {
                let clicked = loop {
                    match TrayIconEvent::receiver().try_recv() {
                        Ok(TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }) => break true,
                        Ok(_) => continue,
                        Err(_) => break false,
                    }
                };

                if clicked {
                    self.show_window()
                } else {
                    Task::none()
                }
            }
            Message::Drawer(message) => self.drawers.update(message).map(Message::Drawer),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::open_events().map(Message::WindowOpened),
            window::close_requests().map(Message::CloseRequested),
            time::every(Duration::from_millis(100)).map(|_| Message::PollTray),
            self.drawers.subscription().map(Message::Drawer),
        ])
    }

    fn show_window(&mut self) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };

        self.window_hidden = false;
        let show_window = window::set_mode(id, window::Mode::Windowed);

        #[cfg(target_os = "macos")]
        let show_window = show_window.chain(
            window::run(id, |_| {
                if let Err(error) = activate_macos_app() {
                    eprintln!("Failed to activate macOS app: {error}");
                }
            })
            .then(|_| Task::<Message>::none()),
        );

        show_window.chain(window::gain_focus(id))
    }

    fn hide_window(&mut self, id: window::Id) -> Task<Message> {
        self.window_hidden = true;
        window::set_mode(id, window::Mode::Hidden)
    }

    fn toggle_pin(&mut self) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };

        self.window_pinned = !self.window_pinned;
        let level = if self.window_pinned {
            window::Level::AlwaysOnTop
        } else {
            window::Level::Normal
        };

        let set_level = window::set_level(id, level);

        #[cfg(target_os = "macos")]
        let set_level = set_level.chain(
            window::run(id, |handle| {
                if let Err(error) = set_macos_window_space_behavior(handle) {
                    eprintln!("Failed to configure macOS window Spaces behavior: {error}");
                }
            })
            .then(|_| Task::<Message>::none()),
        );

        set_level
    }

    fn view(&self) -> Element<'_, Message> {
        let title_bar = row![
            button("×").on_press(Message::HideWindow),
            mouse_area(
                container(text("Tie"))
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(Message::DragWindow),
            button(if self.window_pinned {
                "取消置顶"
            } else {
                "置顶"
            })
            .on_press(Message::TogglePin),
        ]
        .height(40)
        .padding(4)
        .spacing(4);

        column![title_bar, self.drawers.view().map(Message::Drawer)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn create_tray_icon() -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let mut rgba = vec![0; 16 * 16 * 4];

    for y in 2..14 {
        for x in 3..13 {
            if y < 5 || (7..9).contains(&x) {
                let pixel = (y * 16 + x) * 4;
                rgba[pixel..pixel + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }

    let icon = Icon::from_rgba(rgba, 16, 16)?;
    let builder = TrayIconBuilder::new().with_tooltip("tie").with_icon(icon);

    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_as_template(true);

    Ok(builder.with_menu_on_left_click(false).build()?)
}

#[cfg(target_os = "macos")]
fn set_dock_icon() -> Result<(), &'static str> {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let main_thread = MainThreadMarker::new().ok_or("must be called on the main thread")?;
    let data = NSData::with_bytes(include_bytes!("../assets/tie-icon.png"));
    let image = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or("the embedded PNG icon could not be decoded")?;

    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn set_macos_window_space_behavior(handle: &dyn window::Window) -> Result<(), &'static str> {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior};
    use window::raw_window_handle::RawWindowHandle;

    let handle = handle
        .window_handle()
        .map_err(|_| "failed to get the macOS window handle")?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("the window does not use the AppKit windowing system");
    };
    let view = unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }
        .ok_or("failed to retain the macOS window view")?;
    let window = view
        .window()
        .ok_or("the macOS window view is not attached to a window")?;

    let behavior = window.collectionBehavior();
    let space_behavior = NSWindowCollectionBehavior::Managed
        | NSWindowCollectionBehavior::Transient
        | NSWindowCollectionBehavior::Stationary;
    window
        .setCollectionBehavior((behavior & !space_behavior) | NSWindowCollectionBehavior::Managed);

    Ok(())
}

#[cfg(target_os = "macos")]
fn activate_macos_app() -> Result<(), &'static str> {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

    let options = NSApplicationActivationOptions::ActivateAllWindows;
    if !NSRunningApplication::currentApplication().activateWithOptions(options) {
        return Err("macOS did not activate the application");
    }

    Ok(())
}
