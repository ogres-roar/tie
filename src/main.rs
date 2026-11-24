use std::time::Duration;

use iced::widget::{center, column};
use iced::{Element, Size, Subscription, Task, time, window};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub fn main() -> iced::Result {
    iced::application(Closet::new, Closet::update, Closet::view)
        .title("")
        .window_size(Size::new(300.0, 500.0))
        .exit_on_close_request(false)
        .subscription(Closet::subscription)
        .run()
}

#[derive(Default)]
struct Closet {
    window_id: Option<window::Id>,
    tray_icon: Option<TrayIcon>,
    window_hidden: bool,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    WindowOpened(window::Id),
    CloseRequested(window::Id),
    PollTray,
}

impl Closet {
    fn new() -> Self {
        Closet::default()
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

                Task::none()
            }
            Message::CloseRequested(id) if Some(id) == self.window_id => {
                self.window_hidden = true;
                window::set_mode(id, window::Mode::Hidden)
            }
            Message::CloseRequested(_) => Task::none(),
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

                if clicked && self.window_hidden {
                    self.show_window()
                } else {
                    Task::none()
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::open_events().map(Message::WindowOpened),
            window::close_requests().map(Message::CloseRequested),
            time::every(Duration::from_millis(100)).map(|_| Message::PollTray),
        ])
    }

    fn show_window(&mut self) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };

        self.window_hidden = false;
        Task::batch([
            window::set_mode(id, window::Mode::Windowed),
            window::gain_focus(id),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let content = column!(iced::widget::text("hello!"));
        center(content).padding(20).into()
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
